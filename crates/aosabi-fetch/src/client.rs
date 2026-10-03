// SPDX-License-Identifier: MIT
/*
 * aosabi-fetch: the outbound client and its idle connections.
 *
 * No analogue file on the reference side.
 */

//! `NativeFetch`, the driver of the network capability over TCP and TLS.
//!
//! A call parses the absolute URL, takes an idle connection to its origin or
//! opens one, writes the request and reads the answer through `head.rs`, and
//! keeps the connection for the next call to its origin where the request and
//! the answer allow it. A connection opens by resolving the host through the
//! [`AddressCache`] of the client, connecting on the runtime of `aosabi-serve`
//! and running the TLS 1.3 handshake for `https:`. The client follows no
//! redirect, which is `redirect: 'manual'`, and speaks HTTP/1.1 alone.
//!
//! The call starts when [`Fetch::fetch`] is called, as the fetch method of
//! the Fetch standard starts its fetch before it answers its promise, so it
//! runs as a task of the runtime to its end whether or not its caller awaits
//! the answer. A caller that drops the future, as the transport of the Sentry
//! plugin does after one poll, sends its request all the same.
//!
//! A URL whose port is one of [`BAD_PORTS`] ends the call before it resolves
//! or connects, as the fetch of Node ends it with the cause `bad port`, so
//! no caller of this client reaches such a port.
//!
//! A call ends after [`TIMEOUT_MS`] with `Timeout`, the timeout the discovery
//! of openid-client sets, and a body above [`CEILING`] ends it with
//! `BodyTooLarge`. A host with several addresses is tried address by address,
//! the families alternating, each but the last for [`ATTEMPT_MS`] at most,
//! which is the `autoSelectFamily` of Node with its attempt timeout of 250 ms.
//!
//! The TLS client speaks TLS 1.3 alone with the provider `ring`, sends the
//! host as its server name, which rustls leaves out for an IP address, and
//! offers `http/1.1` through ALPN. Its root store is read at the first call to
//! an `https:` URL and kept for the process, so a process that calls no such
//! URL reads no bundle.
//!
//! The client keeps one idle connection per origin, the scheme, the host and
//! the port, whose host names the server name of TLS as well, as the Client of
//! undici keeps the connection of an origin between its requests. It keeps it
//! for [`KEEP_ALIVE_MS`], the `keepAliveTimeout` of undici, or for the timeout
//! the `Keep-Alive` field of the answer names less
//! [`KEEP_ALIVE_THRESHOLD_MS`], at most [`KEEP_ALIVE_MAX_MS`], and not at all
//! where that leaves no time. It keeps no connection after an answer of
//! HTTP/1.0, after `connection: close` from either side or
//! `proxy-connection: close` from the server, after a body the end of the
//! connection frames, after a failure, or where the connection holds bytes
//! after the answer, and a call takes an idle connection only while its socket
//! holds no byte and its peer has not closed it, which `TcpStream::peek` tells
//! with no `unsafe`. A call takes a connection out of the pool until its
//! answer ends, so two calls never share one, and a host resolves only when a
//! connection opens. One task of the runtime closes each idle connection once
//! its time passed, as the Client of undici closes its socket at the end of
//! its timeout.
//!
//! The idle connections and the address cache belong to the runtime of the
//! last call. A process may run one runtime after another under the one client
//! the network capability keeps, as `aosabi-devtool` runs each request of a
//! batch on a runtime of its own, while a socket stays in the reactor of the
//! runtime that opened it, which no loop polls once its `block_on` returned,
//! and the times of the pool and of the cache read the clock of a runtime,
//! which reads 0 at its start. So the first call on another runtime closes the
//! idle connections, counts the task that closed them as gone with the
//! executor that ran it, and empties the address cache, a call that ends on a
//! runtime the pool no longer belongs to closes its connection instead of
//! keeping it, and a call never waits on a socket of a runtime that no longer
//! runs. The task that closes the idle connections holds the clock and the
//! timer of its runtime and no handle of it, since a task that holds the
//! handle of its own runtime keeps the executor that holds the task, and with
//! it the reactor, after the runtime ended. An idle connection holds the
//! handle of its runtime until it closes, so a runtime that ended with one
//! lasts until the first call on another runtime closes it.

use core::{
    cell::{
        OnceCell,
        RefCell, //
    },
    future::{
        Future,
        poll_fn, //
    },
    pin::Pin,
    task::{
        Context,
        Poll,
        Waker, //
    },
    time::Duration, //
};
use std::{
    io,
    net::SocketAddr,
    rc::{
        Rc,
        Weak, //
    },
    sync::Arc, //
};

use aosabi_core::net::{
    Fetch,
    FetchError, //
};
use aosabi_http::{
    Request,
    Response,
    Url,
    parse_url, //
};
use aosabi_serve::runtime::{
    Handle,
    LocalBoxFuture,
    RuntimeClock,
    hyper_rt::{
        HyperIo,
        RuntimeTimer, //
    },
    net::TcpStream, //
};
use hyper::rt::{
    Read,
    ReadBufCursor,
    Write, //
};
use rustls::{
    ClientConfig,
    crypto::CryptoProvider, //
};
use rustls_pki_types::ServerName;

use crate::{
    body::CEILING,
    error,
    head::{
        check_request,
        read_response,
        write_request, //
    },
    resolve::AddressCache,
    roots::RootSources,
    tls::TlsStream, //
};

/// The bound of one call in milliseconds, 30 seconds.
pub const TIMEOUT_MS: u64 = 30_000;

/// The time one attempt at an address takes at most while another address is
/// left, in milliseconds.
pub const ATTEMPT_MS: u64 = 250;

/// The protocol the TLS client offers through ALPN.
pub const ALPN: &[u8] = b"http/1.1";

/// The time an idle connection is kept when the answer names no timeout, in
/// milliseconds, the default `keepAliveTimeout` of undici 7.22.0.
pub const KEEP_ALIVE_MS: u64 = 4_000;

/// The time the client takes off the timeout the `Keep-Alive` field of an
/// answer names, in milliseconds, the default `keepAliveTimeoutThreshold` of
/// undici 7.22.0.
pub const KEEP_ALIVE_THRESHOLD_MS: u64 = 2_000;

/// The longest time an idle connection is kept, in milliseconds, the default
/// `keepAliveMaxTimeout` of undici 7.22.0.
pub const KEEP_ALIVE_MAX_MS: u64 = 600_000;

/// The ports the fetch of undici refuses, its list `badPorts`, in ascending
/// order.
///
/// A fetch of an `http:` or `https:` URL whose port is one of them ends in a
/// network error before it connects, which the fetch of Node rejects with the
/// `TypeError` `fetch failed` and the cause `bad port`. The undici 7.18.2 that
/// node v24.13.0 bundles holds the same 82 ports in the same order. The table
/// of bad ports of the Fetch standard lists port 0 before them, which undici
/// does not refuse, so the fetch of node tries to connect to port 0 and this
/// client does as well.
pub const BAD_PORTS: &[u16] = &[
    1, 7, 9, 11, 13, 15, 17, 19, 20, 21, 22, 23, 25, 37, 42, 43, 53, 69, 77, 79, 87, 95, 101, 102,
    103, 104, 109, 110, 111, 113, 115, 117, 119, 123, 135, 137, 139, 143, 161, 179, 389, 427, 465,
    512, 513, 514, 515, 526, 530, 531, 532, 540, 548, 554, 556, 563, 587, 601, 636, 989, 990, 993,
    995, 1719, 1720, 1723, 2049, 3659, 4045, 4190, 5060, 5061, 6000, 6566, 6665, 6666, 6667, 6668,
    6669, 6679, 6697, 10080,
];

/// The outbound HTTP client of the Linux cards, the driver of the network capability.
#[derive(Clone)]
pub struct NativeFetch {
    /// The parts every call shares.
    shared: Rc<Shared>,
}

/// The parts of a client its calls share.
struct Shared {
    /// The crypto provider of the TLS client.
    provider: Arc<CryptoProvider>,
    /// The files of the root store.
    roots: RootSources,
    /// The configuration of the TLS client once the first call to an
    /// `https:` URL built it, or the reason it could not be built.
    config: OnceCell<Result<Arc<ClientConfig>, String>>,
    /// The bound of a call in milliseconds.
    timeout_ms: u64,
    /// The bytes a body holds at most.
    ceiling: usize,
    /// The last good answer of the resolver for each host.
    addresses: RefCell<AddressCache>,
    /// The idle connections, one per origin.
    pool: RefCell<Pool>,
}

impl NativeFetch {
    /// Returns the client the environment configures.
    ///
    /// The variables of the root store are read here, and the files they
    /// name at the first call to an `https:` URL.
    ///
    /// # Errors
    ///
    /// Returns an error when the provider `ring` offers no suite of TLS 1.3.
    pub fn from_env() -> io::Result<Self> {
        let provider = Arc::new(rustls::crypto::ring::default_provider());
        ClientConfig::builder_with_provider(Arc::clone(&provider))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(io::Error::other)?;
        Ok(Self {
            shared: Rc::new(Shared {
                provider,
                roots: RootSources::from_env(),
                config: OnceCell::new(),
                timeout_ms: TIMEOUT_MS,
                ceiling: CEILING,
                addresses: RefCell::new(AddressCache::new()),
                pool: RefCell::new(Pool::default()),
            }),
        })
    }
}

impl Fetch for NativeFetch {
    fn fetch(&self, request: Request) -> LocalBoxFuture<'static, Result<Response, FetchError>> {
        let Ok(handle) = Handle::current() else {
            return Box::pin(async { Err(FetchError::NoNetwork) });
        };
        let slot = Rc::new(RefCell::new(Slot::default()));
        let running = call(Rc::clone(&self.shared), request);
        let running = bounded(&handle, self.shared.timeout_ms, running);
        let filled = Rc::clone(&slot);
        handle.spawn(Box::pin(async move {
            let result = running.await;
            let waker = {
                let mut slot = filled.borrow_mut();
                slot.result = Some(result);
                slot.waker.take()
            };
            if let Some(waker) = waker {
                waker.wake();
            }
        }));
        Box::pin(Answer { slot })
    }
}

/// The answer of a call that its task fills and its caller waits for.
#[derive(Default)]
struct Slot {
    /// The result, once the call ended.
    result: Option<Result<Response, FetchError>>,
    /// The waker of the caller that waits.
    waker: Option<Waker>,
}

/// The future a caller awaits, ready once the task of the call filled the slot.
struct Answer {
    /// The slot the task fills.
    slot: Rc<RefCell<Slot>>,
}

impl Future for Answer {
    type Output = Result<Response, FetchError>;

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut slot = self.slot.borrow_mut();
        if let Some(result) = slot.result.take() {
            return Poll::Ready(result);
        }
        slot.waker = Some(cx.waker().clone());
        Poll::Pending
    }
}

/// Returns `call` bounded by `timeout_ms` on the timer of the runtime of
/// `handle`, which ends it with `Timeout`.
fn bounded(
    handle: &Handle,
    timeout_ms: u64,
    call: LocalBoxFuture<'static, Result<Response, FetchError>>,
) -> LocalBoxFuture<'static, Result<Response, FetchError>> {
    let mut deadline = RuntimeTimer::of(handle).sleep_for(Duration::from_millis(timeout_ms));
    let mut call = call;
    Box::pin(poll_fn(move |cx| {
        if let Poll::Ready(result) = call.as_mut().poll(cx) {
            return Poll::Ready(result);
        }
        match Pin::new(&mut deadline).poll(cx) {
            Poll::Ready(()) => Poll::Ready(Err(FetchError::Timeout)),
            Poll::Pending => Poll::Pending,
        }
    }))
}

/// Runs one call of `request` to its answer.
///
/// A failure drops the connection of the call wherever it happens, so only a
/// connection whose answer ended whole returns to the pool.
fn call(
    shared: Rc<Shared>,
    request: Request,
) -> LocalBoxFuture<'static, Result<Response, FetchError>> {
    Box::pin(async move {
        let url = target(&request.url)?;
        let checked = check_request(&request)?;
        let (method, close) = (checked.method.into_owned(), checked.close);
        let secure = url.protocol == "https:";
        let port = match url.port.parse::<u16>() {
            Ok(port) => port,
            Err(_) if secure => 443,
            Err(_) => 80,
        };
        let handle = Handle::current().map_err(|_| FetchError::NoNetwork)?;
        let clock = shared.enter(&handle);
        let deadline_ms = clock.now_ms().saturating_add(shared.timeout_ms);
        let origin = Origin {
            secure,
            hostname: url.hostname.clone(),
            port,
        };
        let idle = shared.pool.borrow_mut().take(&origin, clock.now_ms());
        let mut conn = match idle {
            Some(conn) => conn,
            None => connect(&shared, &handle, &origin, deadline_ms).await?,
        };
        write_request(&mut conn, &request).await?;
        let answered = read_response(&mut conn, &method, shared.ceiling).await?;
        if !close
            && answered.reusable
            && let Some(keep_ms) = keep_time(answered.timeout_s)
        {
            shared.keep(&handle, origin, conn, keep_ms);
        }
        Ok(answered.response)
    })
}

/// Returns the time a connection is kept after an answer whose `Keep-Alive`
/// field names `timeout_s`, in milliseconds, or `None` where it is not kept.
///
/// An answer that names no timeout keeps it for [`KEEP_ALIVE_MS`], and one
/// that names a timeout for that timeout less [`KEEP_ALIVE_THRESHOLD_MS`], at
/// most [`KEEP_ALIVE_MAX_MS`], and not at all where that leaves no time, as
/// `onHeadersComplete` of undici computes it.
fn keep_time(timeout_s: Option<u64>) -> Option<u64> {
    let Some(timeout_s) = timeout_s else {
        return Some(KEEP_ALIVE_MS);
    };
    let keep_ms = timeout_s
        .saturating_mul(1_000)
        .saturating_sub(KEEP_ALIVE_THRESHOLD_MS)
        .min(KEEP_ALIVE_MAX_MS);
    (keep_ms > 0).then_some(keep_ms)
}

/// Opens a connection to `origin`: resolves its host through the address
/// cache, connects to the first address that answers before `deadline_ms`
/// and, for `https:`, runs the TLS handshake.
async fn connect(
    shared: &Shared,
    handle: &Handle,
    origin: &Origin,
    deadline_ms: u64,
) -> Result<Conn, FetchError> {
    let addrs = shared.addresses.borrow_mut().resolve(
        &origin.hostname,
        origin.port,
        handle.clock().now_ms(),
    )?;
    let stream = open(handle, &addrs, deadline_ms).await?;
    if !origin.secure {
        return Ok(Conn::Plain(HyperIo::new(stream)));
    }
    let config = shared.config()?;
    let name = server_name(&origin.hostname)?;
    let tls = TlsStream::handshake(stream, config, name)
        .await
        .map_err(|failure| error::handshake(&failure))?;
    Ok(Conn::Tls(Box::new(tls)))
}

/// Returns the absolute URL of `href`, refused unless it is `http:` or
/// `https:` with no credentials and no port of [`BAD_PORTS`], as the fetch
/// of Node refuses it.
///
/// The port of a URL is empty for the default of its scheme, which is no
/// bad port, as `requestBadPort` of undici reads `url.port`.
fn target(href: &str) -> Result<Url, FetchError> {
    let invalid = || FetchError::InvalidUrl(String::from(href));
    let url = parse_url(href, None).map_err(|_| invalid())?;
    if !matches!(url.protocol.as_str(), "http:" | "https:")
        || !url.username.is_empty()
        || !url.password.is_empty()
    {
        return Err(invalid());
    }
    if url
        .port
        .parse::<u16>()
        .is_ok_and(|port| BAD_PORTS.binary_search(&port).is_ok())
    {
        return Err(FetchError::Connect(format!("{}: bad port", url.host)));
    }
    Ok(url)
}

/// Connects to the first of `addrs` that answers before `deadline_ms`, each
/// but the last for [`ATTEMPT_MS`] at most.
async fn open(
    handle: &Handle,
    addrs: &[SocketAddr],
    deadline_ms: u64,
) -> Result<TcpStream, FetchError> {
    let mut failure = FetchError::Timeout;
    for (i, addr) in addrs.iter().enumerate() {
        let now_ms = handle.clock().now_ms();
        if now_ms >= deadline_ms {
            return Err(FetchError::Timeout);
        }
        let until = if i + 1 < addrs.len() {
            deadline_ms.min(now_ms.saturating_add(ATTEMPT_MS))
        } else {
            deadline_ms
        };
        match TcpStream::connect(*addr, until).await {
            Ok(stream) => return Ok(stream),
            Err(cause) => failure = error::connect(*addr, &cause),
        }
    }
    Err(failure)
}

/// Returns the server name of `hostname`, an IP address without its brackets
/// or a DNS name.
fn server_name(hostname: &str) -> Result<ServerName<'static>, FetchError> {
    let literal = hostname
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(hostname);
    ServerName::try_from(literal)
        .map(|name| name.to_owned())
        .map_err(|_| FetchError::Tls(format!("{hostname} is no server name of TLS")))
}

impl Shared {
    /// Returns the configuration of the TLS client, built at the first call.
    fn config(&self) -> Result<Arc<ClientConfig>, FetchError> {
        self.config
            .get_or_init(|| self.build_config())
            .clone()
            .map_err(FetchError::Tls)
    }

    /// Builds the configuration of the TLS client over the root store.
    fn build_config(&self) -> Result<Arc<ClientConfig>, String> {
        let roots = self.roots.load()?;
        let mut config = ClientConfig::builder_with_provider(Arc::clone(&self.provider))
            .with_protocol_versions(&[&rustls::version::TLS13])
            .map_err(|failure| failure.to_string())?
            .with_root_certificates(roots)
            .with_no_client_auth();
        config.alpn_protocols = vec![ALPN.to_vec()];
        Ok(Arc::new(config))
    }

    /// Returns the clock of the runtime of `handle`, once the pool and the
    /// address cache belong to that runtime.
    ///
    /// Where the last call ran on another runtime, the idle connections close,
    /// since the reactor that holds their sockets is no longer polled, the
    /// task of [`sweep`] counts as gone with the executor that ran it, and the
    /// address cache empties, since its times read the clock of that runtime,
    /// as the module documentation describes.
    fn enter(&self, handle: &Handle) -> Rc<RuntimeClock> {
        let clock = handle.clock();
        let mut pool = self.pool.borrow_mut();
        if !Weak::ptr_eq(&pool.runtime, &Rc::downgrade(&clock)) {
            pool.idle.clear();
            pool.sweeping = false;
            pool.runtime = Rc::downgrade(&clock);
            *self.addresses.borrow_mut() = AddressCache::new();
        }
        clock
    }

    /// Keeps `conn` idle for the next call to `origin` for `keep_ms`, and
    /// starts the task that closes it once that time passed, unless one runs.
    ///
    /// Where the pool belongs to another runtime than the one of `handle`,
    /// since a call on another runtime began while this call ran, `conn`
    /// closes instead, as its socket stays in the reactor of `handle`.
    fn keep(self: &Rc<Self>, handle: &Handle, origin: Origin, conn: Conn, keep_ms: u64) {
        let clock = handle.clock();
        let mut pool = self.pool.borrow_mut();
        if !Weak::ptr_eq(&pool.runtime, &Rc::downgrade(&clock)) {
            return;
        }
        let until_ms = clock.now_ms().saturating_add(keep_ms);
        pool.put(origin, conn, until_ms);
        if !pool.sweeping {
            pool.sweeping = true;
            let timer = RuntimeTimer::of(handle);
            handle.spawn(sweep(Rc::downgrade(self), clock, timer));
        }
    }
}

/// Returns the task that closes each idle connection of the client of
/// `shared` once its time passed on `clock`, waiting on `timer`, and ends
/// once the pool holds none or belongs to the runtime of another clock.
///
/// The task holds the clock and the timer of its runtime and no handle of
/// it, so a runtime that ends while the task waits frees its executor, and
/// the task with it, once the idle connections of that runtime close, at
/// the first call on another runtime, as the module documentation describes.
fn sweep(
    shared: Weak<Shared>,
    clock: Rc<RuntimeClock>,
    timer: RuntimeTimer,
) -> LocalBoxFuture<'static, ()> {
    Box::pin(async move {
        while let Some(wait_ms) = expire(&shared, &clock) {
            timer.sleep_for(Duration::from_millis(wait_ms)).await;
        }
    })
}

/// Closes the idle connections of the client of `shared` whose time passed on
/// `clock`, and answers the milliseconds until the next one passes, or `None`
/// once the pool holds none, belongs to the runtime of another clock or the
/// client is gone.
fn expire(shared: &Weak<Shared>, clock: &Rc<RuntimeClock>) -> Option<u64> {
    let shared = shared.upgrade()?;
    let mut pool = shared.pool.borrow_mut();
    if !Weak::ptr_eq(&pool.runtime, &Rc::downgrade(clock)) {
        return None;
    }
    let now_ms = clock.now_ms();
    pool.expire(now_ms);
    let next = pool.idle.iter().map(|idle| idle.until_ms).min();
    pool.sweeping = next.is_some();
    next.map(|until_ms| until_ms.saturating_sub(now_ms))
}

/// The origin of a connection, the scheme, the host and the port, whose host
/// names the server name of TLS as well.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Origin {
    /// Whether the scheme is `https:`.
    secure: bool,
    /// The host of the URL without its port.
    hostname: String,
    /// The port, the default of the scheme where the URL names none.
    port: u16,
}

/// A connection of the client, over TCP or over TLS.
enum Conn {
    /// A connection of `http:`.
    Plain(HyperIo<TcpStream>),
    /// A connection of `https:`, boxed, since its state of rustls is large.
    Tls(Box<TlsStream<TcpStream>>),
}

impl Conn {
    /// Returns whether the connection holds no byte of its peer and its peer
    /// has not closed it, so it can carry the next request.
    fn is_idle(&self) -> bool {
        match self {
            Self::Plain(io) => socket_idle(io.get_ref()),
            Self::Tls(tls) => tls.holds_nothing() && socket_idle(tls.get_ref()),
        }
    }
}

/// Returns whether `stream` holds no byte and its peer has not closed it,
/// which a peek of one byte that would block tells.
fn socket_idle(stream: &TcpStream) -> bool {
    let mut byte = [0_u8; 1];
    matches!(
        stream.as_std().peek(&mut byte),
        Err(failure) if failure.kind() == io::ErrorKind::WouldBlock
    )
}

impl Read for Conn {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(io) => Pin::new(io).poll_read(cx, buf),
            Self::Tls(tls) => Pin::new(tls.as_mut()).poll_read(cx, buf),
        }
    }
}

impl Write for Conn {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match self.get_mut() {
            Self::Plain(io) => Pin::new(io).poll_write(cx, buf),
            Self::Tls(tls) => Pin::new(tls.as_mut()).poll_write(cx, buf),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(io) => Pin::new(io).poll_flush(cx),
            Self::Tls(tls) => Pin::new(tls.as_mut()).poll_flush(cx),
        }
    }

    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.get_mut() {
            Self::Plain(io) => Pin::new(io).poll_shutdown(cx),
            Self::Tls(tls) => Pin::new(tls.as_mut()).poll_shutdown(cx),
        }
    }
}

/// The idle connections of a client, one per origin.
#[derive(Default)]
struct Pool {
    /// The idle connections.
    idle: Vec<Idle>,
    /// Whether the task of [`sweep`] runs.
    sweeping: bool,
    /// The clock of the runtime whose reactor holds the idle connections and
    /// whose executor runs the task of [`sweep`], which no clock matches
    /// before the first call.
    runtime: Weak<RuntimeClock>,
}

/// An idle connection and the time until which the pool keeps it.
struct Idle {
    /// The origin of the connection.
    origin: Origin,
    /// The connection.
    conn: Conn,
    /// The time in milliseconds of the clock of the runtime at which the
    /// connection closes.
    until_ms: u64,
}

impl Pool {
    /// Closes every idle connection whose time passed at `now_ms`.
    fn expire(&mut self, now_ms: u64) {
        self.idle.retain(|idle| idle.until_ms > now_ms);
    }

    /// Takes the idle connection to `origin` out of the pool at `now_ms`, or
    /// `None` where there is none or it holds a byte or its peer closed it,
    /// and then it closes.
    fn take(&mut self, origin: &Origin, now_ms: u64) -> Option<Conn> {
        self.expire(now_ms);
        let at = self.idle.iter().position(|idle| idle.origin == *origin)?;
        let idle = self.idle.swap_remove(at);
        idle.conn.is_idle().then_some(idle.conn)
    }

    /// Keeps `conn` to `origin` until `until_ms`, in place of the idle
    /// connection to the origin, which closes.
    fn put(&mut self, origin: Origin, conn: Conn, until_ms: u64) {
        self.idle.retain(|idle| idle.origin != origin);
        self.idle.push(Idle {
            origin,
            conn,
            until_ms,
        });
    }
}

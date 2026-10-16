// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool http: the Node connection of the fork driven in process over
 * an IO in memory, a clock the request line moves and a service that records
 * what it is handed.
 *
 * No analogue in the reference. The request lines are those of family H and
 * of the other families of tests/gen/, and the capture of their bytes from
 * the reference is tests/capture/m5_rows.py, row C48, which sends each case in
 * its writes and pauses and waits 0.4 seconds of quiet after the last.
 */

//! `answer_all` and the pieces it drives.
//!
//! One request line is one connection of `node1` under the configuration of
//! the card. Before each write the clock of the connection moves by the pause
//! of that write, the connection runs until it waits, and then the write is
//! the next read of the IO, so a timer of Node fires where the pause lets it,
//! the cleanup of an unread body 2 ms after an answer among them. A client
//! that shuts its write half is read as the end of the stream after its last
//! write, and the clock moves by the 0.4 seconds of quiet the capture waits
//! before the connection is judged. A write after the connection ended is
//! dropped, as a peer that closed drops it.
//!
//! The recording service stands where the application stands. It reads the
//! body of a `POST` whose path ends in `/graphql` to its end before it
//! answers, as Yoga reads it, answers every other request without reading
//! its body, as the unhandled route of the reference does, and marks the
//! answer of a method other than `GET` and `HEAD` for the cleanup of its
//! unread body, as the adapter of `@hono/node-server` does (A10 of the plan's
//! version 2 draft). Its answer is `200` with a `content-type`, so a notebook
//! tells an answer of the application from one of Node's own, which carry
//! none. It answers from a later turn of the driver, one poll of the
//! connection later at the earliest and one answer a turn, as Node's
//! recording service of `tests/hardening/upstream/node-recorder.mjs` answers
//! from the next turn of the event loop through `setImmediate`, so a read is
//! parsed whole before such an answer, while the connection runs the service
//! once at each head as Node runs its listener.

use core::{
    cell::{
        Cell,
        RefCell, //
    },
    convert::Infallible,
    future::Future,
    pin::{
        Pin,
        pin, //
    },
    task::{
        Context,
        Poll,
        Waker, //
    },
    time::Duration,
};
use std::{
    collections::VecDeque,
    io,
    rc::Rc,
    sync::{
        Arc,
        Mutex,
        PoisonError, //
    },
};

use aosabi_json::{
    JsObject,
    JsValue, //
};
use hyper::{
    body::{
        Body,
        Bytes,
        Frame,
        Incoming,
        SizeHint, //
    },
    ext::{
        FirstWrite,
        NodeAnswer,
        RawTarget, //
    },
    http::{
        HeaderValue,
        Request,
        Response,
        header, //
    },
    rt::{
        ReadBufCursor,
        Sleep,
        bare::Timer, //
    },
    server::conn::node1::{
        Builder,
        NodeConfig, //
    },
    service::Service,
};

use crate::{
    json::reader,
    provenance::sha256::Sha256,
    yoga::base64, //
};

/// The quiet the capture waits after the last write, in milliseconds.
pub const IDLE_MS: u64 = 400;

/// The body of every answer of the recording service.
pub const ANSWER_BODY: &[u8] = b"recorded";

/// The digits of lowercase hex.
const HEX: &[u8; 16] = b"0123456789abcdef";

/// The polls one run of the connection may take before it is taken as
/// waiting, a bound no case of the families reaches.
const POLLS: usize = 1_000_000;

/// Answers every request line of `bytes` under `config`.
pub fn answer_all(config: NodeConfig, bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    for line in bytes.split(|&byte| byte == b'\n') {
        if line.trim_ascii().is_empty() {
            continue;
        }
        aosabi_json::write(&mut out, &JsValue::Object(answer(config, line)));
        out.push(b'\n');
    }
    out
}

/// Returns the response line of one request line.
fn answer(config: NodeConfig, line: &[u8]) -> JsObject {
    let request = match reader::parse(line) {
        Ok(JsValue::Object(request)) => request,
        Ok(_) => return error(JsValue::Null, "the request line is not an object"),
        Err(failure) => {
            return error(
                JsValue::Null,
                &format!("the request line is not JSON: {failure}"),
            );
        }
    };
    let id = request.get("id").cloned().unwrap_or(JsValue::Null);
    match Case::read(&request) {
        Ok(case) => case.run(config).line(id),
        Err(reason) => error(id, &reason),
    }
}

/// Returns the response line `{"id", "error"}`.
fn error(id: JsValue, reason: &str) -> JsObject {
    let mut line = JsObject::with_capacity(2);
    line.insert("id", id);
    line.insert("error", reason);
    line
}

/// One request line: the writes, the pause before each and the shut.
struct Case {
    /// The bytes of each write.
    writes: Vec<Vec<u8>>,
    /// The pause before each write, in milliseconds.
    pauses_ms: Vec<u64>,
    /// Whether the client shuts its write half after the last write.
    shut: bool,
}

impl Case {
    /// Reads the case of a request line.
    fn read(request: &JsObject) -> Result<Self, String> {
        let Some(JsValue::Array(writes)) = request.get("writes") else {
            return Err(String::from("the request line carries no array writes"));
        };
        let writes = writes
            .iter()
            .map(|write| match write {
                JsValue::String(coded) => coded
                    .as_str()
                    .and_then(base64::decode)
                    .ok_or_else(|| String::from("a write is not Base64")),
                _ => Err(String::from("a write is not a string")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        let pauses_ms = match request.get("pauses_ms") {
            None | Some(JsValue::Null) => vec![0; writes.len()],
            Some(JsValue::Array(pauses)) => pauses
                .iter()
                .map(|pause| match pause {
                    JsValue::Number(ms) if *ms >= 0.0 && ms.fract() == 0.0 => {
                        // A pause is a whole number of milliseconds below
                        // 2^53, so the conversion is exact.
                        #[expect(
                            clippy::cast_possible_truncation,
                            clippy::cast_sign_loss,
                            reason = "a whole, positive number of milliseconds below 2^53"
                        )]
                        Ok(*ms as u64)
                    }
                    _ => Err(String::from(
                        "a pause is not a whole number of milliseconds",
                    )),
                })
                .collect::<Result<Vec<_>, _>>()?,
            Some(_) => return Err(String::from("pauses_ms is not an array")),
        };
        if pauses_ms.len() != writes.len() {
            return Err(String::from("pauses_ms and writes differ in length"));
        }
        let shut = matches!(request.get("shut"), Some(JsValue::Bool(true)));
        Ok(Self {
            writes,
            pauses_ms,
            shut,
        })
    }

    /// Runs the case over one connection under `config`.
    fn run(self, config: NodeConfig) -> Outcome {
        let wire = Rc::new(RefCell::new(Wire::default()));
        let clock = VirtualClock::default();
        let log = Rc::new(RefCell::new(Vec::new()));
        let turns = Turns::default();
        let mut builder = Builder::new(config);
        builder.bare_timer(clock.clone());
        let connection = builder.serve_connection(
            MemIo {
                wire: Rc::clone(&wire),
            },
            Recorder {
                log: Rc::clone(&log),
                turns: Rc::clone(&turns),
            },
        );
        let mut driver = Driver {
            connection: pin!(connection),
            wire: Rc::clone(&wire),
            log: Rc::clone(&log),
            turns,
            ended: false,
        };
        for (write, pause) in self.writes.into_iter().zip(self.pauses_ms) {
            clock.advance(pause);
            driver.drive();
            if !driver.ended {
                wire.borrow_mut().input.push_back(write);
            }
            driver.drive();
        }
        if self.shut {
            wire.borrow_mut().shut = true;
            driver.drive();
        }
        clock.advance(IDLE_MS);
        driver.drive();
        let ended = driver.ended;
        drop(driver);
        let wire = wire.borrow();
        Outcome {
            written: wire.output.clone(),
            closed: ended || wire.closed,
            requests: log.borrow().clone(),
        }
    }
}

/// What a connection did.
struct Outcome {
    /// Every byte it wrote.
    written: Vec<u8>,
    /// Whether it ended or shut its write half.
    closed: bool,
    /// The requests it handed the service.
    requests: Vec<Recorded>,
}

impl Outcome {
    /// Returns the response line with `id`.
    fn line(&self, id: JsValue) -> JsObject {
        let requests = self
            .requests
            .iter()
            .map(|recorded| {
                let mut request = JsObject::with_capacity(3);
                request.insert("method", recorded.method.as_str());
                request.insert("target", latin1(&recorded.target));
                request.insert(
                    "body_sha256",
                    recorded
                        .body_sha256
                        .as_deref()
                        .map_or(JsValue::Null, JsValue::from),
                );
                JsValue::Object(request)
            })
            .collect();
        let mut line = JsObject::with_capacity(4);
        line.insert("id", id);
        line.insert("written", base64::encode(&self.written));
        line.insert("closed", self.closed);
        line.insert("requests", JsValue::Array(requests));
        line
    }
}

/// Returns `bytes` read as Latin-1, one character per byte, as Node reads a
/// head.
fn latin1(bytes: &[u8]) -> String {
    bytes.iter().copied().map(char::from).collect()
}

/// The state the IO and the driver share.
#[derive(Default)]
struct Wire {
    /// The writes of the client not read yet.
    input: VecDeque<Vec<u8>>,
    /// Every byte the connection wrote.
    output: Vec<u8>,
    /// Whether the client shut its write half.
    shut: bool,
    /// Whether the connection shut its write half.
    closed: bool,
    /// The bytes that moved in either direction, which tells a run that
    /// progressed from one that waits.
    moved: usize,
}

/// The IO of one connection in memory.
struct MemIo {
    /// The state the driver shares.
    wire: Rc<RefCell<Wire>>,
}

impl hyper::rt::Read for MemIo {
    fn poll_read(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        mut buf: ReadBufCursor<'_>,
    ) -> Poll<io::Result<()>> {
        let mut wire = self.wire.borrow_mut();
        let Some(front) = wire.input.front_mut() else {
            // The end of the stream, or nothing to read until the next write.
            return if wire.shut {
                Poll::Ready(Ok(()))
            } else {
                Poll::Pending
            };
        };
        let n = front.len().min(buf.remaining());
        buf.put_slice(&front[..n]);
        front.drain(..n);
        if front.is_empty() {
            wire.input.pop_front();
        }
        wire.moved += n;
        Poll::Ready(Ok(()))
    }
}

impl hyper::rt::Write for MemIo {
    fn poll_write(
        self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut wire = self.wire.borrow_mut();
        if wire.closed {
            return Poll::Ready(Err(io::ErrorKind::BrokenPipe.into()));
        }
        wire.output.extend_from_slice(buf);
        wire.moved += buf.len().max(1);
        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut wire = self.wire.borrow_mut();
        if !wire.closed {
            wire.closed = true;
            wire.moved += 1;
        }
        Poll::Ready(Ok(()))
    }
}

/// The clock of one connection, which a request line moves.
///
/// The time is shared as `Arc<Mutex<..>>`, because hyper bounds a timer and
/// its sleep by `Send + Sync`, and one thread never contends it.
#[derive(Clone, Default)]
struct VirtualClock {
    /// The time since the epoch of the connection.
    now: Arc<Mutex<Duration>>,
}

impl VirtualClock {
    /// Moves the clock by `ms` milliseconds.
    fn advance(&self, ms: u64) {
        *lock(&self.now) += Duration::from_millis(ms);
    }
}

impl Timer for VirtualClock {
    fn now(&self) -> Duration {
        *lock(&self.now)
    }

    fn sleep_until(&self, deadline: Duration) -> Pin<Box<dyn Sleep>> {
        Box::pin(VirtualSleep {
            deadline,
            now: Arc::clone(&self.now),
        })
    }
}

/// A sleep of the virtual clock, ready once the clock passed its deadline.
struct VirtualSleep {
    /// When the sleep ends.
    deadline: Duration,
    /// The time of the clock.
    now: Arc<Mutex<Duration>>,
}

impl Future for VirtualSleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        if *lock(&self.now) >= self.deadline {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

impl Sleep for VirtualSleep {}

/// Locks `mutex`, which one thread never poisons for another.
fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One request the service was handed.
#[derive(Clone)]
struct Recorded {
    /// The method.
    method: String,
    /// The target as it arrived.
    target: Vec<u8>,
    /// The SHA-256 of the body the service read, as lowercase hex, `None`
    /// when it answered without reading or the body failed.
    body_sha256: Option<String>,
}

/// The service that stands where the application stands.
#[derive(Clone)]
struct Recorder {
    /// The requests handed so far, in order.
    log: Rc<RefCell<Vec<Recorded>>>,
    /// The turns of the driver, from a later one of which it answers.
    turns: Turns,
}

impl Service<Request<Incoming>> for Recorder {
    type Response = Response<Fixed>;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response<Fixed>, Infallible>>>>;

    fn call(&self, request: Request<Incoming>) -> Self::Future {
        let method = request.method().as_str().to_owned();
        let target = request.extensions().get::<RawTarget>().map_or_else(
            || request.uri().to_string().into_bytes(),
            |raw| raw.0.to_vec(),
        );
        let reads = method == "POST" && graphql_path(&target);
        let cleanup = !matches!(method.as_str(), "GET" | "HEAD");
        let index = {
            let mut log = self.log.borrow_mut();
            log.push(Recorded {
                method,
                target,
                body_sha256: None,
            });
            log.len() - 1
        };
        let log = Rc::clone(&self.log);
        let turns = Rc::clone(&self.turns);
        let mut body = request.into_body();
        Box::pin(async move {
            if reads && let Some(digest) = read_body(&mut body).await {
                log.borrow_mut()[index].body_sha256 = Some(digest);
            }
            NextTurn { turns, from: None }.await;
            let mut answer = Response::new(Fixed(Some(Bytes::from_static(ANSWER_BODY))));
            let headers = answer.headers_mut();
            headers.insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=UTF-8"),
            );
            headers.insert(header::CONTENT_LENGTH, HeaderValue::from(ANSWER_BODY.len()));
            answer.extensions_mut().insert(NodeAnswer {
                first_write: FirstWrite::Bytes,
                cleanup_incoming: cleanup,
                destroy: false,
            });
            Ok(answer)
        })
    }
}

/// Tells whether the path of `target` ends in `/graphql`, the endpoint of
/// Yoga in a served app, for an origin form and an absolute form alike.
fn graphql_path(target: &[u8]) -> bool {
    let path = target
        .split(|&byte| byte == b'?')
        .next()
        .unwrap_or_default();
    path.ends_with(b"/graphql")
}

/// Reads `body` to its end and returns the SHA-256 of its bytes as lowercase
/// hex, or `None` when it failed.
async fn read_body(body: &mut Incoming) -> Option<String> {
    let mut digest = Sha256::new();
    loop {
        let frame = core::future::poll_fn(|cx| Pin::new(&mut *body).poll_frame(cx)).await;
        match frame {
            None => break,
            Some(Err(_)) => return None,
            Some(Ok(frame)) => {
                if let Ok(data) = frame.into_data() {
                    digest.update(&data);
                }
            }
        }
    }
    let mut hex = String::with_capacity(64);
    for byte in digest.finalize() {
        hex.push(char::from(HEX[usize::from(byte >> 4)]));
        hex.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    Some(hex)
}

/// The fixed body of an answer of the service.
///
/// Its size hint is not exact, so the connection keeps the `content-length`
/// the service wrote, as it keeps the length line of a record of the adapter.
struct Fixed(Option<Bytes>);

impl Body for Fixed {
    type Data = Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Bytes>, Infallible>>> {
        Poll::Ready(self.0.take().map(|bytes| Ok(Frame::data(bytes))))
    }

    fn size_hint(&self) -> SizeHint {
        SizeHint::default()
    }
}

/// The turns of the driver, one for each poll of the connection.
#[derive(Default)]
struct TurnCount {
    /// The turns so far.
    now: Cell<u64>,
    /// The turn in which a waiting answer last went on.
    taken: Cell<Option<u64>>,
}

/// The turns of the driver, shared by the driver and the service.
type Turns = Rc<TurnCount>;

/// Waits for a later turn of the driver, as `setImmediate` waits for the next
/// turn of Node's event loop.
///
/// One waiting answer goes on in a turn, the first the connection polls, as
/// Node runs one callback of `setImmediate` at a time and its ticks after
/// each, in which the `finish` of an answer hands the socket to the answer
/// that waits behind it. The driver polls the connection again at every turn
/// until a poll moves nothing, so the future registers no waker, as a sleep
/// of the virtual clock registers none.
struct NextTurn {
    /// The turns of the driver.
    turns: Turns,
    /// The turn of the first poll.
    from: Option<u64>,
}

impl Future for NextTurn {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<()> {
        let now = self.turns.now.get();
        let from = *self.from.get_or_insert(now);
        if now > from && self.turns.taken.get() != Some(now) {
            self.turns.taken.set(Some(now));
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

/// The connection of one request line with what the driver watches.
struct Driver<'a, F> {
    /// The connection.
    connection: Pin<&'a mut F>,
    /// The IO state.
    wire: Rc<RefCell<Wire>>,
    /// The requests handed so far.
    log: Rc<RefCell<Vec<Recorded>>>,
    /// The turns, one for each poll of the connection.
    turns: Turns,
    /// Whether the connection future ended.
    ended: bool,
}

impl<F: Future> Driver<'_, F> {
    /// Polls the connection until a poll moves nothing, which is where it
    /// waits for the next write, for its clock or for nothing at all.
    fn drive(&mut self) {
        if self.ended {
            return;
        }
        let mut cx = Context::from_waker(Waker::noop());
        let mut still = 0;
        for _ in 0..POLLS {
            let before = (self.wire.borrow().moved, self.log.borrow().len());
            let ready = self.connection.as_mut().poll(&mut cx).is_ready();
            self.turns.now.set(self.turns.now.get() + 1);
            if ready {
                self.ended = true;
                return;
            }
            let after = (self.wire.borrow().moved, self.log.borrow().len());
            if after == before {
                still += 1;
                if still == 2 {
                    return;
                }
            } else {
                still = 0;
            }
        }
    }
}

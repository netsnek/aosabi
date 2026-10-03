// SPDX-License-Identifier: MIT
/*
 * hyper::proto::h1::node::conn: the connection machine of Node 24.13.0.
 *
 * Translation of the connection of Node 24.13.0, lib/_http_server.js:899-993,
 * :1014-1066 and :1084-1215, lib/_http_outgoing.js:320-396 and :1077-1082 and
 * src/node_http_parser.cc:768-838, and of the listener of @hono/node-server
 * 1.19.9, dist/listener.js:303-311 and :509-548. Each item names the quirk of
 * docs/byte-equality.md section 13.1 it implements.
 */
//! The Node connection, its requests in flight, its queues and the events of a read in the order of llhttp.
//!
//! A read is parsed whole before the services move, as llhttp executes a
//! read before Node runs the application's promises (quirk Q35), with the two
//! points where Node runs code inside the read: the request listener inside
//! the callback of each head, whose service is polled once there, and the
//! ticks after the callback of each piece of a body, where the services and
//! the bodies move until neither does. Each head is handed to the service as
//! soon as it is complete, each answer is written into a queue of its own
//! until the answers before it have ended, and the answer of the oldest
//! request goes to the socket as it comes.

use alloc::boxed::Box;
use alloc::collections::VecDeque;
use alloc::vec::Vec;
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};
use core::time::Duration;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use http::{Request, StatusCode};

use super::answers;
use super::head;
use super::parse::{refusal_of, Framing, Head, NodeParser, Parsed, Part, Refusal};
use super::NodeConfig;
use crate::body::{Body, DecodedLength, Incoming, Sender};
use crate::common::io;
use crate::common::time::Time;
use crate::error::StdError;
use crate::ext::{FirstWrite, NodeAnswer, NodeExpect, NodeHead, NodeVersion, RawTarget};
use crate::proto::h1::io::MemRead;
use crate::proto::h1::{Decoder, Encoder};
use crate::rt::bare::{Read, Write};
use crate::rt::{ReadBuf, Sleep};
use crate::server::conn::node1::ConnectionEnd;
use crate::service::HttpService;

/// The line Node writes for an `Expect` of `100-continue`.
const CONTINUE: &[u8] = b"HTTP/1.1 100 Continue\r\n\r\n";

/// The bytes of its read buffer an idle connection keeps, 8 KiB.
const IDLE_BUFFER: usize = 8 * 1024;

/// The bytes of one read that the parser and the body decoder have not taken.
struct Input<'a>(&'a mut BytesMut);

impl MemRead for Input<'_> {
    fn read_mem(&mut self, _: &mut Context<'_>, len: usize) -> Poll<io::Result<Bytes>> {
        if self.0.is_empty() {
            return Poll::Pending;
        }
        let n = len.min(self.0.len());
        Poll::Ready(Ok(self.0.split_to(n).freeze()))
    }
}

/// What the connection reads next.
enum Reading {
    /// A request head.
    Head,
    /// The body of the request announced last, its decoded bytes so far and
    /// the bytes of its kept head.
    Body(Decoder, u64, u64),
}

/// The body of an answer being written, and its framing.
struct Answer<B> {
    body: Pin<Box<B>>,
    encoder: Encoder,
}

/// A request in flight and its answer.
struct Flight<F, B> {
    /// The number of the request on its connection.
    id: u64,
    /// The method of the request.
    method: http::Method,
    /// What Node's own checks read of the request.
    node: NodeHead,
    /// The service future of the request, until its answer is ready.
    future: Option<Pin<Box<F>>>,
    /// Node's own 400 or 417 in place of the service's answer.
    reply: Option<StatusCode>,
    /// The body of the answer being written.
    answer: Option<Answer<B>>,
    /// The bytes of the answer while it is not the oldest.
    queue: BytesMut,
    /// Whether a `100 Continue` waits first in `queue`.
    continued: bool,
    /// Whether the head of the answer is written or queued.
    head_written: bool,
    /// Whether the head of the answer has gone to the socket.
    started: bool,
    /// Whether every byte of the answer is written or queued.
    finished: bool,
    /// Whether the answer is a reply written inside the read in progress.
    held: bool,
    /// How the connection ends after the answer, when it does.
    close: Option<ConnectionEnd>,
    /// How the connection ends when the answer becomes the oldest.
    failed: Option<ConnectionEnd>,
    /// Whether an unread request body closes the connection after the answer.
    cleanup: bool,
    /// Whether `100 Continue` waits for the first read of the body.
    lazy_continue: bool,
    /// The channel of the request body.
    tx: Option<Sender>,
    /// The bytes of the request body the service has not read yet.
    pending: VecDeque<Bytes>,
    /// The length of `pending`.
    pending_len: usize,
    /// Whether the request body has ended.
    body_ended: bool,
}

/// The connection machine of one Node connection.
pub(crate) struct NodeConn<I, S>
where
    S: HttpService<Incoming>,
{
    io: I,
    service: Box<S>,
    config: NodeConfig,
    time: Time,
    parser: NodeParser,
    reading: Reading,
    input: BytesMut,
    buf: Vec<u8>,
    flight: VecDeque<Flight<S::Future, S::ResBody>>,
    next_id: u64,
    body_id: u64,
    out: BytesMut,
    end: Option<ConnectionEnd>,
    error: Option<crate::Error>,
    eof: bool,
    shut: bool,
    paused: bool,
    cleanup: Option<(u64, Duration)>,
    #[cfg(feature = "node-timers")]
    timers: super::timers::Timers,
    #[cfg(not(feature = "node-timers"))]
    head_since: Option<Duration>,
    sleep: Option<(Duration, Pin<Box<dyn Sleep>>)>,
}

impl<I, S, B> NodeConn<I, S>
where
    I: Read + Write + Unpin,
    S: HttpService<Incoming, ResBody = B>,
    B: Body + 'static,
    B::Error: Into<Box<dyn StdError + Send + Sync>>,
{
    /// Returns the machine of a connection over `io` accepted now.
    pub(crate) fn new(io: I, service: S, config: NodeConfig, time: Time) -> Self {
        let now = time.now();
        NodeConn {
            io,
            service: Box::new(service),
            config,
            parser: NodeParser::new(config.limits),
            reading: Reading::Head,
            input: BytesMut::new(),
            buf: Vec::new(),
            flight: VecDeque::new(),
            next_id: 0,
            body_id: 0,
            out: BytesMut::new(),
            end: None,
            error: None,
            eof: false,
            shut: false,
            paused: false,
            cleanup: None,
            #[cfg(feature = "node-timers")]
            timers: super::timers::Timers::new(now),
            #[cfg(not(feature = "node-timers"))]
            head_since: Some(now),
            sleep: None,
            time,
        }
    }

    /// Serves the connection until it ends.
    pub(crate) fn poll_connection(
        &mut self,
        cx: &mut Context<'_>,
    ) -> Poll<crate::Result<ConnectionEnd>> {
        loop {
            if let Some(end) = self.end {
                return self.poll_end(end, cx);
            }
            let mut progress = self.poll_write(cx);
            progress |= self.poll_answers(cx);
            progress |= self.poll_bodies(cx);
            progress |= self.poll_timers(cx);
            if self.end.is_none() {
                progress |= self.poll_read(cx);
            }
            if !progress {
                return Poll::Pending;
            }
        }
    }

    /// Writes the bytes handed to the socket and returns whether any went.
    fn poll_write(&mut self, cx: &mut Context<'_>) -> bool {
        let mut progress = false;
        while !self.out.is_empty() {
            match Pin::new(&mut self.io).poll_write(cx, &self.out) {
                Poll::Ready(Ok(0)) => {
                    let zero = io::Error::from(io::ErrorKind::WriteZero);
                    self.fail(crate::Error::new_io(zero));
                    return true;
                }
                Poll::Ready(Ok(n)) => {
                    self.out.advance(n);
                    progress = true;
                }
                Poll::Ready(Err(e)) => {
                    self.fail(crate::Error::new_io(e.into()));
                    return true;
                }
                Poll::Pending => break,
            }
        }
        if self.paused && self.unwritten() < self.config.write_high_water {
            self.paused = false;
            progress = true;
        }
        progress
    }

    /// Returns the answer bytes not yet handed to the network.
    fn unwritten(&self) -> usize {
        self.out.len() + self.flight.iter().map(|f| f.queue.len()).sum::<usize>()
    }

    /// Reads once, while the connection reads, and parses the read whole.
    ///
    /// The reads pause while the unread request bodies hold `body_buffer`
    /// bytes, as a Node request stream pauses, and after a new head while the
    /// answers wait to be written (quirk Q35).
    fn poll_read(&mut self, cx: &mut Context<'_>) -> bool {
        let pending: usize = self.flight.iter().map(|f| f.pending_len).sum();
        if self.eof || self.paused || pending >= self.config.limits.body_buffer {
            return false;
        }
        // The read goes into the spare capacity of the read buffer, so a
        // buffer taken again after an idle release is never zeroed.
        let size = self.config.read_size;
        self.buf.clear();
        self.buf.reserve(size);
        let mut buf = ReadBuf::uninit(&mut self.buf.spare_capacity_mut()[..size]);
        let n = match Pin::new(&mut self.io).poll_read(cx, buf.unfilled()) {
            Poll::Pending => {
                self.release();
                return false;
            }
            Poll::Ready(Err(e)) => {
                self.fail(crate::Error::new_io(e.into()));
                return true;
            }
            Poll::Ready(Ok(())) => {
                self.input.extend_from_slice(buf.filled());
                buf.filled().len()
            }
        };
        if n == 0 {
            self.on_eof(cx);
            return true;
        }
        #[cfg(feature = "node-timers")]
        self.timers.read(self.time.now(), &self.config);
        self.execute(cx);
        // A reply of Node written inside the read ends after it, as Node's
        // `finish` of that reply follows the execute of the parser (quirk Q37).
        for flight in &mut self.flight {
            flight.held = false;
        }
        true
    }

    /// Releases the buffers of a connection that waits for a request with
    /// none in flight, the read buffer past `IDLE_BUFFER` bytes, the input
    /// and the answer bytes written, so an idle kept connection holds little.
    /// The next read or answer takes them again.
    fn release(&mut self) {
        if !self.flight.is_empty()
            || !self.input.is_empty()
            || !matches!(self.reading, Reading::Head)
        {
            return;
        }
        if self.buf.capacity() > IDLE_BUFFER {
            self.buf = Vec::new();
        }
        self.input = BytesMut::new();
        if self.out.is_empty() {
            self.out = BytesMut::new();
        }
    }

    /// Parses every byte of the read, as llhttp executes a read, before any
    /// service future is polled (quirk Q35).
    fn execute(&mut self, cx: &mut Context<'_>) {
        while !self.input.is_empty() && self.end.is_none() {
            match &mut self.reading {
                Reading::Head => {
                    let begun = self.parser.begun();
                    let parsed = self.parser.parse(&self.input);
                    if !begun && self.parser.begun() {
                        self.begun();
                    }
                    match parsed {
                        Err(refusal) => return self.refuse(refusal),
                        Ok(Parsed::Partial) => self.input.clear(),
                        Ok(Parsed::Head(n, head)) => {
                            self.input.advance(n);
                            self.on_head(head, cx);
                        }
                    }
                }
                Reading::Body(decoder, decoded, head) => {
                    let frame = match decoder.decode(cx, &mut Input(&mut self.input)) {
                        Poll::Pending => return,
                        Poll::Ready(frame) => frame,
                    };
                    let eof = decoder.is_eof();
                    let data = match frame {
                        Ok(frame) => frame.into_data().unwrap_or_default(),
                        // A body llhttp refuses ends the connection, with
                        // Node's fixed answer while the oldest answer has not
                        // written its head (quirk Q42).
                        Err(e) => return self.refuse_body(refusal_of(&e)),
                    };
                    *decoded += data.len() as u64;
                    let (decoded, head) = (*decoded, *head);
                    if !data.is_empty() {
                        if let Err(refusal) = self.bound(decoded, head) {
                            return self.refuse_body(refusal);
                        }
                        let id = self.body_id;
                        if let Some(flight) = self
                            .flight
                            .back_mut()
                            .filter(|f| f.id == id && f.tx.is_some())
                        {
                            flight.pending_len += data.len();
                            flight.pending.push_back(data);
                        }
                        // Node calls the callback of a piece of a body with
                        // its ticks after it, so the services and the bodies
                        // move before the parser reads on (quirk Q35).
                        self.tick(cx);
                    }
                    if eof {
                        self.message_complete();
                    }
                }
            }
        }
    }

    /// Refuses with 413 a chunked body past `max_body`, or with its kept
    /// head past `max_request`, the body ceiling of a server card and the
    /// bounds of a device card.
    fn bound(&self, decoded: u64, head: u64) -> Result<(), Refusal> {
        let limits = &self.config.limits;
        if limits.max_body.is_some_and(|max| decoded > max)
            || limits
                .max_request
                .is_some_and(|max| head.saturating_add(decoded) > max)
        {
            return Err(Refusal::TooLarge);
        }
        Ok(())
    }

    /// Moves the services and the bodies until neither moves, the ticks Node
    /// runs after the callback of a piece of a body, in which the `finish` of
    /// an answer written inside the read is emitted, so the hold on it ends
    /// (quirk Q35).
    fn tick(&mut self, cx: &mut Context<'_>) {
        for flight in &mut self.flight {
            flight.held = false;
        }
        while self.end.is_none() && (self.poll_answers(cx) | self.poll_bodies(cx)) {}
    }

    /// Polls the service of the request handed last once, as Node runs the
    /// request listener inside the callback of the head, so an answer the
    /// listener writes before its first wait goes out inside the read and is
    /// held to its end, as a reply of Node's own checks is (quirk Q35).
    fn listen(&mut self, cx: &mut Context<'_>) {
        let Some(i) = self.flight.len().checked_sub(1) else {
            return;
        };
        if self.flight[i].future.is_none() {
            return;
        }
        self.poll_flight(i, cx);
        let flight = &mut self.flight[i];
        if flight.finished {
            flight.held = true;
        }
    }

    /// Starts the timers of a message at its first byte llhttp does not skip.
    fn begun(&mut self) {
        #[cfg(feature = "node-timers")]
        self.timers.begun(self.time.now());
    }

    /// Hands a complete head to the service or answers it as Node does.
    fn on_head(&mut self, head: Head, cx: &mut Context<'_>) {
        #[cfg(feature = "node-timers")]
        self.timers.head_complete();
        #[cfg(not(feature = "node-timers"))]
        {
            self.head_since = None;
        }
        // A `CONNECT` ends the connection with no answer before any check of
        // Node, and the answers not yet written are lost (quirk Q39).
        if head.node.connect {
            return self.finish(ConnectionEnd::Closed);
        }
        // A device card refuses a head past the requests it holds in flight
        // with 413.
        if self
            .config
            .max_in_flight
            .is_some_and(|max| self.flight.len() >= max)
        {
            return self.refuse(Refusal::TooLarge);
        }
        // Node pauses its reads at a new head while its answers wait to be
        // written (quirk Q35).
        if self.unwritten() >= self.config.write_high_water {
            self.paused = true;
        }
        let oldest = self.flight.is_empty();
        let framing = head.framing;
        let kept = head.kept_len();
        let id = self.next_id;
        self.next_id += 1;
        let mut flight = Flight {
            id,
            method: head.method.clone(),
            node: head.node.clone(),
            future: None,
            reply: None,
            answer: None,
            queue: BytesMut::new(),
            continued: false,
            head_written: false,
            started: false,
            finished: false,
            held: false,
            close: None,
            failed: None,
            cleanup: false,
            lazy_continue: false,
            tx: None,
            pending: VecDeque::new(),
            pending_len: 0,
            body_ended: framing == Framing::None,
        };
        let http11 = head.node.version == NodeVersion::Http11;
        // Node checks an HTTP/1.1 head alone, a missing `Host` first and then
        // its `Expect`, and answers either inside the read (quirk Q37).
        let status = match head.node.expect {
            _ if !http11 => None,
            _ if head.node.host.is_none() => Some(StatusCode::BAD_REQUEST),
            NodeExpect::Other => Some(StatusCode::EXPECTATION_FAILED),
            _ => None,
        };
        if let Some(status) = status {
            flight.reply = Some(status);
        } else {
            if http11 && head.node.expect == NodeExpect::Continue {
                self.expect_continue(&mut flight, oldest);
            }
            self.call(&mut flight, head);
        }
        self.body_id = id;
        self.reading = match framing {
            Framing::None | Framing::Refused => Reading::Head,
            Framing::Length(len) => Reading::Body(Decoder::length(len), 0, kept),
            Framing::Chunked => {
                let max_head = Some(self.config.limits.max_head);
                Reading::Body(Decoder::chunked(None, max_head), 0, kept)
            }
        };
        if flight.reply.is_some() {
            self.write_reply(&mut flight, oldest);
        }
        self.flight.push_back(flight);
        if self.end.is_none() {
            self.listen(cx);
        }
        // A transfer coding without chunked last is refused after the
        // callback of the head, as llhttp's `after_headers_complete` refuses
        // it, so Node's checks and the listener have run (quirk Q24).
        if framing == Framing::Refused {
            return self.refuse_body(Refusal::Invalid(Part::Encoding));
        }
        if framing == Framing::None {
            self.message_complete();
        }
    }

    /// Queues `100 Continue` as Node's `writeContinue` does, at once for the
    /// oldest request and first in its own queue otherwise (quirk Q38).
    #[cfg(feature = "node-continue")]
    fn expect_continue(&mut self, flight: &mut Flight<S::Future, B>, oldest: bool) {
        if oldest {
            self.out.extend_from_slice(CONTINUE);
        } else {
            flight.queue.extend_from_slice(CONTINUE);
            flight.continued = true;
        }
    }

    /// Leaves `100 Continue` to the first read of the body, as hyper's own
    /// connection writes it.
    #[cfg(not(feature = "node-continue"))]
    fn expect_continue(&mut self, flight: &mut Flight<S::Future, B>, _: bool) {
        flight.lazy_continue = true;
    }

    /// Calls the service with the request of `head`.
    fn call(&mut self, flight: &mut Flight<S::Future, B>, head: Head) {
        let body = match head.framing {
            Framing::None => Incoming::empty(),
            Framing::Length(len) => {
                let length = DecodedLength::checked_new(len).unwrap_or(DecodedLength::CHUNKED);
                let (tx, body) = Incoming::new_channel(length, flight.lazy_continue);
                flight.tx = Some(tx);
                body
            }
            Framing::Chunked | Framing::Refused => {
                let (tx, body) =
                    Incoming::new_channel(DecodedLength::CHUNKED, flight.lazy_continue);
                flight.tx = Some(tx);
                body
            }
        };
        let mut request = Request::new(body);
        *request.method_mut() = head.method.clone();
        *request.uri_mut() = head.uri();
        *request.version_mut() = match head.node.version {
            NodeVersion::Http09 => http::Version::HTTP_09,
            NodeVersion::Http10 => http::Version::HTTP_10,
            NodeVersion::Http11 => http::Version::HTTP_11,
            NodeVersion::Http20 => http::Version::HTTP_2,
        };
        if head.headers(request.headers_mut(), |_, _| ()).is_err() {
            return self.refuse(Refusal::Invalid(super::parse::Part::Field));
        }
        request
            .extensions_mut()
            .insert(RawTarget(head.raw_target()));
        request.extensions_mut().insert(head.node);
        flight.future = Some(Box::pin(self.service.call(request)));
    }

    /// Writes the 400 or the 417 of Node's own checks, inside the read when
    /// its request is the oldest, as Node's `writeHead` and `end` reach the
    /// socket while llhttp executes the read (quirk Q37).
    fn write_reply(&mut self, flight: &mut Flight<S::Future, B>, oldest: bool) {
        let Some(status) = flight.reply else {
            return;
        };
        let mut response = http::Response::new(());
        *response.status_mut() = status;
        if status == StatusCode::BAD_REQUEST {
            let close = http::HeaderValue::from_static("close");
            response
                .headers_mut()
                .insert(http::header::CONNECTION, close);
            #[cfg(feature = "node-record")]
            {
                let mut record = crate::ext::HeaderCaseMap::new();
                record.append(http::header::CONNECTION, Bytes::from_static(b"Connection"));
                response.extensions_mut().insert(record);
            }
        }
        let (parts, ()) = response.into_parts();
        let mut dst = Vec::new();
        let date = self.date();
        let config = &self.config;
        match head::encode(
            parts,
            &flight.method,
            &flight.node,
            config,
            date.as_deref(),
            &mut dst,
        ) {
            Ok(encoder) => {
                if let Ok(Some(end)) = encoder.end::<Bytes>() {
                    extend(&mut dst, end);
                }
                if encoder.is_last() || encoder.is_close_delimited() {
                    flight.close = Some(ConnectionEnd::Closed);
                }
            }
            Err(e) => {
                self.error = Some(e);
                flight.failed = Some(ConnectionEnd::Closed);
            }
        }
        flight.head_written = true;
        flight.finished = true;
        if oldest {
            self.out.extend_from_slice(&dst);
            flight.started = true;
            flight.held = true;
        } else {
            flight.queue.extend_from_slice(&dst);
        }
    }

    /// Ends the message of the request announced last.
    fn message_complete(&mut self) {
        #[cfg(feature = "node-timers")]
        self.timers.message_complete();
        #[cfg(not(feature = "node-timers"))]
        {
            self.head_since = Some(self.time.now());
        }
        let id = self.body_id;
        let (keep_alive, upgrade) = match self.flight.back_mut().filter(|f| f.id == id) {
            Some(flight) => {
                flight.body_ended = true;
                (flight.node.keep_alive, flight.node.upgrade)
            }
            None => (true, false),
        };
        if self.cleanup.is_some_and(|(cleanup, _)| cleanup == id) {
            self.cleanup = None;
        }
        self.reading = Reading::Head;
        // The parser refuses a request after one that is not kept alive,
        // which the connection answers as any refusal (quirk Q41).
        self.parser.next_message(keep_alive);
        // After a request marked as an upgrade the rest of its read is dropped
        // unparsed and the next read is parsed from its start (quirk Q40).
        if upgrade {
            self.input.clear();
        }
    }

    /// Ends the connection for a refusal of llhttp or of a bound, with Node's
    /// fixed answer while the oldest answer in flight has not written its
    /// head and with no byte otherwise (quirk Q36).
    fn refuse(&mut self, refusal: Refusal) {
        if answers::written(self.flight.front().map(|f| f.started)) {
            self.out.extend_from_slice(answers::fixed(refusal));
        }
        self.finish(ConnectionEnd::Refused);
    }

    /// Ends the connection for a body llhttp refuses, whose request body ends
    /// with an error (quirk Q42).
    fn refuse_body(&mut self, refusal: Refusal) {
        let id = self.body_id;
        if let Some(tx) = self
            .flight
            .back_mut()
            .filter(|f| f.id == id)
            .and_then(|f| f.tx.as_mut())
        {
            tx.send_error(crate::Error::new_body("the body was refused"));
        }
        self.refuse(refusal);
    }

    /// Handles the end of the client's stream.
    fn on_eof(&mut self, cx: &mut Context<'_>) {
        self.eof = true;
        // A message in progress at the end is refused as llhttp's finish
        // refuses it (quirk Q42).
        if matches!(self.reading, Reading::Body(..)) || self.parser.begun() {
            return self.refuse(Refusal::Invalid(Part::Field));
        }
        // Node ends its side at the client's FIN, so an answer its handler
        // wrote before the tick of the FIN goes out and one written after it
        // is lost (quirk Q48). Under `node-half-open` the services and the
        // bodies run until neither moves, as Node's microtasks run before it
        // reads the FIN, and the answers ready then go out before the
        // connection ends. Without it the end drops every answer byte not yet
        // written, as hyper's own connection ends it without `half_close`.
        if cfg!(feature = "node-half-open") {
            while self.end.is_none() && (self.poll_answers(cx) | self.poll_bodies(cx)) {}
        } else {
            self.out.clear();
        }
        self.finish(ConnectionEnd::Closed);
    }

    /// Ends the connection and drops every answer not yet written.
    ///
    /// A request body that has ended keeps its bytes and its channel, so its
    /// handler reads it whole while it runs to its answer, as a Node request
    /// stream that ended holds its bytes when the socket goes (quirk Q45). A
    /// body that has not ended ends with an error.
    fn finish(&mut self, end: ConnectionEnd) {
        if self.end.is_none() {
            self.end = Some(end);
        }
        self.input.clear();
        for flight in &mut self.flight {
            flight.queue.clear();
            flight.answer = None;
            flight.lazy_continue = false;
            if flight.body_ended {
                continue;
            }
            flight.pending.clear();
            flight.pending_len = 0;
            if let Some(mut tx) = flight.tx.take() {
                tx.send_error(crate::Error::new_incomplete());
            }
        }
    }

    /// Ends the connection for an error of the IO.
    fn fail(&mut self, error: crate::Error) {
        if self.error.is_none() {
            self.error = Some(error);
        }
        self.out.clear();
        self.finish(ConnectionEnd::Closed);
    }

    /// Writes what is left, shuts the stream and runs every service called
    /// to its answer, which is dropped, handing each the bytes of its body
    /// that ended (quirk Q45).
    fn poll_end(
        &mut self,
        end: ConnectionEnd,
        cx: &mut Context<'_>,
    ) -> Poll<crate::Result<ConnectionEnd>> {
        let mut pending = false;
        while !self.out.is_empty() && self.error.is_none() {
            match Pin::new(&mut self.io).poll_write(cx, &self.out) {
                Poll::Ready(Ok(n)) if n > 0 => self.out.advance(n),
                Poll::Ready(_) => self.out.clear(),
                Poll::Pending => {
                    pending = true;
                    break;
                }
            }
        }
        if !pending && !self.shut {
            match Pin::new(&mut self.io).poll_shutdown(cx) {
                Poll::Pending => pending = true,
                Poll::Ready(_) => self.shut = true,
            }
        }
        loop {
            let mut progress = self.poll_bodies(cx);
            for flight in &mut self.flight {
                if let Some(future) = flight.future.as_mut() {
                    if future.as_mut().poll(cx).is_ready() {
                        flight.future = None;
                        progress = true;
                    }
                }
            }
            if !progress {
                break;
            }
        }
        if pending || self.flight.iter().any(|f| f.future.is_some()) {
            return Poll::Pending;
        }
        self.flight.clear();
        match self.error.take() {
            Some(error) => Poll::Ready(Err(error)),
            None => Poll::Ready(Ok(end)),
        }
    }

    /// Polls the services and the bodies of the answers in order, and hands
    /// the oldest answer to the socket.
    fn poll_answers(&mut self, cx: &mut Context<'_>) -> bool {
        let mut progress = false;
        for i in 0..self.flight.len() {
            progress |= self.poll_flight(i, cx);
            if self.end.is_some() {
                return true;
            }
        }
        while self
            .flight
            .front()
            .is_some_and(|f| (f.finished && !f.held) || f.failed.is_some())
        {
            progress = true;
            let Some(done) = self.flight.pop_front() else {
                break;
            };
            if let Some(end) = done.failed.or(done.close) {
                self.out.extend_from_slice(&done.queue);
                self.finish(end);
                return true;
            }
            // A body the service did not read closes the connection when it
            // has not ended shortly after its answer (quirk Q47).
            if done.cleanup && !done.body_ended {
                let after = Duration::from_millis(u64::from(self.config.cleanup_after_ms));
                self.cleanup = Some((done.id, self.time.now() + after));
            }
            if let Some(next) = self.flight.front_mut() {
                self.out.extend_from_slice(&next.queue);
                next.queue.clear();
                next.started = next.head_written;
            } else {
                #[cfg(feature = "node-timers")]
                self.timers.idle(self.time.now(), &self.config);
            }
        }
        progress
    }

    /// Polls the service and the body of the answer of `flight[i]`.
    fn poll_flight(&mut self, i: usize, cx: &mut Context<'_>) -> bool {
        let mut progress = false;
        let oldest = i == 0;
        let date = self.date();
        let config = self.config;
        let flight = &mut self.flight[i];
        if flight.finished || flight.failed.is_some() {
            return false;
        }
        if flight.answer.is_none() {
            let Some(future) = flight.future.as_mut() else {
                return false;
            };
            let response = match future.as_mut().poll(cx) {
                Poll::Pending => return false,
                Poll::Ready(response) => response,
            };
            flight.future = None;
            progress = true;
            let response = match response {
                Ok(response) => response,
                Err(e) => {
                    self.error = Some(crate::Error::new_user_service(e));
                    flight.failed = Some(ConnectionEnd::StreamError);
                    return true;
                }
            };
            let (parts, body) = response.into_parts();
            let answer = parts.extensions.get::<NodeAnswer>().copied();
            let first_write = answer.map_or(FirstWrite::End, |a| a.first_write);
            flight.cleanup = answer.is_some_and(|a| a.cleanup_incoming);
            if answer.is_some_and(|a| a.destroy) {
                flight.close = Some(ConnectionEnd::Destroyed);
            }
            // A device card refuses a whole body past `max_response` with 413
            // before its head.
            let exact = body.size_hint().exact();
            if let (Some(max), Some(len)) = (config.max_response, exact) {
                if len > max as u64 {
                    let refused = answers::fixed(Refusal::TooLarge);
                    if oldest {
                        self.out.extend_from_slice(refused);
                    } else {
                        flight.queue.extend_from_slice(refused);
                    }
                    flight.failed = Some(ConnectionEnd::Refused);
                    return true;
                }
            }
            let mut dst = Vec::new();
            let (method, node) = (&flight.method, &flight.node);
            let encoder =
                match head::encode(parts, method, node, &config, date.as_deref(), &mut dst) {
                    Ok(encoder) => encoder,
                    Err(e) => {
                        self.error = Some(e);
                        flight.failed = Some(ConnectionEnd::Closed);
                        return true;
                    }
                };
            if encoder.is_last() || encoder.is_close_delimited() {
                flight.close.get_or_insert(ConnectionEnd::Closed);
            }
            // The head goes in front of a queued `100 Continue` on a first
            // write of bytes of a body with a length, as Node's `_send`
            // unshifts it (quirk Q38).
            let bytes_first =
                first_write == FirstWrite::Bytes && !encoder.is_eof() && !encoder.is_chunked();
            if oldest {
                self.out.extend_from_slice(&dst);
                flight.started = true;
            } else if flight.continued && bytes_first {
                let mut queue = BytesMut::from(&dst[..]);
                queue.extend_from_slice(&flight.queue);
                flight.queue = queue;
            } else {
                flight.queue.extend_from_slice(&dst);
            }
            flight.head_written = true;
            flight.answer = Some(Answer {
                body: Box::pin(body),
                encoder,
            });
        }
        let Some(answer) = flight.answer.as_mut() else {
            return progress;
        };
        loop {
            // The body of an answer that waits is polled while its queue holds
            // fewer than `answer_high_water` bytes (quirk Q35).
            if !oldest && flight.queue.len() >= config.answer_high_water {
                break;
            }
            match answer.body.as_mut().poll_frame(cx) {
                Poll::Pending => break,
                Poll::Ready(Some(Ok(frame))) => {
                    progress = true;
                    let Ok(mut data) = frame.into_data() else {
                        continue;
                    };
                    let len = data.remaining();
                    if len == 0 {
                        continue;
                    }
                    // A device card ends the connection at a chunk past
                    // `max_response`, as at a failed stream.
                    if config.max_response.is_some_and(|max| len > max) {
                        flight.failed = Some(ConnectionEnd::StreamError);
                        flight.queue.clear();
                        break;
                    }
                    let bytes = data.copy_to_bytes(len);
                    let dst = if oldest {
                        &mut self.out
                    } else {
                        &mut flight.queue
                    };
                    extend(dst, answer.encoder.encode(bytes));
                }
                Poll::Ready(Some(Err(e))) => {
                    progress = true;
                    self.error = Some(crate::Error::new_user_body(e));
                    flight.failed = Some(ConnectionEnd::StreamError);
                    // A stream that fails ends the connection with no further
                    // byte, its queue dropped (quirk Q46).
                    flight.queue.clear();
                    break;
                }
                Poll::Ready(None) => {
                    progress = true;
                    if let Ok(Some(end)) = answer.encoder.end::<Bytes>() {
                        let dst = if oldest {
                            &mut self.out
                        } else {
                            &mut flight.queue
                        };
                        extend(dst, end);
                    }
                    flight.finished = true;
                    flight.answer = None;
                    break;
                }
            }
        }
        progress
    }

    /// Hands the bytes of each request body to its channel as the service
    /// reads them.
    fn poll_bodies(&mut self, cx: &mut Context<'_>) -> bool {
        let mut progress = false;
        let mut continues = Vec::new();
        for (i, flight) in self.flight.iter_mut().enumerate() {
            let Some(tx) = flight.tx.as_mut() else {
                continue;
            };
            if flight.lazy_continue && tx.poll_ready(cx).is_ready() {
                flight.lazy_continue = false;
                continues.push(i);
                progress = true;
            }
            while let Some(chunk) = flight.pending.front() {
                match tx.poll_ready(cx) {
                    Poll::Ready(Ok(())) => {
                        let len = chunk.len();
                        if let Some(chunk) = flight.pending.pop_front() {
                            let _ = tx.try_send_data(chunk);
                        }
                        flight.pending_len -= len;
                        progress = true;
                    }
                    Poll::Ready(Err(_)) => {
                        // The service dropped the body, whose bytes are read to
                        // their end and dropped (quirk Q47).
                        flight.pending.clear();
                        flight.pending_len = 0;
                        flight.tx = None;
                        progress = true;
                        break;
                    }
                    Poll::Pending => break,
                }
            }
            if flight.pending.is_empty() && flight.body_ended && flight.tx.is_some() {
                flight.tx = None;
                progress = true;
            }
        }
        for i in continues {
            if i == 0 {
                self.out.extend_from_slice(CONTINUE);
            } else {
                self.flight[i].queue.extend_from_slice(CONTINUE);
            }
        }
        progress
    }

    /// Answers the deadlines of the timers, the cleanup of an unread body
    /// among them (quirk Q47).
    fn poll_timers(&mut self, cx: &mut Context<'_>) -> bool {
        if matches!(self.time, Time::Empty(_)) {
            return false;
        }
        let now = self.time.now();
        if self.cleanup.is_some_and(|(_, at)| now >= at) {
            self.finish(ConnectionEnd::Closed);
            return true;
        }
        if self.expired(now) {
            return true;
        }
        let deadline = match (self.deadline(), self.cleanup.map(|(_, at)| at)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        };
        let Some(deadline) = deadline else {
            self.sleep = None;
            return false;
        };
        let sleep = match &mut self.sleep {
            Some((at, sleep)) if *at == deadline => sleep,
            slot => &mut slot.insert((deadline, self.time.sleep_until(deadline))).1,
        };
        if sleep.as_mut().poll(cx).is_ready() {
            self.sleep = None;
            return true;
        }
        false
    }

    /// Returns the next deadline of Node's timers (quirk Q43).
    #[cfg(feature = "node-timers")]
    fn deadline(&self) -> Option<Duration> {
        self.timers.deadline(&self.config)
    }

    /// Answers the timers of Node at `now`, a 408 or the idle close (quirk
    /// Q43).
    #[cfg(feature = "node-timers")]
    fn expired(&mut self, now: Duration) -> bool {
        match self.timers.expired(now, &self.config) {
            Some(super::timers::Expired::Request) => {
                if answers::written(self.flight.front().map(|f| f.started)) {
                    self.out.extend_from_slice(answers::REQUEST_TIMEOUT);
                }
                self.finish(ConnectionEnd::TimedOut);
                true
            }
            Some(super::timers::Expired::Idle) => {
                self.finish(ConnectionEnd::Closed);
                true
            }
            None => false,
        }
    }

    /// Returns the deadline of hyper's own header read timeout of 30 seconds.
    #[cfg(not(feature = "node-timers"))]
    fn deadline(&self) -> Option<Duration> {
        self.head_since.map(|since| since + Duration::from_secs(30))
    }

    /// Closes a connection whose head passed hyper's own header read timeout,
    /// with no answer.
    #[cfg(not(feature = "node-timers"))]
    fn expired(&mut self, now: Duration) -> bool {
        if self.deadline().is_some_and(|deadline| now >= deadline) {
            self.finish(ConnectionEnd::TimedOut);
            return true;
        }
        false
    }

    /// Returns the date of an answer, `None` without a wall clock.
    fn date(&self) -> Option<Vec<u8>> {
        #[cfg(feature = "std")]
        {
            crate::common::date::update();
            let mut date = Vec::with_capacity(crate::common::date::DATE_VALUE_LENGTH);
            crate::common::date::extend(&mut date);
            Some(date)
        }
        #[cfg(not(feature = "std"))]
        {
            let seconds = self.time.unix_seconds();
            seconds.map(|secs| crate::common::date::CachedDate::at(secs).buffer().to_vec())
        }
    }
}

/// Appends the bytes of `buf` to `dst`, each chunk as one slice.
fn extend(dst: &mut impl BufMut, mut buf: impl Buf) {
    while buf.has_remaining() {
        let chunk = buf.chunk();
        let n = chunk.len();
        dst.put_slice(chunk);
        buf.advance(n);
    }
}

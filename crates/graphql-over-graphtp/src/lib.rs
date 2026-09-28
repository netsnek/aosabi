// SPDX-License-Identifier: MIT
/*
 * graphql-over-graphtp: the GraphTP listener, which serves an app's fetch over
 * Homa with the fixed fields of GraphTP and its streams.
 *
 * No analogue in the reference. The listener is the one of
 * docs/graphtp-in-aosabi.md section 3 over the server of the crate
 * graphtp-homa, a server of docs/graphtp.md section 7, and its two turns are
 * the surface the Homa track proposes to it.
 */

//! The `GraphTP` listener.
//!
//! A [`Listener`] serves one Homa endpoint or one socket of Homa's kernel
//! module through a fetch function of the app, with the bounds of its card. For
//! each OPERATION it takes the five steps of `docs/graphtp.md` section 7: the
//! server of `graphtp-homa` checks and decodes the request into the request
//! buffer, the listener hands the app a POST with the fixed fields of
//! [`fetch`], the app answers, and the listener opens a stream for an event
//! stream or answers the status, the substatus and the body by the rules of
//! step 4, which the server codes at >128 B into the answer buffer.
//!
//! A driver calls [`Listener::turn_endpoint`] after each turn of its endpoint,
//! or [`Listener::turn_rpcs`] after the readiness of a kernel socket, with the
//! context of its task, and calls again when the context wakes or a packet
//! arrives. A turn takes every whole request of the engine, polls the answers
//! and the streams in flight, and hands the engine every answer that is
//! ready. It polls every OPERATION it takes before it returns, so the waker
//! of each answer the app still owes is the context's, or it wakes the
//! context itself. The listener never reads a byte of the link, and the
//! driver never reads a byte of `GraphTP`.
//!
//! A stream is read only while its queue has room, so back pressure reaches its
//! source, and its source is dropped when a CANCEL or a link down ends it.
//!
//! A refusal the app decides, 400.5 of the card's limit hook, 503.1 of the
//! arena floor or 403.1 of a carrier's allowlist, reaches the listener as an
//! answer with its status, its substatus, the answer type and an empty body,
//! and the listener answers it with the two bytes of its code. The app names
//! the answer type on such an answer, since step 4 reads an answer of class 4
//! or 5 under another media type or none as the status alone, 403.0 in place of
//! 403.1.
//!
//! A served build hands its listeners the fetch of its booted app, which the
//! module `graphtp::served` of `aosabi-core` builds with its features `runtime`
//! and `homa`: it runs a call straight through the hooks and the engine of the
//! GraphQL server the boot mounted, with no request through `BootedApp::fetch`
//! for an app whose router holds the core alone, and copies the substatus the
//! HTTP layer set on the `Response` into the [`Answer`], so the substatus on
//! the wire is the one `graphql-over-http` decides.
//!
//! The crate builds with no standard library, needs an allocator and takes the
//! crates `graphtp`, `graphtp-homa` and `homa` and no other, so a card with no
//! standard library links it, and `aosabi-core` names it as its module
//! `graphtp` under its feature `homa`. Its judges are the Jupyter notebooks of
//! the carriers and of `GraphTP`, `tests/notebooks/25_carrier_unix.ipynb` and
//! `tests/notebooks/32_graphtp_fetch.ipynb` among them, and there is no test in
//! this crate, which is the rule of CONTRIBUTING.md section 3.

#![no_std]
#![forbid(unsafe_code)]

extern crate alloc;

mod answer;
mod events;
pub mod fetch;
pub mod limits;

use alloc::{
    boxed::Box,
    vec::Vec, //
};
use core::{
    hash::Hash,
    task::{
        Context,
        Poll, //
    }, //
};

use graphtp::{status::Code, stream::Handle};
use graphtp_homa::{
    handler::Handler,
    rpcs::{
        self,
        Served, //
    },
    server::{
        self,
        Exchange,
        Flow,
        Server, //
    },
};
use homa::{
    Endpoint,
    PeerPort,
    Rpcs,
    Store, //
    wire::Form,
};

use crate::{
    answer::Decision,
    events::{
        Event,
        Reader, //
    },
};
pub use crate::{
    fetch::{
        Answer,
        Body,
        Broken,
        Chunks,
        Fetch,
        Reply,
        Request, //
    },
    limits::CardLimits,
};

/// What a turn did, and whether answers are in flight after it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Progress {
    /// The answers the turn handed the engine.
    pub answered: u32,
    /// The OPERATIONs whose answer the app has not given yet, and the streams
    /// whose source is still read.
    pub pending: u32,
}

/// The `GraphTP` listener of one Homa endpoint or socket: the server of
/// `graphtp-homa`, the app's fetch, the answers in flight and the card's
/// bounds.
pub struct Listener<P> {
    /// The server of `graphtp-homa` with the card's bounds.
    server: Server<P>,
    /// The fetch and what is in flight.
    work: Work<P>,
}

impl<P: Copy + Eq + Hash + PeerPort> Listener<P> {
    /// Returns a listener over the fetch `fetch`, the app's or the harness's,
    /// with the bounds `limits` of its card.
    ///
    /// # Errors
    ///
    /// Returns the error of the server of `graphtp-homa` when the bounds hold
    /// an answer buffer shorter than a batch of the server needs or sizes of
    /// the encoder that make no scratch.
    pub fn new(fetch: Fetch, limits: &'static CardLimits) -> Result<Self, server::Error> {
        Ok(Self {
            server: Server::new(limits.server_config())?,
            work: Work {
                fetch,
                limits,
                calls: Vec::new(),
                streams: Vec::new(),
            },
        })
    }

    /// Returns the bounds of the card.
    #[must_use]
    pub fn limits(&self) -> &'static CardLimits {
        self.work.limits
    }

    /// Takes every event of an endpoint of the crate `homa`, hands each whole
    /// request to the server, polls the answers and the streams in flight with
    /// `cx`, and hands the endpoint every answer that is ready.
    ///
    /// The endpoint may have the role `server` alone, and its peer going down
    /// ends the peer's streams and its parked PULL.
    pub fn turn_endpoint<S, W>(
        &mut self,
        endpoint: &mut Endpoint<P, S, W>,
        cx: &mut Context<'_>,
    ) -> Progress
    where
        S: Store<P>,
        W: Form,
    {
        let mut answered = rpcs::serve_endpoint(endpoint, &mut self.server, &mut self.work).replies;
        if self.work.drive(&mut self.server, cx) {
            answered += rpcs::serve_endpoint(endpoint, &mut self.server, &mut self.work).replies;
        }
        self.progress(answered)
    }

    /// Takes every event of `rpcs`, a socket of Homa's kernel module or an
    /// endpoint with both roles, as [`Listener::turn_endpoint`] does.
    ///
    /// A socket reads the requests that arrive while the turn runs, so the
    /// serve that hands the engine the answers a poll settled may hand the
    /// app new OPERATIONs as well. The turn therefore polls after every
    /// serve and serves again while a poll moves anything, so no call is left
    /// unpolled with no waker behind it. After a bound of passes it wakes its
    /// task, and the next turn goes on.
    ///
    /// # Errors
    ///
    /// Returns the error of `rpcs` when it cannot read its events.
    pub fn turn_rpcs<R: Rpcs<Peer = P>>(
        &mut self,
        rpcs: &mut R,
        cx: &mut Context<'_>,
    ) -> Result<Progress, R::Error> {
        let mut answered = 0;
        for _ in 0..PASSES_PER_TURN {
            let Served { replies, .. } = rpcs::serve(rpcs, &mut self.server, &mut self.work)?;
            answered += replies;
            if !self.work.drive(&mut self.server, cx) {
                return Ok(self.progress(answered));
            }
        }
        cx.waker().wake_by_ref();
        Ok(self.progress(answered))
    }

    /// Returns the progress of a turn that handed the engine `answered`
    /// answers.
    fn progress(&self, answered: usize) -> Progress {
        let pending = self.work.calls.len() + self.work.streams.len();
        Progress {
            answered: u32::try_from(answered).unwrap_or(u32::MAX),
            pending: u32::try_from(pending).unwrap_or(u32::MAX),
        }
    }
}

/// The fetch of a listener and what is in flight.
struct Work<P> {
    /// The app's fetch.
    fetch: Fetch,
    /// The bounds of the card.
    limits: &'static CardLimits,
    /// The OPERATIONs whose answer is not whole yet.
    calls: Vec<Call<P>>,
    /// The streams whose source is still read.
    streams: Vec<Open<P>>,
}

/// One OPERATION whose answer is not whole yet.
struct Call<P> {
    /// The exchange the answer completes.
    exchange: Exchange<P>,
    /// How far the answer is.
    stage: Stage,
}

/// How far the answer of an OPERATION is.
enum Stage {
    /// The app has not answered yet.
    Fetching(Reply),
    /// The app answered the head, and the body that is no event stream is
    /// being read whole.
    Reading {
        /// The status the app wrote.
        status: u16,
        /// The substatus of the answer.
        substatus: u8,
        /// The value of `content-type`.
        content_type: Option<Vec<u8>>,
        /// The source of the body.
        source: Box<dyn Chunks>,
        /// The body read so far.
        body: Vec<u8>,
    },
}

/// A stream whose source is still read.
struct Open<P> {
    /// The peer that holds the stream.
    peer: P,
    /// Its handle.
    handle: Handle,
    /// The source of the event stream, `None` for a body the app wrote whole.
    source: Option<Box<dyn Chunks>>,
    /// The reader of its events.
    reader: Reader,
}

/// What one poll of a call gave.
enum Polled<P> {
    /// The call waits.
    Waiting(Call<P>),
    /// The call has an answer to settle.
    Settled,
}

impl<P: Copy + Eq> Handler<P> for Work<P> {
    fn operation(
        &mut self,
        exchange: Exchange<P>,
        credential: Option<&[u8]>,
        content: &[u8],
        _server: &mut Server<P>,
    ) {
        let request = Request {
            credential: credential.map(<[u8]>::to_vec),
            content: content.to_vec(),
        };
        let reply = (self.fetch)(request);
        self.calls.push(Call {
            exchange,
            stage: Stage::Fetching(reply),
        });
    }
}

impl<P: Copy + Eq> Work<P> {
    /// Polls every call and every stream in flight with `cx` and hands the
    /// server what is ready, and tells whether anything moved.
    fn drive(&mut self, server: &mut Server<P>, cx: &mut Context<'_>) -> bool {
        let mut moved = false;
        for call in core::mem::take(&mut self.calls) {
            match self.poll_call(call, server, cx) {
                Polled::Waiting(call) => self.calls.push(call),
                Polled::Settled => moved = true,
            }
        }
        for open in &mut self.streams {
            moved |= read_stream(open, server, cx);
        }
        while let Some((peer, handle)) = server.poll_ended() {
            self.streams
                .retain(|open| !(open.peer == peer && open.handle == handle));
        }
        let before = self.streams.len();
        self.streams
            .retain(|open| server.flow(open.peer, open.handle) != Flow::Ended);
        moved || before != self.streams.len()
    }

    /// Polls one call with `cx`, and answers the exchange or opens its stream
    /// once the answer is whole.
    ///
    /// A body read whole takes at most [`PIECES_PER_TURN`] pieces a turn, and
    /// a call that stops there wakes its task, so the next turn reads on.
    fn poll_call(
        &mut self,
        mut call: Call<P>,
        server: &mut Server<P>,
        cx: &mut Context<'_>,
    ) -> Polled<P> {
        let mut pieces = 0;
        loop {
            match call.stage {
                Stage::Fetching(mut reply) => {
                    let Poll::Ready(answer) = reply.as_mut().poll(cx) else {
                        call.stage = Stage::Fetching(reply);
                        return Polled::Waiting(call);
                    };
                    let Answer {
                        status,
                        substatus,
                        content_type,
                        body,
                    } = answer;
                    let event_stream = content_type.as_deref().is_some_and(answer::is_event_stream);
                    match body {
                        Body::Stream(source) if event_stream => {
                            self.open(call.exchange, Some(source), &[], server);
                            return Polled::Settled;
                        }
                        Body::Stream(source) => {
                            call.stage = Stage::Reading {
                                status,
                                substatus,
                                content_type,
                                source,
                                body: Vec::new(),
                            };
                        }
                        Body::Full(bytes) => {
                            self.settle(
                                call.exchange,
                                status,
                                substatus,
                                content_type.as_deref(),
                                &bytes,
                                server,
                            );
                            return Polled::Settled;
                        }
                    }
                }
                Stage::Reading {
                    status,
                    substatus,
                    content_type,
                    mut source,
                    mut body,
                } => match poll_piece(&mut *source, &mut pieces, cx) {
                    Poll::Pending => {
                        call.stage = Stage::Reading {
                            status,
                            substatus,
                            content_type,
                            source,
                            body,
                        };
                        return Polled::Waiting(call);
                    }
                    Poll::Ready(Some(Ok(chunk))) => {
                        body.extend_from_slice(&chunk);
                        call.stage = Stage::Reading {
                            status,
                            substatus,
                            content_type,
                            source,
                            body,
                        };
                    }
                    Poll::Ready(Some(Err(Broken))) => {
                        server.answer(call.exchange, Code::INTERNAL_SERVER_ERROR, &[]);
                        return Polled::Settled;
                    }
                    Poll::Ready(None) => {
                        self.settle(
                            call.exchange,
                            status,
                            substatus,
                            content_type.as_deref(),
                            &body,
                            server,
                        );
                        return Polled::Settled;
                    }
                },
            }
        }
    }

    /// Answers the exchange `exchange` for a whole answer of the app, by the
    /// rules of step 4.
    fn settle(
        &mut self,
        exchange: Exchange<P>,
        status: u16,
        substatus: u8,
        content_type: Option<&[u8]>,
        body: &[u8],
        server: &mut Server<P>,
    ) {
        match answer::decide(status, substatus, content_type, body) {
            Decision::Stream => self.open(exchange, None, body, server),
            Decision::Answer(code) => server.answer(exchange, code, body),
            Decision::Refuse(code) => server.answer(exchange, code, &[]),
        }
    }

    /// Opens a stream for the exchange `exchange` over the source `source`,
    /// with the bytes `first` of the stream already read.
    ///
    /// A stream the server refuses drops its source, and the server answers
    /// the refusal itself.
    fn open(
        &mut self,
        exchange: Exchange<P>,
        source: Option<Box<dyn Chunks>>,
        first: &[u8],
        server: &mut Server<P>,
    ) {
        let peer = exchange.peer();
        let Ok(handle) = server.open_stream(exchange) else {
            return;
        };
        let mut reader = Reader::default();
        reader.push(first);
        self.streams.push(Open {
            peer,
            handle,
            source,
            reader,
        });
    }
}

/// The passes of serve and poll one [`Listener::turn_rpcs`] runs at most, so
/// a socket whose requests keep arriving or a source that always has a piece
/// leaves the task to the others.
const PASSES_PER_TURN: usize = 8;

/// The pieces of one stream's source a turn reads at most, so a source that
/// always has a piece ready leaves the turn to the other streams and the
/// engine.
const PIECES_PER_TURN: usize = 64;

/// Polls the next piece of `source`, the piece `pieces` of this turn, and
/// answers pending once the turn has read [`PIECES_PER_TURN`] of them, after
/// waking the task, so a source that always has a piece ready is read on in
/// the next turn.
fn poll_piece(
    source: &mut dyn Chunks,
    pieces: &mut usize,
    cx: &mut Context<'_>,
) -> Poll<Option<Result<Vec<u8>, Broken>>> {
    if *pieces == PIECES_PER_TURN {
        cx.waker().wake_by_ref();
        return Poll::Pending;
    }
    *pieces += 1;
    source.poll_chunk(cx)
}

/// Reads the source of the stream `open` while its queue has room, hands each
/// event to the server, and tells whether anything moved.
///
/// A source that ends or fails before its `complete` event ends the stream
/// with `aborted`, and the bytes of an event the source did not end are
/// dropped, as a reader of an event stream drops them at its end.
fn read_stream<P: Copy + Eq>(
    open: &mut Open<P>,
    server: &mut Server<P>,
    cx: &mut Context<'_>,
) -> bool {
    let mut moved = false;
    let mut pieces = 0;
    while server.flow(open.peer, open.handle) == Flow::More {
        if let Some(event) = open.reader.next() {
            moved = true;
            match event {
                Event::Next(value) => {
                    server.next(open.peer, open.handle, &value);
                }
                Event::Complete => {
                    server.complete(open.peer, open.handle);
                    open.source = None;
                }
            }
            continue;
        }
        let Some(source) = open.source.as_mut() else {
            server.aborted(open.peer, open.handle);
            return true;
        };
        match poll_piece(&mut **source, &mut pieces, cx) {
            Poll::Pending => return moved,
            Poll::Ready(Some(Ok(piece))) => {
                moved = true;
                open.reader.push(&piece);
            }
            Poll::Ready(Some(Err(Broken))) => {
                server.aborted(open.peer, open.handle);
                open.source = None;
                return true;
            }
            Poll::Ready(None) => {
                moved = true;
                open.source = None;
            }
        }
    }
    moved
}

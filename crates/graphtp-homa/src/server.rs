// SPDX-License-Identifier: MIT
/*
 * graphtp-homa server: the sans I/O server of GraphTP/1, keyed by peer, with
 * the stream table, the one parked PULL and the batch number of each peer.
 *
 * No analogue in the reference. The exchanges, the streams, the refusals and
 * the order of the checks are those of the specification.
 */

//! The sans I/O server of GraphTP/1.
//!
//! A caller hands each whole request with its peer and its RPC to
//! [`Server::request`], which checks the 11 rules of `docs/graphtp.md` section
//! 5.3 in their order and answers a refusal, a PULL and a CANCEL itself. An
//! OPERATION that passes goes to the caller's handler as an [`Exchange`], which
//! the handler completes with [`Server::answer`], or with
//! [`Server::open_stream`] and then [`Server::next`], [`Server::complete`] and
//! [`Server::aborted`] per stream, at once or later. [`Server::poll`] yields
//! each answer to send, and a batch for every parked PULL whose peer has an
//! entry ready.
//!
//! The server keeps per peer the stream table, the handles it minted, the one
//! parked PULL, the entries ready in the order they became ready, each stream's
//! bounded queue and the batch number `seq`, until [`Server::link_down`] ends
//! the peer. The table is a vector searched by the peer, since a peer type of
//! Homa gives equality and a server holds few peers, and the workspace takes no
//! map for it.

use alloc::{
    collections::VecDeque,
    vec::Vec, //
};
use core::fmt;

use graphtp::{
    credential::Credential,
    deflate::{
        Params,
        ScratchError, //
    },
    status::Code,
    stream::{
        self,
        Entry,
        Handle,
        Seq,
        Target, //
    },
    wire::{
        self,
        Content, //
    },
    zlib,
};
use homa::RpcId;

/// A message to send as the answer of an RPC, the whole `GraphTP` answer.
pub type Message = Vec<u8>;

/// The smallest answer buffer, which holds the 65 bytes of the longest answer
/// the server writes with no byte of the service: the batch of one `cancelled`
/// entry with the largest handle and the largest batch number, and its head.
pub const MIN_ANSWER_BUFFER: usize = 65;

/// The bytes of an answer head.
const HEAD: usize = 2;

/// The bytes that close a stream body, `]}`.
const CLOSE: usize = 2;

/// The bounds of a server, which a card sets.
///
/// Each bound answers its code of `docs/graphtp.md` sections 2.5 and 5.5 when a
/// request or an answer passes it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// The longest credential, 431.0 above it.
    pub max_credential: u16,
    /// The longest content once decoded, 413.0 above it.
    pub max_content: usize,
    /// The streams one peer may hold, 429.1 at the bound.
    pub streams_per_peer: u16,
    /// The longest answer, 500.1 for an answer of the service above it, and at
    /// least [`MIN_ANSWER_BUFFER`].
    pub answer_buffer: usize,
    /// The `next` entries one stream's queue holds between two batches, after
    /// which the source waits and a further entry ends the stream with
    /// `aborted`.
    pub queue: usize,
    /// The peers whose streams and batch number the server keeps, 503.1 for a
    /// new peer above it.
    pub peers: usize,
    /// The sizes of the encoder's scratch, such as [`zlib::PARAMS`].
    pub coding: Params,
}

/// Why a server could not be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The answer buffer is shorter than [`MIN_ANSWER_BUFFER`].
    AnswerBuffer,
    /// The encoder's scratch could not be made.
    Scratch(ScratchError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::AnswerBuffer => write!(
                f,
                "the answer buffer is shorter than {MIN_ANSWER_BUFFER} bytes"
            ),
            Error::Scratch(error) => write!(f, "the scratch of the encoder: {error}"),
        }
    }
}

impl core::error::Error for Error {}

/// One OPERATION in the hands of the handler, which the handler completes once.
///
/// The server hands it out with the credential and the decoded content, and
/// takes it back through [`Server::answer`] or [`Server::open_stream`], so an
/// exchange is answered at most once.
#[derive(Debug, PartialEq, Eq)]
pub struct Exchange<P> {
    /// The peer that sent the OPERATION.
    peer: P,
    /// The server RPC that carries it.
    id: RpcId,
}

impl<P: Copy> Exchange<P> {
    /// Returns the peer that sent the OPERATION.
    #[must_use]
    pub fn peer(&self) -> P {
        self.peer
    }

    /// Returns the server RPC that carries the OPERATION.
    #[must_use]
    pub fn id(&self) -> RpcId {
        self.id
    }
}

/// What an OPERATION carries once its request passed the 11 rules.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Operation {
    /// The value of `Authorization`, `None` for none.
    pub credential: Option<Vec<u8>>,
    /// The GraphQL request as its client wrote it, decoded.
    pub content: Vec<u8>,
}

/// What a request asks of the caller.
#[derive(Debug, PartialEq, Eq)]
pub enum Step<P> {
    /// An OPERATION for the handler.
    Execute(Exchange<P>, Operation),
    /// The answer of the RPC, which the server wrote itself.
    ///
    /// The caller sends every message [`Server::poll`] holds before this one,
    /// which keeps the order of `docs/graphtp.md` section 2.4: the parked PULL
    /// that a CANCEL ends is answered before the CANCEL.
    Reply(RpcId, Message),
    /// A PULL the server parked until an entry of the peer is ready.
    Parked,
}

/// What a stream's queue takes after an entry, which is how back pressure
/// reaches the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Flow {
    /// The queue has room, and the source may be read.
    More,
    /// The queue is full, and the source waits until a batch takes its
    /// entries.
    Wait,
    /// The stream has ended, and its source is returned.
    Ended,
}

/// One entry waiting in the queue of a peer.
#[derive(Clone, Debug, PartialEq, Eq)]
enum Pending {
    /// One execution result, as the service wrote it.
    Next(Vec<u8>),
    /// The source ended the stream.
    Complete,
    /// The source failed or the queue overflowed.
    Aborted,
}

/// An entry of one stream, ready for the next batch.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Ready {
    /// The stream it belongs to.
    handle: Handle,
    /// The entry.
    pending: Pending,
}

impl Ready {
    /// Returns the entry as the writers of `graphtp` take it.
    fn entry(&self) -> Entry<'_> {
        match &self.pending {
            Pending::Next(value) => Entry::Next(self.handle, value),
            Pending::Complete => Entry::Complete(self.handle),
            Pending::Aborted => Entry::Aborted(self.handle),
        }
    }
}

/// A stream the peer holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Stream {
    /// Its handle.
    handle: Handle,
    /// The `next` entries of its queue.
    queued: usize,
    /// Whether its last entry, `complete` or `aborted`, is queued, so the
    /// stream lives only until a batch takes it.
    ended: bool,
}

/// What the server keeps of one peer.
#[derive(Clone, Debug)]
struct Peer<P> {
    /// The peer as Homa names it.
    name: P,
    /// The batch number of the next stream answer.
    seq: Seq,
    /// The handle the server tries first when it mints the next.
    next_handle: u16,
    /// The PULL the server holds, if one is parked.
    parked: Option<RpcId>,
    /// The streams the peer holds.
    streams: Vec<Stream>,
    /// The entries of every stream of the peer, in the order they became
    /// ready.
    ready: VecDeque<Ready>,
}

impl<P> Peer<P> {
    /// Returns the entry of a peer the server did not know, whose batch number
    /// starts at 0.
    fn new(name: P) -> Peer<P> {
        Peer {
            name,
            seq: Seq::ZERO,
            next_handle: 1,
            parked: None,
            streams: Vec::new(),
            ready: VecDeque::new(),
        }
    }

    /// Returns the stream of the handle `handle`, if the peer holds it.
    fn stream(&mut self, handle: Handle) -> Option<&mut Stream> {
        self.streams
            .iter_mut()
            .find(|stream| stream.handle == handle)
    }

    /// Mints a handle that no stream of the peer holds, 1 to 65,535.
    fn mint(&mut self) -> Option<Handle> {
        for _ in 0..u16::MAX {
            let handle = Handle::new(self.next_handle);
            self.next_handle = self.next_handle.checked_add(1).unwrap_or(1);
            if let Some(handle) = handle
                && !self.streams.iter().any(|stream| stream.handle == handle)
            {
                return Some(handle);
            }
        }
        None
    }

    /// Ends the stream `handle` at once, with its queued entries, and tells
    /// whether its source was still being read.
    fn drop_stream(&mut self, handle: Handle) -> Option<bool> {
        let index = self
            .streams
            .iter()
            .position(|stream| stream.handle == handle)?;
        let stream = self.streams.remove(index);
        self.ready.retain(|ready| ready.handle != handle);
        Some(!stream.ended)
    }

    /// Takes the first `count` ready entries into a batch and ends each stream
    /// whose last entry leaves with them.
    fn deliver(&mut self, count: usize) {
        for ready in self.ready.drain(..count.min(self.ready.len())) {
            let index = self
                .streams
                .iter()
                .position(|stream| stream.handle == ready.handle);
            match (ready.pending, index) {
                (Pending::Next(_), Some(index)) => {
                    if let Some(stream) = self.streams.get_mut(index) {
                        stream.queued = stream.queued.saturating_sub(1);
                    }
                }
                (Pending::Complete | Pending::Aborted, Some(index)) => {
                    self.streams.remove(index);
                }
                (_, None) => {}
            }
        }
        self.seq = self.seq.next();
    }

    /// Ends the stream of the first ready entry with `aborted` in place of its
    /// queued entries, since that entry alone outgrows every answer, and
    /// tells whether its source was still being read.
    fn overflow(&mut self) -> Option<(Handle, bool)> {
        let handle = self.ready.front()?.handle;
        let reading = self
            .stream(handle)
            .is_some_and(|stream| !core::mem::replace(&mut stream.ended, true));
        if let Some(stream) = self.stream(handle) {
            stream.queued = 0;
        }
        self.ready.retain(|ready| ready.handle != handle);
        self.ready.push_front(Ready {
            handle,
            pending: Pending::Aborted,
        });
        Some((handle, reading))
    }
}

/// The sans I/O server of GraphTP/1 over the peers of type `P`.
pub struct Server<P> {
    /// The bounds.
    config: Config,
    /// The memory of the encoder, lent to one answer at a time.
    scratch: zlib::Scratch,
    /// The peers the server keeps.
    peers: Vec<Peer<P>>,
    /// The OPERATIONs in the hands of the handler, by peer and RPC.
    exchanges: Vec<(P, RpcId)>,
    /// The answers to send, in order, each with the peer of its RPC.
    outbox: VecDeque<(P, RpcId, Message)>,
    /// The streams a CANCEL or a link down ended while their source was read.
    ended: VecDeque<(P, Handle)>,
}

impl<P: Copy + Eq> Server<P> {
    /// Returns a server with the bounds `config` and no peer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::AnswerBuffer`] when the answer buffer is shorter than
    /// [`MIN_ANSWER_BUFFER`], and [`Error::Scratch`] when the encoder's scratch
    /// of `config.coding` cannot be made.
    pub fn new(config: Config) -> Result<Server<P>, Error> {
        if config.answer_buffer < MIN_ANSWER_BUFFER {
            return Err(Error::AnswerBuffer);
        }
        let scratch = zlib::Scratch::new(config.coding).map_err(Error::Scratch)?;
        Ok(Server {
            config,
            scratch,
            peers: Vec::new(),
            exchanges: Vec::new(),
            outbox: VecDeque::new(),
            ended: VecDeque::new(),
        })
    }

    /// Returns the bounds of the server.
    #[must_use]
    pub fn config(&self) -> &Config {
        &self.config
    }

    /// Takes the whole request `message` of the RPC `id` from `peer`.
    ///
    /// The request is checked by the 11 rules of `docs/graphtp.md` section 5.3
    /// in their order, and a request that breaks one is answered with its
    /// code and an empty body. An OPERATION that passes is handed out with its
    /// decoded content, a PULL is answered with a batch or parked, and a
    /// CANCEL is answered with a batch of its `cancelled` entries.
    pub fn request(&mut self, peer: P, id: RpcId, message: &[u8]) -> Step<P> {
        let request = match wire::decode_request(message, self.config.max_credential) {
            Ok(request) => request,
            Err(code) => return Step::Reply(id, refusal(code)),
        };
        match request {
            wire::Request::Operation {
                credential,
                content,
            } => self.operation(peer, id, credential, content),
            wire::Request::Pull => self.pull(peer, id),
            wire::Request::Cancel(target) => self.cancel(peer, id, target),
        }
    }

    /// Completes the exchange `exchange` with the code `code` and the body
    /// `body`, the service's JSON as it wrote it.
    ///
    /// The body is coded when it holds more than 128 bytes, and an answer that
    /// outgrows the answer buffer answers 500.1. A code the registry gives to
    /// the server alone is a refusal and carries no body, 200.3, which
    /// [`Server::open_stream`] alone answers, becomes 500.0, and an empty body
    /// is a refusal whatever its code. An exchange whose peer went down is
    /// dropped with no answer, since its RPC is gone.
    // The exchange is taken by value although only its fields are read, so an
    // exchange answered once can be answered no more.
    #[expect(clippy::needless_pass_by_value)]
    pub fn answer(&mut self, exchange: Exchange<P>, code: Code, body: &[u8]) {
        let Exchange { peer, id } = exchange;
        if !self.take_exchange(peer, id) {
            return;
        }
        let message = if code == Code::STREAM_OPENED {
            refusal(Code::INTERNAL_SERVER_ERROR)
        } else if code.is_server_only() || body.is_empty() {
            refusal(code)
        } else {
            self.coded(code, body)
                .unwrap_or_else(|| refusal(Code::ANSWER_TOO_LARGE))
        };
        self.outbox.push_back((peer, id, message));
    }

    /// Opens a stream for the exchange `exchange` and returns its handle.
    ///
    /// The answer 200.3 with the head of the stream, whose `events` are empty,
    /// goes to [`Server::poll`], and the stream's entries go to the peer's
    /// batches through [`Server::next`], [`Server::complete`] and
    /// [`Server::aborted`].
    ///
    /// # Errors
    ///
    /// Returns 429.1 when the peer holds the streams one peer may hold and
    /// 503.1 when the server keeps as many peers as it may, each of which the
    /// server answers itself, and 502.2 when the peer went down and the
    /// exchange has no RPC left to answer.
    // The exchange is taken by value for the reason `answer` gives.
    #[expect(clippy::needless_pass_by_value)]
    pub fn open_stream(&mut self, exchange: Exchange<P>) -> Result<Handle, Code> {
        let Exchange { peer, id } = exchange;
        if !self.take_exchange(peer, id) {
            return Err(Code::LINK_DOWN);
        }
        let limit = usize::from(self.config.streams_per_peer);
        let opened = self
            .peer_or_new(peer)
            .ok_or(Code::INSUFFICIENT_MEMORY)
            .and_then(|entry| {
                if entry.streams.len() >= limit {
                    return Err(Code::TOO_MANY_STREAMS);
                }
                let handle = entry.mint().ok_or(Code::TOO_MANY_STREAMS)?;
                let seq = entry.seq;
                entry.seq = seq.next();
                entry.streams.push(Stream {
                    handle,
                    queued: 0,
                    ended: false,
                });
                Ok((handle, seq))
            });
        let (handle, seq) = match opened {
            Ok(opened) => opened,
            Err(code) => {
                self.outbox.push_back((peer, id, refusal(code)));
                return Err(code);
            }
        };
        let mut head = Vec::new();
        let written = stream::write_head(&mut head, handle, seq)
            .and_then(|events| stream::close(&mut head, events));
        let message = match written {
            Ok(_) => self
                .coded(Code::STREAM_OPENED, &head)
                .unwrap_or_else(|| refusal(Code::INSUFFICIENT_MEMORY)),
            Err(_) => refusal(Code::INSUFFICIENT_MEMORY),
        };
        self.outbox.push_back((peer, id, message));
        Ok(handle)
    }

    /// Queues one execution result `value` of the stream `handle` of `peer`,
    /// the bytes of one JSON value as the service wrote them.
    ///
    /// The return value says whether the source may be read on. A value that
    /// finds the queue full, or an empty value, which is no JSON value, ends
    /// the stream with `aborted` and returns [`Flow::Ended`], and so does a
    /// stream that has ended already.
    pub fn next(&mut self, peer: P, handle: Handle, value: &[u8]) -> Flow {
        let bound = self.config.queue;
        let Some(entry) = self.peer_mut(peer) else {
            return Flow::Ended;
        };
        let Some(stream) = entry
            .streams
            .iter_mut()
            .find(|stream| stream.handle == handle)
        else {
            return Flow::Ended;
        };
        if stream.ended {
            return Flow::Ended;
        }
        if value.is_empty() || stream.queued >= bound {
            stream.ended = true;
            entry.ready.push_back(Ready {
                handle,
                pending: Pending::Aborted,
            });
            return Flow::Ended;
        }
        stream.queued += 1;
        let flow = if stream.queued >= bound {
            Flow::Wait
        } else {
            Flow::More
        };
        entry.ready.push_back(Ready {
            handle,
            pending: Pending::Next(value.to_vec()),
        });
        flow
    }

    /// Ends the stream `handle` of `peer` as its source ended it, with a
    /// `complete` entry after the entries it queued.
    pub fn complete(&mut self, peer: P, handle: Handle) {
        self.finish(peer, handle, Pending::Complete);
    }

    /// Ends the stream `handle` of `peer` whose source failed, with an
    /// `aborted` entry after the entries it queued.
    pub fn aborted(&mut self, peer: P, handle: Handle) {
        self.finish(peer, handle, Pending::Aborted);
    }

    /// Returns what the queue of the stream `handle` of `peer` takes now.
    #[must_use]
    pub fn flow(&self, peer: P, handle: Handle) -> Flow {
        let stream = self
            .peers
            .iter()
            .find(|entry| entry.name == peer)
            .and_then(|entry| entry.streams.iter().find(|stream| stream.handle == handle));
        match stream {
            None => Flow::Ended,
            Some(stream) if stream.ended => Flow::Ended,
            Some(stream) if stream.queued >= self.config.queue => Flow::Wait,
            Some(_) => Flow::More,
        }
    }

    /// Ends `peer` after a link down or a dead peer.
    ///
    /// Every stream of the peer ends, its parked PULL is forgotten with no
    /// answer and its batch number is forgotten, so the next stream answer to
    /// it counts from 0, since the transport has already dropped every RPC of
    /// the peer. The exchanges of the peer and the answers still to send to it
    /// are dropped as well.
    pub fn link_down(&mut self, peer: P) {
        if let Some(index) = self.peers.iter().position(|entry| entry.name == peer) {
            let entry = self.peers.remove(index);
            for stream in entry.streams.iter().filter(|stream| !stream.ended) {
                self.ended.push_back((peer, stream.handle));
            }
        }
        self.exchanges.retain(|&(held, _)| held != peer);
        self.outbox.retain(|&(held, _, _)| held != peer);
    }

    /// Returns the next answer to send, in the order the server wrote them,
    /// then a batch for a parked PULL whose peer has an entry ready.
    pub fn poll(&mut self) -> Option<(RpcId, Message)> {
        if let Some((_, id, message)) = self.outbox.pop_front() {
            return Some((id, message));
        }
        let index = self
            .peers
            .iter()
            .position(|entry| entry.parked.is_some() && !entry.ready.is_empty())?;
        let id = self.peers.get_mut(index)?.parked.take()?;
        Some((id, self.batch(index)))
    }

    /// Returns the next stream that a CANCEL or a link down ended while its
    /// source was still read, so the caller returns that source.
    ///
    /// A stream that the handler ended, or that [`Server::next`] ended with
    /// [`Flow::Ended`], is not reported again.
    pub fn poll_ended(&mut self) -> Option<(P, Handle)> {
        self.ended.pop_front()
    }

    /// Hands out an OPERATION whose content decodes within the bound.
    fn operation(
        &mut self,
        peer: P,
        id: RpcId,
        credential: Option<Credential<'_>>,
        content: Content<'_>,
    ) -> Step<P> {
        let mut plain = Vec::new();
        if let Err(code) = wire::decode_content(content, self.config.max_content, &mut plain) {
            return Step::Reply(id, refusal(code));
        }
        self.exchanges.push((peer, id));
        let operation = Operation {
            credential: credential.map(|credential| credential.as_bytes().to_vec()),
            content: plain,
        };
        Step::Execute(Exchange { peer, id }, operation)
    }

    /// Answers a PULL with a batch of the ready entries, parks it, or refuses
    /// it with 404.1 for a peer with no stream and 409.1 beside a parked one.
    fn pull(&mut self, peer: P, id: RpcId) -> Step<P> {
        let Some(index) = self.peers.iter().position(|entry| entry.name == peer) else {
            return Step::Reply(id, refusal(Code::UNKNOWN_STREAM));
        };
        let Some(entry) = self.peers.get_mut(index) else {
            return Step::Reply(id, refusal(Code::UNKNOWN_STREAM));
        };
        if entry.streams.is_empty() {
            return Step::Reply(id, refusal(Code::UNKNOWN_STREAM));
        }
        if entry.parked.is_some() {
            return Step::Reply(id, refusal(Code::PULL_ALREADY_PARKED));
        }
        if entry.ready.is_empty() {
            entry.parked = Some(id);
            return Step::Parked;
        }
        Step::Reply(id, self.batch(index))
    }

    /// Ends the streams a CANCEL names and answers it with one `cancelled`
    /// entry per stream it ended, after a 404.1 to the parked PULL of a peer
    /// left with no stream.
    fn cancel(&mut self, peer: P, id: RpcId, target: Target) -> Step<P> {
        let entry = match target {
            Target::All => self.peer_or_new(peer),
            Target::One(_) => self.peer_mut(peer),
        };
        let Some(entry) = entry else {
            let code = match target {
                Target::All => Code::INSUFFICIENT_MEMORY,
                Target::One(_) => Code::UNKNOWN_STREAM,
            };
            return Step::Reply(id, refusal(code));
        };
        let handles: Vec<Handle> = match target {
            Target::All => entry.streams.iter().map(|stream| stream.handle).collect(),
            Target::One(handle) if entry.streams.iter().any(|stream| stream.handle == handle) => {
                alloc::vec![handle]
            }
            Target::One(_) => return Step::Reply(id, refusal(Code::UNKNOWN_STREAM)),
        };
        let mut reading = Vec::new();
        for &handle in &handles {
            if entry.drop_stream(handle) == Some(true) {
                reading.push(handle);
            }
        }
        let parked = if entry.streams.is_empty() {
            entry.parked.take()
        } else {
            None
        };
        let seq = entry.seq;
        if let Some(pull) = parked {
            self.outbox
                .push_back((peer, pull, refusal(Code::UNKNOWN_STREAM)));
        }
        self.ended
            .extend(reading.into_iter().map(|handle| (peer, handle)));
        match self.cancelled(seq, &handles) {
            Some(message) => {
                if let Some(entry) = self.peer_mut(peer) {
                    entry.seq = seq.next();
                }
                Step::Reply(id, message)
            }
            None => Step::Reply(id, refusal(Code::ANSWER_TOO_LARGE)),
        }
    }

    /// Writes the batch of the number `seq` with one `cancelled` entry per
    /// handle, or `None` when it outgrows the answer buffer.
    fn cancelled(&mut self, seq: Seq, handles: &[Handle]) -> Option<Message> {
        let mut plain = Vec::new();
        let mut events = stream::write_batch(&mut plain, seq).ok()?;
        for &handle in handles {
            stream::write_entry(&mut plain, &mut events, &Entry::Cancelled(handle)).ok()?;
        }
        stream::close(&mut plain, events).ok()?;
        self.coded(Code::OK, &plain)
    }

    /// Answers the parked PULL or the PULL of the peer at `index` with the
    /// ready entries that fit the answer buffer, in the order they became
    /// ready.
    ///
    /// An entry that outgrows the answer buffer alone ends its stream with
    /// `aborted`, so every batch holds at least one entry.
    fn batch(&mut self, index: usize) -> Message {
        let budget = self.config.answer_buffer;
        loop {
            let Some(entry) = self.peers.get(index) else {
                return refusal(Code::INTERNAL_SERVER_ERROR);
            };
            let mut most = entry.ready.len();
            while most > 0 {
                let (plain, count) = plain_batch(entry, most, budget);
                if count == 0 {
                    break;
                }
                if let Some(message) = code_within(&mut self.scratch, &plain, budget) {
                    if let Some(entry) = self.peers.get_mut(index) {
                        entry.deliver(count);
                    }
                    return message;
                }
                most = count - 1;
            }
            let Some(entry) = self.peers.get_mut(index) else {
                return refusal(Code::INTERNAL_SERVER_ERROR);
            };
            match entry.overflow() {
                Some((handle, true)) => {
                    let peer = entry.name;
                    self.ended.push_back((peer, handle));
                }
                Some((_, false)) => {}
                None => return self.empty_batch(index),
            }
        }
    }

    /// Answers with a batch of no entry, which a PULL with nothing ready never
    /// meets, since the server parks it.
    fn empty_batch(&mut self, index: usize) -> Message {
        let Some(entry) = self.peers.get_mut(index) else {
            return refusal(Code::INTERNAL_SERVER_ERROR);
        };
        let seq = entry.seq;
        entry.seq = seq.next();
        let mut plain = Vec::new();
        let written = stream::write_batch(&mut plain, seq)
            .and_then(|events| stream::close(&mut plain, events));
        match written {
            Ok(_) => self
                .coded(Code::OK, &plain)
                .unwrap_or_else(|| refusal(Code::ANSWER_TOO_LARGE)),
            Err(_) => refusal(Code::INSUFFICIENT_MEMORY),
        }
    }

    /// Ends the stream `handle` of `peer` with the last entry `last`.
    fn finish(&mut self, peer: P, handle: Handle, last: Pending) {
        let Some(entry) = self.peer_mut(peer) else {
            return;
        };
        let Some(stream) = entry
            .streams
            .iter_mut()
            .find(|stream| stream.handle == handle)
        else {
            return;
        };
        if stream.ended {
            return;
        }
        stream.ended = true;
        entry.ready.push_back(Ready {
            handle,
            pending: last,
        });
    }

    /// Codes the answer of `code` with the body `plain`, or `None` when it
    /// outgrows the answer buffer.
    fn coded(&mut self, code: Code, plain: &[u8]) -> Option<Message> {
        let mut message = Vec::new();
        wire::write_answer(&mut message, code, plain, &mut self.scratch).ok()?;
        (message.len() <= self.config.answer_buffer).then_some(message)
    }

    /// Takes the exchange back, and tells whether its RPC still stands.
    fn take_exchange(&mut self, peer: P, id: RpcId) -> bool {
        match self.exchanges.iter().position(|&held| held == (peer, id)) {
            Some(index) => {
                self.exchanges.swap_remove(index);
                true
            }
            None => false,
        }
    }

    /// Returns the entry of `peer`, if the server keeps it.
    fn peer_mut(&mut self, peer: P) -> Option<&mut Peer<P>> {
        self.peers.iter_mut().find(|entry| entry.name == peer)
    }

    /// Returns the entry of `peer`, made when the server has room for it.
    fn peer_or_new(&mut self, peer: P) -> Option<&mut Peer<P>> {
        let index = match self.peers.iter().position(|entry| entry.name == peer) {
            Some(index) => index,
            None if self.peers.len() < self.config.peers => {
                self.peers.push(Peer::new(peer));
                self.peers.len() - 1
            }
            None => return None,
        };
        self.peers.get_mut(index)
    }
}

/// Writes the batch of the first ready entries of `entry`, at most `most` of
/// them, whose plain body and answer head stay within `budget`, and returns it
/// with the count it holds.
fn plain_batch<P>(entry: &Peer<P>, most: usize, budget: usize) -> (Vec<u8>, usize) {
    let mut plain = Vec::new();
    let Ok(mut events) = stream::write_batch(&mut plain, entry.seq) else {
        return (plain, 0);
    };
    for ready in entry.ready.iter().take(most) {
        let mark = plain.len();
        let before = events;
        let written = stream::write_entry(&mut plain, &mut events, &ready.entry()).is_ok();
        if !written || plain.len() + CLOSE + HEAD > budget {
            plain.truncate(mark);
            events = before;
            break;
        }
    }
    let count = events.count();
    if stream::close(&mut plain, events).is_err() {
        return (plain, 0);
    }
    (plain, count)
}

/// Codes the batch `plain` as the answer 200.0, or `None` when the coded
/// answer outgrows `budget`.
fn code_within(scratch: &mut zlib::Scratch, plain: &[u8], budget: usize) -> Option<Message> {
    let mut message = Vec::new();
    wire::write_answer(&mut message, Code::OK, plain, scratch).ok()?;
    (message.len() <= budget).then_some(message)
}

/// Returns the refusal `code`, its 2 bytes.
fn refusal(code: Code) -> Message {
    let mut message = Vec::new();
    // A vector refuses a put only when the allocator refuses 2 bytes, and the
    // empty message that leaves then is malformed for the client, which is
    // all a server can still send.
    let _ = wire::write_refusal(&mut message, code);
    message
}

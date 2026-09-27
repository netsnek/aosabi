// SPDX-License-Identifier: MIT
/*
 * graphtp-homa client: the sans I/O client of GraphTP/1, with the repeat mark,
 * the start rule and the application of stream answers in the order of seq.
 *
 * No analogue in the reference. The exchanges, the streams, the rule of at most
 * once and the malformed answer are those of the specification.
 */

//! The sans I/O client of GraphTP/1.
//!
//! The client writes each OPERATION, PULL and CANCEL as an [`Outgoing`], the
//! message with the options of its Homa request, and a caller sends it and
//! hands the RPC back with [`Client::sent`]. Each answer and each failure of an
//! RPC goes to [`Client::answer`] and [`Client::failure`], which turn it into an
//! [`Incoming`]: the service's answer or a refusal, the stream events it
//! releases, an unknown outcome, a link down or a request to repeat.
//!
//! **The repeat mark.** An OPERATION that its caller did not mark repeatable
//! asks Homa with [`RequestOptions::report_unknown`] to report `RPC_UNKNOWN` in
//! place of its silent restart, and the client reports its outcome as unknown,
//! since `RPC_UNKNOWN` never tells whether the server ran it. A marked
//! OPERATION, a PULL and a CANCEL are repeated.
//!
//! **The batch number.** Every stream answer carries `seq`, and the client
//! applies them in its order, holding an answer that is ahead until the gap
//! fills, since Homa may deliver the answers of two RPCs out of order. A client
//! that cannot know the count, at start on a link whose end the server may not
//! see, or after a PULL met 404.1 while it held a stream it had not cancelled,
//! sends a CANCEL of handle 0 before its next OPERATION and PULL, which
//! [`Client::poll`] yields, and counts on from the `seq` of its answer. The
//! 404.1 of that PULL says the server forgot the count that numbered the
//! answers the client holds, so the client drops them, and a stream one of them
//! opened ends as opened and lost.

use alloc::vec::Vec;
use core::fmt;

use graphtp::{
    credential::Credential,
    deflate::{
        Params,
        ScratchError, //
    },
    kind::Kind,
    response::{
        Parts,
        Response, //
    },
    status::Code,
    stream::{
        Entries,
        Entry,
        Handle,
        Seq,
        Target, //
    },
    wire::{
        self,
        Answer,
        Body,
        WireError, //
    },
    zlib,
};
use homa::{
    Failure,
    RequestOptions,
    RpcId, //
};

/// Half the values of `seq`, the distance up to which an answer counts as
/// ahead of the one the client expects and not behind it.
const AHEAD: u32 = 1 << 31;

/// Whether the caller marked an OPERATION repeatable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Repeat {
    /// The outcome is reported as unknown after `RPC_UNKNOWN`.
    No,
    /// The OPERATION is sent again after `RPC_UNKNOWN` or a dead peer.
    Yes,
}

/// What the client knows of the end of its link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Link {
    /// Both ends see the link go down, as on a Unix Socket, so the count of
    /// `seq` starts at 0 and starts again at 0 after a link down.
    Seen,
    /// The server may not see the end of an earlier run of the client, as on a
    /// serial line, a radio, IP, or a USB or Bluetooth link whose end the
    /// device may not see, so the client starts with a CANCEL of handle 0.
    Unseen,
}

/// The settings of a client.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Config {
    /// The longest body the client decodes, beyond which an answer fails.
    pub max_body: usize,
    /// What the client knows of the end of its link.
    pub link: Link,
    /// The sizes of the encoder's scratch, such as [`zlib::PARAMS`].
    pub coding: Params,
}

/// Why the client writes no request.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// The credential breaks rule 6 or 7 of `docs/graphtp.md` section 5.3,
    /// with its code.
    Credential(Code),
    /// The count of `seq` is unknown until the answer of the CANCEL of handle
    /// 0 that [`Client::poll`] yields.
    Count,
    /// A PULL is in flight, and a peer parks one at a time.
    Parked,
    /// The allocator refused the message.
    Memory,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Credential(code) => write!(f, "the credential is refused with {code}"),
            Error::Count => {
                f.write_str("the count of seq waits for the answer of a CANCEL of handle 0")
            }
            Error::Parked => f.write_str("a PULL is in flight"),
            Error::Memory => f.write_str("the allocator refused the message"),
        }
    }
}

impl core::error::Error for Error {}

/// A request to send: the whole `GraphTP` message and the options of its Homa
/// request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Outgoing {
    /// The message.
    pub message: Vec<u8>,
    /// The options of the Homa request, `patient` for a PULL and
    /// `report_unknown` for an OPERATION that is not marked. A caller may set
    /// `source_port`.
    pub options: RequestOptions,
    /// The kind of the request.
    kind: Kind,
    /// The target of a CANCEL.
    target: Option<Target>,
    /// The mark of an OPERATION.
    repeat: Repeat,
}

impl Outgoing {
    /// Returns the kind of the request.
    #[must_use]
    pub fn kind(&self) -> Kind {
        self.kind
    }

    /// Returns the mark of the request, [`Repeat::Yes`] for a PULL and a
    /// CANCEL, which are always repeatable.
    #[must_use]
    pub fn repeat(&self) -> Repeat {
        self.repeat
    }
}

/// One event of a stream, in the order of `seq`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    /// The OPERATION of the RPC opened the stream.
    Opened(RpcId, Handle),
    /// One execution result, the bytes of a JSON value as the service wrote
    /// them.
    Next(Handle, Vec<u8>),
    /// The source ended the stream.
    Complete(Handle),
    /// A CANCEL ended the stream.
    Cancelled(Handle),
    /// The source failed or the server's queue overflowed.
    Aborted(Handle),
    /// The stream ended with no entry of the server: the link went down, the
    /// server holds no such stream or forgot the count that numbered its
    /// answer, or a CANCEL of handle 0 ended every stream of the peer and named
    /// it not.
    Lost(Handle),
}

/// What an answer or a failure gives the caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Incoming {
    /// The answer of an OPERATION, or a refusal of any request, with its code
    /// and its body, empty for a refusal.
    Result(Response<Vec<u8>>),
    /// The stream events the answer releases in the order of `seq`, none while
    /// the answer is held until a gap fills, or, for a PULL refused with 404.1
    /// that loses the count, each stream a dropped answer opened, opened and
    /// lost.
    Stream(Vec<Event>),
    /// The outcome is unknown: the server may have run the OPERATION or not.
    Unknown,
    /// The link went down during the exchange.
    LinkDown,
    /// The request to send again, a marked OPERATION or a CANCEL.
    Repeat(Outgoing),
    /// The answer is malformed, and the exchange failed.
    Malformed(WireError),
    /// No request in flight has the RPC.
    Stray,
}

/// The count of `seq`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Count {
    /// The number of the next stream answer.
    Known(Seq),
    /// Unknown until the answer of a CANCEL of handle 0.
    Unknown,
}

/// A stream the client holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Open {
    /// Its handle.
    handle: Handle,
    /// Whether a CANCEL that names it is in flight.
    cancelling: bool,
}

/// A stream answer, held until the client applies it.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Held {
    /// Its batch number.
    seq: Seq,
    /// Its events.
    events: Vec<Event>,
    /// Whether it answers a CANCEL of handle 0, which ends every stream.
    all: bool,
}

/// The sans I/O client of GraphTP/1 toward one server.
pub struct Client {
    /// The settings.
    config: Config,
    /// The memory of the encoder, lent to one content at a time.
    scratch: zlib::Scratch,
    /// The requests in flight, by RPC.
    pending: Vec<(RpcId, Outgoing)>,
    /// The streams the client holds.
    streams: Vec<Open>,
    /// The count of `seq`.
    count: Count,
    /// The stream answers not applied yet.
    held: Vec<Held>,
}

impl Client {
    /// Returns a client with the settings `config`.
    ///
    /// On a link of [`Link::Unseen`] the first request is the CANCEL of handle
    /// 0 that [`Client::poll`] yields.
    ///
    /// # Errors
    ///
    /// Returns the error of the encoder's scratch of `config.coding` when it
    /// cannot be made.
    pub fn new(config: Config) -> Result<Client, ScratchError> {
        let scratch = zlib::Scratch::new(config.coding)?;
        Ok(Client {
            config,
            scratch,
            pending: Vec::new(),
            streams: Vec::new(),
            count: start(config.link),
            held: Vec::new(),
        })
    }

    /// Writes an OPERATION with the credential `credential` and the content
    /// `content`, marked by `repeat`.
    ///
    /// A content of more than 128 bytes is coded.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Count`] while the count of `seq` is unknown,
    /// [`Error::Credential`] for a credential a server refuses by its bytes or
    /// its length, and [`Error::Memory`] when the allocator refuses the
    /// message.
    pub fn operation(
        &mut self,
        credential: Option<&[u8]>,
        content: &[u8],
        repeat: Repeat,
    ) -> Result<Outgoing, Error> {
        if self.count == Count::Unknown {
            return Err(Error::Count);
        }
        let credential = match credential {
            Some(bytes) if !bytes.is_empty() => {
                Some(Credential::new(bytes).map_err(Error::Credential)?)
            }
            _ => None,
        };
        let mut message = Vec::new();
        wire::write_operation(&mut message, credential, content, &mut self.scratch)
            .map_err(|_| Error::Memory)?;
        let options = RequestOptions {
            report_unknown: repeat == Repeat::No,
            ..RequestOptions::default()
        };
        Ok(Outgoing {
            message,
            options,
            kind: Kind::Operation,
            target: None,
            repeat,
        })
    }

    /// Writes a PULL, a patient request the server parks until an entry of a
    /// stream of the peer is ready.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Count`] while the count of `seq` is unknown,
    /// [`Error::Parked`] while a PULL is in flight, and [`Error::Memory`] when
    /// the allocator refuses the message.
    pub fn pull(&self) -> Result<Outgoing, Error> {
        if self.count == Count::Unknown {
            return Err(Error::Count);
        }
        if self.pulling() {
            return Err(Error::Parked);
        }
        let mut message = Vec::new();
        wire::write_pull(&mut message).map_err(|_| Error::Memory)?;
        let options = RequestOptions {
            patient: true,
            ..RequestOptions::default()
        };
        Ok(Outgoing {
            message,
            options,
            kind: Kind::Pull,
            target: None,
            repeat: Repeat::Yes,
        })
    }

    /// Writes a CANCEL of the target `target`, one stream or every stream of
    /// the peer.
    ///
    /// # Errors
    ///
    /// Returns [`Error::Memory`] when the allocator refuses the message.
    pub fn cancel(&self, target: Target) -> Result<Outgoing, Error> {
        let mut message = Vec::new();
        wire::write_cancel(&mut message, target).map_err(|_| Error::Memory)?;
        Ok(Outgoing {
            message,
            options: RequestOptions::default(),
            kind: Kind::Cancel,
            target: Some(target),
            repeat: Repeat::Yes,
        })
    }

    /// Returns the request the client needs sent now, if one: the CANCEL of
    /// handle 0 while the count of `seq` is unknown, and a PULL while a stream
    /// that no CANCEL names is open and no PULL is in flight.
    ///
    /// The client changes nothing until [`Client::sent`] takes the request, so
    /// a request that could not be sent is yielded again.
    #[must_use]
    pub fn poll(&self) -> Option<Outgoing> {
        match self.count {
            Count::Unknown => {
                let syncing = self
                    .pending
                    .iter()
                    .any(|(_, outgoing)| outgoing.target == Some(Target::All));
                if syncing {
                    None
                } else {
                    self.cancel(Target::All).ok()
                }
            }
            Count::Known(_) if self.streams.iter().all(|open| open.cancelling) => None,
            Count::Known(_) => self.pull().ok(),
        }
    }

    /// Takes the request `outgoing` that the caller sent as the RPC `id`.
    pub fn sent(&mut self, id: RpcId, outgoing: Outgoing) {
        match outgoing.target {
            Some(Target::All) => {
                for open in &mut self.streams {
                    open.cancelling = true;
                }
            }
            Some(Target::One(handle)) => {
                if let Some(open) = self.streams.iter_mut().find(|open| open.handle == handle) {
                    open.cancelling = true;
                }
            }
            None => {}
        }
        self.pending.push((id, outgoing));
    }

    /// Reads the whole answer `message` of the RPC `id`.
    ///
    /// A malformed answer fails its exchange, and a malformed answer of a
    /// stream loses the count of `seq`, which the next CANCEL of handle 0
    /// settles. A PULL refused with 404.1 while the client holds a stream it
    /// has not cancelled loses the count as well and drops the held answers,
    /// since the server forgot the count that numbered them.
    pub fn answer(&mut self, id: RpcId, message: &[u8]) -> Incoming {
        let Some((_, outgoing)) = self.take(id) else {
            return Incoming::Stray;
        };
        match outgoing.kind {
            Kind::Operation => self.operation_answer(id, message),
            Kind::Pull => self.pull_answer(message),
            _ => self.cancel_answer(outgoing.target, message),
        }
    }

    /// Reads the failure `why` of the RPC `id`, which ended with no answer.
    ///
    /// A link down is [`Incoming::LinkDown`] and a request longer than the peer
    /// accepts is 413.0. After `RPC_UNKNOWN` or a dead peer a marked OPERATION
    /// and a CANCEL are handed back to be sent again, an OPERATION that is not
    /// marked has an unknown outcome, and a PULL ends, since [`Client::poll`]
    /// parks the next one while a stream is open. An RPC the application
    /// aborted has an unknown outcome.
    pub fn failure(&mut self, id: RpcId, why: Failure) -> Incoming {
        let Some((_, outgoing)) = self.take(id) else {
            return Incoming::Stray;
        };
        match why {
            Failure::LinkDown => Incoming::LinkDown,
            Failure::TooLong => result(Code::CONTENT_TOO_LARGE, Vec::new()),
            Failure::Aborted => Incoming::Unknown,
            Failure::RpcUnknown | Failure::PeerTimeout => match outgoing.kind {
                Kind::Pull => Incoming::Unknown,
                _ if outgoing.repeat == Repeat::Yes => Incoming::Repeat(outgoing),
                _ => Incoming::Unknown,
            },
        }
    }

    /// Ends every stream after the caller saw the link go down, and returns
    /// a [`Event::Lost`] for each.
    ///
    /// On a link of [`Link::Seen`] the count starts again at 0, since the
    /// server forgets the peer on the same link down, and on a link of
    /// [`Link::Unseen`] it waits for a CANCEL of handle 0. The requests in
    /// flight end through their own failures.
    pub fn link_down(&mut self) -> Vec<Event> {
        let lost = self
            .streams
            .drain(..)
            .map(|open| Event::Lost(open.handle))
            .collect();
        self.held.clear();
        self.count = start(self.config.link);
        lost
    }

    /// Returns the handles of the streams the client holds.
    pub fn streams(&self) -> impl Iterator<Item = Handle> + '_ {
        self.streams.iter().map(|open| open.handle)
    }

    /// Reads the answer of an OPERATION.
    fn operation_answer(&mut self, id: RpcId, message: &[u8]) -> Incoming {
        match wire::decode_answer(Kind::Operation, message) {
            Ok(Answer::Refused(code)) => result(code, Vec::new()),
            Ok(Answer::Result { code, body }) => match self.decode(body) {
                Ok(plain) => result(code, plain),
                Err(error) => Incoming::Malformed(error),
            },
            Ok(Answer::Opened { head, .. }) => match self.stream_body(head) {
                Ok((Some(handle), seq, mut events)) => {
                    events.insert(0, Event::Opened(id, handle));
                    self.arrive(seq, events, false)
                }
                Ok((None, ..)) => self.lose(WireError::Stream),
                Err(error) => self.lose(error),
            },
            Ok(Answer::Batch { .. }) => Incoming::Malformed(WireError::Body),
            Err(error) => Incoming::Malformed(error),
        }
    }

    /// Reads the answer of a PULL.
    ///
    /// A 404.1 while the client holds a stream it has not cancelled means the
    /// server lost the peer, after a restart or a dead peer it never saw, so
    /// the count of `seq` is lost with it, and so are the held answers.
    fn pull_answer(&mut self, message: &[u8]) -> Incoming {
        match wire::decode_answer(Kind::Pull, message) {
            Ok(Answer::Batch { body, .. }) => match self.stream_body(body) {
                Ok((_, seq, events)) => self.arrive(seq, events, false),
                Err(error) => self.lose(error),
            },
            Ok(Answer::Refused(code)) => {
                let holding = self.streams.iter().any(|open| !open.cancelling);
                if code == Code::UNKNOWN_STREAM && holding {
                    return self.forget();
                }
                result(code, Vec::new())
            }
            Ok(_) => self.lose(WireError::Body),
            Err(error) => self.lose(error),
        }
    }

    /// Reads the answer of a CANCEL.
    fn cancel_answer(&mut self, target: Option<Target>, message: &[u8]) -> Incoming {
        match wire::decode_answer(Kind::Cancel, message) {
            Ok(Answer::Batch { body, .. }) => match self.stream_body(body) {
                Ok((_, seq, events)) => self.arrive(seq, events, target == Some(Target::All)),
                Err(error) => self.lose(error),
            },
            Ok(Answer::Refused(code)) => match target {
                Some(Target::One(handle)) if code == Code::UNKNOWN_STREAM && self.close(handle) => {
                    Incoming::Stream(alloc::vec![Event::Lost(handle)])
                }
                _ => result(code, Vec::new()),
            },
            Ok(_) => self.lose(WireError::Body),
            Err(error) => self.lose(error),
        }
    }

    /// Takes the stream answer of the number `seq` and returns the events it
    /// releases.
    fn arrive(&mut self, seq: Seq, events: Vec<Event>, all: bool) -> Incoming {
        self.held.push(Held { seq, events, all });
        let mut out = Vec::new();
        if self.count == Count::Unknown {
            self.settle(&mut out);
        }
        self.release(&mut out);
        Incoming::Stream(out)
    }

    /// Settles an unknown count on the answer of a CANCEL of handle 0: the
    /// answers held before it apply first in their order, then it, and the
    /// count goes on from its `seq`.
    fn settle(&mut self, out: &mut Vec<Event>) {
        let Some(index) = self.held.iter().position(|held| held.all) else {
            return;
        };
        let settling = self.held.remove(index);
        let mut older: Vec<Held> = Vec::new();
        let mut newer: Vec<Held> = Vec::new();
        for held in self.held.drain(..) {
            let behind = settling.seq.get().wrapping_sub(held.seq.get());
            if behind != 0 && behind < AHEAD {
                older.push(held);
            } else {
                newer.push(held);
            }
        }
        older.sort_by_key(|held| {
            core::cmp::Reverse(settling.seq.get().wrapping_sub(held.seq.get()))
        });
        for held in older {
            self.apply(held, out);
        }
        self.count = Count::Known(settling.seq.next());
        self.apply(settling, out);
        self.held = newer;
    }

    /// Applies every held answer from the one the count expects, until a gap,
    /// and drops an answer that is behind the count.
    fn release(&mut self, out: &mut Vec<Event>) {
        while let Count::Known(expected) = self.count {
            self.held
                .retain(|held| held.seq.get().wrapping_sub(expected.get()) < AHEAD);
            let Some(index) = self.held.iter().position(|held| held.seq == expected) else {
                return;
            };
            let held = self.held.remove(index);
            self.count = Count::Known(expected.next());
            self.apply(held, out);
        }
    }

    /// Applies the events of one stream answer to the streams the client
    /// holds and hands them on.
    ///
    /// After the answer of a CANCEL of handle 0 no stream of the peer lives on
    /// the server, so a stream it did not name is lost.
    fn apply(&mut self, held: Held, out: &mut Vec<Event>) {
        for event in held.events {
            match &event {
                Event::Opened(_, handle) => {
                    if !self.streams.iter().any(|open| open.handle == *handle) {
                        self.streams.push(Open {
                            handle: *handle,
                            cancelling: false,
                        });
                    }
                }
                Event::Complete(handle)
                | Event::Cancelled(handle)
                | Event::Aborted(handle)
                | Event::Lost(handle) => {
                    self.close(*handle);
                }
                Event::Next(..) => {}
            }
            out.push(event);
        }
        if held.all {
            out.extend(self.streams.drain(..).map(|open| Event::Lost(open.handle)));
        }
    }

    /// Reads a stream body into its handle, its batch number and its events.
    fn stream_body(&self, body: Body<'_>) -> Result<(Option<Handle>, Seq, Vec<Event>), WireError> {
        let plain = self.decode(body)?;
        let entries = Entries::new(&plain)?;
        let handle = entries.handle();
        let seq = entries.seq();
        let mut events = Vec::new();
        for entry in entries {
            events.push(match entry? {
                Entry::Next(handle, value) => Event::Next(handle, value.to_vec()),
                Entry::Complete(handle) => Event::Complete(handle),
                Entry::Cancelled(handle) => Event::Cancelled(handle),
                Entry::Aborted(handle) => Event::Aborted(handle),
            });
        }
        Ok((handle, seq, events))
    }

    /// Decodes a body under the client's bound.
    fn decode(&self, body: Body<'_>) -> Result<Vec<u8>, WireError> {
        let mut plain = Vec::new();
        wire::decode_body(body, self.config.max_body, &mut plain)?;
        Ok(plain)
    }

    /// Fails a stream answer as malformed and loses the count of `seq`, since
    /// the server counted the answer the client could not read.
    fn lose(&mut self, error: WireError) -> Incoming {
        self.count = Count::Unknown;
        Incoming::Malformed(error)
    }

    /// Loses the count of `seq` after the server forgot the peer, and drops
    /// the held answers, which the forgotten count numbered.
    ///
    /// A held answer never applies under the count the next CANCEL of handle
    /// 0 gives, since its `seq` belongs to the forgotten count. A stream that
    /// a dropped answer opened ends as opened and lost, so the caller of its
    /// OPERATION learns the outcome, and with no such stream the caller gets
    /// the refusal 404.1.
    fn forget(&mut self) -> Incoming {
        self.count = Count::Unknown;
        let mut out = Vec::new();
        for held in self.held.drain(..) {
            for event in held.events {
                if let Event::Opened(id, handle) = event {
                    out.push(Event::Opened(id, handle));
                    out.push(Event::Lost(handle));
                }
            }
        }
        if out.is_empty() {
            result(Code::UNKNOWN_STREAM, Vec::new())
        } else {
            Incoming::Stream(out)
        }
    }

    /// Ends the stream `handle`, and tells whether the client held it.
    fn close(&mut self, handle: Handle) -> bool {
        match self.streams.iter().position(|open| open.handle == handle) {
            Some(index) => {
                self.streams.remove(index);
                true
            }
            None => false,
        }
    }

    /// Takes the request in flight as the RPC `id`.
    fn take(&mut self, id: RpcId) -> Option<(RpcId, Outgoing)> {
        let index = self.pending.iter().position(|(held, _)| *held == id)?;
        Some(self.pending.remove(index))
    }

    /// Tells whether a PULL is in flight.
    fn pulling(&self) -> bool {
        self.pending
            .iter()
            .any(|(_, outgoing)| outgoing.kind == Kind::Pull)
    }
}

/// Returns the count a client starts with on a link of `link`.
fn start(link: Link) -> Count {
    match link {
        Link::Seen => Count::Known(Seq::ZERO),
        Link::Unseen => Count::Unknown,
    }
}

/// Returns the answer of the code `code` with the body `body`.
fn result(code: Code, body: Vec<u8>) -> Incoming {
    let parts = Parts {
        status: code.status,
        substatus: code.substatus,
    };
    Incoming::Result(Response::from_parts(parts, body))
}

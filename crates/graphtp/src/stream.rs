// SPDX-License-Identifier: MIT
/*
 * graphtp stream: the handles, the stream head, the batches and their entries,
 * and the reader of a plain stream body.
 *
 * No analogue in the reference. The stream bodies are those of the specification,
 * and the grammar the reader follows is that of JSON.
 */

//! The stream bodies: the handle, the target of a CANCEL, the batch number, the
//! writers of the stream head, the batch and its entries, and the reader of a
//! plain stream body.
//!
//! A stream body is JSON of a fixed shape, which a sender writes with its keys in
//! the order shown, no other key and no whitespace, so a device writes it with no
//! JSON writer:
//!
//! ```text
//! the head of a stream, the body of 200.3    {"stream":<handle>,"seq":<seq>,"events":[<entry>,...]}
//! a batch, the body of 200.0 of PULL, CANCEL {"seq":<seq>,"events":[<entry>,...]}
//! an entry                                   {"stream":<handle>,"next":<one execution result>}
//!                                            {"stream":<handle>,"complete":true}
//!                                            {"stream":<handle>,"cancelled":true}
//!                                            {"stream":<handle>,"aborted":true}
//! ```
//!
//! The writers put a body into a [`Sink`], and [`Entries`] reads a plain body
//! with no allocation. The reader takes any key order and any whitespace JSON
//! allows and skips a key it does not know, as a receiver does. It slices a
//! `next` value by the depth of its brackets, aware of strings, and hands its
//! bytes on as the service wrote them, without reading them as JSON.

use core::{
    fmt,
    num::NonZeroU16, //
};

use crate::{
    sink::{
        Full,
        Sink, //
    },
    wire::WireError,
};

/// The handle of a stream, 1 to 65,535, which a server mints per peer and never
/// reuses while its stream lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Handle(NonZeroU16);

impl Handle {
    /// Returns the handle of the value `value`, or `None` for 0, which names no
    /// stream.
    #[must_use]
    pub const fn new(value: u16) -> Option<Handle> {
        match NonZeroU16::new(value) {
            Some(value) => Some(Handle(value)),
            None => None,
        }
    }

    /// Returns the value of the handle, 1 to 65,535.
    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

impl fmt::Display for Handle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The target of a CANCEL, one stream or every stream of the peer.
///
/// The content of a CANCEL is 2 bytes, the handle, and 0 names every stream of
/// the peer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    /// The stream of the handle.
    One(Handle),
    /// Every stream of the peer, the handle 0.
    All,
}

impl Target {
    /// Returns the target of the 2 bytes of a CANCEL as a number, [`Target::All`]
    /// for 0.
    #[must_use]
    pub const fn from_u16(value: u16) -> Target {
        match Handle::new(value) {
            Some(handle) => Target::One(handle),
            None => Target::All,
        }
    }

    /// Returns the number a CANCEL carries for the target, 0 for
    /// [`Target::All`].
    #[must_use]
    pub const fn to_u16(self) -> u16 {
        match self {
            Target::One(handle) => handle.get(),
            Target::All => 0,
        }
    }
}

/// The batch number of a stream answer, a count the server keeps per peer.
///
/// The count is 0 on the first stream answer to a peer the server does not know,
/// one more on each after it, and wraps after 4,294,967,295, so a client applies
/// the stream answers in its order.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Seq(u32);

impl Seq {
    /// The count of the first stream answer to a peer, 0.
    pub const ZERO: Seq = Seq(0);

    /// Returns the batch number of the value `value`.
    #[must_use]
    pub const fn new(value: u32) -> Seq {
        Seq(value)
    }

    /// Returns the value of the batch number.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }

    /// Returns the batch number after this one, 0 after 4,294,967,295.
    #[must_use]
    pub const fn next(self) -> Seq {
        Seq(self.0.wrapping_add(1))
    }
}

impl fmt::Display for Seq {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One entry of a stream body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Entry<'a> {
    /// One execution result of the stream, the bytes of a JSON value as the
    /// service wrote them.
    Next(Handle, &'a [u8]),
    /// The source ended the stream.
    Complete(Handle),
    /// A CANCEL ended the stream.
    Cancelled(Handle),
    /// The source failed or the bounded queue overflowed, which ended the
    /// stream.
    Aborted(Handle),
}

impl Entry<'_> {
    /// Returns the handle of the stream the entry belongs to.
    #[must_use]
    pub const fn handle(&self) -> Handle {
        match *self {
            Entry::Next(handle, _)
            | Entry::Complete(handle)
            | Entry::Cancelled(handle)
            | Entry::Aborted(handle) => handle,
        }
    }
}

/// The open list of entries of a stream body that a writer has begun.
///
/// [`write_head`] and [`write_batch`] return it, [`write_entry`] adds to it,
/// and [`close`] ends the body.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Events {
    /// The count of entries written, which decides the comma before the next.
    count: usize,
}

impl Events {
    /// Returns the count of entries written.
    #[must_use]
    pub fn count(&self) -> usize {
        self.count
    }
}

/// Writes the opening of the head of a stream, the body of the answer 200.3,
/// into `sink`.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn write_head(sink: &mut dyn Sink, handle: Handle, seq: Seq) -> Result<Events, Full> {
    sink.put(b"{\"stream\":")?;
    put_decimal(sink, u32::from(handle.get()))?;
    sink.put(b",\"seq\":")?;
    put_decimal(sink, seq.get())?;
    sink.put(b",\"events\":[")?;
    Ok(Events { count: 0 })
}

/// Writes the opening of a batch, the body of the answer 200.0 of a PULL or a
/// CANCEL, into `sink`.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn write_batch(sink: &mut dyn Sink, seq: Seq) -> Result<Events, Full> {
    sink.put(b"{\"seq\":")?;
    put_decimal(sink, seq.get())?;
    sink.put(b",\"events\":[")?;
    Ok(Events { count: 0 })
}

/// Writes the entry `entry` into the open body `events` in `sink`.
///
/// The bytes of a `next` value are written as they are, since they are the JSON
/// value the service wrote, so they carry no whitespace around the value, which a
/// sender never writes into a stream body.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn write_entry(
    sink: &mut dyn Sink,
    events: &mut Events,
    entry: &Entry<'_>,
) -> Result<(), Full> {
    if events.count > 0 {
        sink.put(b",")?;
    }
    sink.put(b"{\"stream\":")?;
    put_decimal(sink, u32::from(entry.handle().get()))?;
    match *entry {
        Entry::Next(_, value) => {
            sink.put(b",\"next\":")?;
            sink.put(value)?;
            sink.put(b"}")?;
        }
        Entry::Complete(_) => sink.put(b",\"complete\":true}")?,
        Entry::Cancelled(_) => sink.put(b",\"cancelled\":true}")?,
        Entry::Aborted(_) => sink.put(b",\"aborted\":true}")?,
    }
    events.count += 1;
    Ok(())
}

/// Closes the open body `events` in `sink` and returns the count of entries
/// the body holds.
///
/// # Errors
///
/// Returns [`Full`] when `sink` has no room.
pub fn close(sink: &mut dyn Sink, events: Events) -> Result<usize, Full> {
    sink.put(b"]}")?;
    Ok(events.count)
}

/// Puts the decimal digits of `value` into `sink`, with no sign and no leading
/// zero.
fn put_decimal(sink: &mut dyn Sink, value: u32) -> Result<(), Full> {
    let mut digits = [0u8; 10];
    let mut len = 0;
    let mut rest = value;
    for slot in digits.iter_mut().rev() {
        *slot = b'0' + u8::try_from(rest % 10).unwrap_or(0);
        len += 1;
        rest /= 10;
        if rest == 0 {
            break;
        }
    }
    sink.put(digits.get(digits.len() - len..).unwrap_or_default())
}

/// The reader of a plain stream body, an iterator over its entries.
///
/// [`Entries::new`] reads the object around the entries, the handle of a head,
/// the batch number and where the list `events` stands, and each step of the
/// iterator reads one entry. A body is malformed when it is no object, lacks
/// `seq` or `events`, carries a known key twice, a handle outside 1 to 65,535
/// or a batch number outside 32 bits, or holds bytes after the object. A number
/// is read as a sender writes it, digits with no sign, no leading zero, no
/// fraction and no exponent. An entry is malformed when it lacks `stream`, when
/// it carries none or more than one of `next`, `complete`, `cancelled` and
/// `aborted`, or when one of the last three is not `true`. After a malformed
/// entry the iterator ends.
#[derive(Clone, Debug)]
pub struct Entries<'a> {
    /// The handle of a head, `None` for a batch.
    handle: Option<Handle>,
    /// The batch number.
    seq: Seq,
    /// The list `events`, from its opening bracket to its end.
    list: Cursor<'a>,
    /// Whether the iterator has read the opening bracket.
    started: bool,
    /// Whether the iterator has ended.
    done: bool,
}

impl<'a> Entries<'a> {
    /// Returns the reader of the plain stream body `body`.
    ///
    /// # Errors
    ///
    /// Returns [`WireError::Stream`] when the object around the entries is
    /// malformed.
    pub fn new(body: &'a [u8]) -> Result<Entries<'a>, WireError> {
        let mut cursor = Cursor { bytes: body, at: 0 };
        let mut handle = None;
        let mut seq = None;
        let mut list = None;
        cursor.object(|key, cursor| {
            if key_is(key, b"stream") {
                let value = cursor.integer(u16::MAX.into())?;
                let value = u16::try_from(value).map_err(|_| WireError::Stream)?;
                once(&mut handle, Handle::new(value).ok_or(WireError::Stream)?)
            } else if key_is(key, b"seq") {
                once(&mut seq, Seq(cursor.integer(u32::MAX)?))
            } else if key_is(key, b"events") {
                if cursor.peek() != Some(b'[') {
                    return Err(WireError::Stream);
                }
                once(&mut list, cursor.value()?)
            } else {
                cursor.value().map(|_| ())
            }
        })?;
        cursor.space();
        if cursor.peek().is_some() {
            return Err(WireError::Stream);
        }
        let (Some(seq), Some(list)) = (seq, list) else {
            return Err(WireError::Stream);
        };
        Ok(Entries {
            handle,
            seq,
            list: Cursor { bytes: list, at: 0 },
            started: false,
            done: false,
        })
    }

    /// Returns the handle of the head of a stream, or `None` for a batch.
    #[must_use]
    pub fn handle(&self) -> Option<Handle> {
        self.handle
    }

    /// Returns the batch number of the body.
    #[must_use]
    pub fn seq(&self) -> Seq {
        self.seq
    }

    /// Reads the next entry, or `None` at the end of the list.
    fn step(&mut self) -> Result<Option<Entry<'a>>, WireError> {
        let list = &mut self.list;
        if self.started {
            list.space();
            match list.take() {
                Some(b']') => return Ok(None),
                Some(b',') => list.space(),
                _ => return Err(WireError::Stream),
            }
        } else {
            self.started = true;
            list.expect(b'[')?;
            list.space();
            if list.peek() == Some(b']') {
                return Ok(None);
            }
        }
        entry(list).map(Some)
    }
}

impl<'a> Iterator for Entries<'a> {
    type Item = Result<Entry<'a>, WireError>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }
        let step = self.step();
        self.done = !matches!(step, Ok(Some(_)));
        step.transpose()
    }
}

/// What an entry names besides its handle.
#[derive(Clone, Copy)]
enum Mark<'a> {
    /// The key `next` with the bytes of its value.
    Next(&'a [u8]),
    /// The key `complete`.
    Complete,
    /// The key `cancelled`.
    Cancelled,
    /// The key `aborted`.
    Aborted,
}

/// Reads one entry object at `list`.
fn entry<'a>(list: &mut Cursor<'a>) -> Result<Entry<'a>, WireError> {
    let mut handle = None;
    let mut mark = None;
    list.object(|key, cursor| {
        if key_is(key, b"stream") {
            let value = cursor.integer(u16::MAX.into())?;
            let value = u16::try_from(value).map_err(|_| WireError::Stream)?;
            once(&mut handle, Handle::new(value).ok_or(WireError::Stream)?)
        } else if key_is(key, b"next") {
            once(&mut mark, Mark::Next(cursor.value()?))
        } else if key_is(key, b"complete") {
            cursor.literal_true()?;
            once(&mut mark, Mark::Complete)
        } else if key_is(key, b"cancelled") {
            cursor.literal_true()?;
            once(&mut mark, Mark::Cancelled)
        } else if key_is(key, b"aborted") {
            cursor.literal_true()?;
            once(&mut mark, Mark::Aborted)
        } else {
            cursor.value().map(|_| ())
        }
    })?;
    let (Some(handle), Some(mark)) = (handle, mark) else {
        return Err(WireError::Stream);
    };
    Ok(match mark {
        Mark::Next(value) => Entry::Next(handle, value),
        Mark::Complete => Entry::Complete(handle),
        Mark::Cancelled => Entry::Cancelled(handle),
        Mark::Aborted => Entry::Aborted(handle),
    })
}

/// Sets `slot` to `value`, or refuses a key that stands twice.
fn once<T>(slot: &mut Option<T>, value: T) -> Result<(), WireError> {
    if slot.replace(value).is_some() {
        return Err(WireError::Stream);
    }
    Ok(())
}

/// Tells whether the key `raw`, the bytes between the quotes of a JSON string,
/// names the ASCII key `name`.
///
/// An escape of the string is read as JSON reads it, so `"seq"` names `seq`.
fn key_is(raw: &[u8], name: &[u8]) -> bool {
    let mut rest = raw;
    let mut want = name.iter();
    while let [first, tail @ ..] = rest {
        let (byte, after) = match (*first, tail) {
            (b'\\', [b'u', a, b, c, d, after @ ..]) => match (hex(*a), hex(*b), hex(*c), hex(*d)) {
                (Some(0), Some(0), Some(high), Some(low)) if high < 8 => (high << 4 | low, after),
                _ => return false,
            },
            (b'\\', [escaped, after @ ..]) => match *escaped {
                b'"' | b'\\' | b'/' => (*escaped, after),
                _ => return false,
            },
            (byte, after) => (byte, after),
        };
        if want.next() != Some(&byte) {
            return false;
        }
        rest = after;
    }
    want.next().is_none()
}

/// Returns the value of the hex digit `digit`.
fn hex(digit: u8) -> Option<u8> {
    char::from(digit)
        .to_digit(16)
        .and_then(|value| u8::try_from(value).ok())
}

/// A position in the bytes of a stream body.
#[derive(Clone, Debug)]
struct Cursor<'a> {
    /// The bytes read.
    bytes: &'a [u8],
    /// The index of the next byte.
    at: usize,
}

impl<'a> Cursor<'a> {
    /// Returns the next byte, or `None` at the end.
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }

    /// Returns the next byte and steps past it.
    fn take(&mut self) -> Option<u8> {
        let byte = self.peek()?;
        self.at += 1;
        Some(byte)
    }

    /// Steps past the next byte, which must be `byte`.
    fn expect(&mut self, byte: u8) -> Result<(), WireError> {
        if self.take() != Some(byte) {
            return Err(WireError::Stream);
        }
        Ok(())
    }

    /// Steps past the whitespace of JSON: space, tab, line feed and carriage
    /// return.
    fn space(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\n' | b'\r')) {
            self.at += 1;
        }
    }

    /// Reads an object and calls `member` with the raw bytes of each key and the
    /// cursor at its value, which `member` steps past.
    fn object(
        &mut self,
        mut member: impl FnMut(&'a [u8], &mut Cursor<'a>) -> Result<(), WireError>,
    ) -> Result<(), WireError> {
        self.space();
        self.expect(b'{')?;
        self.space();
        if self.peek() == Some(b'}') {
            self.at += 1;
            return Ok(());
        }
        loop {
            self.space();
            let key = self.string()?;
            self.space();
            self.expect(b':')?;
            self.space();
            member(key, self)?;
            self.space();
            match self.take() {
                Some(b',') => {}
                Some(b'}') => return Ok(()),
                _ => return Err(WireError::Stream),
            }
        }
    }

    /// Reads a string and returns the bytes between its quotes.
    fn string(&mut self) -> Result<&'a [u8], WireError> {
        self.expect(b'"')?;
        let start = self.at;
        loop {
            match self.take() {
                Some(b'"') => break,
                Some(b'\\') => {
                    self.take().ok_or(WireError::Stream)?;
                }
                Some(_) => {}
                None => return Err(WireError::Stream),
            }
        }
        self.bytes.get(start..self.at - 1).ok_or(WireError::Stream)
    }

    /// Reads one value and returns its bytes.
    ///
    /// A string ends at its closing quote, an object or an array at the bracket
    /// that brings the depth back to 0, and any other value at the byte that
    /// ends a value in its place. The value is sliced and not read as JSON.
    fn value(&mut self) -> Result<&'a [u8], WireError> {
        let start = self.at;
        match self.peek() {
            Some(b'"') => {
                self.string()?;
            }
            Some(b'{' | b'[') => self.nested()?,
            _ => {
                while !matches!(
                    self.peek(),
                    None | Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\n' | b'\r')
                ) {
                    self.at += 1;
                }
                if self.at == start {
                    return Err(WireError::Stream);
                }
            }
        }
        self.bytes.get(start..self.at).ok_or(WireError::Stream)
    }

    /// Steps past an object or an array by the depth of its brackets, aware of
    /// the strings inside it.
    fn nested(&mut self) -> Result<(), WireError> {
        let mut depth = 0usize;
        loop {
            match self.peek() {
                Some(b'"') => {
                    self.string()?;
                    continue;
                }
                Some(b'{' | b'[') => depth += 1,
                Some(b'}' | b']') => depth -= 1,
                Some(_) => {}
                None => return Err(WireError::Stream),
            }
            self.at += 1;
            if depth == 0 {
                return Ok(());
            }
        }
    }

    /// Reads an integer of 0 to `max` as a sender writes it.
    fn integer(&mut self, max: u32) -> Result<u32, WireError> {
        let digits = self.value()?;
        let well_formed = match digits {
            [b'0'] => true,
            [b'1'..=b'9', rest @ ..] => rest.iter().all(u8::is_ascii_digit),
            _ => false,
        };
        if !well_formed || digits.len() > 10 {
            return Err(WireError::Stream);
        }
        let value = digits
            .iter()
            .fold(0u64, |value, &digit| value * 10 + u64::from(digit - b'0'));
        u32::try_from(value)
            .ok()
            .filter(|value| *value <= max)
            .ok_or(WireError::Stream)
    }

    /// Reads the literal `true`.
    fn literal_true(&mut self) -> Result<(), WireError> {
        if self.value()? != b"true" {
            return Err(WireError::Stream);
        }
        Ok(())
    }
}

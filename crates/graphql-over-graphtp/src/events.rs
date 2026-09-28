// SPDX-License-Identifier: MIT
/*
 * graphql-over-graphtp events: the reader of an event stream that turns its
 * events into the entries of a GraphTP stream.
 *
 * No analogue in the reference. The parse is that of the event stream format
 * of the HTML standard, section 9.2.6, Interpreting an event stream, and the
 * mapping is docs/graphtp.md section 7, step 4: the bytes after "data: " of
 * each next event become a next entry, the complete event complete, and the
 * comments and pings of the stream are dropped.
 */

//! The reader of an event stream.
//!
//! The reader takes the bytes of a stream in pieces of any size and yields
//! its events in order. A line ends at CR LF, LF or CR, a line that starts
//! with a colon is a comment, a field `event` sets the event's type, each
//! field `data` adds its value and a line feed to the event's data, and an
//! empty line ends the event, whose data loses its last line feed. An event
//! with no data line is dropped. Yoga writes each result as an event `next`
//! whose one data line holds the JSON, and the end as an event `complete`
//! with an empty data line.

use alloc::{
    collections::VecDeque,
    vec::Vec, //
};

/// An event of the stream as the listener reads it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Event {
    /// An event `next` with its data, one execution result.
    Next(Vec<u8>),
    /// An event `complete`, which ends the stream.
    Complete,
}

/// The reader of one event stream.
#[derive(Debug, Default)]
pub(crate) struct Reader {
    /// The bytes of the line not yet ended.
    line: Vec<u8>,
    /// Whether the last byte was a CR, so a LF after it ends no line.
    after_cr: bool,
    /// Whether the first bytes of the stream have been read, so a byte order
    /// mark is no longer skipped.
    started: bool,
    /// The type of the event not yet ended.
    kind: Vec<u8>,
    /// The data of the event not yet ended.
    data: Vec<u8>,
    /// Whether the event not yet ended has a data line.
    has_data: bool,
    /// The events ended and not yet taken.
    ready: VecDeque<Event>,
}

impl Reader {
    /// Reads the bytes `bytes`, the next piece of the stream.
    // An unaligned word read through a raw pointer would find CR and LF a word at a time.
    pub(crate) fn push(&mut self, mut bytes: &[u8]) {
        if !self.started && !bytes.is_empty() {
            self.started = true;
            bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
        }
        for &byte in bytes {
            match byte {
                b'\n' if self.after_cr => self.after_cr = false,
                b'\n' => self.end_line(),
                b'\r' => {
                    self.end_line();
                    self.after_cr = true;
                }
                _ => {
                    self.after_cr = false;
                    self.line.push(byte);
                }
            }
        }
    }

    /// Returns the next event ended and not yet taken.
    pub(crate) fn next(&mut self) -> Option<Event> {
        self.ready.pop_front()
    }

    /// Ends the current line.
    fn end_line(&mut self) {
        let line = core::mem::take(&mut self.line);
        if line.is_empty() {
            self.dispatch();
            return;
        }
        if line.first() == Some(&b':') {
            return;
        }
        let (field, value) = match line.iter().position(|&byte| byte == b':') {
            Some(colon) => {
                let value = line.get(colon + 1..).unwrap_or_default();
                let value = value.strip_prefix(b" ").unwrap_or(value);
                (line.get(..colon).unwrap_or_default(), value)
            }
            None => (line.as_slice(), &[][..]),
        };
        match field {
            b"event" => {
                self.kind.clear();
                self.kind.extend_from_slice(value);
            }
            b"data" => {
                self.data.extend_from_slice(value);
                self.data.push(b'\n');
                self.has_data = true;
            }
            _ => {}
        }
    }

    /// Ends the current event.
    fn dispatch(&mut self) {
        let kind = core::mem::take(&mut self.kind);
        let mut data = core::mem::take(&mut self.data);
        if !core::mem::take(&mut self.has_data) {
            return;
        }
        data.pop();
        match kind.as_slice() {
            b"next" => self.ready.push_back(Event::Next(data)),
            b"complete" => self.ready.push_back(Event::Complete),
            _ => {}
        }
    }
}

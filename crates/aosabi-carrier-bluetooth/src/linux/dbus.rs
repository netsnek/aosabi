// SPDX-License-Identifier: MIT
/*
 * aosabi-carrier-bluetooth linux dbus: a client of the D-Bus system bus
 * written in the project, the wire format of the messages BlueZ exchanges
 * over UnixStream.
 *
 * No analogue on the reference side.
 */

//! A client of the system bus.
//!
//! [`Bus`] connects to the system bus over a Unix stream socket,
//! authenticates with `EXTERNAL` and the uid of the process, which the
//! kernel checks against the credentials of the socket, says `Hello`, and
//! then moves [`Message`]s: [`Bus::call`] sends a method call and waits for
//! its reply, keeping every other message it meets for [`Bus::next_message`],
//! [`Bus::send`] sends a message and returns its serial, and [`Bus::next_message`]
//! returns the next method call to an object this process exports or a
//! signal it watches, without blocking. The client passes no file
//! descriptor, so the carrier takes `BlueZ`'s values through `WriteValue` and
//! gives its own through `PropertiesChanged`, and no call needs `unsafe`.
//!
//! A message is little endian, and a [`Value`] carries every type `BlueZ`'s
//! GATT API names: `y`, `b`, `n`, `q`, `i`, `u`, `x`, `t`, `s`, `o`, `g`,
//! arrays, structs, dictionaries and variants. Every length and every nested
//! signature read from the bus is checked against the bytes that remain, and
//! a message that breaks a rule is refused with `InvalidData`, since these
//! are the bytes the carrier parses from a process it does not control.

use alloc::{
    boxed::Box,
    collections::VecDeque,
    format,
    string::{
        String,
        ToString, //
    },
    vec,
    vec::Vec, //
};
use core::fmt::Write as _;
use std::{
    env,
    fs,
    io::{
        self,
        ErrorKind,
        Read as _,
        Write as _, //
    },
    os::{
        fd::{
            AsFd,
            BorrowedFd, //
        },
        unix::net::UnixStream, //
    },
    time::{
        Duration,
        Instant, //
    }, //
};

/// The socket of the system bus where the environment names none.
const SYSTEM_BUS: &str = "/run/dbus/system_bus_socket";

/// The variable that names the address of the system bus.
const SYSTEM_BUS_ENV: &str = "DBUS_SYSTEM_BUS_ADDRESS";

/// The longest message the client reads, the bound of the specification, kept
/// far lower since `BlueZ`'s messages of the GATT API hold a few hundred
/// bytes.
const MAX_MESSAGE: usize = 1 << 20;

/// The deepest nesting of containers the client reads.
const MAX_DEPTH: usize = 64;

/// How long [`Bus::call`] waits for its reply.
const CALL_TIMEOUT: Duration = Duration::from_secs(25);

/// The bus name of the message bus itself.
const BUS_NAME: &str = "org.freedesktop.DBus";

/// The kind of a message.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A method call, which a reply answers.
    MethodCall,
    /// The reply of a method call.
    MethodReturn,
    /// The error that answers a method call.
    Error,
    /// A signal.
    Signal,
}

impl Kind {
    /// Returns the byte of the kind on the wire.
    const fn byte(self) -> u8 {
        match self {
            Self::MethodCall => 1,
            Self::MethodReturn => 2,
            Self::Error => 3,
            Self::Signal => 4,
        }
    }

    /// Returns the kind of a byte on the wire.
    const fn of(byte: u8) -> Option<Self> {
        match byte {
            1 => Some(Self::MethodCall),
            2 => Some(Self::MethodReturn),
            3 => Some(Self::Error),
            4 => Some(Self::Signal),
            _ => None,
        }
    }
}

/// The flag of a method call that asks for no reply.
pub const NO_REPLY_EXPECTED: u8 = 0x1;

/// One value of the D-Bus type system.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    /// `y`.
    Byte(u8),
    /// `b`.
    Bool(bool),
    /// `q`.
    U16(u16),
    /// `n`.
    I16(i16),
    /// `u`.
    U32(u32),
    /// `i`.
    I32(i32),
    /// `t`.
    U64(u64),
    /// `x`.
    I64(i64),
    /// `s`.
    Str(String),
    /// `o`.
    Path(String),
    /// `g`.
    Signature(String),
    /// An array of values of the element signature.
    Array(String, Vec<Value>),
    /// A struct.
    Struct(Vec<Value>),
    /// A dictionary of the key and the value signatures.
    Dict(String, String, Vec<(Value, Value)>),
    /// A variant.
    Variant(Box<Value>),
}

impl Value {
    /// Returns the signature of the value.
    #[must_use]
    pub fn signature(&self) -> String {
        match self {
            Self::Byte(_) => "y".to_string(),
            Self::Bool(_) => "b".to_string(),
            Self::U16(_) => "q".to_string(),
            Self::I16(_) => "n".to_string(),
            Self::U32(_) => "u".to_string(),
            Self::I32(_) => "i".to_string(),
            Self::U64(_) => "t".to_string(),
            Self::I64(_) => "x".to_string(),
            Self::Str(_) => "s".to_string(),
            Self::Path(_) => "o".to_string(),
            Self::Signature(_) => "g".to_string(),
            Self::Array(element, _) => format!("a{element}"),
            Self::Struct(fields) => {
                let inner: String = fields.iter().map(Self::signature).collect();
                format!("({inner})")
            }
            Self::Dict(key, value, _) => format!("a{{{key}{value}}}"),
            Self::Variant(_) => "v".to_string(),
        }
    }

    /// Returns the text of a string, a path or a signature.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Str(text) | Self::Path(text) | Self::Signature(text) => Some(text),
            _ => None,
        }
    }

    /// Returns the bytes of an array of bytes.
    #[must_use]
    pub fn as_bytes(&self) -> Option<Vec<u8>> {
        let Self::Array(element, items) = self else {
            return None;
        };
        if element != "y" {
            return None;
        }
        items
            .iter()
            .map(|item| match item {
                Self::Byte(byte) => Some(*byte),
                _ => None,
            })
            .collect()
    }

    /// Returns the value a variant holds, or the value itself.
    #[must_use]
    pub fn inner(&self) -> &Self {
        match self {
            Self::Variant(inner) => inner.inner(),
            other => other,
        }
    }

    /// Returns the value of the key `key` of a dictionary whose keys are
    /// strings.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Self> {
        let Self::Dict(_, _, entries) = self else {
            return None;
        };
        entries
            .iter()
            .find(|(name, _)| name.as_str() == Some(key))
            .map(|(_, value)| value.inner())
    }

    /// Returns an array of bytes.
    #[must_use]
    pub fn bytes(bytes: &[u8]) -> Self {
        Self::Array(
            "y".to_string(),
            bytes.iter().copied().map(Self::Byte).collect(),
        )
    }

    /// Returns an array of strings.
    #[must_use]
    pub fn strings(items: &[&str]) -> Self {
        Self::Array(
            "s".to_string(),
            items
                .iter()
                .map(|item| Self::Str((*item).to_string()))
                .collect(),
        )
    }

    /// Returns a dictionary of `a{sv}` from pairs of a name and a value.
    #[must_use]
    pub fn properties(entries: Vec<(&str, Self)>) -> Self {
        Self::Dict(
            "s".to_string(),
            "v".to_string(),
            entries
                .into_iter()
                .map(|(name, value)| (Self::Str(name.to_string()), Self::Variant(Box::new(value))))
                .collect(),
        )
    }
}

/// One message of the D-Bus wire format.
#[derive(Clone, Debug, PartialEq)]
pub struct Message {
    /// The kind.
    pub kind: Kind,
    /// The flags.
    pub flags: u8,
    /// The serial the sender gave it.
    pub serial: u32,
    /// The object path of a call or a signal.
    pub path: Option<String>,
    /// The interface of a call or a signal.
    pub interface: Option<String>,
    /// The member of a call or a signal.
    pub member: Option<String>,
    /// The name of an error.
    pub error_name: Option<String>,
    /// The serial of the call a reply or an error answers.
    pub reply_serial: Option<u32>,
    /// The bus name the message goes to.
    pub destination: Option<String>,
    /// The unique name of the sender, which the bus writes.
    pub sender: Option<String>,
    /// The values of the body.
    pub body: Vec<Value>,
}

impl Message {
    /// Returns a method call of `member` of `interface` on the object `path`
    /// of `destination` with the values `body`.
    #[must_use]
    pub fn call(
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: Vec<Value>,
    ) -> Self {
        Self {
            kind: Kind::MethodCall,
            flags: 0,
            serial: 0,
            path: Some(path.to_string()),
            interface: Some(interface.to_string()),
            member: Some(member.to_string()),
            error_name: None,
            reply_serial: None,
            destination: Some(destination.to_string()),
            sender: None,
            body,
        }
    }

    /// Returns a signal `member` of `interface` of the object `path` with the
    /// values `body`.
    #[must_use]
    pub fn signal(path: &str, interface: &str, member: &str, body: Vec<Value>) -> Self {
        Self {
            kind: Kind::Signal,
            flags: 0,
            serial: 0,
            path: Some(path.to_string()),
            interface: Some(interface.to_string()),
            member: Some(member.to_string()),
            error_name: None,
            reply_serial: None,
            destination: None,
            sender: None,
            body,
        }
    }

    /// Returns the reply to the method call `call` with the values `body`.
    #[must_use]
    pub fn reply(call: &Self, body: Vec<Value>) -> Self {
        Self {
            kind: Kind::MethodReturn,
            flags: 0,
            serial: 0,
            path: None,
            interface: None,
            member: None,
            error_name: None,
            reply_serial: Some(call.serial),
            destination: call.sender.clone(),
            sender: None,
            body,
        }
    }

    /// Returns the error `name` with the text `text` that answers the method
    /// call `call`.
    #[must_use]
    pub fn error(call: &Self, name: &str, text: &str) -> Self {
        Self {
            kind: Kind::Error,
            error_name: Some(name.to_string()),
            ..Self::reply(call, vec![Value::Str(text.to_string())])
        }
    }

    /// Returns whether the message is a method call that waits for a reply.
    #[must_use]
    pub fn wants_reply(&self) -> bool {
        self.kind == Kind::MethodCall && self.flags & NO_REPLY_EXPECTED == 0
    }

    /// Returns whether the message is a call of `member` of `interface`.
    #[must_use]
    pub fn is_call(&self, interface: &str, member: &str) -> bool {
        self.kind == Kind::MethodCall
            && self.interface.as_deref() == Some(interface)
            && self.member.as_deref() == Some(member)
    }

    /// Returns the signature of the body.
    #[must_use]
    pub fn signature(&self) -> String {
        self.body.iter().map(Value::signature).collect()
    }

    /// Writes the message with the serial `serial` in the wire format.
    #[must_use]
    pub fn encode(&self, serial: u32) -> Vec<u8> {
        let mut body = Writer::default();
        for value in &self.body {
            body.value(value);
        }
        let mut fields: Vec<Value> = Vec::new();
        let mut field = |code: u8, value: Value| {
            fields.push(Value::Struct(vec![
                Value::Byte(code),
                Value::Variant(Box::new(value)),
            ]));
        };
        if let Some(path) = &self.path {
            field(1, Value::Path(path.clone()));
        }
        if let Some(interface) = &self.interface {
            field(2, Value::Str(interface.clone()));
        }
        if let Some(member) = &self.member {
            field(3, Value::Str(member.clone()));
        }
        if let Some(name) = &self.error_name {
            field(4, Value::Str(name.clone()));
        }
        if let Some(reply) = self.reply_serial {
            field(5, Value::U32(reply));
        }
        if let Some(destination) = &self.destination {
            field(6, Value::Str(destination.clone()));
        }
        if !self.body.is_empty() {
            field(8, Value::Signature(self.signature()));
        }
        let mut out = Writer::default();
        out.bytes
            .extend_from_slice(&[b'l', self.kind.byte(), self.flags, 1]);
        out.u32(u32::try_from(body.bytes.len()).unwrap_or(u32::MAX));
        out.u32(serial);
        out.value(&Value::Array("(yv)".to_string(), fields));
        out.pad(8);
        out.bytes.extend_from_slice(&body.bytes);
        out.bytes
    }

    /// Reads one message from the front of `bytes` and returns it with the
    /// bytes it took, or `None` where `bytes` holds less than one message.
    ///
    /// # Errors
    ///
    /// Returns an error of the kind `InvalidData` for a message that breaks a
    /// rule of the wire format.
    pub fn decode(bytes: &[u8]) -> io::Result<Option<(Self, usize)>> {
        if bytes.len() < 16 {
            return Ok(None);
        }
        if bytes[0] != b'l' || bytes[3] != 1 {
            return Err(invalid("a message that is not little endian of version 1"));
        }
        let body_len = u32_at(bytes, 4) as usize;
        let fields_len = u32_at(bytes, 12) as usize;
        let head = 16usize
            .checked_add(fields_len)
            .map(|end| end.next_multiple_of(8))
            .ok_or_else(|| invalid("a header too long"))?;
        let total = head
            .checked_add(body_len)
            .filter(|&total| total <= MAX_MESSAGE)
            .ok_or_else(|| invalid("a message too long"))?;
        if bytes.len() < total {
            return Ok(None);
        }
        let kind = Kind::of(bytes[1]).ok_or_else(|| invalid("an unknown kind"))?;
        let mut reader = Reader::new(&bytes[..head], 12);
        let Value::Array(_, fields) = reader.value("a(yv)", 0)? else {
            return Err(invalid("header fields that are no array"));
        };
        let mut message = Self {
            kind,
            flags: bytes[2],
            serial: u32_at(bytes, 8),
            path: None,
            interface: None,
            member: None,
            error_name: None,
            reply_serial: None,
            destination: None,
            sender: None,
            body: Vec::new(),
        };
        let mut signature = String::new();
        for field in fields {
            let Value::Struct(parts) = field else {
                continue;
            };
            let (Some(Value::Byte(code)), Some(value)) = (parts.first(), parts.get(1)) else {
                continue;
            };
            let value = value.inner();
            let text = value.as_str().map(ToString::to_string);
            match code {
                1 => message.path = text,
                2 => message.interface = text,
                3 => message.member = text,
                4 => message.error_name = text,
                5 => {
                    if let Value::U32(serial) = value {
                        message.reply_serial = Some(*serial);
                    }
                }
                6 => message.destination = text,
                7 => message.sender = text,
                8 => signature = text.unwrap_or_default(),
                _ => {}
            }
        }
        let mut body = Reader::new(&bytes[head..total], 0);
        for single in split_signature(&signature)? {
            message.body.push(body.value(single, 0)?);
        }
        Ok(Some((message, total)))
    }
}

/// Returns an error of the kind `InvalidData` with the reason `why`.
fn invalid(why: &str) -> io::Error {
    io::Error::new(ErrorKind::InvalidData, format!("D-Bus: {why}"))
}

/// Returns the little endian `u32` at `at`, which the caller has checked
/// lies inside `bytes`.
fn u32_at(bytes: &[u8], at: usize) -> u32 {
    let mut word = [0u8; 4];
    word.copy_from_slice(&bytes[at..at + 4]);
    u32::from_le_bytes(word)
}

/// Returns the alignment of the type that opens `signature`.
fn alignment(signature: &str) -> usize {
    match signature.as_bytes().first() {
        Some(b'y' | b'g' | b'v') => 1,
        Some(b'n' | b'q') => 2,
        Some(b'x' | b't' | b'd' | b'(' | b'{') => 8,
        _ => 4,
    }
}

/// Returns the length of the one complete type that opens `signature`.
fn single_len(signature: &str) -> io::Result<usize> {
    let bytes = signature.as_bytes();
    match bytes.first() {
        Some(b'a') => Ok(1 + single_len(&signature[1..])?),
        Some(open @ (b'(' | b'{')) => {
            let close = if *open == b'(' { b')' } else { b'}' };
            let mut at = 1;
            while bytes.get(at).is_some_and(|&byte| byte != close) {
                at += single_len(&signature[at..])?;
            }
            if bytes.get(at) == Some(&close) {
                Ok(at + 1)
            } else {
                Err(invalid("an unclosed container in a signature"))
            }
        }
        Some(
            b'y' | b'b' | b'n' | b'q' | b'i' | b'u' | b'x' | b't' | b'd' | b's' | b'o' | b'g'
            | b'v' | b'h',
        ) => Ok(1),
        _ => Err(invalid("an unknown type in a signature")),
    }
}

/// Splits a signature into its complete types.
fn split_signature(signature: &str) -> io::Result<Vec<&str>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < signature.len() {
        let len = single_len(&signature[at..])?;
        out.push(&signature[at..at + len]);
        at += len;
    }
    Ok(out)
}

/// The writer of the wire format, whose offsets count from the start of the
/// message or the body it writes.
#[derive(Default)]
struct Writer {
    /// The bytes written.
    bytes: Vec<u8>,
}

impl Writer {
    /// Pads to a multiple of `alignment`.
    fn pad(&mut self, alignment: usize) {
        while !self.bytes.len().is_multiple_of(alignment) {
            self.bytes.push(0);
        }
    }

    /// Writes an aligned `u32`.
    fn u32(&mut self, value: u32) {
        self.pad(4);
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    /// Writes a string or a path.
    fn string(&mut self, text: &str) {
        self.u32(u32::try_from(text.len()).unwrap_or(u32::MAX));
        self.bytes.extend_from_slice(text.as_bytes());
        self.bytes.push(0);
    }

    /// Writes a signature.
    fn signature(&mut self, text: &str) {
        self.bytes.push(u8::try_from(text.len()).unwrap_or(u8::MAX));
        self.bytes.extend_from_slice(text.as_bytes());
        self.bytes.push(0);
    }

    /// Writes one value.
    fn value(&mut self, value: &Value) {
        match value {
            Value::Byte(byte) => self.bytes.push(*byte),
            Value::Bool(flag) => self.u32(u32::from(*flag)),
            Value::U16(number) => self.fixed(2, &number.to_le_bytes()),
            Value::I16(number) => self.fixed(2, &number.to_le_bytes()),
            Value::U32(number) => self.u32(*number),
            Value::I32(number) => self.fixed(4, &number.to_le_bytes()),
            Value::U64(number) => self.fixed(8, &number.to_le_bytes()),
            Value::I64(number) => self.fixed(8, &number.to_le_bytes()),
            Value::Str(text) | Value::Path(text) => self.string(text),
            Value::Signature(text) => self.signature(text),
            Value::Array(element, items) => self.array(element, items.iter().map(Item::One)),
            Value::Struct(fields) => {
                self.pad(8);
                for field in fields {
                    self.value(field);
                }
            }
            Value::Dict(key, value, entries) => {
                let element = format!("{{{key}{value}}}");
                self.array(
                    &element,
                    entries.iter().map(|(key, value)| Item::Pair(key, value)),
                );
            }
            Value::Variant(inner) => {
                self.signature(&inner.signature());
                self.value(inner);
            }
        }
    }

    /// Writes a fixed width number aligned to its width.
    fn fixed(&mut self, width: usize, bytes: &[u8]) {
        self.pad(width);
        self.bytes.extend_from_slice(bytes);
    }

    /// Writes an array of `items` of the element signature `element`, whose
    /// length counts the bytes after the padding of the first element.
    fn array<'v>(&mut self, element: &str, items: impl Iterator<Item = Item<'v>>) {
        self.u32(0);
        let length_at = self.bytes.len() - 4;
        self.pad(alignment(element));
        let start = self.bytes.len();
        for item in items {
            match item {
                Item::One(value) => self.value(value),
                Item::Pair(key, value) => {
                    self.pad(8);
                    self.value(key);
                    self.value(value);
                }
            }
        }
        let length = u32::try_from(self.bytes.len() - start).unwrap_or(u32::MAX);
        self.bytes[length_at..length_at + 4].copy_from_slice(&length.to_le_bytes());
    }
}

/// One element of an array the writer writes.
enum Item<'v> {
    /// A value.
    One(&'v Value),
    /// The key and the value of a dictionary entry.
    Pair(&'v Value, &'v Value),
}

/// The reader of the wire format.
struct Reader<'b> {
    /// The bytes of the message or of its body.
    bytes: &'b [u8],
    /// The offset of the next byte.
    at: usize,
}

impl<'b> Reader<'b> {
    /// Returns a reader of `bytes` from `at`.
    fn new(bytes: &'b [u8], at: usize) -> Self {
        Self { bytes, at }
    }

    /// Skips the padding to a multiple of `alignment`.
    fn pad(&mut self, alignment: usize) -> io::Result<()> {
        let end = self.at.next_multiple_of(alignment);
        if end > self.bytes.len() {
            return Err(invalid("padding past the end"));
        }
        self.at = end;
        Ok(())
    }

    /// Takes `len` bytes.
    fn take(&mut self, len: usize) -> io::Result<&'b [u8]> {
        let end = self
            .at
            .checked_add(len)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| invalid("a value past the end"))?;
        let taken = &self.bytes[self.at..end];
        self.at = end;
        Ok(taken)
    }

    /// Takes an aligned fixed width number of `N` bytes.
    fn fixed<const N: usize>(&mut self) -> io::Result<[u8; N]> {
        self.pad(N)?;
        let mut out = [0u8; N];
        out.copy_from_slice(self.take(N)?);
        Ok(out)
    }

    /// Takes a string of a length of `len_width` bytes.
    fn text(&mut self, len_width: usize) -> io::Result<String> {
        let len = if len_width == 1 {
            usize::from(self.take(1)?[0])
        } else {
            u32::from_le_bytes(self.fixed::<4>()?) as usize
        };
        let bytes = self.take(len)?;
        if self.take(1)? != [0] {
            return Err(invalid("a string with no closing zero"));
        }
        String::from_utf8(bytes.to_vec()).map_err(|_| invalid("a string that is not UTF-8"))
    }

    /// Reads one value of the complete type `signature` at the nesting
    /// `depth`.
    fn value(&mut self, signature: &str, depth: usize) -> io::Result<Value> {
        if depth > MAX_DEPTH {
            return Err(invalid("containers nested too deep"));
        }
        let Some(&first) = signature.as_bytes().first() else {
            return Err(invalid("an empty signature"));
        };
        Ok(match first {
            b'y' => Value::Byte(self.take(1)?[0]),
            b'b' => match u32::from_le_bytes(self.fixed::<4>()?) {
                0 => Value::Bool(false),
                1 => Value::Bool(true),
                _ => return Err(invalid("a boolean that is neither 0 nor 1")),
            },
            b'n' => Value::I16(i16::from_le_bytes(self.fixed::<2>()?)),
            b'q' => Value::U16(u16::from_le_bytes(self.fixed::<2>()?)),
            b'i' => Value::I32(i32::from_le_bytes(self.fixed::<4>()?)),
            b'u' | b'h' => Value::U32(u32::from_le_bytes(self.fixed::<4>()?)),
            b'x' => Value::I64(i64::from_le_bytes(self.fixed::<8>()?)),
            b't' | b'd' => Value::U64(u64::from_le_bytes(self.fixed::<8>()?)),
            b's' => Value::Str(self.text(4)?),
            b'o' => Value::Path(self.text(4)?),
            b'g' => Value::Signature(self.text(1)?),
            b'v' => {
                let inner = self.text(1)?;
                if single_len(&inner)? != inner.len() {
                    return Err(invalid("a variant of more than one type"));
                }
                Value::Variant(Box::new(self.value(&inner, depth + 1)?))
            }
            b'(' => {
                self.pad(8)?;
                let inner = &signature[1..single_len(signature)? - 1];
                let mut fields = Vec::new();
                for field in split_signature(inner)? {
                    fields.push(self.value(field, depth + 1)?);
                }
                Value::Struct(fields)
            }
            b'a' => self.array(&signature[1..], depth)?,
            _ => return Err(invalid("an unknown type")),
        })
    }

    /// Reads an array whose element signature is `element`.
    fn array(&mut self, element: &str, depth: usize) -> io::Result<Value> {
        let length = u32::from_le_bytes(self.fixed::<4>()?) as usize;
        self.pad(alignment(element))?;
        let end = self
            .at
            .checked_add(length)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| invalid("an array past the end"))?;
        if let Some(entry) = element.strip_prefix('{') {
            let pair = &entry[..entry.len().saturating_sub(1)];
            let key_len = single_len(pair)?;
            let (key, value) = pair.split_at(key_len);
            let mut entries = Vec::new();
            while self.at < end {
                self.pad(8)?;
                let key_value = self.value(key, depth + 1)?;
                entries.push((key_value, self.value(value, depth + 1)?));
            }
            return Ok(Value::Dict(key.to_string(), value.to_string(), entries));
        }
        let mut items = Vec::new();
        while self.at < end {
            items.push(self.value(element, depth + 1)?);
        }
        if self.at != end {
            return Err(invalid("an array whose elements pass its length"));
        }
        Ok(Value::Array(element.to_string(), items))
    }
}

/// A connection to the system bus, authenticated with `EXTERNAL`.
pub struct Bus {
    /// The socket, non blocking once the connection is up.
    stream: UnixStream,
    /// The next serial this client gives a message.
    serial: u32,
    /// The bytes read and not yet a whole message.
    rx: Vec<u8>,
    /// The bytes not yet written.
    tx: VecDeque<u8>,
    /// The messages [`Bus::call`] met while it waited for its reply.
    held: VecDeque<Message>,
    /// The unique name the bus gave this connection.
    unique: String,
}

impl Bus {
    /// Connects to the system bus and says `Hello`.
    ///
    /// # Errors
    ///
    /// Returns the error of the socket, and an error of the kind
    /// `PermissionDenied` where the bus refuses the authentication.
    pub fn system() -> io::Result<Self> {
        let path = env::var(SYSTEM_BUS_ENV)
            .ok()
            .and_then(|address| address.strip_prefix("unix:path=").map(ToString::to_string))
            .unwrap_or_else(|| SYSTEM_BUS.to_string());
        let mut stream = UnixStream::connect(path)?;
        stream.set_read_timeout(Some(CALL_TIMEOUT))?;
        authenticate(&mut stream)?;
        let mut bus = Self {
            stream,
            serial: 1,
            rx: Vec::new(),
            tx: VecDeque::new(),
            held: VecDeque::new(),
            unique: String::new(),
        };
        bus.stream.set_nonblocking(true)?;
        let hello = Message::call(
            BUS_NAME,
            "/org/freedesktop/DBus",
            BUS_NAME,
            "Hello",
            Vec::new(),
        );
        let reply = bus.call(&hello)?;
        bus.unique = reply
            .body
            .first()
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        Ok(bus)
    }

    /// Returns the unique name the bus gave this connection.
    #[must_use]
    pub fn unique_name(&self) -> &str {
        &self.unique
    }

    /// Sends `message` and returns the serial it took.
    ///
    /// # Errors
    ///
    /// Returns the error of the socket.
    pub fn send(&mut self, message: &Message) -> io::Result<u32> {
        let serial = self.serial;
        self.serial = self.serial.wrapping_add(1).max(1);
        self.tx.extend(message.encode(serial));
        self.flush()?;
        Ok(serial)
    }

    /// Emits the signal `signal`.
    ///
    /// # Errors
    ///
    /// Returns the error of the socket.
    pub fn emit(&mut self, signal: &Message) -> io::Result<()> {
        self.send(signal).map(|_| ())
    }

    /// Sends the method call `message` and waits for its reply, keeping every
    /// other message it meets for [`Bus::next_message`].
    ///
    /// # Errors
    ///
    /// Returns the error of the socket, an error of the kind `TimedOut` where
    /// no reply comes, and an error of the kind `Other` with the name and the
    /// text of an error that answers the call.
    pub fn call(&mut self, message: &Message) -> io::Result<Message> {
        let serial = self.send(message)?;
        let deadline = Instant::now() + CALL_TIMEOUT;
        loop {
            while let Some(incoming) = self.read_message()? {
                if incoming.reply_serial == Some(serial)
                    && matches!(incoming.kind, Kind::MethodReturn | Kind::Error)
                {
                    return answer(incoming);
                }
                self.held.push_back(incoming);
            }
            if Instant::now() > deadline {
                return Err(io::Error::new(ErrorKind::TimedOut, "D-Bus: no reply"));
            }
            self.wait(Duration::from_millis(5))?;
        }
    }

    /// Returns the next message this process did not ask for, a method call
    /// to one of its objects, a signal it watches or a reply of a message it
    /// sent with [`Bus::send`], or `None` where none waits.
    ///
    /// # Errors
    ///
    /// Returns the error of the socket and of a message that breaks the wire
    /// format.
    pub fn next_message(&mut self) -> io::Result<Option<Message>> {
        if let Some(held) = self.held.pop_front() {
            return Ok(Some(held));
        }
        self.read_message()
    }

    /// Returns whether bytes wait to be written.
    #[must_use]
    pub fn wants_write(&self) -> bool {
        !self.tx.is_empty()
    }

    /// Returns whether more than `bytes` bytes wait to be written.
    #[must_use]
    pub fn wants_write_more_than(&self, bytes: usize) -> bool {
        self.tx.len() > bytes
    }

    /// Writes the bytes that wait as far as the socket takes them.
    ///
    /// # Errors
    ///
    /// Returns the error of the socket.
    pub fn flush(&mut self) -> io::Result<()> {
        while !self.tx.is_empty() {
            let (front, _) = self.tx.as_slices();
            match self.stream.write(front) {
                Ok(0) => {
                    return Err(io::Error::new(
                        ErrorKind::WriteZero,
                        "D-Bus: the bus closed",
                    ));
                }
                Ok(written) => {
                    self.tx.drain(..written);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(()),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    /// Reads what the socket holds and returns the next whole message.
    fn read_message(&mut self) -> io::Result<Option<Message>> {
        loop {
            if let Some((message, used)) = Message::decode(&self.rx)? {
                self.rx.drain(..used);
                return Ok(Some(message));
            }
            let mut chunk = [0u8; 4096];
            match self.stream.read(&mut chunk) {
                Ok(0) => {
                    return Err(io::Error::new(
                        ErrorKind::UnexpectedEof,
                        "D-Bus: the bus closed",
                    ));
                }
                Ok(read) => self.rx.extend_from_slice(&chunk[..read]),
                Err(error) if error.kind() == ErrorKind::WouldBlock => return Ok(None),
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => return Err(error),
            }
        }
    }

    /// Writes what waits and sleeps `pause` where nothing arrived, the wait
    /// of a call that blocks.
    fn wait(&mut self, pause: Duration) -> io::Result<()> {
        self.flush()?;
        std::thread::sleep(pause);
        Ok(())
    }

    /// Adds the match rule `rule` to the bus, so the signals it names reach
    /// this connection.
    ///
    /// # Errors
    ///
    /// Returns what [`Bus::call`] returns.
    pub fn add_match(&mut self, rule: &str) -> io::Result<()> {
        let call = Message::call(
            BUS_NAME,
            "/org/freedesktop/DBus",
            BUS_NAME,
            "AddMatch",
            vec![Value::Str(rule.to_string())],
        );
        self.call(&call).map(|_| ())
    }
}

impl AsFd for Bus {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.stream.as_fd()
    }
}

/// Returns the reply `incoming`, or its error as an error of the kind
/// `Other`.
fn answer(incoming: Message) -> io::Result<Message> {
    if incoming.kind == Kind::Error {
        let text = incoming
            .body
            .first()
            .and_then(Value::as_str)
            .unwrap_or_default();
        return Err(io::Error::other(format!(
            "D-Bus: {}: {text}",
            incoming.error_name.as_deref().unwrap_or_default()
        )));
    }
    Ok(incoming)
}

/// Authenticates `stream` with `EXTERNAL` and the uid of the process, and
/// begins the message stream.
fn authenticate(stream: &mut UnixStream) -> io::Result<()> {
    let uid = process_uid()?;
    let hex = uid.bytes().fold(String::new(), |mut hex, byte| {
        // A write to a string does not fail.
        let _ = write!(hex, "{byte:02x}");
        hex
    });
    stream.write_all(b"\0")?;
    stream.write_all(format!("AUTH EXTERNAL {hex}\r\n").as_bytes())?;
    let line = read_line(stream)?;
    if !line.starts_with("OK ") {
        return Err(io::Error::new(
            ErrorKind::PermissionDenied,
            format!("D-Bus: {line}"),
        ));
    }
    stream.write_all(b"BEGIN\r\n")
}

/// Returns the real uid of the process as decimal text, read from
/// `/proc/self/status`, since the standard library has no safe call for it.
fn process_uid() -> io::Result<String> {
    let status = fs::read_to_string("/proc/self/status")?;
    status
        .lines()
        .find_map(|line| line.strip_prefix("Uid:"))
        .and_then(|rest| rest.split_whitespace().next())
        .map(ToString::to_string)
        .ok_or_else(|| invalid("no Uid in /proc/self/status"))
}

/// Reads one line of the authentication, up to its `\r\n`.
fn read_line(stream: &mut UnixStream) -> io::Result<String> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while !line.ends_with(b"\r\n") {
        if stream.read(&mut byte)? == 0 {
            return Err(io::Error::new(
                ErrorKind::UnexpectedEof,
                "D-Bus: the bus closed",
            ));
        }
        line.push(byte[0]);
        if line.len() > 512 {
            return Err(invalid("an authentication line too long"));
        }
    }
    line.truncate(line.len() - 2);
    String::from_utf8(line).map_err(|_| invalid("an authentication line that is not UTF-8"))
}

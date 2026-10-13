// SPDX-License-Identifier: MIT
/*
 * aosabi-js host: the buffers the module shares with the host and the tagged
 * record codec of every value that crosses the host boundary.
 *
 * No analogue in the reference.
 */

//! The buffers the host writes and reads, and the records that cross the
//! boundary.
//!
//! One `thread_local` cell holds the input buffer, which the host writes, and
//! the output buffer, which the host reads, a plain state in a module without
//! threads and in the one thread the host calls the native library on. The
//! export `alloc` sizes the input buffer and answers its
//! address, and every address the module hands out is taken through
//! `as_mut_ptr`, so its exposed provenance permits the write of the host. The
//! host writes the input buffer between `alloc` and the call that names the
//! record, while no reference of the module to it is alive, and keeps no
//! address and no view of the memory across calls, since a later call may
//! move the buffer or grow the memory. A record is copied out of the input
//! buffer before any code of the engine runs, since a call into the host may
//! write the buffer again.
//!
//! A count, a length and a handle are four little endian bytes, and a number
//! is the eight little endian bytes of its `f64`, bit for bit. A string value,
//! the name and the message of an error, the key of a member of an error and
//! the text of a bigint or a symbol are a length and WTF-8 bytes, which keep a
//! lone surrogate, and a key, the name of a function and a field of the record
//! of a request are a length and UTF-8 bytes. A value record opens with one
//! byte, its [`Tag`]:
//!
//! | Tag | Value | Followed by, from the host | Followed by, from the engine |
//! | ---: | --- | --- | --- |
//! | 0 | `undefined` | Nothing | Nothing |
//! | 1 | `null` | Nothing | Nothing |
//! | 2, 3 | `false`, `true` | Nothing | Nothing |
//! | 4 | A number | The number | The number |
//! | 5 | A string | The string | The string |
//! | 6 | A `Date` | Its time value | Its time value |
//! | 7 | An object of the host | Its handle in the table of the request | Not written |
//! | 8 | An array | A count and the items | A count and the items |
//! | 9 | A function | Its handle and its name | Not written |
//! | 10 | A promise or a thenable | Its handle | Not written |
//! | 11 | An error | Its handle | Its [`Brand`] and what the text below names |
//! | 12 | A plain object | A count and each own key with its value, a copy of JSON data | A count and each own key with its value |
//! | 13 | An object of the engine with no prototype | Not written | As for tag 12 |
//! | 14 | The value of a variable | Not written | Its name and the record of its value |
//! | 15 | A bigint or a symbol | Its kind and the string `String` answers | Not written |
//! | 16 | The context `c` of the request | Not written | Nothing |
//! | 20 | A value of the host the engine holds | Not written | Its handle in the table of the request |
//! | 21 | Bytes, a chunk of a body | A length and the bytes | Not written |
//! | 22 | A web `File` of a multipart request | Not written | Its name, its type, its time, and the address and the length of its bytes |
//! | 25 | A topic of a pub sub of the engine | The number of its pub sub, its routing key and its id | Not written |
//! | 26 | The answer of the `fetch` of the host | A byte 1 with the status, the status text, the header fields and the body of the response, or a byte 0 with the message of the failure | Not written |
//!
//! An error of the host crosses as its handle alone, and the module reads its
//! brand and the rest of what its class needs through the import `js_error`,
//! as `crate::value` says, while the handle `u32::MAX` names a thrown value
//! the host could not read. An error of the engine crosses with its brand,
//! its name and its message, and then three value records: the path and the
//! locations of a `GraphQLError` and `undefined` for any other error, and the
//! members, the extensions of a `GraphQLError`, the errors of an
//! `AggregateError`, the own members of an error of a class with members of
//! its own and `undefined` for any other error, so the host makes an instance
//! of the class of the wrapper or of the intrinsic class the brand names.
//!
//! The host writes the tag 12 for the JSON data it copies of a member of an
//! error, the extensions, the member `http` of the extensions of a masked
//! error, the path of a located error and the members of an error inside a
//! value, which `JSON.stringify` reads as Yoga reads them when it writes a
//! result, so the copy holds plain objects, arrays and primitives and no
//! object of the app.
//!
//! The engine writes the tags 13 and 14 for the arguments of a call of the
//! host alone, as `crate::value` says: an object the document writes as a
//! literal takes no prototype, as `valueFromAST` of graphql-js makes it, and
//! the value of a variable crosses under its name, so the host makes one
//! value of it for the whole request, as graphql-js hands every place of a
//! variable the one value it coerced.
//!
//! The engine hands a function of the app that takes the context of its
//! request, the headers function of a federation among them, the tag 16, which
//! the host answers with the `c` of its hono the request runs in, as
//! `crate::context` says, and the body of a request the host holds crosses
//! chunk by chunk, each chunk a tag 21. A value of the host the engine keeps,
//! the hooks the hook `onExecute` of a plugin of the app answered, crosses back
//! to its done hook as tag 20 and its handle, so the app sees the object it
//! made. The tags 17 to 19 name no value, since the routes and the middleware
//! of an app run in the hono of the host, `aosabi/pylon`, and never cross into
//! the engine.
//!
//! A file of a multipart request, which the request parsers of graphql-over-http
//! make as the parser of undici makes a web `File`, crosses as tag 22: its name,
//! its type, both as WTF-8 strings, its `lastModified` as a number, and the
//! address of its bytes as eight little endian bytes and their length. The bytes
//! stand at their part of the body of the request, the one copy of the body in
//! the memory of the module, which the entry of the request keeps until the
//! request ends, or, for a part in base64, in the bytes of its own the file
//! holds, and the host reads them there before the call that names them returns
//! and makes a `File` of its platform of them, so a file crosses with no copy in
//! the module beside the bytes a part in base64 decodes to, and a resolver of
//! the app reads `name`, `type`, `size` and the bytes of the file.
//! The time is 0 where the engine reads no clock, a module on a Worker, and the
//! host then stamps the file with its own clock, as the parser of the platform
//! stamps it at its making.
//!
//! The host writes the tag 25 for the value `subscribe` of a pub sub of the
//! package answers, a marker whose topic lives in the pub sub of the engine:
//! the number of its pub sub, its routing key as a WTF-8 string and its id
//! as a copy of JSON data, `undefined` where `subscribe` took none, which the
//! module reads into the repeater of that topic, as `crate::pubsub` says.
//! The numbers 23 and 24 open the payloads of the hooks of a plugin and are
//! no tag of a value.
//!
//! The host writes the tag 15 for a bigint and a symbol, which the engine
//! holds no value of, with its kind, the byte 0 for a bigint and 1 for a
//! symbol, and the string `String(value)` answers. The module reads that
//! string at a leaf of `ID` or `GID` and a function whose every call answers
//! another such function at any other place, as `crate::value` says.

#![forbid(unsafe_code)]

use core::cell::RefCell;
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi::auth::ForbiddenError;
use aosabi_execute::value::{
    ErrorObject,
    HostValue,
    Value, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue,
    Opaque,
    value::OwnKeys, //
};
use graphql_over_http::file::FileBody;

/// The depth past which a value of the engine crosses as `undefined`, a bound
/// no argument of a fixture reaches and which stops a cycle.
pub const DEPTH: usize = 64;

/// The byte that opens a value record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Tag {
    /// The value `undefined`.
    Undefined = 0,
    /// The value `null`.
    Null = 1,
    /// The value `false`.
    False = 2,
    /// The value `true`.
    True = 3,
    /// A number.
    Number = 4,
    /// A string.
    String = 5,
    /// A `Date`.
    Date = 6,
    /// An object of the host.
    Object = 7,
    /// An array.
    List = 8,
    /// A function of the host.
    Function = 9,
    /// A promise or a thenable of the host.
    Promise = 10,
    /// An error.
    Error = 11,
    /// A plain object of the engine.
    Plain = 12,
    /// A plain object of the engine with no prototype.
    Bare = 13,
    /// The value of a variable of the operation, after its name.
    Variable = 14,
    /// A bigint or a symbol of the host.
    Foreign = 15,
    /// The context `c` of the request, which a function of the app receives.
    Context = 16,
    /// A value of the host the engine holds, after its handle.
    Held = 20,
    /// Bytes, after their length.
    Bytes = 21,
    /// A web `File` of a multipart request, after its name, its type, its
    /// time and its bytes.
    File = 22,
    /// A topic of a pub sub of the engine, after the number of its pub sub,
    /// its routing key and its id.
    Topic = 25,
    /// The answer of the `fetch` of the host, a byte 1 with the status, the
    /// status text, the header fields and the body of the response, or a
    /// byte 0 with the message of the failure.
    Fetched = 26,
    /// A delegation of a federation of the app, after the number of its
    /// promise in the list of its request, `crate::federation`.
    Delegation = 27,
}

/// The byte that opens the payload of the hook `onExecute` of a plugin of the
/// app, a value record the engine writes for the host and never reads.
pub const EXECUTE: u8 = 23;

/// The byte that opens the payload of the hook `onExecuteDone` of a plugin of
/// the app, after which come the number of its call and the outcome of the
/// execution, a value record the engine writes for the host and never reads.
pub const DONE: u8 = 24;

impl Tag {
    /// Returns the tag of the byte `byte`, or `None` for a byte no tag takes.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::Undefined,
            1 => Self::Null,
            2 => Self::False,
            3 => Self::True,
            4 => Self::Number,
            5 => Self::String,
            6 => Self::Date,
            7 => Self::Object,
            8 => Self::List,
            9 => Self::Function,
            10 => Self::Promise,
            11 => Self::Error,
            12 => Self::Plain,
            13 => Self::Bare,
            14 => Self::Variable,
            15 => Self::Foreign,
            16 => Self::Context,
            20 => Self::Held,
            21 => Self::Bytes,
            22 => Self::File,
            25 => Self::Topic,
            26 => Self::Fetched,
            27 => Self::Delegation,
            _ => return None,
        })
    }

    /// Returns the byte of the tag.
    #[must_use]
    pub const fn byte(self) -> u8 {
        self as u8
    }
}

/// The byte of an error record that names the class family of the error, the
/// brand the classes of the wrapper stamp.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Brand {
    /// An error no class of the wrapper made.
    None = 0,
    /// A `GraphQLError`, made by its class, by `createGraphQLError` or by a
    /// subclass, `ServiceError` among them.
    GraphQL = 1,
    /// A `ForbiddenError`.
    Forbidden = 2,
    /// An `AggregateError`, the intrinsic class, whose errors cross with it.
    Aggregate = 3,
}

impl Brand {
    /// Returns the brand of the byte `byte`, or `None` for a byte no brand
    /// takes.
    #[must_use]
    pub const fn from_byte(byte: u8) -> Option<Self> {
        Some(match byte {
            0 => Self::None,
            1 => Self::GraphQL,
            2 => Self::Forbidden,
            3 => Self::Aggregate,
            _ => return None,
        })
    }

    /// Returns the byte of the brand.
    #[must_use]
    pub const fn byte(self) -> u8 {
        self as u8
    }
}

/// The two buffers the module shares with the host.
struct Buffers {
    /// The input buffer, where the host writes a record.
    input: Vec<u8>,
    /// The output buffer, a length and the record of an answer.
    output: Vec<u8>,
}

thread_local! {
    /// The buffers of the module.
    static BUFFERS: RefCell<Buffers> = const {
        RefCell::new(Buffers {
            input: Vec::new(),
            output: Vec::new(),
        })
    };
}

/// Returns the address of `bytes` in the memory of the module or of the
/// process of the native library.
///
/// The address is taken through `as_mut_ptr`, so its exposed provenance is
/// that of a mutable borrow and permits the host to write the range, which
/// an address of a shared slice would not. It is a `usize`, four bytes in
/// the module and eight in the native library.
pub fn address(bytes: &mut [u8]) -> usize {
    bytes.as_mut_ptr().expose_provenance()
}

/// Returns `len` as the four bytes a length takes at the boundary.
#[must_use]
pub fn length(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// Sizes the input buffer to `len` zero bytes and returns its address, or 0
/// when the allocator gives no buffer of that length.
// A buffer left unwritten for the host would skip this zeroing.
#[must_use]
pub fn alloc(len: u32) -> usize {
    let Ok(len) = usize::try_from(len) else {
        return 0;
    };
    BUFFERS.with_borrow_mut(|buffers| {
        let input = &mut buffers.input;
        input.clear();
        if input.try_reserve_exact(len).is_err() {
            return 0;
        }
        input.resize(len, 0);
        address(input)
    })
}

/// Releases both buffers.
pub fn dealloc() {
    BUFFERS.with_borrow_mut(|buffers| {
        buffers.input = Vec::new();
        buffers.output = Vec::new();
    });
}

/// Returns a copy of the first `len` bytes of the input buffer, or of the
/// whole buffer where it is shorter.
// A read in place of the record the host holds would skip its write into the
// input buffer and this copy in the native library.
#[must_use]
pub fn input(len: u32) -> Vec<u8> {
    BUFFERS.with_borrow(|buffers| {
        let input = &buffers.input;
        let end = usize::try_from(len).map_or(input.len(), |len| len.min(input.len()));
        input.get(..end).map_or_else(Vec::new, <[u8]>::to_vec)
    })
}

/// Writes `record` with its length into the output buffer and returns the
/// address of the length, which the host reads before its next call.
#[must_use]
pub fn output(record: &[u8]) -> usize {
    BUFFERS.with_borrow_mut(|buffers| {
        let output = &mut buffers.output;
        output.clear();
        output.extend_from_slice(&length(record.len()).to_le_bytes());
        output.extend_from_slice(record);
        address(output)
    })
}

/// A reader of the fields of a record, which answers `None` past its end.
pub struct Reader<'a> {
    /// The bytes not read yet.
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    /// Returns the reader of `bytes`.
    #[must_use]
    pub const fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    /// Reads one byte.
    pub fn byte(&mut self) -> Option<u8> {
        let (head, rest) = self.rest.split_first()?;
        self.rest = rest;
        Some(*head)
    }

    /// Reads the tag that opens a value record.
    pub fn tag(&mut self) -> Option<Tag> {
        Tag::from_byte(self.byte()?)
    }

    /// Reads a count, a length or a handle of four little endian bytes.
    pub fn word(&mut self) -> Option<u32> {
        let (head, rest) = self.rest.split_first_chunk::<4>()?;
        self.rest = rest;
        Some(u32::from_le_bytes(*head))
    }

    /// Reads a count of four little endian bytes as a size.
    pub fn count(&mut self) -> Option<usize> {
        usize::try_from(self.word()?).ok()
    }

    /// Reads a number of eight little endian bytes.
    pub fn number(&mut self) -> Option<f64> {
        let (head, rest) = self.rest.split_first_chunk::<8>()?;
        self.rest = rest;
        Some(f64::from_le_bytes(*head))
    }

    /// Reads a length and that many bytes.
    pub fn bytes(&mut self) -> Option<&'a [u8]> {
        let len = self.count()?;
        let (bytes, rest) = self.rest.split_at_checked(len)?;
        self.rest = rest;
        Some(bytes)
    }

    /// Takes every byte not read yet, a value record a later call answers as
    /// it is.
    pub fn rest(&mut self) -> &'a [u8] {
        core::mem::take(&mut self.rest)
    }

    /// Reads a length and that many bytes as UTF-8, a key, the name of a
    /// function or a field of the record of a request.
    // An unchecked read would skip this check of UTF-8. The wrapper writes a key
    // or a name that is not well formed as WTF-8, which this check refuses, so
    // an unchecked read here would take bytes a client can choose.
    pub fn text(&mut self) -> Option<&'a str> {
        core::str::from_utf8(self.bytes()?).ok()
    }

    /// Reads a length and that many bytes as WTF-8, a string value, the name
    /// or the message of an error or the key of a member of an error, so a
    /// lone surrogate crosses as itself.
    pub fn string(&mut self) -> Option<JsString> {
        JsString::from_wtf8(self.bytes()?).ok()
    }
}

/// Appends the tag `tag`.
pub fn push_tag(out: &mut Vec<u8>, tag: Tag) {
    out.push(tag.byte());
}

/// Appends a handle or a number of four little endian bytes.
pub fn push_word(out: &mut Vec<u8>, word: u32) {
    out.extend_from_slice(&word.to_le_bytes());
}

/// Appends a count of four little endian bytes.
pub fn push_count(out: &mut Vec<u8>, count: usize) {
    out.extend_from_slice(&length(count).to_le_bytes());
}

/// Appends a number of eight little endian bytes.
pub fn push_number(out: &mut Vec<u8>, number: f64) {
    out.extend_from_slice(&number.to_le_bytes());
}

/// Appends the address `address` of a range the host reads in place, as
/// eight little endian bytes, which hold the address of the module and of the
/// native library alike.
pub fn push_address(out: &mut Vec<u8>, address: usize) {
    out.extend_from_slice(&u64::try_from(address).unwrap_or(u64::MAX).to_le_bytes());
}

/// Appends a length and `bytes`.
pub fn push_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    push_count(out, bytes.len());
    out.extend_from_slice(bytes);
}

/// Appends a length and the WTF-8 bytes of the string value `string`.
pub fn push_string(out: &mut Vec<u8>, string: &JsString) {
    push_bytes(out, string.as_wtf8());
}

/// Appends the record of the value of the engine `value` for the host.
///
/// A primitive and a `Date` cross inline, a string as WTF-8, an array with
/// its items, an object as a plain object with its own keys and their values,
/// and an error with its brand and what its class needs, as
/// [`encode_error`] writes it. An error a route of the host threw crosses as
/// the handle of the value the host made, an answer of a helper of `c` as its
/// number and the context of the request as its tag, as `crate::route`
/// says. A file of a multipart request crosses with its name, its type, its
/// time and the place of its bytes. A function, a promise and any other value of the executor cross as
/// `undefined`, and so does a value nested deeper than [`DEPTH`], which stops
/// a cycle.
pub fn encode(out: &mut Vec<u8>, value: &Value) {
    encode_at(out, value, 0);
}

/// Appends the record of `value`, nested `depth` levels deep.
fn encode_at(out: &mut Vec<u8>, value: &Value, depth: usize) {
    if depth > DEPTH {
        push_tag(out, Tag::Undefined);
        return;
    }
    match value {
        Value::Null => push_tag(out, Tag::Null),
        Value::Bool(flag) => push_tag(out, if *flag { Tag::True } else { Tag::False }),
        Value::Number(number) => {
            push_tag(out, Tag::Number);
            push_number(out, *number);
        }
        Value::String(text) => {
            push_tag(out, Tag::String);
            push_string(out, text);
        }
        Value::Date(date) => {
            push_tag(out, Tag::Date);
            push_number(out, date.get_time());
        }
        Value::List(items) => {
            push_tag(out, Tag::List);
            push_count(out, items.len());
            for item in items.iter() {
                encode_at(out, item, depth + 1);
            }
        }
        Value::Object(object) => {
            let keys = object.own_keys();
            push_tag(out, Tag::Plain);
            push_count(out, keys.len());
            for key in keys {
                push_bytes(out, key.as_bytes());
                encode_at(out, &Rc::clone(object).get(&key), depth + 1);
            }
        }
        Value::Error(error) => {
            push_tag(out, Tag::Error);
            encode_error(out, error, depth);
        }
        Value::Host(HostValue::Opaque(opaque)) => {
            if opaque
                .downcast_ref::<crate::context::ContextMark>()
                .is_some()
            {
                push_tag(out, Tag::Context);
            } else if let Some(file) = opaque.downcast_ref::<FileBody>() {
                push_tag(out, Tag::File);
                push_bytes(out, file.name.as_bytes());
                push_bytes(out, file.r#type.as_bytes());
                push_number(out, file.last_modified);
                let (address, len) = file_place(file);
                push_address(out, address);
                push_count(out, len);
            } else {
                push_tag(out, Tag::Undefined);
            }
        }
        Value::Undefined | Value::Function(_) | Value::Promise(_) | Value::Host(_) => {
            push_tag(out, Tag::Undefined);
        }
    }
}

/// Returns the address and the length of the bytes of `file`, its part of the
/// body of the request that runs, where it stands there, or its own bytes.
fn file_place(file: &FileBody) -> (usize, usize) {
    if let Some(part) = &file.part
        && let Some(place) = crate::request::body_part(crate::request::current(), part)
    {
        return place;
    }
    (file.bytes.as_ptr().expose_provenance(), file.bytes.len())
}

/// Appends the rest of the record of the error of the engine `error`, nested
/// `depth` levels deep, after its tag.
///
/// The record is the brand, the name and the message, then the path, the
/// locations and the members. A `GraphQLError` gives its path, its locations and
/// its extensions as its formatted error holds them, each `undefined` where it
/// holds none, so an empty object of extensions crosses as `undefined`, which
/// the class of the wrapper makes an empty object again. An `AggregateError`
/// gives its errors as its members, each a value record, so the host makes the
/// intrinsic class with them. Any other error gives its name and its message
/// alone. The class the brand names makes the host side instance, so
/// `instanceof` holds in the app.
pub fn encode_error(out: &mut Vec<u8>, error: &ErrorObject, depth: usize) {
    let (brand, path, locations, members) = match error {
        ErrorObject::GraphQL(error) => {
            let formatted = JsValue::from(error.to_json());
            let member = |key: &str| match &formatted {
                JsValue::Object(object) => object.get(key).cloned(),
                _ => None,
            };
            (
                Brand::GraphQL,
                member("path"),
                member("locations"),
                member("extensions"),
            )
        }
        ErrorObject::Custom { props, host, .. } => {
            let forbidden = host
                .as_ref()
                .and_then(|host| host.downcast_ref::<Opaque>())
                .and_then(|opaque| opaque.downcast_ref::<ForbiddenError>())
                .is_some();
            (
                if forbidden {
                    Brand::Forbidden
                } else {
                    Brand::None
                },
                None,
                None,
                Some(JsValue::Object((**props).clone())),
            )
        }
        ErrorObject::Aggregate { errors, .. } => {
            out.push(Brand::Aggregate.byte());
            push_string(out, &error.name());
            push_string(out, &error.message());
            push_tag(out, Tag::Undefined);
            push_tag(out, Tag::Undefined);
            encode_at(out, &Value::List(Rc::new(errors.clone())), depth + 1);
            return;
        }
        ErrorObject::Plain { .. } => (Brand::None, None, None, None),
    };
    out.push(brand.byte());
    push_string(out, &error.name());
    push_string(out, &error.message());
    for member in [path, locations, members] {
        match member {
            Some(value) => encode_at(out, &Value::from_js(&value), depth + 1),
            None => push_tag(out, Tag::Undefined),
        }
    }
}

/// Appends the record of the JSON value `value`, the copy of a member of a
/// result of an execution, at any depth.
///
/// The record is the one [`encode`] writes for the value [`Value::from_js`]
/// makes of `value`, but that each key crosses with its WTF-8, so a key with a
/// lone surrogate crosses as itself. The walk keeps a stack of its own in place
/// of recursion and no bound of [`DEPTH`], so a result nested deeper than that
/// crosses whole into a done hook of a plugin of the app, which receives the
/// result the executor answered.
pub fn push_json(out: &mut Vec<u8>, value: &JsValue) {
    /// The members of an array or of an object the walk has not written yet.
    enum Open<'a> {
        /// The items of an array.
        Items(core::slice::Iter<'a, JsValue>),
        /// The members of an object, in the order of its own keys.
        Members(OwnKeys<'a>),
    }
    let mut open: Vec<Open<'_>> = Vec::new();
    let mut next = Some(value);
    loop {
        match next.take() {
            Some(JsValue::Array(items)) => {
                push_tag(out, Tag::List);
                push_count(out, items.len());
                open.push(Open::Items(items.iter()));
            }
            Some(JsValue::Object(object)) => {
                push_tag(out, Tag::Plain);
                push_count(out, object.len());
                open.push(Open::Members(object.own_keys()));
            }
            Some(leaf) => encode(out, &Value::from_js(leaf)),
            None => {}
        }
        let Some(top) = open.last_mut() else {
            return;
        };
        next = match top {
            Open::Items(items) => items.next(),
            Open::Members(members) => members.next().map(|(key, member)| {
                push_string(out, key);
                member
            }),
        };
        if next.is_none() {
            open.pop();
        }
    }
}

/// Reads one record of the JSON data the host copied at the cursor of
/// `reader`, a member of a result a done hook of a plugin of the app leaves,
/// at any depth, or answers `None` for a broken record and for a tag no copy
/// of JSON holds.
///
/// The host writes the tags of JSON data alone for such a copy, `undefined`,
/// null, the booleans, a number, a string, an array and a plain object, and each
/// key with its WTF-8, so a key with a lone surrogate reads as itself. The walk
/// keeps a stack of its own in place of recursion and no bound of [`DEPTH`], as
/// [`push_json`] writes, so the copy a host makes of a result nested deeper than
/// that reads whole. The data of the arguments of a delegation of the app holds
/// one more tag, a file of a request the app hands on, which `read_file` reads
/// and a copy of JSON never holds.
pub fn read_json(reader: &mut Reader<'_>) -> Option<JsValue> {
    /// An array or an object the walk fills, with the count of its members
    /// left and, for an object, the key of the member that comes next.
    enum Open {
        /// The items of an array.
        Items(Vec<JsValue>, usize),
        /// The members of an object.
        Members(JsObject, usize, JsString),
    }
    let mut open: Vec<Open> = Vec::new();
    loop {
        let mut value = match reader.tag()? {
            Tag::Undefined => JsValue::Undefined,
            Tag::Null => JsValue::Null,
            Tag::False => JsValue::Bool(false),
            Tag::True => JsValue::Bool(true),
            Tag::Number => JsValue::Number(reader.number()?),
            Tag::String => JsValue::String(reader.string()?),
            Tag::List => match reader.count()? {
                0 => JsValue::Array(Vec::new()),
                count => {
                    open.push(Open::Items(Vec::with_capacity(count.min(1024)), count));
                    continue;
                }
            },
            Tag::Plain => match reader.count()? {
                0 => JsValue::Object(JsObject::new()),
                count => {
                    let key = reader.string()?;
                    open.push(Open::Members(JsObject::new(), count, key));
                    continue;
                }
            },
            Tag::File => read_file(reader)?,
            _ => return None,
        };
        loop {
            let left = match open.last_mut() {
                None => return Some(value),
                Some(Open::Items(items, left)) => {
                    items.push(value);
                    *left -= 1;
                    *left
                }
                Some(Open::Members(object, left, key)) => {
                    object.insert(key.clone(), value);
                    *left -= 1;
                    if *left > 0 {
                        *key = reader.string()?;
                    }
                    *left
                }
            };
            if left > 0 {
                break;
            }
            value = match open.pop()? {
                Open::Items(items, _) => JsValue::Array(items),
                Open::Members(object, _, _) => JsValue::Object(object),
            };
        }
    }
}

/// Reads the file of the host at the cursor of `reader`, after its tag: its
/// name, its type, its time and its bytes, which the wrapper copied from the
/// file the engine handed the app, and answers it as the host object of an
/// upload the core reads, `crate::federation`.
///
/// A resolver of the app hands a federation a file of its request among the
/// arguments of a delegation, which the executor of the federation sends as a
/// part of a multipart body, so the file reads back as the upload it was.
fn read_file(reader: &mut Reader<'_>) -> Option<JsValue> {
    let name = String::from(reader.text()?);
    let r#type = String::from(reader.text()?);
    let last_modified = reader.number()?;
    // An `unsafe` `Rc` would skip the atomic count each copy of the bytes and of
    // the file takes.
    let bytes: Arc<[u8]> = Arc::from(reader.bytes()?);
    Some(JsValue::Opaque(Opaque(Arc::new(FileBody {
        name,
        r#type,
        last_modified,
        bytes,
        part: None,
    }))))
}

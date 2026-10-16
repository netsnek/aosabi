// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool homa_packet: encodes and decodes one Homa packet in either
 * wire form.
 *
 * No analogue in the reference. Its judging notebooks are
 * tests/notebooks/03_homa_packets.ipynb and
 * tests/notebooks/05_homa_wire_full.ipynb, whose references are
 * tests/harness/homa/compact.py and tests/harness/homa/full.py.
 */

//! The `homa-packet` primitive of the seam.
//!
//! `--requests PATH` answers a file of request lines, one response line each,
//! in the batch form of [`crate::batch`]. Two operations exist.
//!
//! - `encode` takes `form`, `packet`, `ports` and `need_limits` and answers
//!   `hex`, the header the form writes followed by the payload of a DATA
//!   packet, or `error` with the name `NoRoom` where the form has no layout
//!   for the packet.
//! - `decode` takes `form` and `hex` and answers `decoded`, the ports, the
//!   packet and the flag `need_limits`, or `error` with the name of the
//!   `DecodeError` the form answers.
//! - `take` takes `form` `full`, `port` and `hexes` and hands each packet in
//!   order from one peer to an endpoint of the Network profile that owns
//!   `port`, and answers `results`, for each packet `taken`, `wrong_port` or
//!   the name of the refusal, and the endpoint's counters in `stats`.
//!
//! A packet travels as an object whose `type` names it, `DATA`, `GRANT`,
//! `RESEND`, `RPC_UNKNOWN`, `BUSY`, `CUTOFFS`, `FREEZE`, `NEED_ACK`, `ACK`,
//! `START_MSG` or `LIMITS`, followed by its fields in the order of
//! [`homa::wire::Packet`], and a payload travels as lowercase hex. The
//! `form` is `compact`, version 0 of the first version of the design note,
//! or `full`, `homa_wire.h` at the pinned commit of `HomaModule`. An
//! identifier travels as a number, which holds the 32 bits of the compact
//! form exactly and an identifier of the full form below 2^53, the range
//! every identifier the notebooks draw lies in.
//!
//! The field readers and the hex helpers of this module serve the other two
//! Homa primitives as well.

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use homa::{
    Config,
    Encap,
    Endpoint,
    Limits,
    Owned,
    Priority,
    Profile,
    Refused,
    wire::{
        AckEntry,
        Acks,
        Compact,
        Data,
        DecodeError,
        Decoded,
        Form,
        Full,
        Packet,
        Ports, //
    }, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Grammar,
        Matches, //
    },
    seam, //
};

/// The name of the primitive.
const NAME: &str = "homa-packet";

/// The stage of the Homa track the primitive lands at.
pub const LANDS_AT: &str = "H0";

/// The largest integer a double carries without a gap, 2^53 - 1.
const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;

/// The command line of the `homa-packet` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[cli::SMOKE, cli::REQUESTS],
    positional: None,
    conflicts: &[],
};

/// What the `homa-packet` primitive accepts.
#[derive(Debug)]
pub struct HomaPacketArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl HomaPacketArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
        }
    }
}

/// The operations of the batch form.
const OPERATIONS: &Operations = &[
    ("encode", op_encode),
    ("decode", op_decode),
    ("take", op_take),
];

/// Runs the `homa-packet` primitive.
pub fn run(args: &HomaPacketArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    let outcome = seam::required(args.requests.as_deref(), "--requests")
        .and_then(|path| batch::run(OPERATIONS, path));
    seam::finish(NAME, outcome)
}

/// The fields of one request object, read with the name of its operation in
/// every refusal.
pub struct Fields<'a> {
    /// The object read.
    object: &'a JsObject,
    /// The operation the object belongs to, which a refusal names.
    op: &'a str,
}

impl<'a> Fields<'a> {
    /// Returns the reader of `object`, whose refusals name `op`.
    pub fn new(object: &'a JsObject, op: &'a str) -> Self {
        Self { object, op }
    }

    /// Returns the operation whose refusals this reader names.
    pub fn op_name(&self) -> &'a str {
        self.op
    }

    /// Returns the refusal of the field `key` for `why`.
    pub fn refuse(&self, key: &str, why: &str) -> JsString {
        JsString::from(format!("{}: {key} {why}", self.op))
    }

    /// Returns the value under `key`, `None` where it is absent or null.
    pub fn get(&self, key: &str) -> Option<&'a JsValue> {
        match self.object.get(key) {
            None | Some(JsValue::Undefined | JsValue::Null) => None,
            Some(value) => Some(value),
        }
    }

    /// Returns the integer from 0 under `key`, or `None` where it is absent.
    ///
    /// # Errors
    ///
    /// Returns a refusal for a value that is no whole number from 0 up to
    /// 2^53 - 1.
    pub fn opt_u64(&self, key: &str) -> Result<Option<u64>, JsString> {
        let Some(value) = self.get(key) else {
            return Ok(None);
        };
        whole(value)
            .map(Some)
            .ok_or_else(|| self.refuse(key, "is no whole number from 0 up to 2^53 - 1"))
    }

    /// Returns the integer from 0 under `key`.
    ///
    /// # Errors
    ///
    /// Returns a refusal where the field is absent or no such number.
    pub fn u64(&self, key: &str) -> Result<u64, JsString> {
        self.opt_u64(key)?
            .ok_or_else(|| self.refuse(key, "is missing"))
    }

    /// Returns the integer under `key` that fits the type `T`.
    ///
    /// # Errors
    ///
    /// Returns a refusal where the field is absent, no whole number from 0,
    /// or larger than `T` holds.
    pub fn int<T: TryFrom<u64>>(&self, key: &str) -> Result<T, JsString> {
        let value = self.u64(key)?;
        T::try_from(value).map_err(|_| self.refuse(key, "is out of range"))
    }

    /// Returns the integer under `key` that fits the type `T`, or `default`
    /// where it is absent.
    ///
    /// # Errors
    ///
    /// Returns a refusal for a value that is no whole number from 0 or larger
    /// than `T` holds.
    pub fn int_or<T: TryFrom<u64>>(&self, key: &str, default: T) -> Result<T, JsString> {
        match self.opt_u64(key)? {
            None => Ok(default),
            Some(value) => T::try_from(value).map_err(|_| self.refuse(key, "is out of range")),
        }
    }

    /// Returns the boolean under `key`, or `default` where it is absent.
    ///
    /// # Errors
    ///
    /// Returns a refusal for a value that is no boolean.
    pub fn bool_or(&self, key: &str, default: bool) -> Result<bool, JsString> {
        match self.get(key) {
            None => Ok(default),
            Some(JsValue::Bool(value)) => Ok(*value),
            Some(_) => Err(self.refuse(key, "is no boolean")),
        }
    }

    /// Returns the string under `key`, or `None` where it is absent.
    ///
    /// # Errors
    ///
    /// Returns a refusal for a value that is no string a Rust string holds.
    pub fn opt_str(&self, key: &str) -> Result<Option<&'a str>, JsString> {
        match self.get(key) {
            None => Ok(None),
            Some(JsValue::String(text)) => text
                .as_str()
                .map(Some)
                .ok_or_else(|| self.refuse(key, "carries a lone surrogate")),
            Some(_) => Err(self.refuse(key, "is no string")),
        }
    }

    /// Returns the string under `key`.
    ///
    /// # Errors
    ///
    /// Returns a refusal where the field is absent or no string.
    pub fn str(&self, key: &str) -> Result<&'a str, JsString> {
        self.opt_str(key)?
            .ok_or_else(|| self.refuse(key, "is missing"))
    }

    /// Returns the bytes the hex string under `key` spells.
    ///
    /// # Errors
    ///
    /// Returns a refusal where the field is absent or no even run of hex
    /// digits.
    pub fn hex(&self, key: &str) -> Result<Vec<u8>, JsString> {
        unhex(self.str(key)?).ok_or_else(|| self.refuse(key, "is no even run of hex digits"))
    }

    /// Returns the object under `key`, or `None` where it is absent.
    ///
    /// # Errors
    ///
    /// Returns a refusal for a value that is no object.
    pub fn opt_object(&self, key: &str) -> Result<Option<&'a JsObject>, JsString> {
        match self.get(key) {
            None => Ok(None),
            Some(JsValue::Object(object)) => Ok(Some(object)),
            Some(_) => Err(self.refuse(key, "is no object")),
        }
    }

    /// Returns the object under `key`.
    ///
    /// # Errors
    ///
    /// Returns a refusal where the field is absent or no object.
    pub fn object(&self, key: &str) -> Result<&'a JsObject, JsString> {
        self.opt_object(key)?
            .ok_or_else(|| self.refuse(key, "is missing"))
    }

    /// Returns the array under `key`, empty where it is absent.
    ///
    /// # Errors
    ///
    /// Returns a refusal for a value that is no array.
    pub fn array(&self, key: &str) -> Result<&'a [JsValue], JsString> {
        match self.get(key) {
            None => Ok(&[]),
            Some(JsValue::Array(items)) => Ok(items),
            Some(_) => Err(self.refuse(key, "is no array")),
        }
    }

    /// Returns the reader of the object under `key`, whose refusals name the
    /// same operation.
    ///
    /// # Errors
    ///
    /// Returns a refusal where the field is absent or no object.
    pub fn nested(&self, key: &str) -> Result<Fields<'a>, JsString> {
        Ok(Fields::new(self.object(key)?, self.op))
    }
}

/// Returns the whole number from 0 up to 2^53 - 1 that `value` holds, `None`
/// for any other value.
pub fn whole(value: &JsValue) -> Option<u64> {
    let JsValue::Number(number) = *value else {
        return None;
    };
    if number.fract() != 0.0 || !(0.0..=MAX_SAFE_INTEGER).contains(&number) {
        return None;
    }
    // The cast is exact, because the value is a whole number inside the range
    // a double carries without a gap.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(number as u64)
}

/// Returns a count as the number a JSON value carries.
pub fn count(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

/// Returns `bytes` as lowercase hex.
pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(char::from(DIGITS[usize::from(byte >> 4)]));
        out.push(char::from(DIGITS[usize::from(byte & 0x0f)]));
    }
    out
}

/// Returns the bytes `text` spells in hex, `None` for an odd length or a
/// character that is no hex digit.
pub fn unhex(text: &str) -> Option<Vec<u8>> {
    let digits = text.as_bytes();
    if !digits.len().is_multiple_of(2) {
        return None;
    }
    digits
        .chunks_exact(2)
        .map(|pair| Some((nibble(pair[0])? << 4) | nibble(pair[1])?))
        .collect()
}

/// Returns the value of one hex digit.
fn nibble(digit: u8) -> Option<u8> {
    match digit {
        b'0'..=b'9' => Some(digit - b'0'),
        b'a'..=b'f' => Some(digit - b'a' + 10),
        b'A'..=b'F' => Some(digit - b'A' + 10),
        _ => None,
    }
}

/// Returns the name of a decode error, the name of its variant.
pub fn decode_error_name(error: DecodeError) -> &'static str {
    match error {
        DecodeError::Short => "Short",
        DecodeError::Version => "Version",
        DecodeError::Reserved => "Reserved",
        DecodeError::Type => "Type",
        DecodeError::Length => "Length",
        DecodeError::Range => "Range",
        DecodeError::Form => "Form",
    }
}

/// A wire form a request names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WireForm {
    /// Version 0 of the compact form.
    Compact,
    /// `homa_wire.h` at the pinned commit.
    Full,
}

/// Reads the wire form under `form`.
///
/// # Errors
///
/// Returns a refusal for a value that names no wire form.
pub fn check_form(fields: &Fields<'_>) -> Result<WireForm, JsString> {
    match fields.str("form")? {
        "compact" => Ok(WireForm::Compact),
        "full" => Ok(WireForm::Full),
        other => Err(fields.refuse("form", &format!("names no wire form: {other}"))),
    }
}

/// Returns the bytes of the largest header of either form.
const HEADER_ROOM: usize = if Compact::MAX_HEADER > Full::MAX_HEADER {
    Compact::MAX_HEADER
} else {
    Full::MAX_HEADER
};

/// Answers the operation `encode`.
fn op_encode(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "encode");
    let form = check_form(&fields)?;
    let spec = fields.nested("packet")?;
    let ports = match fields.opt_object("ports")? {
        Some(object) => {
            let ports = Fields::new(object, "encode");
            Ports {
                src: ports.int_or("src", 0)?,
                dst: ports.int_or("dst", 0)?,
            }
        }
        None => Ports::default(),
    };
    let need_limits = fields.bool_or("need_limits", false)?;
    let owned = OwnedPacket::read(&spec)?;
    let packet = owned.packet();
    let mut header = [0u8; HEADER_ROOM];
    let mut payload = JsObject::with_capacity(1);
    let written = match form {
        WireForm::Compact => Compact::header(&packet, ports, need_limits, &mut header),
        WireForm::Full => Full::header(&packet, ports, need_limits, &mut header),
    };
    match written {
        Ok(len) => {
            let mut bytes = header.get(..len).unwrap_or(&[]).to_vec();
            if let Packet::Data(data) = packet {
                bytes.extend_from_slice(data.payload);
            }
            payload.insert("hex", hex(&bytes));
        }
        Err(_) => {
            payload.insert("error", "NoRoom");
        }
    }
    Ok(payload)
}

/// Answers the operation `decode`.
fn op_decode(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "decode");
    let form = check_form(&fields)?;
    let bytes = fields.hex("hex")?;
    let mut payload = JsObject::with_capacity(1);
    let decoded = match form {
        WireForm::Compact => Compact::decode(&bytes),
        WireForm::Full => Full::decode(&bytes),
    };
    match decoded {
        Ok(decoded) => payload.insert("decoded", decoded_json(&decoded)),
        Err(error) => payload.insert("error", decode_error_name(error)),
    };
    Ok(payload)
}

/// Returns the name of a refusal of a packet, the name of its variant with
/// the name of the decode error a `Malformed` carries.
pub fn refused_name(refused: Refused) -> String {
    match refused {
        Refused::Malformed(error) => format!("Malformed:{}", decode_error_name(error)),
        Refused::NoPeer => String::from("NoPeer"),
        Refused::OverLimits => String::from("OverLimits"),
        Refused::NoSlot => String::from("NoSlot"),
        Refused::ThirdGap => String::from("ThirdGap"),
        Refused::ClockBackwards => String::from("ClockBackwards"),
        Refused::Freeze => String::from("Freeze"),
    }
}

/// Answers the operation `take`.
///
/// The endpoint runs the Network profile over IPv4 at an MTU of 1,500 with
/// the full form, owns the Homa port `port`, and takes every packet from the
/// peer 1 at the time of its index in nanoseconds.
fn op_take(request: &JsObject) -> Result<JsObject, JsString> {
    let fields = Fields::new(request, "take");
    if check_form(&fields)? != WireForm::Full {
        return Err(fields.refuse("form", "is not full, the one form take runs"));
    }
    let port: u16 = fields.int_or("port", 3_000)?;
    let config = Config {
        peers: 4,
        client_slots: 4,
        server_slots: 4,
        page_len: 4_096,
        pages: 64,
        max_gaps: 2,
        size_buckets: 0,
        first_id: 2,
        hash_key: [0x5a; 16],
        port,
        extra_ports: 0,
    };
    let profile = Profile::network(1_500, Encap::Ip4);
    let mut endpoint: Endpoint<u16, Owned<u16>, Full> =
        Endpoint::new(config, profile, Owned::new(&config)).map_err(|error| {
            JsString::from(format!("take: the endpoint does not build: {error}"))
        })?;
    let mut results = Vec::new();
    for (time, value) in fields.array("hexes")?.iter().enumerate() {
        let JsValue::String(text) = value else {
            return Err(fields.refuse("hexes", "holds a value that is no string"));
        };
        let bytes = text.as_str().and_then(unhex).ok_or_else(|| {
            fields.refuse("hexes", "holds a value that is no even run of hex digits")
        })?;
        let before = endpoint.stats().wrong_port;
        let taken = endpoint.handle(count(time), 1, &bytes);
        let outcome = match taken {
            Ok(()) if endpoint.stats().wrong_port > before => String::from("wrong_port"),
            Ok(()) => String::from("taken"),
            Err(refused) => refused_name(refused),
        };
        results.push(JsValue::from(outcome));
    }
    let stats = endpoint.stats();
    let mut counters = JsObject::new();
    counters.insert("received", stats.received);
    counters.insert("refused", stats.refused);
    counters.insert("freezes_dropped", stats.freezes_dropped);
    counters.insert("wrong_port", stats.wrong_port);
    let mut payload = JsObject::with_capacity(2);
    payload.insert("results", results);
    payload.insert("stats", counters);
    Ok(payload)
}

/// Returns a decoded packet as the object the notebooks read.
pub fn decoded_json(decoded: &Decoded<'_>) -> JsObject {
    let mut ports = JsObject::with_capacity(2);
    ports.insert("src", u32::from(decoded.ports.src));
    ports.insert("dst", u32::from(decoded.ports.dst));
    let mut object = JsObject::with_capacity(3);
    object.insert("ports", ports);
    object.insert("packet", packet_json(&decoded.packet));
    object.insert("need_limits", decoded.need_limits);
    object
}

/// Returns a packet as the object the notebooks read.
pub fn packet_json(packet: &Packet<'_>) -> JsObject {
    let mut object = JsObject::new();
    match *packet {
        Packet::Data(data) => {
            object.insert("type", "DATA");
            object.insert("id", data.id);
            object.insert("msg_length", data.msg_length);
            object.insert("offset", data.offset);
            object.insert("retransmit", data.retransmit);
            object.insert("ack", data.ack.map_or(JsValue::Null, ack_json));
            object.insert("cutoff_version", u32::from(data.cutoff_version));
            object.insert("incast", data.incast);
            object.insert("payload", hex(data.payload));
        }
        Packet::Grant {
            id,
            offset,
            priority,
        } => {
            object.insert("type", "GRANT");
            object.insert("id", id);
            object.insert("offset", offset);
            object.insert("priority", u32::from(priority.level()));
        }
        Packet::Resend {
            id,
            offset,
            length,
            priority,
        } => {
            object.insert("type", "RESEND");
            object.insert("id", id);
            object.insert("offset", offset);
            object.insert("length", length);
            object.insert("priority", u32::from(priority.level()));
        }
        Packet::RpcUnknown { id } => {
            object.insert("type", "RPC_UNKNOWN");
            object.insert("id", id);
        }
        Packet::Busy { id } => {
            object.insert("type", "BUSY");
            object.insert("id", id);
        }
        Packet::Cutoffs { cutoffs, version } => {
            object.insert("type", "CUTOFFS");
            let levels: Vec<JsValue> = cutoffs.iter().map(|&c| JsValue::from(c)).collect();
            object.insert("cutoffs", levels);
            object.insert("version", u32::from(version));
        }
        Packet::Freeze => {
            object.insert("type", "FREEZE");
        }
        Packet::NeedAck { id } => {
            object.insert("type", "NEED_ACK");
            object.insert("id", id);
        }
        Packet::Ack { id, acks } => {
            object.insert("type", "ACK");
            object.insert("id", id);
            let entries: Vec<JsValue> = acks.iter().map(ack_json).collect();
            object.insert("acks", entries);
        }
        Packet::StartMsg { id, msg_length } => {
            object.insert("type", "START_MSG");
            object.insert("id", id);
            object.insert("msg_length", msg_length);
        }
        Packet::Limits(limits) => {
            object.insert("type", "LIMITS");
            object.insert("max_packet", u32::from(limits.max_packet));
            object.insert("max_message", limits.max_message);
            object.insert("unsched_bytes", limits.unsched_bytes);
            object.insert("max_rpcs", u32::from(limits.max_rpcs));
        }
    }
    object
}

/// Returns one acknowledgement as an object.
fn ack_json(entry: AckEntry) -> JsValue {
    let mut object = JsObject::with_capacity(2);
    object.insert("client_id", entry.client_id);
    object.insert("server_port", u32::from(entry.server_port));
    JsValue::Object(object)
}

/// A packet read from its object, which owns what the packet borrows.
struct OwnedPacket {
    /// The type of the packet.
    kind: String,
    /// The identifier.
    id: u64,
    /// `offset`, of DATA, GRANT and RESEND.
    offset: u32,
    /// `length` of RESEND, or `msg_length` of DATA and `START_MSG`.
    length: u32,
    /// The level of GRANT and RESEND.
    priority: Priority,
    /// Whether DATA is sent again.
    retransmit: bool,
    /// The acknowledgement of DATA.
    ack: Option<AckEntry>,
    /// The cutoff version of DATA and CUTOFFS.
    version: u16,
    /// The incast mark of DATA.
    incast: bool,
    /// The payload of DATA.
    payload: Vec<u8>,
    /// The entries of ACK.
    acks: Vec<AckEntry>,
    /// The cutoffs of CUTOFFS.
    cutoffs: [u32; 8],
    /// The body of LIMITS.
    limits: Limits,
}

impl OwnedPacket {
    /// Reads a packet from its object.
    fn read(fields: &Fields<'_>) -> Result<Self, JsString> {
        let kind = fields.str("type")?.to_owned();
        let priority = Priority::new(fields.int_or("priority", 7)?)
            .ok_or_else(|| fields.refuse("priority", "is above 7"))?;
        let ack = match fields.opt_object("ack")? {
            Some(object) => Some(ack_entry(&Fields::new(object, fields.op))?),
            None => None,
        };
        let mut acks = Vec::new();
        for entry in fields.array("acks")? {
            let JsValue::Object(object) = entry else {
                return Err(fields.refuse("acks", "holds an entry that is no object"));
            };
            acks.push(ack_entry(&Fields::new(object, fields.op))?);
        }
        let mut cutoffs = [0u32; 8];
        for (slot, value) in cutoffs.iter_mut().zip(fields.array("cutoffs")?) {
            *slot = whole(value)
                .and_then(|level| u32::try_from(level).ok())
                .ok_or_else(|| fields.refuse("cutoffs", "holds a value that is no u32"))?;
        }
        let length = if kind == "RESEND" {
            fields.int_or("length", 0)?
        } else {
            fields.int_or("msg_length", 0)?
        };
        let version_key = if kind == "CUTOFFS" {
            "version"
        } else {
            "cutoff_version"
        };
        let payload = match fields.opt_str("payload")? {
            Some(text) => unhex(text)
                .ok_or_else(|| fields.refuse("payload", "is no even run of hex digits"))?,
            None => Vec::new(),
        };
        Ok(Self {
            id: fields.int_or("id", 0)?,
            offset: fields.int_or("offset", 0)?,
            length,
            priority,
            retransmit: fields.bool_or("retransmit", false)?,
            ack,
            version: fields.int_or(version_key, 0)?,
            incast: fields.bool_or("incast", false)?,
            payload,
            acks,
            cutoffs,
            limits: Limits {
                max_packet: fields.int_or("max_packet", 0)?,
                max_message: fields.int_or("max_message", 0)?,
                unsched_bytes: fields.int_or("unsched_bytes", 0)?,
                max_rpcs: fields.int_or("max_rpcs", 0)?,
            },
            kind: Self::check_kind(fields, kind)?,
        })
    }

    /// Refuses a type no packet has.
    fn check_kind(fields: &Fields<'_>, kind: String) -> Result<String, JsString> {
        const KINDS: [&str; 11] = [
            "DATA",
            "GRANT",
            "RESEND",
            "RPC_UNKNOWN",
            "BUSY",
            "CUTOFFS",
            "FREEZE",
            "NEED_ACK",
            "ACK",
            "START_MSG",
            "LIMITS",
        ];
        if KINDS.contains(&kind.as_str()) {
            Ok(kind)
        } else {
            Err(fields.refuse("type", &format!("names no packet: {kind}")))
        }
    }

    /// Returns the packet, borrowing its payload and its entries.
    fn packet(&self) -> Packet<'_> {
        let id = self.id;
        match self.kind.as_str() {
            "DATA" => Packet::Data(Data {
                id,
                msg_length: self.length,
                offset: self.offset,
                retransmit: self.retransmit,
                ack: self.ack,
                cutoff_version: self.version,
                incast: self.incast,
                payload: &self.payload,
            }),
            "GRANT" => Packet::Grant {
                id,
                offset: self.offset,
                priority: self.priority,
            },
            "RESEND" => Packet::Resend {
                id,
                offset: self.offset,
                length: self.length,
                priority: self.priority,
            },
            "RPC_UNKNOWN" => Packet::RpcUnknown { id },
            "BUSY" => Packet::Busy { id },
            "CUTOFFS" => Packet::Cutoffs {
                cutoffs: self.cutoffs,
                version: self.version,
            },
            "FREEZE" => Packet::Freeze,
            "NEED_ACK" => Packet::NeedAck { id },
            "ACK" => Packet::Ack {
                id,
                acks: Acks::from_entries(&self.acks),
            },
            "START_MSG" => Packet::StartMsg {
                id,
                msg_length: self.length,
            },
            _ => Packet::Limits(self.limits),
        }
    }
}

/// Reads one acknowledgement.
fn ack_entry(fields: &Fields<'_>) -> Result<AckEntry, JsString> {
    Ok(AckEntry {
        client_id: fields.u64("client_id")?,
        server_port: fields.int_or("server_port", 0)?,
    })
}

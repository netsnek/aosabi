// SPDX-License-Identifier: MIT
/*
 * aosabi-http: the compress middleware, over the encoder of aosabi-http under
 * the feature web-deflate or with the coding left to the host under the
 * feature platform-coding.
 *
 * The rule is hono's with the list of codings deflate alone, since Aosabi codes
 * no gzip, and with Accept-Encoding read as a list of codings and their weights
 * in place of hono's substring test, divergence D71 of docs/byte-equality.md
 * section 3.
 *
 * Derived from hono 4.11.9, dist/middleware/compress/index.js, MIT.
 * Derived from hono 4.11.9, dist/utils/compress.js, MIT.
 */

//! `compress`, the content coding of an answer.
//!
//! The middleware runs after the rest of the chain and leaves an answer alone
//! that carries `Content-Encoding` or `Transfer-Encoding`, answers a `HEAD`,
//! carries a `Content-Length` below the threshold of 1,024 bytes, has a type
//! outside the compressible expression or carries
//! `Cache-Control: no-transform`. Otherwise it picks the coding of its list
//! that `Accept-Encoding` accepts, codes the body as a stream, deletes
//! `Content-Length` and sets `Content-Encoding`. It reads the field as a list
//! of codings with their weights, [`accepted_coding`], where hono takes the
//! first coding whose name is a substring of the field, the named divergence
//! D71 of `docs/byte-equality.md` section 3. The list is `deflate` alone, since
//! Aosabi codes no gzip, so an answer a TypeScript app codes as `gzip` is
//! `deflate` or identity here, the named divergence D43 of the same section, with the body
//! equal after decoding. A coding is never forced, so the option `encoding` of
//! the reference, which codes one coding whatever the request accepts, is not
//! ported.
//!
//! The base pipeline of a TypeScript app installs `compress()` on every route,
//! while the base
//! pipeline of a Rust service of Aosabi codes no answer, so an app writes
//! `compress()` before the routes it codes, the named divergence D72 of
//! `docs/byte-equality.md` section 3.
//!
//! Under the feature `web-deflate` the body is coded by
//! `aosabi_http::coding::zlib::Encoder` over a scratch of the card's
//! parameters, `card_params`: the sizes of `aosabi_http::coding::SERVER`, a
//! window of 32 KiB, with `LINUX_HEADS` hash heads on a Linux target of 64
//! bits, the `server`, `server-legacy` and `docker` cards, and with the 4,096
//! hash heads of `SERVER` on every other target, the player among them. The
//! scratch stays with the thread between coded answers, so an answer of a
//! thread that coded one before allocates and zeroes no scratch, and the
//! encoder of each stream empties its hash heads and its chains. Under the
//! feature `platform-coding` without `web-deflate`, the Worker's build, the
//! headers change the same way and the answer is marked with
//! [`HostCoding::Deflate`], so the glue of the Worker codes it through the
//! platform's `CompressionStream('deflate')` and the build links no encoder.
//! Where both features are on, `web-deflate` wins. With neither, every answer
//! passes as it is.
//!
//! The coded body is a stream whose first poll is not ready, as the first read
//! of a `CompressionStream` is not within the two ticks the adapter waits, so
//! the adapter writes a coded answer chunked as the reference does (A2 and A5
//! of the plan's version 2 draft, capture row C31).
//!
//! [`HostCoding::Deflate`]: crate::fetch::response::HostCoding::Deflate

use crate::{
    compose::Next,
    context::Context,
    fetch::{
        body::LocalBoxFuture,
        response::Response, //
    },
    web_app::{
        MiddlewareFn,
        RouteError, //
    }, //
};

/// The codings `compress()` picks from, in order, `deflate` alone.
pub const ENCODING_TYPES: [&str; 1] = ["deflate"];

/// The threshold of `compress()`, the length below which an answer stays uncoded.
pub const THRESHOLD: u64 = 1024;

/// The hash heads of the encoder of `compress()` on a Linux target of 64 bits,
/// the `server`, `server-legacy` and `docker` cards, 32,768, which hold 64 KiB
/// of each coded answer.
///
/// A head holds the newest position of one hash of 4 bytes, so eight times the
/// 4,096 heads of `aosabi_http::coding::SERVER` leave fewer positions of
/// another hash on a chain the search follows.
#[cfg(feature = "web-deflate")]
pub const LINUX_HEADS: usize = 32_768;

/// The options of `compress()`, `{ threshold }`.
#[derive(Clone, Copy, Debug)]
pub struct CompressOptions {
    /// The length below which an answer with a `Content-Length` stays uncoded.
    pub threshold: u64,
    /// The parameters of the scratch of the encoder, the card's.
    #[cfg(feature = "web-deflate")]
    pub params: graphtp::deflate::Params,
}

impl Default for CompressOptions {
    fn default() -> Self {
        Self {
            threshold: THRESHOLD,
            #[cfg(feature = "web-deflate")]
            params: card_params(),
        }
    }
}

/// Returns the sizes of the scratch of the encoder on the card the build is for.
///
/// They are the sizes of `aosabi_http::coding::SERVER`, a window of 32 KiB,
/// blocks of 8,192 tokens and a buffer of 65,536 plain bytes, with
/// [`LINUX_HEADS`] hash heads on a Linux target of 64 bits and with the 4,096
/// heads of `SERVER` on every other target. The player on
/// `mipsel-unknown-linux-musl` is among those, since the plan adds no memory on
/// it.
#[cfg(feature = "web-deflate")]
#[must_use]
pub const fn card_params() -> graphtp::deflate::Params {
    let params = crate::coding::SERVER;
    if cfg!(all(target_os = "linux", target_pointer_width = "64")) {
        graphtp::deflate::Params {
            heads: LINUX_HEADS,
            ..params
        }
    } else {
        params
    }
}

/// Returns the middleware `compress()` with no options.
#[must_use]
pub fn compress<E: RouteError>() -> impl MiddlewareFn<E> {
    compress_with(CompressOptions::default())
}

/// Returns the middleware `compress(options)`.
#[must_use]
pub fn compress_with<E: RouteError>(options: CompressOptions) -> impl MiddlewareFn<E> {
    move |c: Context, next: Next<E>| -> LocalBoxFuture<'static, Result<Response, E>> {
        Box::pin(async move {
            next.run(c.clone()).await?;
            let res = c.res();
            if skips(&c, &res, options.threshold) {
                return Ok(res);
            }
            let accepted = c.req().header("Accept-Encoding");
            let Some(encoding) = accepted_coding(accepted.as_deref()) else {
                return Ok(res);
            };
            encode(&c, res, encoding, options)
        })
    }
}

/// Codes the answer `res` of `c` in `encoding`, which the request accepts,
/// where its body is not empty and the coding gives one.
///
/// The tail of `compress/index.js:17-27`.
// Out of line, so an answer to a request that accepts no coding compiles none
// of it.
#[inline(never)]
fn encode<E: RouteError>(
    c: &Context,
    res: Response,
    encoding: &str,
    options: CompressOptions,
) -> Result<Response, E> {
    let body = res.body();
    if body.is_empty() {
        return Ok(res);
    }
    let Some(coded) = code(&body, options) else {
        return Ok(res);
    };
    let coded =
        Response::from_response(coded, &res).map_err(|error| E::type_error(&error.message))?;
    c.set_res(coded);
    let res = c.res();
    res.delete_header("Content-Length");
    res.set_header("Content-Encoding", encoding)
        .map_err(|error| E::type_error(&error.message))?;
    mark_host_coding(&res);
    Ok(res)
}

/// Returns whether the answer `res` stays uncoded before the coding is picked.
fn skips(c: &Context, res: &Response, threshold: u64) -> bool {
    let content_length = res.header("Content-Length");
    #[expect(clippy::cast_precision_loss)]
    let threshold = threshold as f64;
    res.has_header("Content-Encoding")
        || res.has_header("Transfer-Encoding")
        || c.req().method() == "HEAD"
        || content_length
            .as_deref()
            .is_some_and(|length| !length.is_empty() && string_to_number(length) < threshold)
        || !should_compress(res)
        || !should_transform(res)
}

/// Returns whether the type of `res` is compressible.
fn should_compress(res: &Response) -> bool {
    res.header("Content-Type")
        .is_some_and(|content_type| is_compressible(&content_type))
}

/// Returns whether `res` allows a transform, which `Cache-Control: no-transform` forbids.
fn should_transform(res: &Response) -> bool {
    let Some(cache_control) = res.header("Cache-Control") else {
        return true;
    };
    !cache_control.split(',').any(|directive| {
        directive
            .trim_matches(is_js_space)
            .eq_ignore_ascii_case("no-transform")
    })
}

/// Returns the coding of [`ENCODING_TYPES`] that the field `Accept-Encoding`
/// accepts, `None` for identity.
///
/// The field is a list of codings, each with an optional weight `q` from 0 to 1
/// with at most three decimals, and a coding matches its name in any case. A
/// coding takes the weight of the first member that names it, else that of the
/// first member `*`, else 0, and the coding of the highest weight above 0 wins,
/// the first of the list on a tie. A member that breaks the grammar names no
/// coding, so `deflate;q=2` accepts none, and an empty member is skipped. A
/// request without the field gets identity, as from hono, where RFC 9110 would
/// accept any coding. Hono takes the first coding whose name is a substring of
/// the field, so `deflate;q=0` and `x-deflate` get deflate there and identity
/// here, and `DEFLATE` and `*` get deflate here and identity there, the named
/// divergence D71 of `docs/byte-equality.md` section 3.
#[must_use]
pub fn accepted_coding(accept_encoding: Option<&str>) -> Option<&'static str> {
    let field = accept_encoding?;
    let mut picked: Option<(&'static str, u16)> = None;
    for coding in ENCODING_TYPES {
        let weight = weight_of(field, coding);
        if weight > 0 && picked.is_none_or(|(_, best)| weight > best) {
            picked = Some((coding, weight));
        }
    }
    picked.map(|(coding, _)| coding)
}

/// Returns the weight in thousandths that `field` gives `coding`: that of the
/// first member naming it, else that of the first member `*`, else 0.
fn weight_of(field: &str, coding: &str) -> u16 {
    let mut any = None;
    for (name, weight) in field.split(',').filter_map(member) {
        if name.eq_ignore_ascii_case(coding) {
            return weight;
        }
        if name == "*" && any.is_none() {
            any = Some(weight);
        }
    }
    any.unwrap_or(0)
}

/// Returns the coding and the weight in thousandths of one member of the
/// field, `codings [ OWS ";" OWS "q=" qvalue ]` between optional white space,
/// or `None` for an empty member and for one that breaks the grammar.
fn member(text: &str) -> Option<(&str, u16)> {
    let (name, weight) = match text.split_once(';') {
        Some((name, weight)) => (name, Some(weight)),
        None => (text, None),
    };
    let name = name.trim_matches(is_ows);
    if name.is_empty() || !name.bytes().all(is_tchar) {
        return None;
    }
    let weight = match weight {
        None => 1000,
        Some(weight) => {
            let weight = weight.trim_matches(is_ows);
            let value = weight
                .strip_prefix("q=")
                .or_else(|| weight.strip_prefix("Q="))?;
            qvalue(value)?
        }
    };
    Some((name, weight))
}

/// Returns the qvalue `text` in thousandths, `0` with at most three decimals or
/// `1` with at most three zeros, or `None` for any other text.
fn qvalue(text: &str) -> Option<u16> {
    let (whole, fraction) = text.split_once('.').unwrap_or((text, ""));
    if fraction.len() > 3 || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    match whole {
        "0" => Some(
            fraction
                .bytes()
                .chain(core::iter::repeat(b'0'))
                .take(3)
                .fold(0, |sum, digit| sum * 10 + u16::from(digit - b'0')),
        ),
        "1" if fraction.bytes().all(|byte| byte == b'0') => Some(1000),
        _ => None,
    }
}

/// Returns whether `byte` is a `tchar` of a token.
fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte)
}

/// Returns whether `c` is the optional white space of RFC 9110 section 5.6.3,
/// a space or a tab.
fn is_ows(c: char) -> bool {
    c == ' ' || c == '\t'
}

/// Returns whether `content_type` matches the compressible expression of hono.
#[must_use]
pub fn is_compressible(content_type: &str) -> bool {
    const APPLICATION: [&str; 27] = [
        "javascript",
        "json",
        "xml",
        "xml-dtd",
        "ecmascript",
        "dart",
        "postscript",
        "rtf",
        "tar",
        "toml",
        "vnd.dart",
        "vnd.ms-fontobject",
        "vnd.ms-opentype",
        "wasm",
        "x-httpd-php",
        "x-javascript",
        "x-ns-proxy-autoconfig",
        "x-sh",
        "x-tar",
        "x-virtualbox-hdd",
        "x-virtualbox-ova",
        "x-virtualbox-ovf",
        "x-virtualbox-vbox",
        "x-virtualbox-vdi",
        "x-virtualbox-vhd",
        "x-virtualbox-vmdk",
        "x-www-form-urlencoded",
    ];
    const WHOLE: [&str; 12] = [
        "font/otf",
        "font/ttf",
        "image/bmp",
        "image/vnd.adobe.photoshop",
        "image/vnd.microsoft.icon",
        "image/vnd.ms-dds",
        "image/x-icon",
        "image/x-ms-bmp",
        "message/rfc822",
        "model/gltf-binary",
        "x-shader/x-fragment",
        "x-shader/x-vertex",
    ];
    let lower = content_type
        .trim_start_matches(is_js_space)
        .to_ascii_lowercase();
    let token_end = lower
        .find(|c: char| c == ';' || is_js_space(c))
        .unwrap_or(lower.len());
    let token = &lower[..token_end];
    if let Some(subtype) = token.strip_prefix("text/") {
        return !subtype.is_empty() && subtype != "event-stream";
    }
    if let Some(subtype) = token.strip_prefix("application/")
        && APPLICATION.contains(&subtype)
    {
        return true;
    }
    if WHOLE.contains(&token) {
        return true;
    }
    ["+json", "+text", "+xml", "+yaml"]
        .iter()
        .any(|suffix| token.len() > suffix.len() && token.ends_with(suffix))
}

/// Returns whether `c` is white space as `\s` of a JavaScript expression reads it.
fn is_js_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{0B}' | '\u{0C}' | '\r' | ' ' | '\u{A0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// Returns `Number(value)` for a string, the `StringToNumber` of ECMA-262.
///
/// White space around the number is dropped, an empty string is 0, `0x`, `0o`
/// and `0b` read an integer in their base, `Infinity` with a sign reads the
/// infinity, and any other text that is no decimal literal is `NaN`.
#[must_use]
pub fn string_to_number(value: &str) -> f64 {
    let text = value.trim_matches(is_js_space);
    if text.is_empty() {
        return 0.0;
    }
    let lower = text.to_ascii_lowercase();
    for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
        if let Some(digits) = lower.strip_prefix(prefix) {
            if digits.is_empty() || !digits.chars().all(|c| c.is_digit(radix)) {
                return f64::NAN;
            }
            return digits.chars().fold(0.0, |acc, c| {
                acc * f64::from(radix) + f64::from(c.to_digit(radix).unwrap_or(0))
            });
        }
    }
    match text {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    let unsigned = text.strip_prefix(['+', '-']).unwrap_or(text);
    let decimal = unsigned
        .chars()
        .all(|c| c.is_ascii_digit() || matches!(c, '.' | 'e' | 'E' | '+' | '-'))
        && unsigned.starts_with(|c: char| c.is_ascii_digit() || c == '.');
    if !decimal {
        return f64::NAN;
    }
    text.parse::<f64>().unwrap_or(f64::NAN)
}

/// Returns the coded body of `body`, or `None` where the build codes nothing.
#[cfg(feature = "web-deflate")]
fn code(
    body: &crate::fetch::body::Body,
    options: CompressOptions,
) -> Option<crate::fetch::body::Body> {
    let stream = body.to_stream()?;
    Some(crate::fetch::body::Body::Stream(coding::deflate_stream(
        stream,
        options.params,
    )))
}

/// Returns the body of the answer whose coding the host applies, the body as it is.
// The three builds share one signature, and the build of the host's coding
// always has a body to answer.
#[cfg(all(feature = "platform-coding", not(feature = "web-deflate")))]
#[expect(clippy::unnecessary_wraps)]
fn code(
    body: &crate::fetch::body::Body,
    _options: CompressOptions,
) -> Option<crate::fetch::body::Body> {
    Some(body.clone())
}

/// Returns `None`, since a build with neither coding codes nothing.
#[cfg(not(any(feature = "web-deflate", feature = "platform-coding")))]
fn code(
    _body: &crate::fetch::body::Body,
    _options: CompressOptions,
) -> Option<crate::fetch::body::Body> {
    None
}

/// Marks `res` with the coding the host applies.
#[cfg(all(feature = "platform-coding", not(feature = "web-deflate")))]
fn mark_host_coding(res: &Response) {
    res.set_host_coding(crate::fetch::response::HostCoding::Deflate);
}

/// Marks nothing, since the build codes the answer itself or not at all.
#[cfg(not(all(feature = "platform-coding", not(feature = "web-deflate"))))]
fn mark_host_coding(_res: &Response) {}

/// The coding stream of the feature `web-deflate`.
#[cfg(feature = "web-deflate")]
mod coding {
    use core::{
        cell::{
            Cell,
            RefCell, //
        },
        pin::Pin,
        task::{
            Context,
            Poll, //
        }, //
    };
    use std::{
        collections::VecDeque,
        rc::Rc, //
    };

    use crate::coding::zlib::Encoder;
    use graphtp::deflate::{
        Params,
        Scratch,
        ScratchError, //
    };

    use crate::fetch::body::{
        BodyError,
        BodyStream,
        Chunk,
        LocalBoxFuture,
        Stream, //
    };

    /// The coded chunks a coder has made and not yet handed out.
    type Queue = Rc<RefCell<VecDeque<Chunk>>>;

    std::thread_local! {
        /// The scratch the last coded answer of this thread left, which the
        /// next one takes.
        static KEPT: Cell<Option<Scratch>> = const { Cell::new(None) };
    }

    /// The scratch of one coded answer, which goes back to the slot of its
    /// thread when the coder ends or is dropped.
    ///
    /// A scratch stays with the thread between coded answers, so the answer of
    /// a thread that coded one before allocates no scratch and zeroes none. The
    /// DEFLATE encoder that [`Encoder::new`] makes for each stream empties the
    /// hash heads and the chains of the scratch it takes, so a stream codes to
    /// the same bytes whatever streams the scratch served before. The slot
    /// holds one scratch, the last one left, and frees any other, so a thread
    /// keeps at most one scratch, of the sizes [`super::card_params`] gives its
    /// card.
    struct Lent(Option<Scratch>);

    impl Lent {
        /// Returns the scratch of the sizes `params` the slot of this thread
        /// holds, or a new one where it holds none of these sizes.
        fn take(params: Params) -> Result<Lent, ScratchError> {
            let kept = KEPT.try_with(Cell::take).ok().flatten();
            let scratch = match kept {
                Some(scratch) if scratch.params() == params => scratch,
                _ => Scratch::new(params)?,
            };
            Ok(Lent(Some(scratch)))
        }
    }

    impl Drop for Lent {
        fn drop(&mut self) {
            if let Some(scratch) = self.0.take() {
                // A thread whose slot is gone, since the thread ends, frees the scratch.
                let _ = KEPT.try_with(|slot| slot.set(Some(scratch)));
            }
        }
    }

    /// A body coded as `deflate` while it is read.
    ///
    /// The coder is an async block that owns the scratch and the encoder that
    /// borrows it, reads the plain body and puts each coded piece into the
    /// queue, so the stream holds the borrow across its polls in safe code.
    struct DeflateStream {
        /// The coded pieces not yet handed out.
        queue: Queue,
        /// The coder, `None` once it has ended.
        coder: Option<LocalBoxFuture<'static, ()>>,
        /// Whether the first poll has answered.
        polled: bool,
    }

    impl Stream for DeflateStream {
        type Item = Chunk;

        fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Chunk>> {
            let this = self.get_mut();
            if !this.polled {
                this.polled = true;
                cx.waker().wake_by_ref();
                return Poll::Pending;
            }
            loop {
                if let Some(chunk) = this.queue.borrow_mut().pop_front() {
                    return Poll::Ready(Some(chunk));
                }
                let Some(coder) = this.coder.as_mut() else {
                    return Poll::Ready(None);
                };
                match coder.as_mut().poll(cx) {
                    Poll::Ready(()) => this.coder = None,
                    Poll::Pending if this.queue.borrow().is_empty() => return Poll::Pending,
                    Poll::Pending => {}
                }
            }
        }
    }

    /// Returns the stream of `plain` coded as `deflate` over a scratch of `params`.
    ///
    /// The coder takes the scratch its thread keeps (see [`Lent`]) and
    /// leaves it there when it ends.
    pub(super) fn deflate_stream(plain: BodyStream, params: Params) -> BodyStream {
        let queue: Queue = Rc::new(RefCell::new(VecDeque::new()));
        let sink = Rc::clone(&queue);
        let coder: LocalBoxFuture<'static, ()> = Box::pin(async move {
            let push = |chunk: Chunk| sink.borrow_mut().push_back(chunk);
            let Ok(mut lent) = Lent::take(params) else {
                push(Err(BodyError::Failed(String::from(
                    "the scratch of the encoder could not be made",
                ))));
                return;
            };
            let Some(scratch) = lent.0.as_mut() else {
                return;
            };
            let mut encoder = Encoder::new(scratch);
            let mut out: Vec<u8> = Vec::new();
            while let Some(chunk) = plain.next().await {
                let plain = match chunk {
                    Ok(plain) => plain,
                    Err(error) => {
                        push(Err(error));
                        return;
                    }
                };
                if encoder.write(&plain, &mut out).is_err() {
                    push(Err(BodyError::Failed(String::from(
                        "the encoder had no room",
                    ))));
                    return;
                }
                if !out.is_empty() {
                    push(Ok(core::mem::take(&mut out)));
                }
            }
            if encoder.finish(&mut out).is_err() {
                push(Err(BodyError::Failed(String::from(
                    "the encoder had no room",
                ))));
                return;
            }
            push(Ok(out));
        });
        BodyStream::new(DeflateStream {
            queue,
            coder: Some(coder),
            polled: false,
        })
    }
}

// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the multipart parser of a form data body.
 *
 * Derived from node v24.13.0, deps/undici/src/lib/web/fetch/formdata-parser.js, MIT.
 */

//! `FormData` and the multipart parser.
//!
//! `request.formData()` of Node reads the whole body, takes the MIME type of
//! the request from its `Content-Type`, and parses a `multipart/form-data`
//! body with the parser of undici or an `application/x-www-form-urlencoded`
//! body as URL search parameters, and throws a `TypeError` for any other type.
//!
//! The parser keeps the quirks of undici: it starts at the first boundary it
//! finds, takes a body up to the next occurrence of the boundary string
//! without its dashes, and tests `CR` and `LF` after a header line and before
//! an attribute with `&&` where the specification reads `||`. A malformed body
//! throws `Failed to parse body as FormData.`, whose cause names the step, and
//! a percent escape that is no UTF-8 in an extended attribute throws the
//! `URIError` of `decodeURIComponent`, which the refusals of capture row C45
//! read.

use std::sync::Arc;

use aosabi_execute::{
    ErrorObject,
    Value,
    value::error::ErrorName, //
};
use aosabi_http::{
    Request,
    fetch::url::url_search_params, //
};
use aosabi_json::case::lower_ascii_exact;

use crate::file::FileBody;

/// The value of an entry, a string or a file.
#[derive(Clone, Debug, PartialEq)]
pub enum FormDataValue {
    /// A string.
    Text(String),
    /// A file.
    File(Arc<FileBody>),
}

/// An entry of a form, its name and its value.
#[derive(Clone, Debug, PartialEq)]
pub struct FormDataEntry {
    /// The name.
    pub name: String,
    /// The value.
    pub value: FormDataValue,
}

/// A form, `FormData`, its entries in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FormData {
    /// The entries.
    entries: Vec<FormDataEntry>,
}

impl FormData {
    /// Returns the value of the first entry named `name`, `formData.get(name)`.
    #[must_use]
    pub fn get(&self, name: &str) -> Option<&FormDataValue> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .map(|entry| &entry.value)
    }

    /// Returns the entries in order.
    #[must_use]
    pub fn entries(&self) -> &[FormDataEntry] {
        &self.entries
    }
}

/// A MIME type, `{ essence, parameters }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MimeType {
    /// The type and the subtype, lowercased and joined by `/`.
    pub essence: String,
    /// The parameters in order, the first of each name kept.
    pub parameters: Vec<(String, String)>,
}

impl MimeType {
    /// Returns the parameter `name`.
    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&str> {
        self.parameters
            .iter()
            .find(|(key, _)| key == name)
            .map(|(_, value)| value.as_str())
    }
}

/// Returns whether `c` is an HTTP token code point.
fn is_token_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || "!#$%&'*+-.^_|~".contains(c)
}

/// Returns whether `text` matches `HTTP_TOKEN_CODEPOINTS`, one token code point or more.
fn is_token(text: &str) -> bool {
    !text.is_empty() && text.chars().all(is_token_char)
}

/// Returns whether `c` is HTTP whitespace, a line feed, a carriage return, a tab or a space.
fn is_http_whitespace(c: char) -> bool {
    matches!(c, '\n' | '\r' | '\t' | ' ')
}

/// Returns whether `text` matches `HTTP_QUOTED_STRING_TOKENS`.
fn is_quoted_string_tokens(text: &str) -> bool {
    !text.is_empty()
        && text.chars().all(|c| {
            c == '\t' || ('\u{20}'..='\u{7E}').contains(&c) || ('\u{80}'..='\u{FF}').contains(&c)
        })
}

/// Collects the code points of `input` from `position` while `condition` holds.
fn collect_code_points(
    input: &[char],
    position: &mut usize,
    condition: impl Fn(char) -> bool,
) -> String {
    let start = *position;
    while *position < input.len() && condition(input[*position]) {
        *position += 1;
    }
    input[start..*position].iter().collect()
}

/// Collects the code points of `input` up to `stop` or to its end, `collectASequenceOfCodePointsFast`.
fn collect_until(input: &[char], position: &mut usize, stop: char) -> String {
    collect_code_points(input, position, |c| c != stop)
}

/// Collects an HTTP quoted string at `position`, `collectAnHTTPQuotedString(input, position, extractValue)`.
fn collect_quoted_string(input: &[char], position: &mut usize, extract_value: bool) -> String {
    let start = *position;
    let mut value = String::new();
    *position += 1;
    loop {
        value.push_str(&collect_code_points(input, position, |c| {
            c != '"' && c != '\\'
        }));
        if *position >= input.len() {
            break;
        }
        let quote_or_backslash = input[*position];
        *position += 1;
        if quote_or_backslash == '\\' {
            if *position >= input.len() {
                value.push('\\');
                break;
            }
            value.push(input[*position]);
            *position += 1;
        } else {
            break;
        }
    }
    if extract_value {
        value
    } else {
        input[start..*position].iter().collect()
    }
}

/// Parses `input` as a MIME type, `parseMIMEType(input)`, `None` for a failure.
#[must_use]
pub fn parse_mime_type(input: &str) -> Option<MimeType> {
    let input: Vec<char> = input.trim_matches(is_http_whitespace).chars().collect();
    let mut position = 0;
    let r#type = collect_until(&input, &mut position, '/');
    if !is_token(&r#type) || position >= input.len() {
        return None;
    }
    position += 1;
    let subtype = collect_until(&input, &mut position, ';');
    let subtype = subtype.trim_end_matches(is_http_whitespace);
    if !is_token(subtype) {
        return None;
    }
    let mut mime_type = MimeType {
        essence: format!(
            "{}/{}",
            r#type.to_ascii_lowercase(),
            subtype.to_ascii_lowercase()
        ),
        parameters: Vec::new(),
    };
    while position < input.len() {
        position += 1;
        collect_code_points(&input, &mut position, is_http_whitespace);
        let name = collect_code_points(&input, &mut position, |c| c != ';' && c != '=');
        // The name is kept only when it is a token, which is ASCII, so the
        // lower case keeps the mappings that hold ASCII alone.
        let parameter_name = lower_ascii_exact(&name);
        if position < input.len() {
            if input[position] == ';' {
                continue;
            }
            position += 1;
        }
        if position >= input.len() {
            break;
        }
        let parameter_value = if input[position] == '"' {
            let value = collect_quoted_string(&input, &mut position, true);
            collect_until(&input, &mut position, ';');
            value
        } else {
            let value = collect_until(&input, &mut position, ';');
            let value = String::from(value.trim_end_matches(is_http_whitespace));
            if value.is_empty() {
                continue;
            }
            value
        };
        if is_token(&parameter_name)
            && (parameter_value.is_empty() || is_quoted_string_tokens(&parameter_value))
            && mime_type.parameter(&parameter_name).is_none()
        {
            mime_type.parameters.push((parameter_name, parameter_value));
        }
    }
    Some(mime_type)
}

/// Splits a header value on the commas outside its quoted strings, `gettingDecodingSplitting(value)`.
fn getting_decoding_splitting(value: &str) -> Vec<String> {
    let input: Vec<char> = value.chars().collect();
    let mut position = 0;
    let mut values = Vec::new();
    let mut temporary = String::new();
    while position < input.len() {
        temporary.push_str(&collect_code_points(&input, &mut position, |c| {
            c != '"' && c != ','
        }));
        if position < input.len() {
            if input[position] == '"' {
                temporary.push_str(&collect_quoted_string(&input, &mut position, false));
                if position < input.len() {
                    continue;
                }
            } else {
                position += 1;
            }
        }
        values.push(String::from(
            temporary.trim_matches(|c| c == '\t' || c == ' '),
        ));
        temporary.clear();
    }
    values
}

/// Returns the MIME type of the `Content-Type` value `header`, `extractMimeType(headers)`, `None` for a failure.
#[must_use]
pub fn extract_mime_type(header: Option<&str>) -> Option<MimeType> {
    let mut charset: Option<String> = None;
    let mut essence: Option<String> = None;
    let mut mime_type: Option<MimeType> = None;
    for value in getting_decoding_splitting(header?) {
        let Some(mut temporary) =
            parse_mime_type(&value).filter(|temporary| temporary.essence != "*/*")
        else {
            continue;
        };
        if essence.as_deref() == Some(temporary.essence.as_str()) {
            if temporary.parameter("charset").is_none()
                && let Some(charset) = &charset
            {
                temporary
                    .parameters
                    .push((String::from("charset"), charset.clone()));
            }
        } else {
            charset = temporary.parameter("charset").map(String::from);
            essence = Some(temporary.essence.clone());
        }
        mime_type = Some(temporary);
    }
    mime_type
}

/// A failure of the multipart parser, the cause of `Failed to parse body as FormData.`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MultipartError {
    /// A step of the parser failed, `parsingError(cause)`.
    Parsing(&'static str),
    /// A percent escape of an extended attribute is no UTF-8, the `URIError` of `decodeURIComponent`.
    UriMalformed,
}

impl MultipartError {
    /// Returns the value the parser throws.
    #[must_use]
    pub fn thrown(&self) -> Value {
        match self {
            Self::Parsing(_) => {
                Value::from(ErrorObject::type_error("Failed to parse body as FormData."))
            }
            Self::UriMalformed => Value::from(ErrorObject::Plain {
                name: ErrorName::Other(aosabi_json::JsString::from("URIError")),
                message: aosabi_json::JsString::from("URI malformed"),
            }),
        }
    }
}

/// Reads the body of `request` into a form, `await request.formData()`.
///
/// # Errors
///
/// Returns the `TypeError` of a body read twice, the error of a body that
/// failed, the `TypeError` of a type that is no form, and the throw of the
/// multipart parser.
pub async fn request_form_data(request: &Request) -> Result<FormData, Value> {
    let bytes = request
        .bytes()
        .await
        .map_err(|error| Value::from(ErrorObject::type_error(error.to_string())))?;
    let mime_type = extract_mime_type(request.header("content-type").as_deref());
    match mime_type
        .as_ref()
        .map(|mime_type| mime_type.essence.as_str())
    {
        Some("multipart/form-data") => {
            let boundary = mime_type
                .as_ref()
                .and_then(|mime_type| mime_type.parameter("boundary"));
            let entries =
                multipart_form_data_parser(&bytes, boundary).map_err(|error| error.thrown())?;
            Ok(FormData { entries })
        }
        Some("application/x-www-form-urlencoded") => {
            let text = String::from_utf8_lossy(&bytes);
            // The constructor of URLSearchParams drops one leading `?` of a
            // string.
            let entries = url_search_params(text.strip_prefix('?').unwrap_or(&text))
                .into_iter()
                .map(|(name, value)| FormDataEntry {
                    name,
                    value: FormDataValue::Text(value),
                })
                .collect();
            Ok(FormData { entries })
        }
        _ => Err(Value::from(ErrorObject::type_error(
            "Content-Type was not one of \"multipart/form-data\" or \"application/x-www-form-urlencoded\".",
        ))),
    }
}

/// Returns the index of `needle` in `haystack` at or after `from`, `buffer.indexOf(needle, from)`.
fn index_of(haystack: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if needle.is_empty() {
        return Some(from.min(haystack.len()));
    }
    haystack
        .get(from..)?
        .windows(needle.len())
        .position(|window| window == needle)
        .map(|index| index + from)
}

/// Returns whether `input` holds `CR LF` at `position`.
fn is_crlf(input: &[u8], position: usize) -> bool {
    input.get(position) == Some(&b'\r') && input.get(position + 1) == Some(&b'\n')
}

/// Parses a multipart body, `multipartFormDataParser(input, mimeType)`.
///
/// # Errors
///
/// Returns the failed step.
pub fn multipart_form_data_parser(
    input: &[u8],
    boundary: Option<&str>,
) -> Result<Vec<FormDataEntry>, MultipartError> {
    let Some(boundary) = boundary else {
        return Err(MultipartError::Parsing(
            "missing boundary in content-type header",
        ));
    };
    let boundary = format!("--{boundary}").into_bytes();
    let mut entries = Vec::new();
    let Some(mut position) = index_of(input, &boundary, 0) else {
        return Err(MultipartError::Parsing(
            "no boundary found in multipart body",
        ));
    };
    loop {
        if input.get(position..position + boundary.len()) == Some(boundary.as_slice()) {
            position += boundary.len();
        } else {
            return Err(MultipartError::Parsing(
                "expected a value starting with -- and the boundary",
            ));
        }
        if input.len() >= 2 && input.get(position..position + 2) == Some(b"--".as_slice()) {
            return Ok(entries);
        }
        if !is_crlf(input, position) {
            return Err(MultipartError::Parsing("expected CRLF"));
        }
        position += 2;
        let headers = parse_multipart_form_data_headers(input, &mut position)?;
        position += 2;
        let Some(boundary_index) = index_of(input, &boundary[2..], position) else {
            return Err(MultipartError::Parsing("expected boundary after body"));
        };
        let end = boundary_index.saturating_sub(4).max(position);
        let start = position.min(input.len());
        let part = &input[start..end.min(input.len())];
        position += part.len();
        let decoded = (headers.encoding.as_deref() == Some("base64"))
            .then(|| base64_decode(&String::from_utf8_lossy(part)));
        if is_crlf(input, position) {
            position += 2;
        } else {
            return Err(MultipartError::Parsing("expected CRLF"));
        }
        let value = match &headers.filename {
            Some(filename) => {
                let content_type = headers
                    .content_type
                    .clone()
                    .unwrap_or_else(|| String::from("text/plain"));
                let content_type = if content_type.is_ascii() {
                    content_type
                } else {
                    String::new()
                };
                FormDataValue::File(Arc::new(file_of(
                    part,
                    start,
                    decoded.as_deref(),
                    filename,
                    &content_type,
                )))
            }
            None => FormDataValue::Text(aosabi_http::fetch::body::utf8_decode(
                decoded.as_deref().unwrap_or(part),
            )),
        };
        entries.push(FormDataEntry {
            name: headers.name,
            value,
        });
    }
}

/// Returns the file of the part `part` at `start` of the body, or of its
/// bytes `decoded` from base64.
///
/// Under the feature `host-body` a part read as it stands keeps its place in
/// the body and no bytes of its own, and any other build copies its bytes
/// once.
#[cfg(feature = "host-body")]
fn file_of(
    part: &[u8],
    start: usize,
    decoded: Option<&[u8]>,
    name: &str,
    r#type: &str,
) -> FileBody {
    match decoded {
        Some(bytes) => FileBody::new(bytes, name, r#type),
        None => FileBody::in_part(start..start + part.len(), name, r#type),
    }
}

/// Returns the file of the part `part`, or of its bytes `decoded` from
/// base64, with a copy of its bytes.
#[cfg(not(feature = "host-body"))]
fn file_of(
    part: &[u8],
    _start: usize,
    decoded: Option<&[u8]>,
    name: &str,
    r#type: &str,
) -> FileBody {
    FileBody::new(decoded.unwrap_or(part), name, r#type)
}

/// The headers of a part.
struct PartHeaders {
    /// The `name` of `Content-Disposition`.
    name: String,
    /// The `filename` of `Content-Disposition`.
    filename: Option<String>,
    /// `Content-Type`.
    content_type: Option<String>,
    /// `Content-Transfer-Encoding`.
    encoding: Option<String>,
}

/// Returns whether the byte `b` is a token character of the parser, `isToken(char)`.
fn is_token_byte(b: u8) -> bool {
    b <= 127 && b != b' ' && b != b'\t' && b > 31 && b != 127 && !b"()<>@,;:\\\"/[]?=".contains(&b)
}

/// Collects the bytes of `input` from `position` while `condition` holds, `collectASequenceOfBytes`.
fn collect_bytes<'a>(
    input: &'a [u8],
    position: &mut usize,
    condition: impl Fn(u8) -> bool,
) -> &'a [u8] {
    let start = (*position).min(input.len());
    let mut end = start;
    while end < input.len() && condition(input[end]) {
        end += 1;
    }
    *position = end;
    &input[start..end]
}

/// Removes the tabs and spaces around `bytes`, `removeChars(buf, leading, trailing, predicate)`.
///
/// The trailing walk stops before the first byte, as the reference's does.
fn remove_chars(bytes: &[u8], leading: bool, trailing: bool) -> &[u8] {
    let blank = |b: u8| b == b'\t' || b == b' ';
    let mut lead = 0;
    if leading {
        while lead < bytes.len() && blank(bytes[lead]) {
            lead += 1;
        }
    }
    let Some(mut trail) = bytes.len().checked_sub(1) else {
        return bytes;
    };
    if trailing {
        while trail > 0 && blank(bytes[trail]) {
            trail -= 1;
        }
    }
    if lead == 0 && trail == bytes.len() - 1 {
        return bytes;
    }
    if trail < lead {
        return &[];
    }
    &bytes[lead..=trail]
}

/// Decodes `bytes` as UTF-8 with a leading mark dropped, `new TextDecoder().decode(bytes)`.
fn text_decode(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes)).into_owned()
}

/// Parses the headers of a part, `parseMultipartFormDataHeaders(input, position)`.
fn parse_multipart_form_data_headers(
    input: &[u8],
    position: &mut usize,
) -> Result<PartHeaders, MultipartError> {
    let mut name: Option<String> = None;
    let mut filename: Option<String> = None;
    let mut content_type = None;
    let mut encoding = None;
    loop {
        if is_crlf(input, *position) {
            let Some(name) = name else {
                return Err(MultipartError::Parsing("header name is null"));
            };
            return Ok(PartHeaders {
                name,
                filename,
                content_type,
                encoding,
            });
        }
        let header_name = collect_bytes(input, position, |b| b != b'\n' && b != b'\r' && b != b':');
        let header_name = remove_chars(header_name, true, true);
        if !is_token(&String::from_utf8_lossy(header_name)) {
            return Err(MultipartError::Parsing(
                "header name does not match the field-name token production",
            ));
        }
        if input.get(*position) != Some(&b':') {
            return Err(MultipartError::Parsing("expected :"));
        }
        *position += 1;
        collect_bytes(input, position, |b| b == b' ' || b == b'\t');
        match header_name.to_ascii_lowercase().as_slice() {
            b"content-disposition" => {
                name = None;
                filename = None;
                let disposition_type = collect_bytes(input, position, is_token_byte);
                if !disposition_type.eq_ignore_ascii_case(b"form-data") {
                    return Err(MultipartError::Parsing(
                        "expected form-data for content-disposition header",
                    ));
                }
                while *position < input.len()
                    && input[*position] != b'\r'
                    && input.get(*position + 1) != Some(&b'\n')
                {
                    let Some((attribute, value)) =
                        parse_content_disposition_attribute(input, position)?
                    else {
                        break;
                    };
                    if attribute == "name" {
                        name = Some(value);
                    } else if attribute == "filename" {
                        filename = Some(value);
                    }
                }
                if name.is_none() {
                    return Err(MultipartError::Parsing(
                        "name attribute is required in content-disposition header",
                    ));
                }
            }
            b"content-type" => {
                let value = collect_bytes(input, position, |b| b != b'\n' && b != b'\r');
                content_type = Some(isomorphic_decode(remove_chars(value, false, true)));
            }
            b"content-transfer-encoding" => {
                let value = collect_bytes(input, position, |b| b != b'\n' && b != b'\r');
                encoding = Some(isomorphic_decode(remove_chars(value, false, true)));
            }
            _ => {
                collect_bytes(input, position, |b| b != b'\n' && b != b'\r');
            }
        }
        if input.get(*position) != Some(&b'\r') && input.get(*position + 1) != Some(&b'\n') {
            return Err(MultipartError::Parsing("expected CRLF"));
        }
        *position += 2;
    }
}

/// Decodes `bytes` one code point per byte, `isomorphicDecode(input)`.
fn isomorphic_decode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| char::from(*b)).collect()
}

/// Parses one attribute of `Content-Disposition`, `parseContentDispositionAttribute(input, position)`.
fn parse_content_disposition_attribute(
    input: &[u8],
    position: &mut usize,
) -> Result<Option<(String, String)>, MultipartError> {
    if input.get(*position) == Some(&b';') {
        *position += 1;
    }
    collect_bytes(input, position, |b| b == b' ' || b == b'\t');
    let attribute_name = collect_bytes(input, position, |b| {
        is_token_byte(b) && b != b'=' && b != b'*'
    });
    if attribute_name.is_empty() {
        return Ok(None);
    }
    let attribute_name = String::from_utf8_lossy(attribute_name).to_ascii_lowercase();
    let is_extended = input.get(*position) == Some(&b'*');
    if is_extended {
        *position += 1;
    }
    if input.get(*position) != Some(&b'=') {
        return Ok(None);
    }
    *position += 1;
    collect_bytes(input, position, |b| b == b' ' || b == b'\t');
    let value = if is_extended {
        let header_value = collect_bytes(input, position, |b| {
            b != b' ' && b != b'\r' && b != b'\n' && b != b';'
        });
        let utf8 = header_value.len() >= 5
            && header_value[0].eq_ignore_ascii_case(&b'u')
            && header_value[1].eq_ignore_ascii_case(&b't')
            && header_value[2].eq_ignore_ascii_case(&b'f')
            && header_value[3] == b'-'
            && header_value[4] == b'8';
        if !utf8 {
            return Err(MultipartError::Parsing(
                "unknown encoding, expected utf-8''",
            ));
        }
        decode_uri_component(&text_decode(header_value.get(7..).unwrap_or_default()))?
    } else if input.get(*position) == Some(&b'"') {
        *position += 1;
        let quoted = collect_bytes(input, position, |b| b != b'\n' && b != b'\r' && b != b'"');
        if input.get(*position) != Some(&b'"') {
            return Err(MultipartError::Parsing("Closing quote not found"));
        }
        *position += 1;
        replace_ignore_case(
            &replace_ignore_case(&text_decode(quoted), "%0A", "\n"),
            "%0D",
            "\r",
        )
        .replace("%22", "\"")
    } else {
        let token = collect_bytes(input, position, |b| is_token_byte(b) && b != b';');
        text_decode(token)
    };
    Ok(Some((attribute_name, value)))
}

/// Replaces every occurrence of the ASCII `pattern` in `text` with `replacement`, ignoring ASCII case, `text.replace(/pattern/ig, replacement)`.
fn replace_ignore_case(text: &str, pattern: &str, replacement: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if rest.len() >= pattern.len()
            && rest.is_char_boundary(pattern.len())
            && rest[..pattern.len()].eq_ignore_ascii_case(pattern)
        {
            out.push_str(replacement);
            rest = &rest[pattern.len()..];
        } else {
            let mut chars = rest.chars();
            if let Some(c) = chars.next() {
                out.push(c);
            }
            rest = chars.as_str();
        }
    }
    out
}

/// Decodes the percent escapes of `text`, `decodeURIComponent(text)`.
fn decode_uri_component(text: &str) -> Result<String, MultipartError> {
    let bytes = text.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'%' {
            out.push(bytes[i]);
            i += 1;
            continue;
        }
        let first = hex_byte(bytes, i).ok_or(MultipartError::UriMalformed)?;
        i += 3;
        if first < 0x80 {
            out.push(first);
            continue;
        }
        let length = match first {
            0xC0..=0xDF => 2,
            0xE0..=0xEF => 3,
            0xF0..=0xF7 => 4,
            _ => return Err(MultipartError::UriMalformed),
        };
        let mut sequence = vec![first];
        for _ in 1..length {
            if bytes.get(i) != Some(&b'%') {
                return Err(MultipartError::UriMalformed);
            }
            let next = hex_byte(bytes, i).ok_or(MultipartError::UriMalformed)?;
            if next & 0xC0 != 0x80 {
                return Err(MultipartError::UriMalformed);
            }
            sequence.push(next);
            i += 3;
        }
        if core::str::from_utf8(&sequence).is_err() {
            return Err(MultipartError::UriMalformed);
        }
        out.extend_from_slice(&sequence);
    }
    String::from_utf8(out).map_err(|_| MultipartError::UriMalformed)
}

/// Returns the byte of the escape `%XX` at `index`.
fn hex_byte(bytes: &[u8], index: usize) -> Option<u8> {
    let high = char::from(*bytes.get(index + 1)?).to_digit(16)?;
    let low = char::from(*bytes.get(index + 2)?).to_digit(16)?;
    u8::try_from(high * 16 + low).ok()
}

/// Decodes base64 as Node's `Buffer.from(text, 'base64')` does.
fn base64_decode(text: &str) -> Vec<u8> {
    let mut out = Vec::new();
    let mut buffer: u32 = 0;
    let mut bits = 0;
    for c in text.chars() {
        let value = match c {
            'A'..='Z' => u32::from(c) - u32::from('A'),
            'a'..='z' => u32::from(c) - u32::from('a') + 26,
            '0'..='9' => u32::from(c) - u32::from('0') + 52,
            '+' | '-' => 62,
            '/' | '_' => 63,
            '=' => break,
            _ => continue,
        };
        buffer = (buffer << 6) | value;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(u8::try_from((buffer >> bits) & 0xFF).unwrap_or_default());
        }
    }
    out
}

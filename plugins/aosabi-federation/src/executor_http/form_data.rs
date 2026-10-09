// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the GraphQL multipart request body of a request with
 * files.
 *
 * Derived from @graphql-tools/executor-http 3.1.0, dist/index.cjs, MIT.
 * Derived from whatwg-node 0.8.5, cjs/FormData.js, MIT.
 * Derived from whatwg-node 0.8.5, cjs/Body.js, MIT.
 */

//! The multipart body of a request that carries files,
//! `create_form_data_from_variables`.
//!
//! A request whose variables carry a file goes out as the GraphQL multipart
//! request: a part `operations` with the body and every file replaced by
//! null, a part `map` from the index of each file to the paths it stood at,
//! and one part per file named by its index, in the order [`extract_files`]
//! walks the variables. A request without a file is the JSON text of its
//! body.
//!
//! A file of the variables is the host object the multipart parser of a
//! front made of an upload, which the marker `File` of `aosabi-core` reads,
//! so a file is extractable when that marker holds an upload. Its identity
//! is the handle it rides in, so a file that stands at two paths is one part
//! with both paths. An object of the JSON model is a tree with no shared
//! node, so the map of clones of the reference, which reuses the clone of an
//! object reached twice, and its guard against a cycle have no reading, and
//! `isPlainObject` of `:14-21` holds for every object of that model. The
//! branches of `handleUpload` for an upload that is a promise, an async
//! iterable or a stream of `graphql-upload` take values no variable of the
//! JSON model holds and are left out with `isGraphQLUpload` at `:238-240` and
//! `collectAsyncIterableValues` at `:242-260`, and `isBlob` at `:347-349`
//! holds for every file a front received, so the branch it guards is the one
//! taken.
//!
//! The bytes of a form are the encoding of the Fetch implementation the
//! reference runs on Node.js, the ponyfill of `@whatwg-node/node-fetch`
//! 0.8.5, under a boundary of random base 36 digits, which the harness masks
//! as `formdata-boundary`. The boundary is drawn from the randomness
//! capability of `aosabi-core` where the ponyfill calls `Math.random`.

use std::sync::Arc;

use aosabi_core::{
    descriptor::File,
    glue::from_argument::FromArgument,
    random::{
        RandomError,
        fill, //
    }, //
};
use aosabi_execute::value::Value;
use aosabi_json::{
    JsObject,
    JsValue,
    Opaque,
    map::IndexMap, //
};

use crate::{
    executor_http::serialize::SerializedExecutionRequest,
    utils::AddressState, //
};

/// The body of a request, its JSON text or its form.
#[derive(Clone, Debug)]
pub enum RequestBody {
    /// The JSON text of a request with no file.
    Json(String),
    /// The form of a request that carries files.
    FormData(FormData),
}

/// A file a form carries, its name, its type and its bytes.
#[derive(Clone, Debug, PartialEq)]
pub struct FormFile {
    /// The name of the part's file, `filename`.
    pub name: String,
    /// The media type, `type`.
    pub r#type: String,
    /// The bytes.
    pub bytes: Vec<u8>,
}

/// The value of an entry of a form, a string or a file.
#[derive(Clone, Debug, PartialEq)]
pub enum FormDataValue {
    /// A string.
    Text(String),
    /// A file.
    File(FormFile),
}

/// A form, `FormData`, its values grouped by name in the order a name first appears.
#[derive(Clone, Debug, Default)]
pub struct FormData {
    /// The values of each name in the order they were appended, `map` of the ponyfill.
    entries: IndexMap<String, Vec<FormDataValue>>,
}

impl FormData {
    /// Returns an empty form.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Appends an entry, `form.append(name, value)`.
    ///
    /// The ponyfill keeps a `Map` from each name to its values and pushes the
    /// value to the values of its name, so an entry takes one lookup of its
    /// name, and the multipart request appends each name once, so the order
    /// of the entries is the order they were appended.
    pub fn append(&mut self, name: &str, value: FormDataValue) {
        match self.entries.get_mut(name) {
            Some(values) => values.push(value),
            None => {
                self.entries.insert(String::from(name), vec![value]);
            }
        }
    }

    /// Returns the entries in the order the encoding writes them, `_entries` of the ponyfill.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &FormDataValue)> {
        self.entries
            .iter()
            .flat_map(|(name, values)| values.iter().map(move |value| (name.as_str(), value)))
    }

    /// Returns the bytes of the form under `boundary`, `getStreamFromFormData`.
    ///
    /// Each entry is a part with its `Content-Disposition`, a file also with
    /// its `filename` and its `Content-Type`, which is
    /// `application/octet-stream` for a file of no type. Two parts are joined
    /// by a line break and the boundary, and the last ends with the closing
    /// boundary. A name and a file name are written as they are.
    #[must_use]
    pub fn to_body(&self, boundary: &str) -> Vec<u8> {
        let mut body = format!("--{boundary}\r\n").into_bytes();
        for (index, (key, value)) in self.entries().enumerate() {
            if index > 0 {
                body.extend_from_slice(format!("\r\n--{boundary}\r\n").as_bytes());
            }
            match value {
                FormDataValue::Text(text) => {
                    body.extend_from_slice(
                        format!("Content-Disposition: form-data; name=\"{key}\"\r\n\r\n")
                            .as_bytes(),
                    );
                    body.extend_from_slice(text.as_bytes());
                }
                FormDataValue::File(file) => {
                    let filename_part = if file.name.is_empty() {
                        String::new()
                    } else {
                        format!("; filename=\"{}\"", file.name)
                    };
                    let content_type = if file.r#type.is_empty() {
                        "application/octet-stream"
                    } else {
                        file.r#type.as_str()
                    };
                    body.extend_from_slice(
                        format!(
                            "Content-Disposition: form-data; name=\"{key}\"{filename_part}\r\nContent-Type: {content_type}\r\n\r\n"
                        )
                        .as_bytes(),
                    );
                    body.extend_from_slice(&file.bytes);
                }
            }
        }
        body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
        body
    }

    /// Returns the `Content-Type` of the form under `boundary`.
    #[must_use]
    pub fn content_type(boundary: &str) -> String {
        format!("multipart/form-data; boundary={boundary}")
    }
}

impl PartialEq for FormData {
    /// Tells whether both forms hold the same entries in the same order.
    fn eq(&self, other: &Self) -> bool {
        self.entries().eq(other.entries())
    }
}

/// The files a value carries and a clone of it with each file replaced by null.
pub struct Extraction {
    /// The clone of the value, `clone`.
    pub clone: JsValue,
    /// Each file with the paths it stood at, in the order the walk first met it, `files`.
    pub files: Vec<(Opaque, Vec<String>)>,
}

/// Returns a deep clone of `value` with every extractable file replaced by null, and the files with their paths.
///
/// A path is the property names and list indices from `path` down, joined
/// with dots. A list and an object are walked, a file is taken, and any other
/// value stays in the clone as it is.
#[must_use]
pub fn extract_files(
    value: &JsValue,
    is_extractable: &dyn Fn(&JsValue) -> bool,
    path: &str,
) -> Extraction {
    let mut files = IndexMap::default();
    let clone = recurse(value, path, is_extractable, &mut files);
    Extraction {
        clone,
        files: files.into_iter().map(|(_, file)| file).collect(),
    }
}

/// Clones one value of the walk of [`extract_files`].
///
/// The files met so far are keyed by the address of their handle, the
/// identity the `Map` of the reference keys them by, so a file takes one
/// lookup. Each entry holds the handle, so no other value takes its address
/// while the walk runs.
fn recurse(
    value: &JsValue,
    path: &str,
    is_extractable: &dyn Fn(&JsValue) -> bool,
    files: &mut IndexMap<usize, (Opaque, Vec<String>), AddressState>,
) -> JsValue {
    if let JsValue::Opaque(file) = value
        && is_extractable(value)
    {
        files
            .entry(Arc::as_ptr(&file.0).addr())
            .or_insert_with(|| (file.clone(), Vec::new()))
            .1
            .push(String::from(path));
        return JsValue::Null;
    }
    let path_prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{path}.")
    };
    match value {
        JsValue::Array(items) => JsValue::Array(
            items
                .iter()
                .enumerate()
                .map(|(index, item)| {
                    recurse(
                        item,
                        &format!("{path_prefix}{index}"),
                        is_extractable,
                        files,
                    )
                })
                .collect(),
        ),
        JsValue::Object(object) => {
            let mut clone = JsObject::new();
            for (key, property) in object.own_keys() {
                let key_text = String::from_utf8_lossy(key.as_wtf8());
                let property_clone = recurse(
                    property,
                    &format!("{path_prefix}{key_text}"),
                    is_extractable,
                    files,
                );
                clone.insert(key.clone(), property_clone);
            }
            JsValue::Object(clone)
        }
        other => other.clone(),
    }
}

/// Returns whether a value is a file the multipart request extracts, an upload a front received.
#[must_use]
pub fn is_extractable_file(value: &JsValue) -> bool {
    matches!(value, JsValue::Opaque(_))
        && File::from_argument(&Value::from_js(value)).is_ok_and(|file| file.is_upload())
}

/// Returns the body of a request, its JSON text or, when its variables carry files, its multipart form.
///
/// The part `operations` is the body with the clone of the variables, the
/// part `map` maps each index to its paths, and each file is the part of its
/// index, named by its name or `blob-<index>`.
#[must_use]
pub fn create_form_data_from_variables(body: &SerializedExecutionRequest) -> RequestBody {
    let Some(variables) = &body.variables else {
        return RequestBody::Json(body.to_json());
    };
    let vars = JsValue::Object(variables.clone());
    let Extraction { clone, files } = extract_files(&vars, &is_extractable_file, "variables");
    if files.is_empty() {
        return RequestBody::Json(body.to_json());
    }
    let mut map = JsObject::new();
    for (curr_index, (_, curr)) in files.iter().enumerate() {
        map.insert(
            curr_index.to_string(),
            JsValue::Array(
                curr.iter()
                    .map(|path| JsValue::from(path.as_str()))
                    .collect(),
            ),
        );
    }
    let mut operations = body.clone();
    operations.variables = match clone {
        JsValue::Object(clone) => Some(clone),
        _ => None,
    };
    let mut form = FormData::new();
    form.append("operations", FormDataValue::Text(operations.to_json()));
    form.append(
        "map",
        FormDataValue::Text(aosabi_json::to_string(&JsValue::Object(map)).unwrap_or_default()),
    );
    for (i, (upload, _)) in files.iter().enumerate() {
        let index_str = i.to_string();
        form.append(
            &index_str,
            FormDataValue::File(form_file(upload, &index_str)),
        );
    }
    RequestBody::FormData(form)
}

/// Returns a boundary of a multipart body, eleven random base 36 digits.
///
/// The ponyfill writes `Math.random().toString(36).substr(2)`, the digits of
/// a random fraction, and the bytes here come from the randomness capability.
///
/// # Errors
///
/// Returns [`RandomError`] when no source of random bytes is installed.
pub fn form_data_boundary() -> Result<String, RandomError> {
    let mut random = [0_u8; 8];
    fill(&mut random)?;
    let mut number = u64::from_le_bytes(random);
    let mut digits = Vec::with_capacity(11);
    for _ in 0..11 {
        let digit = u8::try_from(number % 36).unwrap_or(0);
        digits.push(char::from_digit(u32::from(digit), 36).unwrap_or('0'));
        number /= 36;
    }
    Ok(digits.into_iter().collect())
}

/// Returns the file of a form that an extracted upload is, named as `handleUpload` names it.
///
/// The name is the upload's name, or `blob-<index>` for an upload with an
/// empty name, and the ponyfill keeps that name as the part's file name.
fn form_file(upload: &Opaque, index_str: &str) -> FormFile {
    let value = Value::from_js(&JsValue::Opaque(upload.clone()));
    let Ok(file) = File::from_argument(&value) else {
        return FormFile {
            name: format!("blob-{index_str}"),
            r#type: String::new(),
            bytes: Vec::new(),
        };
    };
    let mut bytes = Vec::new();
    let mut stream = file.stream();
    while let Some(chunk) = stream.next_chunk() {
        bytes.extend_from_slice(&chunk);
    }
    let name = if file.name().is_empty() {
        format!("blob-{index_str}")
    } else {
        String::from(file.name())
    };
    FormFile {
        name,
        r#type: String::from(file.r#type()),
        bytes,
    }
}

// SPDX-License-Identifier: MIT
/*
 * aosabi-query: the bundled client index.js, the bytes rolldown writes for
 * index.ts and its import ./types.
 *
 * No reference file. The bytes are the output of rolldown 1.2.4 for the files
 * generate.rs writes, printed from the pinned segments of the literal
 * client_index_js_segments by four rules. The file index_js.segments beside
 * this file is written by tools/provenance and by nothing else.
 */

//! The bundled client.
//!
//! The build of the reference bundles `index.ts` with rolldown into `index.js`
//! in the ESM format, keeping the imported query package external
//! and inlining the relative import `./types`. Of `types.ts` only the enum
//! objects carry runtime code, so the bundle is the import line, a region of
//! the enums when the schema has one, the index body with the descriptor
//! printed as an object literal, and the export list.
//!
//! 1. The segment `import`, the external import line and a newline.
//! 2. When the schema holds at least one enum, the region of `types.ts`,
//!    then per enum in type map order `const <Name> = `, the object of its
//!    values by the object rule, each value name as the key and the JSON
//!    string of the value as the value, and `;`, then the end of the region.
//! 3. The segment `head`, the descriptor printed by the object rule, and the
//!    segment `tail`, the rest of the index body through the end of its
//!    region.
//! 4. `export { <names> };` with every exported name sorted by UTF-16 code
//!    units, which are the enums, `client` and `descriptor`, and a newline.
//!
//! The object rule is the printer of rolldown. An object of no property is
//! `{}`, of one property `{ <property> }` on the current line with no change
//! of indent, and of two or more `{`, a newline, one line per property at the
//! indent plus one tab with `,` between them, a newline and the indent before
//! `}`.
//!
//! The region names `.aosabi/client/`, the client directory under the default
//! artifact directory.
//!
//! The rules do not model what rolldown does with an enum whose name
//! collides. An enum named `options`, `locale` or after one of the two
//! imports renames the binding of that name to `<name>$1`. An enum named
//! `window`, `Object`, `Promise`, `NaN` or `Infinity` is itself renamed to
//! `<name>$1` and exported as `<name>$1 as <name>`. An enum named `client` or
//! `descriptor` is shadowed by the export of the index body and leaves the
//! bundle. Nor do they model the parse error rolldown answers for a
//! `types.ts` that is no valid TypeScript, such as one with an enum of no
//! value, a type named by a reserved word, a scalar named after a property of
//! `Object.prototype`, or a type of another kind than scalar named `Data` or
//! `Mutations`, which `types.ts` then declares a second time beside the root
//! type of that name.
//!
//! The schema builder writes every type name with an upper case first
//! letter, which keeps the lower case names out of its schemas, but `Object`,
//! `Promise`, `NaN`, `Infinity`, `Data` and `Mutations` are names it can
//! write. For a schema with an enum named `Object`, `Promise`, `NaN` or
//! `Infinity`, or a type other than a scalar named `Data` or `Mutations`,
//! [`bundle_index_js()`] answers a bundle where the reference writes another
//! one or fails.

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_type::{
    DefinitionError,
    EnumValue,
    NamedType,
    Schema, //
};

use crate::root_type::json_text;

/// The constant parts of the bundle, cut from the golden of `errors-app` at
/// three anchors.
///
/// Each part is a line of its name and its length in bytes followed by its
/// bytes, in the order `import`, `head` and `tail`.
const LITERAL: &str = include_str!("index_js.segments");

/// The three segments of [`LITERAL`].
///
/// A literal that does not split ends the compilation of this crate, so drift
/// of the extracted file never reaches a build as a client with a part
/// missing.
const SEGMENTS: Segments = match split_segments(LITERAL) {
    Some(segments) => segments,
    None => panic!("the client_index_js_segments literal does not carry its three segments"),
};

/// The opening comment of the region of `types.ts`.
const TYPES_REGION: &str = "//#region .aosabi/client/types.ts\n";

/// The closing comment of a region.
const END_REGION: &str = "//#endregion\n";

/// The names the index body exports, `client` and `descriptor`.
const INDEX_EXPORTS: [&str; 2] = ["client", "descriptor"];

/// The constant parts of the bundle.
struct Segments {
    /// The external import line and its newline.
    import: &'static str,
    /// The opening of the region of `index.ts` through `const descriptor = `.
    head: &'static str,
    /// The rest of the index body from the `;` behind the descriptor through
    /// the end of the region.
    tail: &'static str,
}

/// Returns the bytes rolldown writes for `index.ts` and its relative import
/// `./types` of the client of `schema`.
///
/// `descriptor` is the descriptor `index.ts` carries, which
/// [`crate::describe_schema::describe_schema()`] builds from the same schema.
///
/// # Errors
///
/// Returns what the value thunk of an enum throws, which a schema built from
/// SDL has already run.
pub fn bundle_index_js(schema: &Schema, descriptor: &JsValue) -> Result<String, DefinitionError> {
    let mut out = String::from(SEGMENTS.import);
    let mut names: Vec<&str> = Vec::new();

    let enums = schema.get_type_map().values().filter_map(|ty| match ty {
        NamedType::Enum(enum_type) if !enum_type.name.starts_with("__") => Some(enum_type),
        _ => None,
    });
    for enum_type in enums {
        if names.is_empty() {
            out.push_str(TYPES_REGION);
        }
        out.push_str("const ");
        out.push_str(&enum_type.name);
        out.push_str(" = ");
        print_enum_values(&mut out, enum_type.get_values()?);
        out.push_str(";\n");
        names.push(&enum_type.name);
    }
    if !names.is_empty() {
        out.push_str(END_REGION);
    }

    out.push_str(SEGMENTS.head);
    print_value(&mut out, descriptor, 0);
    out.push_str(SEGMENTS.tail);

    names.extend(INDEX_EXPORTS);
    names.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    out.push_str("export { ");
    out.push_str(&names.join(", "));
    out.push_str(" };\n");
    Ok(out)
}

/// Prints the object of the values of an enum, each value name as an
/// identifier key and the JSON string of its value.
fn print_enum_values(out: &mut String, values: &[EnumValue]) {
    print_object(out, 0, values.len(), |out, i, _| {
        let value = &values[i];
        out.push_str(&value.name);
        out.push_str(": ");
        out.push_str(&json_text(&value.value));
    });
}

/// Prints a value of the descriptor at the nesting `indent`.
///
/// An object is printed by the object rule with its keys as JSON strings, a
/// string as its JSON string and `true` as written. A value of another kind is
/// written as JSON, which no descriptor holds.
fn print_value(out: &mut String, value: &JsValue, indent: usize) {
    match value {
        JsValue::Object(object) => print_js_object(out, object, indent),
        _ => out.push_str(&json_text(value)),
    }
}

/// Prints `object` by the object rule, in the order `JSON.stringify` writes
/// its keys.
///
/// A property whose value is undefined is left out, as `JSON.stringify`
/// leaves it out of the text the bundle reads.
fn print_js_object(out: &mut String, object: &JsObject, indent: usize) {
    let properties: Vec<(&JsString, &JsValue)> = object
        .own_keys()
        .filter(|(_, value)| !value.is_undefined())
        .collect();
    print_object(out, indent, properties.len(), |out, i, inner| {
        let (key, value) = properties[i];
        out.push_str(&json_text(&JsValue::String(key.clone())));
        out.push_str(": ");
        print_value(out, value, inner);
    });
}

/// Prints an object of `len` properties by the object rule at the nesting
/// `indent`.
///
/// `property` writes the property at an index, given the nesting its value
/// stands at, which is one deeper for an object of two or more properties
/// and unchanged for an object of one. It is called once for each index below
/// `len`, in order.
fn print_object(
    out: &mut String,
    indent: usize,
    len: usize,
    mut property: impl FnMut(&mut String, usize, usize),
) {
    out.push('{');
    match len {
        0 => {}
        1 => {
            out.push(' ');
            property(out, 0, indent);
            out.push(' ');
        }
        _ => {
            for i in 0..len {
                if i > 0 {
                    out.push(',');
                }
                out.push('\n');
                push_indent(out, indent + 1);
                property(out, i, indent + 1);
            }
            out.push('\n');
            push_indent(out, indent);
        }
    }
    out.push('}');
}

/// Appends `indent` tabs.
fn push_indent(out: &mut String, indent: usize) {
    for _ in 0..indent {
        out.push('\t');
    }
}

/// Splits `literal` into the segments `import`, `head` and `tail`.
///
/// The answer is `None` when a segment is missing, out of order or longer
/// than the text behind its line, or when text follows the last segment.
const fn split_segments(literal: &'static str) -> Option<Segments> {
    let Some((import, rest)) = take_segment(literal, b"import") else {
        return None;
    };
    let Some((head, rest)) = take_segment(rest, b"head") else {
        return None;
    };
    let Some((tail, rest)) = take_segment(rest, b"tail") else {
        return None;
    };
    if !rest.is_empty() {
        return None;
    }
    Some(Segments { import, head, tail })
}

/// Takes the segment named `name` off the front of `text`, the line
/// `<name> <length>` and the `length` bytes behind it, and returns the
/// segment and the rest.
///
/// The parse is a loop because no search method of a slice is callable in a
/// constant.
const fn take_segment(text: &'static str, name: &[u8]) -> Option<(&'static str, &'static str)> {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < name.len() {
        if i >= bytes.len() || bytes[i] != name[i] {
            return None;
        }
        i += 1;
    }
    if i >= bytes.len() || bytes[i] != b' ' {
        return None;
    }
    i += 1;

    let digits = i;
    let mut length: usize = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        let Some(tens) = length.checked_mul(10) else {
            return None;
        };
        let Some(sum) = tens.checked_add((bytes[i] - b'0') as usize) else {
            return None;
        };
        length = sum;
        i += 1;
    }
    if i == digits || i >= bytes.len() || bytes[i] != b'\n' {
        return None;
    }

    let Some((_, body)) = text.split_at_checked(i + 1) else {
        return None;
    };
    body.split_at_checked(length)
}

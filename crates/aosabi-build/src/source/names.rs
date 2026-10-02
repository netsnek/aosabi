// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the rule that spells the GraphQL name of a Rust name on the
 * source path.
 */

//! The names of the source path.
//!
//! A Rust programmer writes a field, a method and a parameter in snake case
//! and a variant in upper camel case, and a GraphQL schema spells them in
//! lower camel case and in screaming snake case, so the builder renames them
//! by the rule of point 8 of
//! `docs/decisions/ADR-0029-application-programming-layers.md`. It is the
//! rule async-graphql and juniper apply by default, under which every field
//! name, argument name and enum value of the goldens comes back unchanged.
//! Strawberry and Graphene apply its half for fields alone by default, snake
//! case into lower camel case. [`graphql_name`] is that rule, and every part
//! of the source path names a member through it.
//!
//! | Rust name | GraphQL name |
//! | --- | --- |
//! | a field, a method or a parameter in snake case, `first_name` | lower camel case, `firstName` |
//! | a variant in upper camel case, `InProgress` | screaming snake case, `IN_PROGRESS` |
//! | a parameter with one leading underscore, `_id` | the rest under the rule, `id` |
//! | a type, `PostsFilter` | verbatim |
//! | any other spelling, `firstName` or `IN_PROGRESS` | verbatim |
//!
//! A raw identifier loses its prefix `r#` first, so `r#type` names `type`.
//! The verbatim spelling is the one override. A field in camel case such as
//! `userID` and an enum value in lower case such as `low`, which the rule
//! gives from no other spelling, are written as the API spells them, and
//! rustc's own lint allowance, `#[allow(non_snake_case)]` or
//! `#[allow(non_camel_case_types)]`, keeps the build quiet where rustc warns.
//! A GraphQL field or argument of lower case words joined by underscores and
//! an enum value in upper camel case have no spelling on the source path,
//! since the rule turns those very spellings into lower camel case and
//! screaming snake case.
//!
//! The rule acts on the source path alone. The derive path of the twins in
//! `mirror` and of the registries `aosabi pull` writes keeps every name
//! verbatim, so no twin and no registry changes by it.
//!
//! The judging notebook is `tests/notebooks/12_authoring_parity.ipynb`, whose
//! names cells take every field name, argument name and enum value of the
//! goldens through the rule and back.

/// The kind of a name, which decides the rule it takes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameKind {
    /// A field of a struct.
    Field,
    /// A method of an inherent block, which is a field of its type.
    Method,
    /// A parameter of a method, which is an argument of its field.
    Argument,
    /// A variant of an enum, which is an enum value.
    Variant,
    /// A struct, an enum or any other type.
    Type,
}

/// Returns the GraphQL name of the Rust name `rust` of the kind `kind`.
///
/// A raw prefix `r#` is taken off first. A parameter loses one leading
/// underscore, so an unused parameter keeps rustc quiet and still names its
/// argument. A field, a method and a parameter in snake case print in lower
/// camel case, a variant in upper camel case prints in screaming snake case,
/// and a type and every other spelling print verbatim, which is the override
/// of the rule.
///
/// # Examples
///
/// ```text
/// graphql_name(NameKind::Field, "first_name")     firstName
/// graphql_name(NameKind::Argument, "_id")         id
/// graphql_name(NameKind::Variant, "InProgress")   IN_PROGRESS
/// graphql_name(NameKind::Field, "firstName")      firstName
/// ```
///
/// The builder cells of check C6 of `tests/notebooks/12_authoring_parity.ipynb`
/// hold each of these answers.
#[must_use]
pub fn graphql_name(kind: NameKind, rust: &str) -> String {
    let name = rust.strip_prefix("r#").unwrap_or(rust);
    match kind {
        NameKind::Type => String::from(name),
        NameKind::Field | NameKind::Method => member(name),
        NameKind::Argument => {
            let name = match name.strip_prefix('_') {
                Some(tail) if !tail.is_empty() => tail,
                _ => name,
            };
            member(name)
        }
        NameKind::Variant => {
            if is_upper_camel_case(name) {
                screaming_snake_case(name)
            } else {
                String::from(name)
            }
        }
    }
}

/// Returns the name the inference gives a struct that a parameter takes as
/// its input.
///
/// A struct in input position has no alias symbol in Rust, so the inference
/// names it after its property: the name of the field followed by the capitalized name
/// of the argument, capitalized, with the suffix `Input` unless it ends in it
/// already. `field` and `argument` are GraphQL names, so the rule of
/// [`graphql_name`] acts before the naming of the site, and a field
/// `create_post` with the argument `input` names `CreatePostInput` and never
/// `Create_postInput`. A second type of the same name takes a counter by the
/// deduplication of the inference, which this name leaves out.
#[must_use]
pub fn input_name(field: &str, argument: &str) -> String {
    let mut name = capitalize(&format!("{field}{}", capitalize(argument)));
    if !name.ends_with("Input") {
        name.push_str("Input");
    }
    name
}

/// Returns `name` in lower camel case when it is in snake case, and verbatim
/// otherwise.
fn member(name: &str) -> String {
    if !is_snake_case(name) {
        return String::from(name);
    }
    let mut words = name.split('_');
    let mut camel = String::from(words.next().unwrap_or_default());
    for word in words {
        camel.push_str(&capitalize(word));
    }
    camel
}

/// Returns whether `name` is in snake case: words of ASCII lower case letters
/// and digits joined by single underscores, the first word opening with a
/// letter.
fn is_snake_case(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
        && name.split('_').all(|word| {
            !word.is_empty()
                && word
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        })
}

/// Returns whether `name` is in upper camel case: ASCII letters and digits
/// opening with an upper case letter.
fn is_upper_camel_case(name: &str) -> bool {
    name.as_bytes().first().is_some_and(u8::is_ascii_uppercase)
        && name.bytes().all(|byte| byte.is_ascii_alphanumeric())
}

/// Returns the name of upper camel case `name` in screaming snake case.
///
/// A word starts at an upper case letter that follows a lower case letter or
/// a digit, and at an upper case letter that follows another one and is
/// followed by a lower case letter, so `HTTPServer` names `HTTP_SERVER` and
/// `V2Beta` names `V2_BETA`.
fn screaming_snake_case(name: &str) -> String {
    let bytes = name.as_bytes();
    let mut out = String::with_capacity(name.len() + 4);
    for (i, &byte) in bytes.iter().enumerate() {
        if i > 0 && byte.is_ascii_uppercase() {
            let before = bytes[i - 1];
            let after = bytes.get(i + 1).copied();
            let opens = before.is_ascii_lowercase()
                || before.is_ascii_digit()
                || (before.is_ascii_uppercase() && after.is_some_and(|c| c.is_ascii_lowercase()));
            if opens {
                out.push('_');
            }
        }
        out.push(char::from(byte.to_ascii_uppercase()));
    }
    out
}

/// Returns `name.charAt(0).toUpperCase() + name.slice(1)`.
///
/// `charAt(0)` is one UTF-16 code unit, so a first character outside the
/// basic plane is returned as it is, as the capitalization of the inference
/// does.
fn capitalize(name: &str) -> String {
    let mut chars = name.chars();
    match chars.next() {
        Some(first) if first.len_utf16() == 1 => first.to_uppercase().chain(chars).collect(),
        _ => String::from(name),
    }
}

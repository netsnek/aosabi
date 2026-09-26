// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the "Did you mean" suffix of a message that names suggestions.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/didYouMean.ts, MIT.
 */

//! The suggestion suffix of the type layer.
//!
//! Every message that ends in a list of suggestions, an unknown type, field,
//! argument or enum value, appends the text [`did_you_mean()`] builds from
//! the list [`crate::suggestion_list()`] returns. The quoting, the commas,
//! the `or` and the count of listed suggestions are the bytes the message
//! carries, so the function is copied clause by clause.

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

/// The count of suggestions a message lists at most.
const MAX_SUGGESTIONS: usize = 5;

/// Given `[ A, B, C ]` returns `' Did you mean A, B, or C?'`.
///
/// The two overloads of the reference, a list alone and a sub message before
/// the list, fold into `sub_message`, which is `None` for the first. The
/// reference tests the sub message for truth, and an empty string is false,
/// so an empty sub message adds nothing, as an absent one does. Each of
/// `suggestions` is wrapped in double quotes, and an empty list yields an
/// empty string.
///
/// Derived from graphql-js 16.12.0, src/jsutils/didYouMean.ts, MIT.
#[must_use]
pub fn did_you_mean(sub_message: Option<&str>, suggestions: &[String]) -> String {
    let mut message = String::from(" Did you mean ");
    if let Some(sub_message) = sub_message.filter(|sub_message| !sub_message.is_empty()) {
        message.push_str(sub_message);
        message.push(' ');
    }

    let suggestions: Vec<String> = suggestions.iter().map(|x| format!("\"{x}\"")).collect();
    match suggestions.as_slice() {
        [] => return String::new(),
        [only] => return format!("{message}{only}?"),
        [first, second] => return format!("{message}{first} or {second}?"),
        _ => {}
    }

    let mut selected: Vec<String> = suggestions.into_iter().take(MAX_SUGGESTIONS).collect();
    // Three or more suggestions reach this line, so the pop finds one.
    let last_item = selected.pop().unwrap_or_default();
    format!("{message}{}, or {last_item}?", selected.join(", "))
}

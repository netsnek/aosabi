// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the suggestions a message offers for a misspelled name, the
 * options close enough to the input in lexical distance, in order.
 *
 * Derived from graphql-js 16.12.0, src/jsutils/suggestionList.ts, MIT.
 */

//! The suggestion list of the type layer.
//!
//! An unknown name in a document or a schema is answered with the names that
//! lie within a threshold of the input in lexical distance, sorted by that
//! distance and then by [`natural_compare()`], and [`crate::did_you_mean()`]
//! turns the list into text. The reference reads a string through
//! `charCodeAt` and `length`, which count UTF-16 code units, so the port
//! measures over code units.

use alloc::{
    string::String,
    vec,
    vec::Vec, //
};

use aosabi_json::{
    case::lower_ascii_exact,
    sort::sort_by_stable, //
};

use crate::{
    definition::{
        ObjMap,
        own_entries, //
    },
    natural_compare::natural_compare, //
};

/// Given an invalid input string and a list of valid options, returns a
/// filtered list of valid options sorted based on their similarity with the
/// input.
///
/// The threshold is `Math.floor(input.length * 0.4) + 1` over the code unit
/// length of `input`. Each of `options` within the threshold is keyed in a
/// null prototype object and read back with `Object.keys`, whose order is
/// the array index keys ascending and then insertion order, and the sort
/// that follows is stable.
///
/// Derived from graphql-js 16.12.0, src/jsutils/suggestionList.ts, MIT.
#[must_use]
pub fn suggestion_list(input: &str, options: &[String]) -> Vec<String> {
    let mut options_by_distance: ObjMap<usize> = ObjMap::default();
    let mut lexical_distance = LexicalDistance::new(input);

    let threshold = distance_threshold(input.encode_utf16().count());
    for option in options {
        let distance = lexical_distance.measure(option, threshold);
        if let Some(distance) = distance {
            options_by_distance.insert(option.clone(), distance);
        }
    }

    let mut keys = own_entries(&options_by_distance);
    sort_by_stable(&mut keys, |(a, distance_a), (b, distance_b)| {
        let distance_diff = distance_a.cmp(distance_b);
        if distance_diff.is_ne() {
            distance_diff
        } else {
            natural_compare(a, b)
        }
    });
    keys.into_iter().map(|(key, _)| key.clone()).collect()
}

/// Returns `Math.floor(length * 0.4) + 1`.
///
/// The product is computed in doubles, as the reference computes it,
/// because the threshold is compared against integer distances and the
/// rounding of the product decides a boundary case.
fn distance_threshold(length: usize) -> usize {
    // A code unit count is far below 2^53, so the conversion is exact.
    #[expect(
        clippy::cast_precision_loss,
        reason = "the reference multiplies a string length as a double"
    )]
    let product = length as f64 * 0.4;
    // A cast to an integer truncates toward zero, which is the floor of a
    // non negative double, and the product is below 2^53, so it fits.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the truncation of a non negative double is its floor"
    )]
    let floored = product as usize;
    floored + 1
}

/// Computes the lexical distance between strings A and B.
///
/// The "distance" between two strings is given by counting the minimum
/// number of edits needed to transform string A into string B. An edit can
/// be an insertion, deletion, or substitution of a single character, or a
/// swap of two adjacent characters.
///
/// Includes a custom alteration from Damerau-Levenshtein to treat case
/// changes as a single edit which helps identify mis-cased values with an
/// edit distance of 1.
///
/// The reference lower cases both strings with
/// `String.prototype.toLowerCase`, and the port lower cases them with
/// [`lower_ascii_exact()`], which keeps the mappings that hold ASCII. Every
/// option is a name, which the specification spells in ASCII and which
/// [`crate::assert_name()`] and the lexer hold to it, and every read of the
/// two lower cases compares a code unit of the input with a code unit of an
/// option, or the whole input with the whole option. Both lower cases of the
/// input have the same length in code units and the same ASCII code unit at
/// the same index, so every distance equals the reference's.
///
/// Derived from graphql-js 16.12.0, src/jsutils/suggestionList.ts, MIT.
struct LexicalDistance {
    /// The input as given.
    input: String,
    /// The input lower cased.
    input_lower_case: String,
    /// The code units of the lower cased input.
    input_array: Vec<u16>,
    /// The three rows of the distance table.
    rows: [Vec<usize>; 3],
}

impl LexicalDistance {
    /// Prepares the rows for `input`.
    ///
    /// The reference sizes a row by `input.length`, and a JavaScript array
    /// grows on a write past its end. The shorter of the two measured
    /// arrays indexes a row, and that is never longer than the lower cased
    /// input, so the rows are sized by the lower cased input once.
    fn new(input: &str) -> Self {
        let input_lower_case = lower_ascii_exact(input);
        let input_array = string_to_array(&input_lower_case);
        let row_length = input_array.len() + 1;
        Self {
            input: String::from(input),
            input_lower_case,
            input_array,
            rows: [
                vec![0; row_length],
                vec![0; row_length],
                vec![0; row_length],
            ],
        }
    }

    /// Returns the distance to `option`, or `None` when it exceeds
    /// `threshold`.
    fn measure(&mut self, option: &str, threshold: usize) -> Option<usize> {
        if self.input == option {
            return Some(0);
        }

        let option_lower_case = lower_ascii_exact(option);

        // Any case change counts as a single edit.
        if self.input_lower_case == option_lower_case {
            return Some(1);
        }

        let option_array = string_to_array(&option_lower_case);
        let (a, b): (&[u16], &[u16]) = if option_array.len() < self.input_array.len() {
            (&self.input_array, &option_array)
        } else {
            (&option_array, &self.input_array)
        };
        let a_length = a.len();
        let b_length = b.len();

        if a_length - b_length > threshold {
            return None;
        }

        let rows = &mut self.rows;
        for (j, cell) in rows[0].iter_mut().enumerate().take(b_length + 1) {
            *cell = j;
        }

        for i in 1..=a_length {
            let up_row = (i - 1) % 3;
            let current_row = i % 3;

            rows[current_row][0] = i;
            let mut smallest_cell = i;
            for j in 1..=b_length {
                let cost = usize::from(a[i - 1] != b[j - 1]);

                let mut current_cell = (rows[up_row][j] + 1)
                    .min(rows[current_row][j - 1] + 1)
                    .min(rows[up_row][j - 1] + cost);

                if i > 1 && j > 1 && a[i - 1] == b[j - 2] && a[i - 2] == b[j - 1] {
                    // A transposition costs one edit.
                    let double_diagonal_cell = rows[(i - 2) % 3][j - 2];
                    current_cell = current_cell.min(double_diagonal_cell + 1);
                }

                if current_cell < smallest_cell {
                    smallest_cell = current_cell;
                }

                rows[current_row][j] = current_cell;
            }

            // The distance cannot go below the smallest cell of the row.
            if smallest_cell > threshold {
                return None;
            }
        }

        let distance = rows[a_length % 3][b_length];
        if distance <= threshold {
            Some(distance)
        } else {
            None
        }
    }
}

/// Returns the code units of `text`.
///
/// Collects `charCodeAt` over the string.
///
/// Derived from graphql-js 16.12.0, src/jsutils/suggestionList.ts, MIT.
fn string_to_array(text: &str) -> Vec<u16> {
    text.encode_utf16().collect()
}

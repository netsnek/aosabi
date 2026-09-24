// SPDX-License-Identifier: MIT
/*
 * aosabi-json space: the white space and the line terminators of ECMA-262.
 */

//! `is_js_space`.
//!
//! The class `\s` of a regular expression is `WhiteSpace` and
//! `LineTerminator`, and `trim` drops the same set, so a reader of a
//! JavaScript text that skips white space takes this one predicate.
//!
//! [class-escape]: https://tc39.es/ecma262/#sec-characterclassescape
//! [trim]: https://tc39.es/ecma262/#sec-trimstring

/// Returns whether `c` is white space or a line terminator of ECMA-262, `\s`.
///
/// `WhiteSpace` is the tab, the vertical tab, the form feed, U+FEFF and the
/// space separators of the category `Zs` of Unicode, U+0020, U+00A0, U+1680,
/// U+2000 to U+200A, U+202F, U+205F and U+3000, and `LineTerminator` the line
/// feed, the carriage return, U+2028 and U+2029, 25 code points. U+0085,
/// which the property `White_Space` of Unicode holds, is no part of it.
#[must_use]
pub fn is_js_space(c: char) -> bool {
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

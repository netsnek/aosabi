// SPDX-License-Identifier: MIT
/*
 * aosabi-codegen: the lint allowances every emitted item carries.
 *
 * No analogue on the reference side. The allowances and their reason are
 * docs/authoring.md section 29.5.
 */

//! The lint allowances of the emitted items.
//!
//! An emitted item carries the names of the reference verbatim, a camel case
//! parameter and an upper case key among them, and a described item carries
//! its comment as API text with no `# Errors` section. So every hidden
//! module, associated item and constant the macros write stands under
//! [`allow`], and a service writes no `#[expect(non_snake_case)]` for what a
//! macro emitted.

use proc_macro2::TokenStream;
use quote::quote;

/// The reason every allowance states.
const REASON: &str = "the names are the GraphQL names of the reference verbatim, and a described comment is API text with no Errors section (docs/authoring.md section 29.5)";

/// Returns the attribute of the four allowances.
#[must_use]
pub fn allow() -> TokenStream {
    quote! {
        #[allow(
            non_snake_case,
            non_upper_case_globals,
            clippy::missing_errors_doc,
            clippy::must_use_candidate,
            reason = #REASON
        )]
    }
}

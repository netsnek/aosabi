// SPDX-License-Identifier: MIT
/*
 * aosabi-macros: #[decorate(expr)], taken off every site it belongs on and
 * handed to the glue in source order.
 *
 * No analogue on the reference side.
 */

//! `#[decorate(expr)]`.

use proc_macro2::{
    Span,
    TokenStream, //
};

use crate::refuse;

/// `#[decorate]` expands where no macro that reads it took it off.
const DECORATE_OUTSIDE_ITS_SITES: &str = "`#[decorate(...)]` belongs on an entry of `graphql!`, on a method or a static of an `#[aosabi::resolvers]` block, or on a free function under `#[aosabi::resolver]`, and stands under none of them here";

/// Refuses a `#[decorate]` that expands on its own, and keeps its item.
///
/// The macros that read a decorator take it off their input before it
/// expands, so an expansion of the attribute is always a decorator outside its
/// sites. The item follows the refusal unchanged, so the refusal is the one
/// error of the site.
pub(crate) fn refuse_outside(_args: &TokenStream, item: &TokenStream) -> TokenStream {
    let mut refused = refuse::refuse(Span::call_site(), DECORATE_OUTSIDE_ITS_SITES);
    refused.extend(item.clone());
    refused
}

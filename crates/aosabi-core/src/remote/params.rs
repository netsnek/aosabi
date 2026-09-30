// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the parameter list an arguments struct of a pulled registry
 * describes.
 *
 * No reference file.
 */

//! The parameter list of a field with arguments.
//!
//! `aosabi pull` writes a field with arguments as a method of its interface,
//! `orders(status?: string, limit?: number): Order[]`, one parameter per
//! argument in the order the remote declares them, a nullable argument
//! optional with its type and no `null`. A front that hands such a member
//! on describes it as a callable whose parameters are these, which the
//! arguments struct of the field states through [`Params`] under the key
//! `#[aosabi(arguments)]`.

use crate::descriptor::ParamInfo;

/// A parameter list, which an arguments struct of a pulled registry
/// describes.
///
/// The derive writes one entry per field in field order, so a field
/// `status: Option<String>` is the optional parameter `status?: string`.
pub trait Params {
    /// The parameters, in the order the remote declares its arguments.
    const PARAMS: &'static [ParamInfo];
}

// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the descriptor of Repeater<T>, the stream a subscription field
 * returns.
 *
 * No file of the reference is translated here. It uses core and alloc alone.
 */

//! The descriptor of `Repeater<T>`.
//!
//! [`Repeater`] is one type in two roles. The build reads it as the marker
//! [`TypeExpr::Repeater`], so a build that links the descriptors without the
//! feature `runtime` still names it, as it names
//! [`Promise`](super::Promise). With the feature `runtime` it is also the
//! value a subscription resolver returns, which holds the repeater of the
//! executor, and its constructor lives in the module `repeater` of the crate
//! root.

use core::marker::PhantomData;

use super::{
    described::Described,
    link::{
        TypeExpr,
        TypeLink, //
    }, //
};

/// A stream of `T`, the `Repeater<T>` a subscription field returns.
///
/// Without the feature `runtime` the type holds no value and only names the
/// stream for the build. With it, the type holds the repeater of the executor
/// that `Repeater::new` and the pub sub make.
pub struct Repeater<T> {
    /// The repeater of the executor, which the value of the stream is.
    #[cfg(feature = "runtime")]
    pub(crate) inner: alloc::rc::Rc<aosabi_execute::repeater::Repeater>,
    /// The type of the values the stream yields.
    marker: PhantomData<fn() -> T>,
}

#[cfg(feature = "runtime")]
impl<T> Repeater<T> {
    /// Returns the typed stream over the repeater `inner` of the executor.
    pub(crate) fn from_inner(inner: alloc::rc::Rc<aosabi_execute::repeater::Repeater>) -> Self {
        Self {
            inner,
            marker: PhantomData,
        }
    }
}

impl<T: Described> Described for Repeater<T> {
    const TYPE: TypeExpr = TypeExpr::Repeater(TypeLink::of::<T>());
}

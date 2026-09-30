// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the traits a described type implements.
 *
 * No analogue on the reference side. The implementations for the types of
 * `core` and `alloc` state which TypeScript type each Rust type is the twin
 * of, which is the left column of the rule table of
 * docs/schema-inference.md section 4.
 */

//! The traits a described type implements.

use alloc::{
    borrow::Cow,
    boxed::Box,
    collections::VecDeque,
    rc::Rc,
    string::String,
    vec::Vec, //
};
use core::{
    convert::Infallible,
    marker::PhantomData, //
};

use super::{
    info::MethodInfo,
    link::{
        Primitive,
        TypeExpr,
        TypeLink, //
    },
    registry::Declaration,
};

/// A type the inference can read.
///
/// `#[derive(Aosabi)]` and `#[aosabi::scalar]` implement the trait for the
/// types of an application, and this module implements it for the types of
/// `core` and `alloc` that have a TypeScript twin. A type with no
/// implementation cannot stand in a field, a parameter or a return, and the
/// compiler says so with the message below, which is the compile time refusal
/// of `docs/schema-inference.md` section 13.
///
/// # Examples
///
/// ```ignore
/// use aosabi::descriptor::{Described, TypeExpr};
///
/// if let TypeExpr::List(element) = <Vec<String> as Described>::TYPE {
///     println!("a list of {}", element.rust_name());
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` has no GraphQL description",
    note = "derive `aosabi::Aosabi` on the type, mark a newtype with `#[aosabi::scalar]`, write `Vec<T>` in place of a tuple, and write an insertion ordered map in place of `HashMap` or `BTreeMap`"
)]
pub trait Described: 'static {
    /// The description of the type.
    const TYPE: TypeExpr;

    /// The declaration list of the crate target that declares the type.
    ///
    /// The derive and the scalar attribute read the list the build script of
    /// the crate wrote for the target they expand in, and a type with no such
    /// list, a type of `core` or `alloc` among them, keeps this empty default.
    /// The list of a root is the list the build reads.
    const DECLARATIONS: &'static [Declaration] = &[];
}

/// A type whose `#[aosabi::resolvers]` block is recorded.
///
/// The attribute implements the trait with the methods of the block.
#[diagnostic::on_unimplemented(message = "`{Self}` has no `#[aosabi::resolvers]` block")]
pub trait Resolvers: 'static {
    /// The methods of the block whose receiver is `&self`, in declaration
    /// order.
    const METHODS: &'static [MethodInfo];
}

/// The return of an `async fn`, the twin of the TypeScript `Promise<T>`.
///
/// The type is a name and holds no value. `#[aosabi::resolvers]` links the
/// return of an `async fn` through it, so the inference unwraps the promise
/// where a TypeScript build does.
pub struct Promise<T>(PhantomData<T>);

/// Implements [`Described`] for types that are one fixed [`TypeExpr`].
macro_rules! describe {
    ($expr:expr => $($type:ty),+ $(,)?) => {
        $(
            impl Described for $type {
                const TYPE: TypeExpr = $expr;
            }
        )+
    };
}

/// Implements [`Described`] for wrappers of one described type.
macro_rules! describe_wrapper {
    ($variant:ident => $($type:ty),+ $(,)?) => {
        $(
            impl<T: Described> Described for $type {
                const TYPE: TypeExpr = TypeExpr::$variant(TypeLink::of::<T>());
            }
        )+
    };
}

describe!(TypeExpr::Primitive(Primitive::String) => String, &'static str, Box<str>, Cow<'static, str>, char);

describe!(TypeExpr::Primitive(Primitive::Number) => f64, f32);
describe!(TypeExpr::Primitive(Primitive::Number) => i8, i16, i32, i64, i128, isize);
describe!(TypeExpr::Primitive(Primitive::Number) => u8, u16, u32, u64, u128, usize);

describe!(TypeExpr::Primitive(Primitive::Boolean) => bool);
describe!(TypeExpr::Void => ());
describe!(TypeExpr::Never => Infallible);

describe_wrapper!(Nullable => Option<T>);
describe_wrapper!(List => Vec<T>, Box<[T]>, VecDeque<T>);
describe_wrapper!(Boxed => Box<T>, Rc<T>);
describe_wrapper!(Promise => Promise<T>);

// `alloc::sync` exists only where the target has atomic pointers.
#[cfg(target_has_atomic = "ptr")]
describe_wrapper!(Boxed => alloc::sync::Arc<T>);

impl<T: Described, const N: usize> Described for [T; N] {
    const TYPE: TypeExpr = TypeExpr::List(TypeLink::of::<T>());
}

impl<T: Described, E: 'static> Described for Result<T, E> {
    const TYPE: TypeExpr = TypeExpr::Fallible(TypeLink::of::<T>());
}

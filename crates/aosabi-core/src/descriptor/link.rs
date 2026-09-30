// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the lazy reference from one described type to another.
 *
 * No analogue on the reference side. A link stands in for the `ts.Type` the
 * checker hands a TypeScript build for a property, a parameter or a return.
 */

//! The lazy reference from one described type to another.

use core::any::TypeId;

use super::{
    callable::CallableInfo,
    described::Described,
    info::TypeInfo, //
};

/// A reference to a described type that evaluates nothing until it is asked.
///
/// A link is three function pointers over the type it names. Building one
/// reads no descriptor, so the descriptor of a type that refers to itself is a
/// constant the compiler can evaluate.
///
/// # Examples
///
/// ```ignore
/// use aosabi::descriptor::{TypeExpr, TypeLink};
///
/// let link = TypeLink::of::<Option<f64>>();
/// if let TypeExpr::Nullable(inner) = link.resolve() {
///     println!("{} wraps {}", link.rust_name(), inner.rust_name());
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub struct TypeLink {
    /// `TypeId::of` over the linked type.
    id: fn() -> TypeId,
    /// The function that reads `Described::TYPE` of the linked type.
    expr: fn() -> TypeExpr,
    /// `core::any::type_name` over the linked type.
    rust_name: fn() -> &'static str,
}

impl TypeLink {
    /// Returns the link to `T`.
    ///
    /// The function is `const`, so a link stands inside the constant of another
    /// descriptor. It reads nothing of `T`.
    #[must_use]
    pub const fn of<T: Described>() -> Self {
        Self {
            id: TypeId::of::<T>,
            expr: expr_of::<T>,
            rust_name: core::any::type_name::<T>,
        }
    }

    /// Returns the identity of the linked type.
    ///
    /// Two links name the same type exactly when their identities are equal.
    /// The inference keys its maps by this value, where a TypeScript build keys them by the
    /// identity of a `ts.Type`.
    #[must_use]
    pub fn id(&self) -> TypeId {
        (self.id)()
    }

    /// Returns the description of the linked type.
    #[must_use]
    pub fn resolve(&self) -> TypeExpr {
        (self.expr)()
    }

    /// Returns the name the compiler gives the linked type.
    ///
    /// The text is that of `core::any::type_name`, which promises no stable
    /// form, so it serves a message and the descriptor dump and never a name
    /// of the schema.
    #[must_use]
    pub fn rust_name(&self) -> &'static str {
        (self.rust_name)()
    }
}

/// Returns the description of `T`, the function a [`TypeLink`] points at.
fn expr_of<T: Described>() -> TypeExpr {
    T::TYPE
}

/// A primitive of the TypeScript type system that a Rust type stands for.
#[derive(Clone, Copy, Debug)]
pub enum Primitive {
    /// The TypeScript `string`.
    String,
    /// The TypeScript `number`, which every Rust number stands for.
    Number,
    /// The TypeScript `boolean`.
    Boolean,
}

/// The description of a type, as the inference sees it.
///
/// Each variant is one shape the TypeScript checker would report for the twin
/// of the Rust type. A wrapper carries the link to what it wraps, and
/// [`TypeExpr::Named`] carries the descriptor a macro wrote.
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub enum TypeExpr {
    /// A primitive, such as `String`, `f64` or `bool`.
    Primitive(Primitive),
    /// The unit type `()`, the TypeScript `void`.
    Void,
    /// A value of any shape, the TypeScript `any`.
    Any,
    /// A type with no value, the TypeScript `never`.
    Never,
    /// `Option<T>`, the TypeScript `T | undefined`.
    Nullable(TypeLink),
    /// `Vec<T>`, `Box<[T]>`, `[T; N]` and `VecDeque<T>`, the TypeScript `T[]`.
    List(TypeLink),
    /// [`Promise<T>`](super::Promise), the return of an `async fn`.
    Promise(TypeLink),
    /// A stream of `T`, the `Repeater<T>` a TypeScript subscription returns.
    Repeater(TypeLink),
    /// `Result<T, E>`, which maps as `T`.
    Fallible(TypeLink),
    /// `Box<T>`, `Rc<T>`, `Arc<T>` and [`Unchecked<T>`](crate::glue::Unchecked),
    /// which map as `T` and share its identity.
    Boxed(TypeLink),
    /// A type that carries a descriptor of its own.
    Named(&'static TypeInfo),
    /// A function valued member, `(a: A) => R`, with its parameters and its
    /// return.
    Callable(&'static CallableInfo),
}

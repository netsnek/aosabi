// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the descriptors of the entries of graphql!, the object literal
 * of the roots of an application.
 *
 * No analogue on the reference side.
 */

//! The descriptors of the entries of `graphql!`.
//!
//! A root of a TypeScript service is an object literal whose members are arrow
//! functions, references to a static or to a function, shorthand properties
//! and spreads of the roots of another module. The macro `graphql!` writes
//! one [`FragmentRoot`] per root it states, whose segments keep the source
//! order of the literal: every run of entries between two spreads is one
//! [`Segment::Entries`], and every spread is one [`Segment::Spread`]. The
//! inference presents the root in the order the checker gives the spread
//! type, the segments in reverse source order, each in its own order, and a
//! name in two segments at the place and with the value of its later
//! occurrence.
//!
//! An entry records the kind of its value. An inline entry carries the
//! [`MethodInfo`] of the function the macro makes of it, a reference and a
//! shorthand name the descriptor of the static or of the free function they
//! reach, and `mutation(...)` wraps any of them. The description of an entry
//! is its own comment, and a referenced function gives only its parameters
//! and their `@param` text.

use super::{
    facts::DecoratorFact,
    info::{
        Docs,
        MethodInfo,
        Site,
        SourceKey, //
    },
};

/// One root of a `graphql!` fragment, `Query`, `Mutation` or `Subscription`.
#[derive(Debug)]
pub struct FragmentRoot {
    /// The name of the root, `Query`, `Mutation` or `Subscription`.
    pub name: &'static str,
    /// The name of the constant `graphql!` declares, the twin of the exported
    /// name of the literal.
    pub fragment: &'static str,
    /// The value of `CARGO_PKG_NAME` in the crate that declares the fragment.
    pub krate: &'static str,
    /// The place of the `graphql!` that declares the fragment, the source key
    /// of its declaration.
    pub source: SourceKey,
    /// Whether the literal states the root, `Mutation: {}` included. A root
    /// the literal leaves out stands empty and unstated, so a spread of it
    /// still names a root.
    pub stated: bool,
    /// The segments of the root in source order.
    pub segments: &'static [Segment],
}

impl FragmentRoot {
    /// Returns an empty root that the literal does not state.
    #[must_use]
    pub const fn unstated(
        name: &'static str,
        fragment: &'static str,
        krate: &'static str,
        source: SourceKey,
    ) -> Self {
        Self {
            name,
            fragment,
            krate,
            source,
            stated: false,
            segments: &[],
        }
    }
}

/// One segment of a root, a run of entries or a spread.
#[derive(Debug)]
pub enum Segment {
    /// The entries between two spreads, in source order.
    Entries(&'static [EntryInfo]),
    /// A spread `..module::graphql.Root`, the function that answers the root
    /// of the other fragment.
    Spread(fn() -> &'static FragmentRoot),
}

/// One entry of a root.
#[derive(Debug)]
pub struct EntryInfo {
    /// The name of the entry, the key of the object literal.
    pub name: &'static str,
    /// The documentation the entry itself carries, which is the description
    /// of the field.
    pub docs: Docs,
    /// Where the entry stands and the documentation block it carries.
    pub site: Site,
    /// The value of the entry.
    pub kind: EntryKind,
    /// The decorators of the entry in source order, the outermost first.
    pub decorators: &'static [DecoratorFact],
}

/// The value of an entry.
#[derive(Debug)]
pub enum EntryKind {
    /// A closure written inline, which the macro turns into a function.
    Inline(&'static MethodInfo),
    /// A reference `Type::static` or `module::function`.
    Reference(fn() -> &'static MethodInfo),
    /// A shorthand property, the name of a function in scope.
    Shorthand(fn() -> &'static MethodInfo),
    /// `mutation(...)` around another entry, whose payload the inference
    /// maps as the reference maps the return of `mutation`.
    Mutation(&'static EntryKind),
}

impl EntryKind {
    /// Returns the descriptor of the function the entry calls, through a
    /// mutation wrapper.
    #[must_use]
    pub fn method(&self) -> &'static MethodInfo {
        match self {
            Self::Inline(method) => method,
            Self::Reference(method) | Self::Shorthand(method) => method(),
            Self::Mutation(inner) => inner.method(),
        }
    }

    /// Returns whether the entry is wrapped in `mutation(...)`.
    #[must_use]
    pub const fn is_mutation(&self) -> bool {
        matches!(self, Self::Mutation(_))
    }

    /// Returns the kind under the mutation wrapper, and the kind itself
    /// otherwise.
    #[must_use]
    pub const fn unwrapped(&self) -> &Self {
        match self {
            Self::Mutation(inner) => inner.unwrapped(),
            other => other,
        }
    }
}

/// One user error of a mutation payload, the interface `UserError` of the mutation wrapper.
///
/// The type reaches the schema of every service that wraps an entry in
/// `mutation(...)`, because the inference appends `userErrors: [UserError!]!`
/// to the payload, so the type and its documentation live here, where a build
/// script reads them with no run time linked. The run time of the module
/// `mutation` gives it its value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UserError {
    /// Path to the offending input field (e.g. `['address','zip']`).
    pub field: alloc::vec::Vec<alloc::string::String>,
    /// The message.
    pub message: alloc::string::String,
    /// Stable machine code (validation code, or a business code).
    pub code: alloc::string::String,
}

/// The fields of [`UserError`] as the checker lists the members of its
/// interface, with the `JSDoc` of each.
const USER_ERROR_FIELDS: &[super::info::FieldInfo] = &[
    super::info::FieldInfo {
        name: "field",
        docs: Docs {
            comment: "Path to the offending input field (e.g. `['address','zip']`).",
            tags: &[],
        },
        site: Site {
            file: "aosabi/core/mutation.ts",
            line: 32,
            docs: &["Path to the offending input field (e.g. `['address','zip']`)."],
            blocks: 1,
        },
        visibility: super::info::Visibility::Public,
        r#type: super::link::TypeLink::of::<alloc::vec::Vec<alloc::string::String>>(),
        spread: false,
    },
    super::info::FieldInfo {
        name: "message",
        docs: Docs::NONE,
        site: Site::bare("aosabi/core/mutation.ts", 33),
        visibility: super::info::Visibility::Public,
        r#type: super::link::TypeLink::of::<alloc::string::String>(),
        spread: false,
    },
    super::info::FieldInfo {
        name: "code",
        docs: Docs {
            comment: "Stable machine code (validation code, or a business code).",
            tags: &[],
        },
        site: Site {
            file: "aosabi/core/mutation.ts",
            line: 35,
            docs: &["Stable machine code (validation code, or a business code)."],
            blocks: 1,
        },
        visibility: super::info::Visibility::Public,
        r#type: super::link::TypeLink::of::<alloc::string::String>(),
        spread: false,
    },
];

impl super::described::Described for UserError {
    const TYPE: super::link::TypeExpr = super::link::TypeExpr::Named(&super::info::TypeInfo {
        kind: super::info::TypeKind::Object,
        name: "UserError",
        krate: "aosabi-core",
        source: SourceKey {
            file: "aosabi/core/mutation.ts",
            line: 29,
            column: 1,
        },
        docs: Docs::NONE,
        site: Site::bare("aosabi/core/mutation.ts", 29),
        typename: None,
        anonymous: false,
        fields: USER_ERROR_FIELDS,
        methods: &[],
        variants: &[],
        members: &[],
        type_arguments: &[],
        heritage: &[],
    });
}

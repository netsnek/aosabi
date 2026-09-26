// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the directive definition, the directive locations and the
 * five specified directives.
 *
 * Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
 * Derived from graphql-js 16.12.0, src/language/directiveLocation.ts, MIT.
 */

//! The directives of the type layer.
//!
//! A [`Directive`] is the definition of a directive, with its locations and its
//! arguments, and [`DirectiveLocation`] is the enum of the language layer that
//! names where a directive may appear, carried here because the language crate
//! has no reader for it. The five specified directives are module level
//! constants in the reference. Here their names, descriptions, locations and
//! arguments are rows of read only data, and one static holds the five
//! directives built from the rows on first use. A function per directive
//! returns the one shared allocation, and [`specified_directives()`] lists them
//! in the reference's order, which is the order a schema without its own
//! directive list carries.
//!
//! The constructor of the reference can fail only in `assertName` and in
//! `defineArguments`, and the five names and their argument names are
//! literals that pass both, so each directive is written directly from its
//! row with the values the constructor would compute.

use alloc::{
    boxed::Box,
    string::String,
    sync::Arc,
    vec::Vec, //
};
use core::fmt;

use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::ast::DirectiveDefinitionNode;
use once_cell::race::OnceBox;

use crate::{
    assert_name::assert_name,
    definition::{
        Argument,
        ArgumentConfig,
        DefinitionError,
        NamedType,
        NonNullType,
        ObjMap,
        ScalarType,
        Type,
        args_to_args_config,
        define_arguments, //
    },
    scalars::{
        graphql_boolean,
        graphql_string, //
    }, //
};

/// The set of allowed directive location values.
///
/// Each variant carries the string the reference stores, which
/// [`DirectiveLocation::as_str`] returns.
///
/// Derived from graphql-js 16.12.0, src/language/directiveLocation.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DirectiveLocation {
    /// `QUERY`.
    Query,
    /// `MUTATION`.
    Mutation,
    /// `SUBSCRIPTION`.
    Subscription,
    /// `FIELD`.
    Field,
    /// `FRAGMENT_DEFINITION`.
    FragmentDefinition,
    /// `FRAGMENT_SPREAD`.
    FragmentSpread,
    /// `INLINE_FRAGMENT`.
    InlineFragment,
    /// `VARIABLE_DEFINITION`.
    VariableDefinition,
    /// `SCHEMA`.
    Schema,
    /// `SCALAR`.
    Scalar,
    /// `OBJECT`.
    Object,
    /// `FIELD_DEFINITION`.
    FieldDefinition,
    /// `ARGUMENT_DEFINITION`.
    ArgumentDefinition,
    /// `INTERFACE`.
    Interface,
    /// `UNION`.
    Union,
    /// `ENUM`.
    Enum,
    /// `ENUM_VALUE`.
    EnumValue,
    /// `INPUT_OBJECT`.
    InputObject,
    /// `INPUT_FIELD_DEFINITION`.
    InputFieldDefinition,
}

impl DirectiveLocation {
    /// Every location, in the reference's declaration order.
    pub const ALL: [Self; 19] = [
        Self::Query,
        Self::Mutation,
        Self::Subscription,
        Self::Field,
        Self::FragmentDefinition,
        Self::FragmentSpread,
        Self::InlineFragment,
        Self::VariableDefinition,
        Self::Schema,
        Self::Scalar,
        Self::Object,
        Self::FieldDefinition,
        Self::ArgumentDefinition,
        Self::Interface,
        Self::Union,
        Self::Enum,
        Self::EnumValue,
        Self::InputObject,
        Self::InputFieldDefinition,
    ];

    /// Returns the string the reference stores for the location.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Query => "QUERY",
            Self::Mutation => "MUTATION",
            Self::Subscription => "SUBSCRIPTION",
            Self::Field => "FIELD",
            Self::FragmentDefinition => "FRAGMENT_DEFINITION",
            Self::FragmentSpread => "FRAGMENT_SPREAD",
            Self::InlineFragment => "INLINE_FRAGMENT",
            Self::VariableDefinition => "VARIABLE_DEFINITION",
            Self::Schema => "SCHEMA",
            Self::Scalar => "SCALAR",
            Self::Object => "OBJECT",
            Self::FieldDefinition => "FIELD_DEFINITION",
            Self::ArgumentDefinition => "ARGUMENT_DEFINITION",
            Self::Interface => "INTERFACE",
            Self::Union => "UNION",
            Self::Enum => "ENUM",
            Self::EnumValue => "ENUM_VALUE",
            Self::InputObject => "INPUT_OBJECT",
            Self::InputFieldDefinition => "INPUT_FIELD_DEFINITION",
        }
    }

    /// Returns the location a name stands for.
    ///
    /// This is the lookup `DirectiveLocation[name]` the parser performs on a
    /// directive definition, and a name that is no location is `None`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|location| location.as_str() == name)
    }
}

impl fmt::Display for DirectiveLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The configuration of a directive.
///
/// The optional `args` of the reference defaults to an empty map, which is the
/// empty map here, and the optional `isRepeatable` defaults to false.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[derive(Debug, Default)]
pub struct DirectiveConfig {
    /// The name, without the `@`.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The locations the directive may appear at.
    pub locations: Vec<DirectiveLocation>,
    /// The name of every location in its order, for a directive of a client
    /// schema with a location [`DirectiveLocation`] does not name, and none
    /// otherwise.
    pub location_names: Option<Vec<String>>,
    /// The arguments, keyed by name.
    pub args: ObjMap<ArgumentConfig>,
    /// Whether the directive may appear more than once at one location.
    pub is_repeatable: bool,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the directive was built from.
    pub ast_node: Option<DirectiveDefinitionNode>,
}

/// Directives are used by the GraphQL runtime as a way of modifying
/// execution behavior.
///
/// Type system creators will usually not create these directly.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[derive(Clone, Debug)]
pub struct Directive {
    /// The name, without the `@`.
    pub name: String,
    /// The description.
    pub description: Option<String>,
    /// The locations the directive may appear at.
    pub locations: Vec<DirectiveLocation>,
    /// The name of every location in its order, for a directive of a client
    /// schema with a location [`DirectiveLocation`] does not name, and none
    /// otherwise.
    ///
    /// A remote may answer a location graphql-js 16.12.0 does not know, such as
    /// one of a later version of GraphQL, and the reference keeps its string,
    /// which no step of a delegation reads. The known locations stand in
    /// `locations`, and the names here keep the count the validation reads and
    /// the text a printed schema writes.
    pub location_names: Option<Vec<String>>,
    /// The arguments, in definition order.
    pub args: Vec<Argument>,
    /// Whether the directive may appear more than once at one location.
    pub is_repeatable: bool,
    /// The extensions.
    pub extensions: JsObject,
    /// The definition node the directive was built from.
    pub ast_node: Option<DirectiveDefinitionNode>,
}

impl Directive {
    /// Builds a directive from `config`.
    ///
    /// The two `devAssert` calls, that the locations are an array and that the
    /// args are an object, have no reading, because the config carries both
    /// shapes.
    ///
    /// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the error of [`assert_name()`] for the directive name or an
    /// argument name.
    pub fn new(config: DirectiveConfig) -> Result<Self, DefinitionError> {
        let name = assert_name(&config.name)?;
        let args = define_arguments(&config.args)?;
        Ok(Self {
            name,
            description: config.description,
            locations: config.locations,
            location_names: config.location_names,
            args,
            is_repeatable: config.is_repeatable,
            extensions: config.extensions,
            ast_node: config.ast_node,
        })
    }

    /// Returns the config the directive was built from, normalized.
    ///
    /// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
    #[must_use]
    pub fn to_config(&self) -> DirectiveConfig {
        DirectiveConfig {
            name: self.name.clone(),
            description: self.description.clone(),
            locations: self.locations.clone(),
            location_names: self.location_names.clone(),
            args: args_to_args_config(&self.args),
            is_repeatable: self.is_repeatable,
            extensions: self.extensions.clone(),
            ast_node: self.ast_node.clone(),
        }
    }
}

impl fmt::Display for Directive {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "@{}", self.name)
    }
}

/// An argument of a specified directive as the image carries it.
///
/// The remaining fields of the argument are what `defineArguments` fills in for
/// an argument config that leaves them out.
struct ArgumentRow {
    /// The name.
    name: &'static str,
    /// The description.
    description: &'static str,
    /// Returns the specified scalar the type names.
    scalar: fn() -> Arc<ScalarType>,
    /// Whether the type is that scalar made non null.
    non_null: bool,
    /// The default value, `None` when it is undefined.
    default_value: Option<&'static str>,
}

/// A specified directive as the image carries it.
///
/// A specified directive is not repeatable and carries no extensions and no
/// node, which is what the constructor of the reference fills in for a config
/// that leaves them out.
struct DirectiveRow {
    /// The name, without the `@`.
    name: &'static str,
    /// The description.
    description: &'static str,
    /// The locations the directive may appear at.
    locations: &'static [DirectiveLocation],
    /// The arguments, in definition order.
    args: &'static [ArgumentRow],
}

/// Constant string used for default reason for a deprecation.
pub const DEFAULT_DEPRECATION_REASON: &str = "No longer supported";

/// The five specified directives, in the order of `specifiedDirectives`, with
/// the configs.
static SPECIFIED: [DirectiveRow; 5] = [
    DirectiveRow {
        name: "include",
        description: "Directs the executor to include this field or fragment only when the `if` argument is true.",
        locations: &[
            DirectiveLocation::Field,
            DirectiveLocation::FragmentSpread,
            DirectiveLocation::InlineFragment,
        ],
        args: &[ArgumentRow {
            name: "if",
            description: "Included when true.",
            scalar: graphql_boolean,
            non_null: true,
            default_value: None,
        }],
    },
    DirectiveRow {
        name: "skip",
        description: "Directs the executor to skip this field or fragment when the `if` argument is true.",
        locations: &[
            DirectiveLocation::Field,
            DirectiveLocation::FragmentSpread,
            DirectiveLocation::InlineFragment,
        ],
        args: &[ArgumentRow {
            name: "if",
            description: "Skipped when true.",
            scalar: graphql_boolean,
            non_null: true,
            default_value: None,
        }],
    },
    DirectiveRow {
        name: "deprecated",
        description: "Marks an element of a GraphQL schema as no longer supported.",
        locations: &[
            DirectiveLocation::FieldDefinition,
            DirectiveLocation::ArgumentDefinition,
            DirectiveLocation::InputFieldDefinition,
            DirectiveLocation::EnumValue,
        ],
        args: &[ArgumentRow {
            name: "reason",
            description: "Explains why this element was deprecated, usually also including a suggestion for how to access supported similar data. Formatted using the Markdown syntax, as specified by [CommonMark](https://commonmark.org/).",
            scalar: graphql_string,
            non_null: false,
            default_value: Some(DEFAULT_DEPRECATION_REASON),
        }],
    },
    DirectiveRow {
        name: "specifiedBy",
        description: "Exposes a URL that specifies the behavior of this scalar.",
        locations: &[DirectiveLocation::Scalar],
        args: &[ArgumentRow {
            name: "url",
            description: "The URL that specifies the behavior of this scalar.",
            scalar: graphql_string,
            non_null: true,
            default_value: None,
        }],
    },
    DirectiveRow {
        name: "oneOf",
        description: "Indicates exactly one field must be supplied and this field must not be `null`.",
        locations: &[DirectiveLocation::InputObject],
        args: &[],
    },
];

/// The position of `@include` in [`SPECIFIED`].
const INCLUDE: usize = 0;

/// The position of `@skip` in [`SPECIFIED`].
const SKIP: usize = 1;

/// The position of `@deprecated` in [`SPECIFIED`].
const DEPRECATED: usize = 2;

/// The position of `@specifiedBy` in [`SPECIFIED`].
const SPECIFIED_BY: usize = 3;

/// The position of `@oneOf` in [`SPECIFIED`].
const ONE_OF: usize = 4;

/// Builds the argument of one row.
fn specified_argument(row: &ArgumentRow) -> Argument {
    let named = NamedType::from((row.scalar)());
    Argument {
        name: String::from(row.name),
        description: Some(String::from(row.description)),
        r#type: if row.non_null {
            Type::from(NonNullType::from(named))
        } else {
            Type::from(named)
        },
        default_value: row.default_value.map_or(JsValue::Undefined, JsValue::from),
        deprecation_reason: None,
        extensions: JsObject::new(),
        ast_node: None,
    }
}

/// Builds the directive of one row.
///
/// The strings and the lists stay in the row, and the directive copies them,
/// because its fields are owned.
fn specified_directive(row: &DirectiveRow) -> Arc<Directive> {
    Arc::new(Directive {
        name: String::from(row.name),
        description: Some(String::from(row.description)),
        locations: row.locations.to_vec(),
        location_names: None,
        args: row.args.iter().map(specified_argument).collect(),
        is_repeatable: false,
        extensions: JsObject::new(),
        ast_node: None,
    })
}

/// The five directives built from [`SPECIFIED`], in its order.
static BUILT: OnceBox<[Arc<Directive>; 5]> = OnceBox::new();

/// Returns the five directives, building them on first use.
fn built() -> &'static [Arc<Directive>; 5] {
    BUILT.get_or_init(|| Box::new(SPECIFIED.each_ref().map(specified_directive)))
}

/// Returns the `@include` directive, used to conditionally include fields or
/// fragments.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[must_use]
pub fn graphql_include_directive() -> Arc<Directive> {
    Arc::clone(&built()[INCLUDE])
}

/// Returns the `@skip` directive, used to conditionally skip (exclude)
/// fields or fragments.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[must_use]
pub fn graphql_skip_directive() -> Arc<Directive> {
    Arc::clone(&built()[SKIP])
}

/// Returns the `@deprecated` directive, used to declare element of a
/// GraphQL schema as deprecated.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[must_use]
pub fn graphql_deprecated_directive() -> Arc<Directive> {
    Arc::clone(&built()[DEPRECATED])
}

/// Returns the `@specifiedBy` directive, used to provide a URL for
/// specifying the behavior of custom scalar definitions.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[must_use]
pub fn graphql_specified_by_directive() -> Arc<Directive> {
    Arc::clone(&built()[SPECIFIED_BY])
}

/// Returns the `@oneOf` directive, used to indicate an Input Object is a
/// `OneOf` Input Object.
///
/// Every call returns the same allocation.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[must_use]
pub fn graphql_one_of_directive() -> Arc<Directive> {
    Arc::clone(&built()[ONE_OF])
}

/// Returns the full list of specified directives, in the reference's order.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[must_use]
pub fn specified_directives() -> &'static [Arc<Directive>] {
    built()
}

/// Returns whether the directive carries the name of a specified directive.
///
/// Compares names and not identities. The names are read from the rows of
/// `SPECIFIED`, so the predicate builds no directive.
///
/// Derived from graphql-js 16.12.0, src/type/directives.ts, MIT.
#[must_use]
pub fn is_specified_directive(directive: &Directive) -> bool {
    SPECIFIED
        .iter()
        .any(|specified| directive.name == specified.name)
}

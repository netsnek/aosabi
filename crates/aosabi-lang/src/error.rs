// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the GraphQL error, the syntax error and the located error.
 *
 * Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
 * Derived from graphql-js 16.12.0, src/error/syntaxError.ts, MIT.
 * Derived from graphql-js 16.12.0, src/error/locatedError.ts, MIT.
 */

//! The error of the language, type, validation and execution layers.
//!
//! [`GraphQLError`] is the one error every graphql-js phase throws, and its
//! JSON shape is the first layer of the error format the wire carries: the
//! keys `message`, `locations`, `path` and `extensions` in that order, each
//! left out when absent, and `extensions` left out when it is empty.
//! [`GraphQLError::to_json`] builds that shape as a [`GraphQLFormattedError`],
//! and the conversions into [`JsObject`] insert the keys in that order.
//!
//! Two shapes of the reference read differently in Rust.
//!
//! - The constructor of the reference accepts an options object or the
//!   deprecated positional list, and `toNormalizedOptions` folds the second
//!   into the first. There is one constructor here and it takes
//!   [`GraphQLErrorOptions`], so the fold has no item.
//! - `originalError` is any thrown value, wrapped by `toError` when it is not
//!   an `Error`. A Rust resolver returns an error value, so the original is
//!   an [`OriginalError`], which is either a [`GraphQLError`] or any other
//!   error, and the `NonErrorThrown` wrapper has no item.
//!
//! A location is computed from a position through [`get_location`] and counts
//! UTF-16 code units, and a path segment is a string or an integer.
//!
//! The message is a [`JsString`], because the message of the reference is a
//! JavaScript string and three lexer errors quote a window of the body that
//! ends on a leading surrogate. The JSON shape carries that code unit to the
//! wire unchanged, and `Display` prints the text of [`to_string_lossy`], which
//! is the one place a lone surrogate reads as U+FFFD.
//!
//! `Debug` of [`GraphQLError`] and [`OriginalError`] is written by hand. An
//! error shows its message, its locations and its path, each as the JSON the
//! writer of `aosabi-json` makes of it, so a lone surrogate reads as its
//! escape. The nodes, the source, the positions, the original error and the
//! extensions stay out of it, so the vtable of `dyn Error` that boxes an error
//! reaches neither the `Debug` of the syntax tree nor the float formatting of
//! `core`.

use alloc::{
    boxed::Box,
    string::{
        String,
        ToString, //
    },
    sync::Arc,
    vec,
    vec::Vec, //
};
use core::fmt;

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};

use crate::{
    ast::AstNode,
    js_string::{
        JsStringBuilder,
        to_string_lossy, //
    },
    location::{
        SourceLocation,
        get_location,
        js_number, //
    },
    print_location::{
        print_location,
        print_source_location, //
    },
    source::Source, //
};

/// Custom extensions of an error.
///
/// The reference types this as an object of arbitrary properties. It is a JSON
/// object position, so the carrier is the insertion ordered object of the JSON
/// writer.
pub type GraphQLErrorExtensions = JsObject;

/// Custom formatted extensions of an error.
pub type GraphQLFormattedErrorExtensions = JsObject;

/// One segment of the path into the execution response.
///
/// The reference types a segment as `string | number`, a field key or a list
/// index, and the two variants are what row D22 of docs/byte-equality.md asks
/// for.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// A response key.
    Key(String),
    /// A list index.
    Index(usize),
}

impl From<&str> for PathSegment {
    fn from(key: &str) -> Self {
        Self::Key(String::from(key))
    }
}

impl From<String> for PathSegment {
    fn from(key: String) -> Self {
        Self::Key(key)
    }
}

impl From<usize> for PathSegment {
    fn from(index: usize) -> Self {
        Self::Index(index)
    }
}

impl From<&PathSegment> for JsValue {
    fn from(segment: &PathSegment) -> Self {
        match segment {
            PathSegment::Key(key) => Self::from(key.as_str()),
            PathSegment::Index(index) => js_number(*index),
        }
    }
}

impl From<PathSegment> for JsValue {
    fn from(segment: PathSegment) -> Self {
        Self::from(&segment)
    }
}

/// The original error thrown from a field resolver during execution.
///
/// The reference stores any `Error` and reads its `message`, and its
/// `extensions` when it has an object there. A Rust error carries no
/// `extensions` property, and the one error of the reference stack that does
/// is a GraphQL error or a subclass of one, so the enum has a variant for that
/// and a variant for every other error.
pub enum OriginalError {
    /// A GraphQL error, or a subclass of one in the reference.
    GraphQL(Box<GraphQLError>),
    /// Any other error.
    Other(Box<dyn core::error::Error + Send + Sync>),
}

impl OriginalError {
    /// Returns the `message` of the error.
    ///
    /// A GraphQL error answers its message, and any other error answers the
    /// text of its `Display`, which is the `message` a Rust error has.
    #[must_use]
    pub fn message(&self) -> JsString {
        match self {
            Self::GraphQL(error) => error.message.clone(),
            Self::Other(error) => JsString::from(error.to_string()),
        }
    }

    /// Returns the `extensions` of the error, when it carries an object there.
    ///
    /// This is `isObjectLike(originalError?.extensions)` of the reference: a
    /// GraphQL error always has the object, and any other error has none.
    #[must_use]
    pub fn extensions(&self) -> Option<&GraphQLErrorExtensions> {
        match self {
            Self::GraphQL(error) => Some(&error.extensions),
            Self::Other(_) => None,
        }
    }

    /// Returns the error as the standard error trait object.
    #[must_use]
    pub fn as_error(&self) -> &(dyn core::error::Error + 'static) {
        match self {
            Self::GraphQL(error) => error.as_ref(),
            Self::Other(error) => error.as_ref(),
        }
    }
}

impl From<GraphQLError> for OriginalError {
    fn from(error: GraphQLError) -> Self {
        Self::GraphQL(Box::new(error))
    }
}

impl From<Box<dyn core::error::Error + Send + Sync>> for OriginalError {
    fn from(error: Box<dyn core::error::Error + Send + Sync>) -> Self {
        Self::Other(error)
    }
}

impl fmt::Display for OriginalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GraphQL(error) => fmt::Display::fmt(error, f),
            Self::Other(error) => fmt::Display::fmt(error, f),
        }
    }
}

impl fmt::Debug for OriginalError {
    /// Writes the variant around the `Debug` of the error it holds.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::GraphQL(error) => f.debug_tuple("GraphQL").field(error).finish(),
            Self::Other(error) => f.debug_tuple("Other").field(error).finish(),
        }
    }
}

/// The options of [`GraphQLError::new`].
///
/// The reference accepts one node or an array of nodes, and a single node is a
/// one element vector here. Every field is optional, so the default is the
/// empty options.
///
/// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
#[derive(Debug, Default)]
pub struct GraphQLErrorOptions {
    /// The nodes to blame.
    pub nodes: Option<Vec<AstNode>>,
    /// The source the positions point into.
    pub source: Option<Arc<Source>>,
    /// The positions in the source, as UTF-16 code unit offsets.
    pub positions: Option<Vec<usize>>,
    /// The path into the execution response.
    pub path: Option<Vec<PathSegment>>,
    /// The error that was thrown originally.
    pub original_error: Option<OriginalError>,
    /// The extensions to add to the formatted error.
    pub extensions: Option<GraphQLErrorExtensions>,
}

/// The path and the locations of an error of a host as graphql-js writes them,
/// where the model of a path and of a location holds no form for them.
///
/// graphql-js writes the path an app gives a `GraphQLError` as it is, and the
/// column of a position that is no whole number as the number `getLocation`
/// computes, so an error whose path holds an item that is no string and no
/// index, or whose positions hold a number that is no whole number, answers
/// that JSON. Only a build of the TypeScript option holds the type, whose host
/// makes such an error.
#[cfg(feature = "host-errors")]
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HostPlaces {
    /// The path as the JSON graphql-js writes, in place of the path of the
    /// error.
    pub path: Option<JsValue>,
    /// The locations as the JSON graphql-js writes, in place of the
    /// locations of the error.
    pub locations: Option<JsValue>,
}

/// A `GraphQLError` describes an Error found during the parse, validate, or
/// execute phases of performing a GraphQL operation.
///
/// In addition to a message, it also includes information about the locations
/// in a GraphQL document and/or execution result that correspond to the Error.
/// The stack trace of the reference has no reading here, and the `name` is the
/// type.
///
/// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
///
/// `Display` is the reference `toString`, the message followed by the
/// excerpt of every blamed location, and [`GraphQLError::to_json`] is the
/// reference `toJSON`. `Debug` shows the message, the locations and the path,
/// see the module documentation.
pub struct GraphQLError {
    /// A short, human-readable summary of the problem.
    ///
    /// The carrier is the JavaScript string of the workspace, see the module
    /// documentation. A `&str` and a `String` convert into it unchanged.
    pub message: JsString,
    /// An array of `{ line, column }` locations within the source GraphQL
    /// document which correspond to this error.
    ///
    /// Errors during validation often contain multiple locations, for example
    /// to point out two things with the same name. Errors during execution
    /// include a single location, the field which produced the error.
    pub locations: Option<Vec<SourceLocation>>,
    /// An array describing the JSON-path into the execution response which
    /// corresponds to this error. Only included for errors during execution.
    pub path: Option<Vec<PathSegment>>,
    /// An array of GraphQL AST Nodes corresponding to this error.
    pub nodes: Option<Vec<AstNode>>,
    /// The source GraphQL document for the first location of this error.
    ///
    /// Note that if this Error represents more than one node, the source may
    /// not represent nodes after the first node.
    pub source: Option<Arc<Source>>,
    /// An array of character offsets within the source GraphQL document
    /// which correspond to this error.
    pub positions: Option<Vec<usize>>,
    /// The original error thrown from a field resolver during execution.
    pub original_error: Option<OriginalError>,
    /// Extension fields to add to the formatted error.
    pub extensions: GraphQLErrorExtensions,
    /// The path and the locations the formatted error takes in place of
    /// `path` and `locations`, where an error of a host holds a path or a
    /// position the model holds no form for.
    #[cfg(feature = "host-errors")]
    pub host_places: Option<Box<HostPlaces>>,
}

impl GraphQLError {
    /// Creates an error with `message` and the blame `options` carry.
    ///
    /// The locations are computed from the positions and the source when both
    /// are given, and from the locations of the nodes otherwise. The source
    /// and the positions fall back to the nodes the same way, and the
    /// extensions fall back to those of the original error and then to an
    /// empty object.
    ///
    /// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
    #[must_use]
    pub fn new(message: impl Into<JsString>, options: GraphQLErrorOptions) -> Self {
        let GraphQLErrorOptions {
            nodes,
            source,
            positions,
            path,
            original_error,
            extensions,
        } = options;

        // Compute list of blame nodes.
        let nodes = undefined_if_empty(nodes);

        let node_locations = undefined_if_empty(nodes.as_ref().map(|nodes| {
            nodes
                .iter()
                .filter_map(|node| node.loc().cloned())
                .collect()
        }));

        // Compute locations in the source for the given nodes/positions.
        let first_node_source = node_locations
            .as_ref()
            .and_then(|locations| locations.first())
            .map(|location| Arc::clone(&location.source));

        // `positions && source` tests the arguments, before either falls back
        // to the nodes, and an array and an object are both truthy whatever
        // they hold.
        let locations = match (&positions, &source) {
            (Some(positions), Some(source)) => Some(
                positions
                    .iter()
                    .map(|&pos| get_location(source, pos))
                    .collect(),
            ),
            _ => node_locations.as_ref().map(|locations| {
                locations
                    .iter()
                    .map(|loc| get_location(&loc.source, loc.start))
                    .collect()
            }),
        };

        let source = source.or(first_node_source);
        let positions = positions.or_else(|| {
            node_locations
                .as_ref()
                .map(|locations| locations.iter().map(|loc| loc.start).collect())
        });

        let original_extensions = original_error
            .as_ref()
            .and_then(OriginalError::extensions)
            .cloned();
        let extensions = extensions.or(original_extensions).unwrap_or_default();

        Self {
            message: message.into(),
            locations,
            path,
            nodes,
            source,
            positions,
            original_error,
            extensions,
            #[cfg(feature = "host-errors")]
            host_places: None,
        }
    }

    /// Formats the error according to the Response Format, Errors section of
    /// the GraphQL Specification.
    ///
    /// The result carries `locations` and `path` when the error has them and
    /// `extensions` when the object is not empty.
    ///
    /// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
    #[must_use]
    pub fn to_json(&self) -> GraphQLFormattedError {
        let mut formatted_error = GraphQLFormattedError {
            message: self.message.clone(),
            locations: None,
            path: None,
            extensions: None,
            #[cfg(feature = "host-errors")]
            host_places: self.host_places.clone(),
        };

        if let Some(locations) = &self.locations {
            formatted_error.locations = Some(locations.clone());
        }

        if let Some(path) = &self.path {
            formatted_error.path = Some(path.clone());
        }

        if !self.extensions.is_empty() {
            formatted_error.extensions = Some(self.extensions.clone());
        }

        formatted_error
    }
}

impl fmt::Display for GraphQLError {
    /// Writes the message followed by the excerpt of every blamed location.
    ///
    /// The nodes win over the source and the locations when both are present,
    /// and a node without a location prints nothing.
    ///
    /// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&to_string_lossy(&self.message))?;

        if let Some(nodes) = &self.nodes {
            for node in nodes {
                if let Some(loc) = node.loc() {
                    f.write_str("\n\n")?;
                    f.write_str(&print_location(loc))?;
                }
            }
        } else if let (Some(source), Some(locations)) = (&self.source, &self.locations) {
            for &location in locations {
                f.write_str("\n\n")?;
                f.write_str(&print_source_location(source, location))?;
            }
        }

        Ok(())
    }
}

impl fmt::Debug for GraphQLError {
    /// Writes the message, the locations and the path, each as the JSON the
    /// writer of `aosabi-json` makes of it, and marks the members it leaves
    /// out.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut members = f.debug_struct("GraphQLError");
        members.field("message", &JsValue::from(self.message.clone()));
        if let Some(locations) = &self.locations {
            let locations: Vec<JsValue> = locations.iter().map(|&loc| JsValue::from(loc)).collect();
            members.field("locations", &JsValue::from(locations));
        }
        if let Some(path) = &self.path {
            let path: Vec<JsValue> = path.iter().map(JsValue::from).collect();
            members.field("path", &JsValue::from(path));
        }
        members.finish_non_exhaustive()
    }
}

impl core::error::Error for GraphQLError {
    fn source(&self) -> Option<&(dyn core::error::Error + 'static)> {
        self.original_error.as_ref().map(OriginalError::as_error)
    }
}

/// Returns `None` for an absent or an empty vector.
///
/// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
fn undefined_if_empty<T>(array: Option<Vec<T>>) -> Option<Vec<T>> {
    array.filter(|array| !array.is_empty())
}

/// The error as the Response Format, Errors section of the GraphQL
/// Specification describes it.
///
/// The conversions into [`JsObject`] and [`JsValue`] insert `message`,
/// `locations`, `path` and `extensions` in that order and skip an absent one,
/// which is the key order of docs/byte-equality.md section 8.1.
///
/// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct GraphQLFormattedError {
    /// A short, human-readable summary of the problem that should not change
    /// from occurrence to occurrence of the problem, except for purposes of
    /// localization.
    pub message: JsString,
    /// If an error can be associated to a particular point in the requested
    /// GraphQL document, it should contain a list of locations.
    pub locations: Option<Vec<SourceLocation>>,
    /// If an error can be associated to a particular field in the GraphQL
    /// result, it must contain an entry with the key `path` that details the
    /// path of the response field which experienced the error. This allows
    /// clients to identify whether a null result is intentional or caused by
    /// a runtime error.
    pub path: Option<Vec<PathSegment>>,
    /// Reserved for implementors to extend the protocol however they see fit,
    /// and hence there are no additional restrictions on its contents.
    pub extensions: Option<GraphQLFormattedErrorExtensions>,
    /// The path and the locations written in place of `path` and
    /// `locations`, those of [`GraphQLError::host_places`].
    #[cfg(feature = "host-errors")]
    pub host_places: Option<Box<HostPlaces>>,
}

impl From<&GraphQLFormattedError> for JsObject {
    fn from(error: &GraphQLFormattedError) -> Self {
        let mut object = Self::with_capacity(4);
        object.insert("message", error.message.clone());
        #[cfg(feature = "host-errors")]
        let (host_locations, host_path) =
            error.host_places.as_deref().map_or((None, None), |places| {
                (places.locations.clone(), places.path.clone())
            });
        #[cfg(not(feature = "host-errors"))]
        let (host_locations, host_path): (Option<JsValue>, Option<JsValue>) = (None, None);
        if let Some(locations) = host_locations {
            object.insert("locations", locations);
        } else if let Some(locations) = &error.locations {
            let locations: Vec<JsValue> = locations.iter().map(|&loc| JsValue::from(loc)).collect();
            object.insert("locations", locations);
        }
        if let Some(path) = host_path {
            object.insert("path", path);
        } else if let Some(path) = &error.path {
            let path: Vec<JsValue> = path.iter().map(JsValue::from).collect();
            object.insert("path", path);
        }
        if let Some(extensions) = &error.extensions {
            object.insert("extensions", extensions.clone());
        }
        object
    }
}

impl From<GraphQLFormattedError> for JsObject {
    fn from(error: GraphQLFormattedError) -> Self {
        Self::from(&error)
    }
}

impl From<&GraphQLFormattedError> for JsValue {
    fn from(error: &GraphQLFormattedError) -> Self {
        Self::Object(JsObject::from(error))
    }
}

impl From<GraphQLFormattedError> for JsValue {
    fn from(error: GraphQLFormattedError) -> Self {
        Self::Object(JsObject::from(&error))
    }
}

/// Prints a `GraphQLError` to a string, representing useful location
/// information about the error's position in the source.
///
/// The reference deprecates this in favour of `error.toString`, which is
/// `Display` here.
///
/// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
#[must_use]
pub fn print_error(error: &GraphQLError) -> String {
    error.to_string()
}

/// Given a `GraphQLError`, format it according to the rules described by the
/// Response Format, Errors section of the GraphQL Specification.
///
/// The reference deprecates this in favour of `error.toJSON`, which is
/// [`GraphQLError::to_json`].
///
/// Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
#[must_use]
pub fn format_error(error: &GraphQLError) -> GraphQLFormattedError {
    error.to_json()
}

/// Produces a `GraphQLError` representing a syntax error, containing useful
/// descriptive information about the syntax error's position in the source.
///
/// The description is a JavaScript string, because three descriptions of the
/// lexer quote the body up to a leading surrogate.
///
/// Derived from graphql-js 16.12.0, src/error/syntaxError.ts, MIT.
#[must_use]
pub fn syntax_error(
    source: &Arc<Source>,
    position: usize,
    description: impl Into<JsString>,
) -> GraphQLError {
    let mut message = JsStringBuilder::new();
    message.push_str("Syntax Error: ").push(&description.into());
    GraphQLError::new(
        message.finish(),
        GraphQLErrorOptions {
            source: Some(Arc::clone(source)),
            positions: Some(vec![position]),
            ..GraphQLErrorOptions::default()
        },
    )
}

/// Given an error thrown while attempting to execute a GraphQL operation,
/// produce a new `GraphQLError` aware of the location in the document
/// responsible for the original Error.
///
/// `original_error` is what the resolver returned, `nodes` the nodes to blame
/// when the original error names none, and `path` the path into the response.
/// A GraphQL error that already carries a path is returned as it is. The
/// `toError` wrapper of the reference has no reading here, because the
/// argument is an error already.
///
/// Derived from graphql-js 16.12.0, src/error/locatedError.ts, MIT.
#[must_use]
pub fn located_error(
    original_error: OriginalError,
    nodes: Option<Vec<AstNode>>,
    path: Option<Vec<PathSegment>>,
) -> GraphQLError {
    // Note: this uses a brand-check to support GraphQL errors originating from other contexts.
    let original_error = match original_error {
        OriginalError::GraphQL(error) if is_located_graphql_error(&error) => return *error,
        error => error,
    };

    let (nodes, source, positions) = match &original_error {
        OriginalError::GraphQL(error) => (
            error.nodes.clone().or(nodes),
            error.source.clone(),
            error.positions.clone(),
        ),
        OriginalError::Other(_) => (nodes, None, None),
    };
    // The located error computes its locations from the positions of the
    // original error, so it takes the locations of a host as they are.
    #[cfg(feature = "host-errors")]
    let host_places = match &original_error {
        OriginalError::GraphQL(error) => error.host_places.clone(),
        OriginalError::Other(_) => None,
    };

    let located = GraphQLError::new(
        original_error.message(),
        GraphQLErrorOptions {
            nodes,
            source,
            positions,
            path,
            original_error: Some(original_error),
            extensions: None,
        },
    );
    #[cfg(feature = "host-errors")]
    let located = GraphQLError {
        host_places,
        ..located
    };
    located
}

/// Returns whether the error already carries a path, which is the brand the
/// reference checks with `Array.isArray(error.path)`.
///
/// Derived from graphql-js 16.12.0, src/error/locatedError.ts, MIT.
fn is_located_graphql_error(error: &GraphQLError) -> bool {
    error.path.is_some()
}

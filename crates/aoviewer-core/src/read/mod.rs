// SPDX-License-Identifier: MIT
/*
 * aoviewer-core read: the two readers of the core and the writer of the
 * canonical introspection form.
 *
 * An introspection result and the ir.json of a build fill the same model, and the
 * model writes back the introspection form the schema hash is taken over.
 */

//! The readers of the core.
//!
//! [`from_introspection`] reads the answer of an introspection query and
//! [`from_ir_json`] maps the `ir.json` of a build, both into a
//! [󰀃](crate::model::Builder), and [`to_introspection`] writes the
//! introspection result a model answers. The readers are the one place the
//! core parses untrusted bytes, so every count, length and nesting they accept
//! is capped by [`ReadLimits`] and every defect is a [`ReadError`].
//!
//! Every loop that parses reads at least one token per iteration, and the
//! parser yields at most one token per byte of an input that
//! [`ReadLimits::max_input_bytes`] caps. Every loop over a list the readers
//! buffer before the builder sees it runs over a list that
//! [`ReadLimits::max_members`] or [`ReadLimits::max_types`] capped while it
//! grew. The others run over the three root operation types or over the
//! path an error names, one segment per level of nesting, which
//! [`ReadLimits::max_depth`] caps.
//!
//! A key the readers know that stands twice in one object is refused, because
//! JSON leaves the meaning of a repeated name open. An unknown key is skipped,
//! so an answer that carries more than [`INTROSPECTION_QUERY`] asks for still
//! reads.
//!
//! [RFC 8259]: https://www.rfc-editor.org/rfc/rfc8259

use alloc::{
    borrow::Cow,
    string::String,
    vec::Vec, //
};
use core::fmt::Write as _;

use crate::{
    json::{
        JsonError,
        Parser,
        Token, //
    },
    limits::ReadLimits,
    model::ModelError,
};

mod introspection;
mod ir;
mod write;

pub use introspection::from_introspection;
pub use ir::from_ir_json;
pub use write::to_introspection;

/// The query the viewer posts and the `--introspection` input answers.
///
/// It asks for descriptions, the deprecations of fields, arguments, input
/// fields and enum values, `specifiedByURL`, repeatable directives and the
/// description of the schema, which is the selection `getIntrospectionQuery`
/// of graphql-js 16.12.0 makes with `descriptions`, `specifiedByUrl`,
/// `directiveIsRepeatable`, `schemaDescription` and `inputValueDeprecation`
/// on. A type reference is followed through nine wrappers, as that query
/// follows it. [`to_introspection`] writes the answer to this query in the
/// order of its selections.
pub const INTROSPECTION_QUERY: &str = concat!(
    "query IntrospectionQuery {\n",
    "  __schema {\n",
    "    description\n",
    "    queryType { name kind }\n",
    "    mutationType { name kind }\n",
    "    subscriptionType { name kind }\n",
    "    types { ...FullType }\n",
    "    directives {\n",
    "      name\n",
    "      description\n",
    "      isRepeatable\n",
    "      locations\n",
    "      args(includeDeprecated: true) { ...InputValue }\n",
    "    }\n",
    "  }\n",
    "}\n",
    "\n",
    "fragment FullType on __Type {\n",
    "  kind\n",
    "  name\n",
    "  description\n",
    "  specifiedByURL\n",
    "  fields(includeDeprecated: true) {\n",
    "    name\n",
    "    description\n",
    "    args(includeDeprecated: true) { ...InputValue }\n",
    "    type { ...TypeRef }\n",
    "    isDeprecated\n",
    "    deprecationReason\n",
    "  }\n",
    "  inputFields(includeDeprecated: true) { ...InputValue }\n",
    "  interfaces { ...TypeRef }\n",
    "  enumValues(includeDeprecated: true) {\n",
    "    name\n",
    "    description\n",
    "    isDeprecated\n",
    "    deprecationReason\n",
    "  }\n",
    "  possibleTypes { ...TypeRef }\n",
    "}\n",
    "\n",
    "fragment InputValue on __InputValue {\n",
    "  name\n",
    "  description\n",
    "  type { ...TypeRef }\n",
    "  defaultValue\n",
    "  isDeprecated\n",
    "  deprecationReason\n",
    "}\n",
    "\n",
    "fragment TypeRef on __Type {\n",
    "  kind\n",
    "  name\n",
    "  ofType { kind name ofType { kind name ofType { kind name ofType { kind name\n",
    "  ofType { kind name ofType { kind name ofType { kind name ofType { kind name\n",
    "  ofType { kind name } } } } } } } } }\n",
    "}\n",
);

/// A reason a reader refuses its input.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReadError {
    /// The bytes are no JSON document within the limits.
    Json {
        /// The byte offset the parser stands at.
        offset: u64,
        /// What the parser found.
        error: JsonError,
    },
    /// The document is JSON of another shape than the reader reads.
    Shape {
        /// The path of the value, such as `data.__schema.types[3].kind`, which
        /// is empty for the document itself.
        path: String,
        /// What the reader expected there.
        expected: &'static str,
    },
    /// The schema the document describes is refused by the model builder.
    Model(ModelError),
}

impl From<ModelError> for ReadError {
    fn from(e: ModelError) -> Self {
        ReadError::Model(e)
    }
}

/// One step of the path from the document to a value.
#[derive(Debug)]
enum Seg<'a> {
    /// A key the reader knows.
    Key(&'static str),
    /// A key of a record, which names a type of `ir.json`.
    Record(Cow<'a, str>),
    /// The position of an element in an array.
    Index(u32),
}

/// The tokens of one document, with the path of the value they stand in and
/// the counts the limits cap.
#[derive(Debug)]
struct Cursor<'a> {
    parser: Parser<'a>,
    /// A token read ahead to tell the end of an array from its next element.
    peeked: Option<Token<'a>>,
    path: Vec<Seg<'a>>,
    limits: ReadLimits,
    /// The fields, arguments and enum values read so far.
    members: u32,
    /// The named types read so far, which the reader of `ir.json` counts.
    types: u32,
}

impl<'a> Cursor<'a> {
    /// Returns a cursor at the start of `bytes`.
    fn new(bytes: &'a [u8], limits: &ReadLimits) -> Self {
        Self {
            parser: Parser::new(bytes, limits),
            peeked: None,
            path: Vec::new(),
            limits: *limits,
            members: 0,
            types: 0,
        }
    }

    /// Returns the next token of the document.
    fn next(&mut self) -> Result<Token<'a>, ReadError> {
        if let Some(token) = self.peeked.take() {
            return Ok(token);
        }
        match self.parser.next() {
            Ok(Some(token)) => Ok(token),
            // The reader asks for one value and never beyond it, so the end
            // arrives only inside a value the parser has not refused yet.
            Ok(None) => Err(self.json(JsonError::Eof)),
            Err(e) => Err(self.json(e)),
        }
    }

    /// Accepts the end of the document after its one value.
    fn end(&mut self) -> Result<(), ReadError> {
        match self.parser.next() {
            Ok(None) => Ok(()),
            Ok(Some(_)) => Err(self.json(JsonError::TrailingData)),
            Err(e) => Err(self.json(e)),
        }
    }

    /// Returns the error of the parser at its offset.
    fn json(&self, error: JsonError) -> ReadError {
        ReadError::Json {
            offset: self.parser.offset(),
            error,
        }
    }

    /// Returns the error for a value that is not what the reader expected.
    fn shape(&self, expected: &'static str) -> ReadError {
        let mut path = String::new();
        for seg in &self.path {
            match seg {
                Seg::Key(key) => dotted(&mut path, key),
                Seg::Record(key) => dotted(&mut path, key),
                Seg::Index(i) => {
                    // Writing into a `String` cannot fail.
                    let _ = write!(path, "[{i}]");
                }
            }
        }
        ReadError::Shape { path, expected }
    }

    /// Enters the value of the key `key`.
    fn enter(&mut self, key: &'static str) {
        self.path.push(Seg::Key(key));
    }

    /// Enters the value of the record key `key`.
    fn enter_record(&mut self, key: Cow<'a, str>) {
        self.path.push(Seg::Record(key));
    }

    /// Leaves the value entered last.
    fn leave(&mut self) {
        self.path.pop();
    }

    /// Reads the value of the known key `key` with `read`.
    ///
    /// Each key of one object carries its own bit in `seen`, which refuses
    /// the key given twice.
    fn member<T>(
        &mut self,
        key: &'static str,
        bit: u32,
        seen: &mut u32,
        read: impl FnOnce(&mut Self) -> Result<T, ReadError>,
    ) -> Result<T, ReadError> {
        self.enter(key);
        if *seen & bit != 0 {
            return Err(self.shape("a key given once"));
        }
        *seen |= bit;
        let value = read(self)?;
        self.leave();
        Ok(value)
    }

    /// Reads the `{` that opens an object.
    fn object(&mut self) -> Result<(), ReadError> {
        match self.next()? {
            Token::BeginObject => Ok(()),
            _ => Err(self.shape("an object")),
        }
    }

    /// Reads the `{` that opens an object, or `null`, and returns whether an
    /// object opened.
    fn object_or_null(&mut self) -> Result<bool, ReadError> {
        match self.next()? {
            Token::BeginObject => Ok(true),
            Token::Null => Ok(false),
            _ => Err(self.shape("an object or null")),
        }
    }

    /// Returns the next key of the open object, or `None` at its end.
    fn key(&mut self) -> Result<Option<Cow<'a, str>>, ReadError> {
        match self.next()? {
            Token::Key(key) => Ok(Some(key)),
            Token::EndObject => Ok(None),
            // The parser yields a key or the end inside an object and nothing
            // else.
            _ => Err(self.shape("a key")),
        }
    }

    /// Reads an array and every element of it with `read`.
    fn list(
        &mut self,
        read: impl FnMut(&mut Self) -> Result<(), ReadError>,
    ) -> Result<(), ReadError> {
        match self.next()? {
            Token::BeginArray => self.elements(read),
            _ => Err(self.shape("an array")),
        }
    }

    /// Reads an array and every element of it with `read`, or `null`, which
    /// holds no element.
    fn list_or_null(
        &mut self,
        read: impl FnMut(&mut Self) -> Result<(), ReadError>,
    ) -> Result<(), ReadError> {
        match self.next()? {
            Token::BeginArray => self.elements(read),
            Token::Null => Ok(()),
            _ => Err(self.shape("an array or null")),
        }
    }

    /// Reads every element of the open array with `read`, each at its index.
    fn elements(
        &mut self,
        mut read: impl FnMut(&mut Self) -> Result<(), ReadError>,
    ) -> Result<(), ReadError> {
        let mut i: u32 = 0;
        loop {
            let token = self.next()?;
            if token == Token::EndArray {
                return Ok(());
            }
            self.peeked = Some(token);
            self.path.push(Seg::Index(i));
            read(self)?;
            self.leave();
            i = i.saturating_add(1);
        }
    }

    /// Reads a string.
    fn string(&mut self) -> Result<Cow<'a, str>, ReadError> {
        match self.next()? {
            Token::Str(text) => Ok(text),
            _ => Err(self.shape("a string")),
        }
    }

    /// Reads a string or `null`.
    fn string_or_null(&mut self) -> Result<Option<Cow<'a, str>>, ReadError> {
        match self.next()? {
            Token::Str(text) => Ok(Some(text)),
            Token::Null => Ok(None),
            _ => Err(self.shape("a string or null")),
        }
    }

    /// Reads `true`, `false` or `null`.
    fn boolean_or_null(&mut self) -> Result<Option<bool>, ReadError> {
        match self.next()? {
            Token::True => Ok(Some(true)),
            Token::False => Ok(Some(false)),
            Token::Null => Ok(None),
            _ => Err(self.shape("a boolean or null")),
        }
    }

    /// Skips one value, however deep it nests.
    fn skip(&mut self) -> Result<(), ReadError> {
        let mut depth: u32 = 0;
        loop {
            match self.next()? {
                Token::BeginObject | Token::BeginArray => depth += 1,
                Token::EndObject | Token::EndArray => depth = depth.saturating_sub(1),
                _ => {}
            }
            if depth == 0 {
                return Ok(());
            }
        }
    }

    /// Counts one more field, argument or enum value against
    /// [`ReadLimits::max_members`].
    fn count_member(&mut self) -> Result<(), ReadError> {
        self.members = self.members.saturating_add(1);
        if self.members > self.limits.max_members {
            return Err(ReadError::Model(ModelError::Limit {
                what: "fields, arguments and values",
                limit: u64::from(self.limits.max_members),
            }));
        }
        Ok(())
    }

    /// Counts one more named type against [`ReadLimits::max_types`].
    fn count_type(&mut self) -> Result<(), ReadError> {
        self.types = self.types.saturating_add(1);
        if self.types > self.limits.max_types {
            return Err(ReadError::Model(ModelError::Limit {
                what: "types",
                limit: u64::from(self.limits.max_types),
            }));
        }
        Ok(())
    }

    /// Refuses a list of names that already holds [`ReadLimits::max_types`]
    /// of them, naming it by `what`.
    fn count_name(&self, len: usize, what: &'static str) -> Result<(), ReadError> {
        if len >= self.limits.max_types as usize {
            return Err(ReadError::Model(ModelError::Limit {
                what,
                limit: u64::from(self.limits.max_types),
            }));
        }
        Ok(())
    }
}

/// Appends the step `key` to the path text `path`.
fn dotted(path: &mut String, key: &str) {
    if !path.is_empty() {
        path.push('.');
    }
    path.push_str(key);
}

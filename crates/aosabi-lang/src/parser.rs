// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the parser, from a source to a document, a value or a type.
 *
 * Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
 * Derived from graphql-js 16.12.0, src/language/directiveLocation.ts, MIT.
 */

//! The parser of the language layer.
//!
//! [`parse`], [`parse_value`], [`parse_const_value`] and [`parse_type`] are
//! the entry points, and [`Parser`] is the class of the reference with every
//! `parseX` method in file order, each keeping its name in snake case. The
//! control flow follows the TypeScript line by line, because the exit number
//! of the milestone is byte equality of error text and locations, and every
//! message is copied from the vendored source. The entry
//! [`parse_value_within`] has no counterpart in the reference: it parses a
//! value as [`parse_value`] does under a bound of the caller on the nesting of
//! its lists and objects, which the reference leaves to the stack of its
//! engine.
//!
//! Four shapes of the reference read differently in Rust.
//!
//! - The reference throws a `GraphQLError`, so every method here returns a
//!   `Result` whose error is that [`GraphQLError`] in a box, and `?` is the
//!   throw. The box is made where the error is made and keeps the error
//!   variant of every `Result` one pointer wide, as it is out of the
//!   [`Lexer`]. The four entry points unbox the error at their one exit, so
//!   a caller of them matches on the [`GraphQLError`] itself.
//! - The reference builds a node as an object literal, evaluates every child
//!   inside the literal first, and only then calls `node`, which reads the
//!   last token of the lexer and sets `loc`. Here the children are parsed into
//!   locals in the same order and [`Parser::node`] is called afterwards, so
//!   the end token of a location is the last token a child consumed, as in the
//!   reference. The literal also fixes the key order of a node in the
//!   reference, the parser's order with `loc` last, which is not the interface
//!   order of `ast.ts`. A rendering of a node that wants the reference bytes
//!   follows the parser's literal order, and the field order of the structs in
//!   [`crate::ast`] is not it.
//! - The `Const` twins of the reference are type aliases of the plain nodes
//!   (see [`crate::ast`]), so `parseConstValueLiteral` returns a
//!   [`ConstValueNode`] that is a [`ValueNode`], and the `isConst` flag is a
//!   `bool` argument as in the reference.
//! - The `lexer` option of `ParseOptions` and the `parseSchemaCoordinate`
//!   entry function need `SchemaCoordinateLexer` from
//!   `schemaCoordinateLexer.ts`, which is not part of this module, so neither
//!   has an item here. [`Parser::parse_schema_coordinate`] is ported and a
//!   caller drives it the way the entry function does, between
//!   `expect_token(TokenKind::Sof)` and `expect_token(TokenKind::Eof)`.
//!
//! Every position is a UTF-16 code unit index, see the crate documentation.

use alloc::{
    boxed::Box,
    format,
    string::String,
    sync::Arc,
    vec::Vec, //
};

use crate::{
    SharedSlice,
    ast::{
        ArgumentCoordinateNode,
        ArgumentNode,
        BooleanValueNode,
        ConstArgumentNode,
        ConstDirectiveNode,
        ConstValueNode,
        DefinitionNode,
        DirectiveArgumentCoordinateNode,
        DirectiveCoordinateNode,
        DirectiveDefinitionNode,
        DirectiveNode,
        DocumentNode,
        EnumTypeDefinitionNode,
        EnumTypeExtensionNode,
        EnumValueDefinitionNode,
        EnumValueNode,
        FieldDefinitionNode,
        FieldNode,
        FloatValueNode,
        FragmentDefinitionNode,
        FragmentSpreadNode,
        InlineFragmentNode,
        InputObjectTypeDefinitionNode,
        InputObjectTypeExtensionNode,
        InputValueDefinitionNode,
        IntValueNode,
        InterfaceTypeDefinitionNode,
        InterfaceTypeExtensionNode,
        ListTypeNode,
        ListValueNode,
        Location,
        MemberCoordinateNode,
        NameNode,
        NamedTypeNode,
        NonNullTypeNode,
        NullValueNode,
        NullableTypeNode,
        ObjectFieldNode,
        ObjectTypeDefinitionNode,
        ObjectTypeExtensionNode,
        ObjectValueNode,
        OperationDefinitionNode,
        OperationTypeDefinitionNode,
        OperationTypeNode,
        ScalarTypeDefinitionNode,
        ScalarTypeExtensionNode,
        SchemaCoordinateNode,
        SchemaDefinitionNode,
        SchemaExtensionNode,
        SelectionNode,
        SelectionSetNode,
        StringValueNode,
        Token,
        TokenId,
        TokenValue,
        TypeCoordinateNode,
        TypeNode,
        TypeSystemExtensionNode,
        UnionTypeDefinitionNode,
        UnionTypeExtensionNode,
        ValueNode,
        VariableDefinitionNode,
        VariableNode, //
    },
    error::{
        GraphQLError,
        GraphQLErrorOptions,
        syntax_error, //
    },
    js_string::number_to_string,
    kinds::TokenKind,
    lexer::{
        Lexer,
        get_token_desc,
        get_token_kind_desc,
        token_capacity, //
    },
    source::{
        Source,
        widen, //
    }, //
};

/// Configuration options to control parser behavior.
///
/// Every flag the reference leaves `undefined` is `false` or `None` here,
/// which is what the reference tests each one against, `noLocation !== true`,
/// `allowLegacyFragmentVariables === true` and `maxTokens !== undefined`. The
/// budget is a double, which has no total equality, so the type is `PartialEq`
/// and not `Eq`.
///
/// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct ParseOptions {
    /// By default, the parser creates AST nodes that know the location
    /// in the source that they correspond to. This configuration flag
    /// disables that behavior for performance or testing.
    pub no_location: bool,

    /// Parser CPU and memory usage is linear to the number of tokens in a
    /// document however in extreme cases it becomes quadratic due to memory
    /// exhaustion. Parsing happens before validation so even invalid queries
    /// can burn lots of CPU time and memory. To prevent this you can set a
    /// maximum number of tokens allowed within a document.
    ///
    /// The reference types the budget as a `number` and admits every one, a
    /// fraction, a negative and an infinity included. The value the caller
    /// passes is compared with the count of tokens as a number and is printed
    /// into the message of the abort as it is, so it stays the double it is in
    /// the reference.
    pub max_tokens: Option<f64>,

    /// If enabled, the parser will understand and parse variable definitions
    /// contained in a fragment definition. They'll be represented in the
    /// `variable_definitions` field of the [`FragmentDefinitionNode`].
    ///
    /// The syntax is identical to normal, query-defined variables. For
    /// example:
    ///
    /// ```graphql
    /// fragment A($var: Boolean = false) on T {
    ///   ...
    /// }
    /// ```
    ///
    /// The reference marks the option deprecated and removes it in v17.
    pub allow_legacy_fragment_variables: bool,
}

/// Given a GraphQL source, parses it into a Document.
///
/// `source` is the document text or a [`Source`], which is the `isSource`
/// branch of the reference constructor, and `options` the [`ParseOptions`].
/// The returned document carries the token count of the parse in
/// `token_count`, which the reference sets as a non enumerable property.
///
/// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
///
/// # Errors
///
/// Returns the [`GraphQLError`] of the first syntax error the lexer or the
/// parser encounters.
// The error leaves the crate here as the type every caller outside it matches
// on, so it is unboxed, and its width is paid once per call. The lint depends
// on the target and on the features, since `GraphQLError` reaches its
// threshold of 128 bytes on a 64-bit target and on wasm32 with the feature std
// of aosabi-json, as in a Worker, and on no 32-bit target without that
// feature.
#[allow(clippy::result_large_err)]
pub fn parse(
    source: impl Into<Source>,
    options: ParseOptions,
) -> Result<DocumentNode, GraphQLError> {
    let mut parser = Parser::new(source, options);
    let mut document = parser.parse_document().map_err(|error| *error)?;
    document.token_count = Some(parser.token_count());
    Ok(document)
}

/// Given a string containing a GraphQL value (ex. `[42]`), parse the AST for
/// that value.
///
/// This is useful within tools that operate upon GraphQL Values directly and
/// in isolation of complete GraphQL documents.
///
/// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
///
/// # Errors
///
/// Returns the [`GraphQLError`] of the first syntax error the lexer or the
/// parser encounters.
// The error leaves the crate here as the type every caller outside it matches
// on, so it is unboxed, and its width is paid once per call. The lint depends
// on the target and on the features, since `GraphQLError` reaches its
// threshold of 128 bytes on a 64-bit target and on wasm32 with the feature std
// of aosabi-json, as in a Worker, and on no 32-bit target without that
// feature.
#[allow(clippy::result_large_err)]
pub fn parse_value(
    source: impl Into<Source>,
    options: ParseOptions,
) -> Result<ValueNode, GraphQLError> {
    let mut parser = Parser::new(source, options);
    parser
        .enclosed(|parser| parser.parse_value_literal(false))
        .map_err(|error| *error)
}

/// The list and object values a parse of [`parse_value_within`] holds open.
///
/// The parser counts a value when it opens it, before it reads the token
/// after its bracket or its brace, and stops counting it when it closes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Nesting {
    /// The list values open where the parser stands.
    pub lists: u32,
    /// The object values open where the parser stands.
    pub objects: u32,
    /// Whether the parse has opened an object value, a closed one among them.
    pub object_opened: bool,
}

/// The error of [`parse_value_within`].
///
/// The syntax error stays in the box the parser made it in, which keeps the
/// error one pointer wide, as the error of every method of [`Parser`] is.
#[derive(Debug)]
pub enum ParseValueError {
    /// The first syntax error the lexer or the parser encounters, the error
    /// of [`parse_value`].
    Syntax(Box<GraphQLError>),
    /// The bound of the caller refuses the nesting at the value the parser
    /// opened last.
    Nesting,
}

/// Parses a value as [`parse_value`] does, under a bound of the caller on
/// the nesting of its list and object values.
///
/// The parser asks `ends` each time it opens a list or an object value, with
/// the values open around it, and stops at the first value `ends` refuses. It
/// reads the tokens of the text in order and opens a value before it reads the
/// token after its bracket or its brace, so the first syntax error of the text
/// before that value is returned in place of the refusal, and no token after
/// it is read. The parser of the reference reads a value by recursion and has
/// no such bound, so a value deep enough ends the stack of its engine at the
/// value it opens there, and a caller that knows where states it through
/// `ends`.
///
/// # Errors
///
/// Returns [`ParseValueError::Syntax`] with the first syntax error the lexer
/// or the parser encounters, and [`ParseValueError::Nesting`] where `ends`
/// refuses a value before it.
pub fn parse_value_within(
    source: impl Into<Source>,
    options: ParseOptions,
    ends: fn(&Nesting) -> bool,
) -> Result<ValueNode, ParseValueError> {
    let mut parser = Parser::new(source, options);
    parser.bound = Some(Bound {
        ends,
        open: Nesting::default(),
        ended: false,
    });
    parser
        .enclosed(|parser| parser.parse_value_literal(false))
        .map_err(|error| {
            if parser.bound.as_ref().is_some_and(|bound| bound.ended) {
                ParseValueError::Nesting
            } else {
                ParseValueError::Syntax(error)
            }
        })
}

/// Similar to [`parse_value`], but raises a parse error if it encounters a
/// variable.
///
/// The return type will be a constant value.
///
/// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
///
/// # Errors
///
/// Returns the [`GraphQLError`] of the first syntax error the lexer or the
/// parser encounters, and a variable is one.
// The error leaves the crate here as the type every caller outside it matches
// on, so it is unboxed, and its width is paid once per call. The lint depends
// on the target and on the features, since `GraphQLError` reaches its
// threshold of 128 bytes on a 64-bit target and on wasm32 with the feature std
// of aosabi-json, as in a Worker, and on no 32-bit target without that
// feature.
#[allow(clippy::result_large_err)]
pub fn parse_const_value(
    source: impl Into<Source>,
    options: ParseOptions,
) -> Result<ConstValueNode, GraphQLError> {
    let mut parser = Parser::new(source, options);
    parser
        .enclosed(Parser::parse_const_value_literal)
        .map_err(|error| *error)
}

/// Given a string containing a GraphQL Type (ex. `[Int!]`), parse the AST for
/// that type.
///
/// This is useful within tools that operate upon GraphQL Types directly and in
/// isolation of complete GraphQL documents.
///
/// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
///
/// # Errors
///
/// Returns the [`GraphQLError`] of the first syntax error the lexer or the
/// parser encounters.
// The error leaves the crate here as the type every caller outside it matches
// on, so it is unboxed, and its width is paid once per call. The lint depends
// on the target and on the features, since `GraphQLError` reaches its
// threshold of 128 bytes on a 64-bit target and on wasm32 with the feature std
// of aosabi-json, as in a Worker, and on no 32-bit target without that
// feature.
#[allow(clippy::result_large_err)]
pub fn parse_type(
    source: impl Into<Source>,
    options: ParseOptions,
) -> Result<TypeNode, GraphQLError> {
    let mut parser = Parser::new(source, options);
    parser
        .enclosed(Parser::parse_type_reference)
        .map_err(|error| *error)
}

/// The recursive descent parser over a [`Lexer`].
///
/// The reference exports the class only to assist people in implementing their
/// own parsers without duplicating too much code, and keeps it part of the
/// internal API. Every method is public as in the reference, the first three
/// fields are the `_options`, `_lexer` and `_tokenCounter` members, the fourth
/// holds the bound of [`parse_value_within`], which no other entry sets, and
/// the fifth the selections of the selection sets the parser holds open.
///
/// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
pub struct Parser {
    /// The options of the parse.
    options: ParseOptions,
    /// The token stream over the source.
    lexer: Lexer,
    /// The number of tokens consumed so far, without the end of file token.
    token_counter: usize,
    /// The bound on the nesting of a value and the values it holds open.
    bound: Option<Bound>,
    /// The selections of the selection sets the parser holds open, the
    /// innermost last, each set moving its own into one shared slice when it
    /// closes, so the vector serves every set of the parse.
    selections: Vec<SelectionNode>,
}

/// The bound of [`parse_value_within`] over one parse.
struct Bound {
    /// Whether the nesting passes the bound of the caller.
    ends: fn(&Nesting) -> bool,
    /// The values open where the parser stands.
    open: Nesting,
    /// Whether `ends` refused a value, which stops the parse.
    ended: bool,
}

impl Parser {
    /// Creates a parser over `source` with `options`.
    ///
    /// `source` is the document text or a [`Source`], and a text becomes a
    /// source with the default name, which is the `isSource` branch of the
    /// reference constructor. The arena of the lexer reserves room for the
    /// tokens of the body, and no more than the token budget of `options` lets
    /// the parse reach.
    #[must_use]
    pub fn new(source: impl Into<Source>, options: ParseOptions) -> Self {
        let source_obj = Arc::new(source.into());
        let capacity = token_capacity(source_obj.bytes().len());
        let capacity =
            budget_capacity(options.max_tokens).map_or(capacity, |most| capacity.min(most));

        Self {
            options,
            lexer: Lexer::with_capacity(source_obj, capacity),
            token_counter: 0,
            bound: None,
            selections: Vec::new(),
        }
    }

    /// Returns the number of tokens consumed so far.
    ///
    /// The `tokenCount` getter of the reference.
    #[must_use]
    pub fn token_count(&self) -> usize {
        self.token_counter
    }

    /// Converts a name lex token into a name parse node.
    ///
    /// A name with a location keeps the span of its token in the body that
    /// location holds, and a name without one copies its text, since it holds
    /// no handle of the source to read the span through.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that is not a name.
    pub fn parse_name(&mut self) -> Result<NameNode, Box<GraphQLError>> {
        let token = self.expect_token(TokenKind::Name)?;
        let loc = self.node(token);
        Ok(match (loc, &self.token(token).value) {
            (Some(loc), TokenValue::Span { start, end }) => NameNode::spanned(loc, *start, *end),
            (loc, _) => NameNode::new(loc, self.value_str(token)),
        })
    }

    // Implements the parsing rules in the Document section.

    /// Parses `Document : Definition+`.
    ///
    /// The token count of the document stays `None` here, [`parse`] fills it.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_document(&mut self) -> Result<DocumentNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let definitions = self.many(TokenKind::Sof, Self::parse_definition, TokenKind::Eof)?;
        Ok(DocumentNode {
            loc: self.node(start),
            definitions,
            token_count: None,
        })
    }

    /// Parses a definition.
    ///
    /// ```text
    /// Definition :
    ///   - ExecutableDefinition
    ///   - TypeSystemDefinition
    ///   - TypeSystemExtension
    ///
    /// ExecutableDefinition :
    ///   - OperationDefinition
    ///   - FragmentDefinition
    ///
    /// TypeSystemDefinition :
    ///   - SchemaDefinition
    ///   - TypeDefinition
    ///   - DirectiveDefinition
    ///
    /// TypeDefinition :
    ///   - ScalarTypeDefinition
    ///   - ObjectTypeDefinition
    ///   - InterfaceTypeDefinition
    ///   - UnionTypeDefinition
    ///   - EnumTypeDefinition
    ///   - InputObjectTypeDefinition
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a description in front of a
    /// shorthand query or of an extension, or of a keyword that starts no
    /// definition.
    pub fn parse_definition(&mut self) -> Result<DefinitionNode, Box<GraphQLError>> {
        if self.peek(TokenKind::BraceL) {
            return Ok(self.parse_operation_definition()?.into());
        }

        // Many definitions begin with a description and require a lookahead.
        let has_description = self.peek_description();
        let keyword_token = if has_description {
            self.lexer.lookahead()?
        } else {
            self.lexer.token
        };

        if has_description && self.token(keyword_token).kind == TokenKind::BraceL {
            return Err(Box::new(syntax_error(
                &self.lexer.source,
                widen(self.token(self.lexer.token).start),
                "Unexpected description, descriptions are not supported on shorthand queries.",
            )));
        }
        if self.token(keyword_token).kind == TokenKind::Name {
            if let Some(definition) = self.parse_type_system_definition(keyword_token)? {
                return Ok(definition);
            }
            // The `switch` of the reference compares the value with strict
            // equality, which on two strings is a code unit comparison, and a
            // `str` comparison reads the same.
            let value = self.value_str(keyword_token);
            match value {
                "query" | "mutation" | "subscription" => {
                    return Ok(self.parse_operation_definition()?.into());
                }
                "fragment" => return Ok(self.parse_fragment_definition()?.into()),
                _ => {}
            }

            if has_description {
                return Err(Box::new(syntax_error(
                    &self.lexer.source,
                    widen(self.token(self.lexer.token).start),
                    "Unexpected description, only GraphQL definitions support descriptions.",
                )));
            }

            if value == "extend" {
                return Ok(self.parse_type_system_extension()?.into());
            }
        }

        Err(self.unexpected(Some(keyword_token)))
    }

    /// Parses the type system definition the keyword token `keyword` opens,
    /// or answers `None` where the keyword opens none.
    ///
    /// It stands out of line, so a host that parses the executable documents
    /// of requests alone compiles none of the type system parsers.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the definition the keyword opens.
    #[inline(never)]
    fn parse_type_system_definition(
        &mut self,
        keyword: TokenId,
    ) -> Result<Option<DefinitionNode>, Box<GraphQLError>> {
        Ok(Some(match self.value_str(keyword) {
            "schema" => self.parse_schema_definition()?.into(),
            "scalar" => self.parse_scalar_type_definition()?.into(),
            "type" => self.parse_object_type_definition()?.into(),
            "interface" => self.parse_interface_type_definition()?.into(),
            "union" => self.parse_union_type_definition()?.into(),
            "enum" => self.parse_enum_type_definition()?.into(),
            "input" => self.parse_input_object_type_definition()?.into(),
            "directive" => self.parse_directive_definition()?.into(),
            _ => return Ok(None),
        }))
    }

    // Implements the parsing rules in the Operations section.

    /// Parses an operation definition.
    ///
    /// ```text
    /// OperationDefinition :
    ///  - SelectionSet
    ///  - OperationType Name? VariableDefinitions? Directives? SelectionSet
    /// ```
    ///
    /// The shorthand form carries empty lists and no description and no name,
    /// as the reference literal does.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_operation_definition(
        &mut self,
    ) -> Result<OperationDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        if self.peek(TokenKind::BraceL) {
            let selection_set = self.parse_selection_set()?;
            return Ok(OperationDefinitionNode {
                description: None,
                loc: self.node(start),
                operation: OperationTypeNode::Query,
                name: None,
                variable_definitions: Some(Vec::new()),
                directives: Some(Vec::new()),
                selection_set,
            });
        }
        let description = self.parse_description()?;
        let operation = self.parse_operation_type()?;
        let mut name = None;
        if self.peek(TokenKind::Name) {
            name = Some(self.parse_name()?);
        }
        let variable_definitions = self.parse_variable_definitions()?;
        let directives = self.parse_directives(false)?;
        let selection_set = self.parse_selection_set()?;
        Ok(OperationDefinitionNode {
            description,
            loc: self.node(start),
            operation,
            name,
            variable_definitions: Some(variable_definitions),
            directives: Some(directives),
            selection_set,
        })
    }

    /// Parses `OperationType : one of query mutation subscription`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that is not a name,
    /// or of a name that is none of the three operation types.
    pub fn parse_operation_type(&mut self) -> Result<OperationTypeNode, Box<GraphQLError>> {
        let operation_token = self.expect_token(TokenKind::Name)?;
        match self.value_str(operation_token) {
            "query" => return Ok(OperationTypeNode::Query),
            "mutation" => return Ok(OperationTypeNode::Mutation),
            "subscription" => return Ok(OperationTypeNode::Subscription),
            _ => {}
        }

        Err(self.unexpected(Some(operation_token)))
    }

    /// Parses `VariableDefinitions : ( VariableDefinition+ )`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_variable_definitions(
        &mut self,
    ) -> Result<Vec<VariableDefinitionNode>, Box<GraphQLError>> {
        self.optional_many(
            TokenKind::ParenL,
            Self::parse_variable_definition,
            TokenKind::ParenR,
        )
    }

    /// Parses `VariableDefinition : Variable : Type DefaultValue? Directives[Const]?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_variable_definition(
        &mut self,
    ) -> Result<VariableDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        let variable = self.parse_variable()?;
        self.expect_token(TokenKind::Colon)?;
        let r#type = self.parse_type_reference()?;
        let default_value = if self.expect_optional_token(TokenKind::Equals)? {
            Some(self.parse_const_value_literal()?)
        } else {
            None
        };
        let directives = self.parse_const_directives()?;
        Ok(VariableDefinitionNode {
            description,
            loc: self.node(start),
            variable,
            r#type,
            default_value,
            directives: Some(directives),
        })
    }

    /// Parses `Variable : $ Name`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_variable(&mut self) -> Result<VariableNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_token(TokenKind::Dollar)?;
        let name = self.parse_name()?;
        Ok(VariableNode {
            loc: self.node(start),
            name,
        })
    }

    /// Parses `SelectionSet : { Selection+ }`.
    ///
    /// The selections are parsed as `many` parses them, onto the vector of the
    /// open sets, and move from it into the shared slice of the set in one
    /// allocation of their length.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_selection_set(&mut self) -> Result<SelectionSetNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let open = self.selections.len();
        if let Err(error) = self.open_selections() {
            self.selections.truncate(open);
            return Err(error);
        }
        let selections: SharedSlice<SelectionNode> = self.selections.drain(open..).collect();
        Ok(SelectionSetNode {
            loc: self.node(start),
            selections,
        })
    }

    /// Parses the selections of one selection set onto the vector of the open sets.
    fn open_selections(&mut self) -> Result<(), Box<GraphQLError>> {
        self.expect_token(TokenKind::BraceL)?;
        loop {
            let selection = self.parse_selection()?;
            self.selections.push(selection);
            if self.expect_optional_token(TokenKind::BraceR)? {
                return Ok(());
            }
        }
    }

    /// Parses a selection.
    ///
    /// ```text
    /// Selection :
    ///   - Field
    ///   - FragmentSpread
    ///   - InlineFragment
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_selection(&mut self) -> Result<SelectionNode, Box<GraphQLError>> {
        if self.peek(TokenKind::Spread) {
            self.parse_fragment()
        } else {
            Ok(self.parse_field()?.into())
        }
    }

    /// Parses `Field : Alias? Name Arguments? Directives? SelectionSet?` with
    /// `Alias : Name :`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_field(&mut self) -> Result<FieldNode, Box<GraphQLError>> {
        let start = self.lexer.token;

        let name_or_alias = self.parse_name()?;
        let (alias, name) = if self.expect_optional_token(TokenKind::Colon)? {
            (Some(name_or_alias), self.parse_name()?)
        } else {
            (None, name_or_alias)
        };

        let arguments = self.parse_arguments(false)?;
        let directives = self.parse_directives(false)?;
        let selection_set = if self.peek(TokenKind::BraceL) {
            Some(self.parse_selection_set()?)
        } else {
            None
        };
        Ok(FieldNode {
            loc: self.node(start),
            alias,
            name,
            arguments: Some(arguments),
            directives: Some(directives),
            selection_set,
        })
    }

    /// Parses `Arguments[Const] : ( Argument[?Const]+ )`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_arguments(
        &mut self,
        is_const: bool,
    ) -> Result<Vec<ArgumentNode>, Box<GraphQLError>> {
        let item = |parser: &mut Self| {
            if is_const {
                parser.parse_const_argument()
            } else {
                parser.parse_argument(false)
            }
        };
        self.optional_many(TokenKind::ParenL, item, TokenKind::ParenR)
    }

    /// Parses `Argument[Const] : Name : Value[?Const]`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_argument(&mut self, is_const: bool) -> Result<ArgumentNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let name = self.parse_name()?;

        self.expect_token(TokenKind::Colon)?;
        let value = self.parse_value_literal(is_const)?;
        Ok(ArgumentNode {
            loc: self.node(start),
            name,
            value,
        })
    }

    /// Parses a constant argument.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production, and a variable is one.
    pub fn parse_const_argument(&mut self) -> Result<ConstArgumentNode, Box<GraphQLError>> {
        self.parse_argument(true)
    }

    // Implements the parsing rules in the Fragments section.

    /// Parses a fragment spread or an inline fragment.
    ///
    /// Corresponds to both `FragmentSpread` and `InlineFragment` in the spec.
    ///
    /// ```text
    /// FragmentSpread : ... FragmentName Directives?
    ///
    /// InlineFragment : ... TypeCondition? Directives? SelectionSet
    /// ```
    ///
    /// The reference returns the union of the two nodes, which is the
    /// [`SelectionNode`] that holds both.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_fragment(&mut self) -> Result<SelectionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_token(TokenKind::Spread)?;

        let has_type_condition = self.expect_optional_keyword("on")?;
        if !has_type_condition && self.peek(TokenKind::Name) {
            let name = self.parse_fragment_name()?;
            let directives = self.parse_directives(false)?;
            return Ok(FragmentSpreadNode {
                loc: self.node(start),
                name,
                directives: Some(directives),
            }
            .into());
        }
        let type_condition = if has_type_condition {
            Some(self.parse_named_type()?)
        } else {
            None
        };
        let directives = self.parse_directives(false)?;
        let selection_set = self.parse_selection_set()?;
        Ok(InlineFragmentNode {
            loc: self.node(start),
            type_condition,
            directives: Some(directives),
            selection_set,
        }
        .into())
    }

    /// Parses a fragment definition.
    ///
    /// ```text
    /// FragmentDefinition :
    ///   - fragment FragmentName on TypeCondition Directives? SelectionSet
    ///
    /// TypeCondition : NamedType
    /// ```
    ///
    /// Without the legacy option the node carries no `variable_definitions` at
    /// all, which is the absent key of the reference literal.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_fragment_definition(
        &mut self,
    ) -> Result<FragmentDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("fragment")?;
        // Legacy support for defining variables within fragments changes
        // the grammar of FragmentDefinition:
        //   - fragment FragmentName VariableDefinitions? on TypeCondition Directives? SelectionSet
        if self.options.allow_legacy_fragment_variables {
            let name = self.parse_fragment_name()?;
            let variable_definitions = self.parse_variable_definitions()?;
            self.expect_keyword("on")?;
            let type_condition = self.parse_named_type()?;
            let directives = self.parse_directives(false)?;
            let selection_set = self.parse_selection_set()?;
            return Ok(FragmentDefinitionNode {
                description,
                loc: self.node(start),
                name,
                variable_definitions: Some(variable_definitions),
                type_condition,
                directives: Some(directives),
                selection_set,
            });
        }
        let name = self.parse_fragment_name()?;
        self.expect_keyword("on")?;
        let type_condition = self.parse_named_type()?;
        let directives = self.parse_directives(false)?;
        let selection_set = self.parse_selection_set()?;
        Ok(FragmentDefinitionNode {
            description,
            loc: self.node(start),
            name,
            variable_definitions: None,
            type_condition,
            directives: Some(directives),
            selection_set,
        })
    }

    /// Parses a fragment name, which is a `Name` but not `on`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that is not a name,
    /// or of the name `on`.
    pub fn parse_fragment_name(&mut self) -> Result<NameNode, Box<GraphQLError>> {
        if self.value_str(self.lexer.token) == "on" {
            return Err(self.unexpected(None));
        }
        self.parse_name()
    }

    // Implements the parsing rules in the Values section.

    /// Parses a value literal.
    ///
    /// ```text
    /// Value[Const] :
    ///   - [~Const] Variable
    ///   - IntValue
    ///   - FloatValue
    ///   - StringValue
    ///   - BooleanValue
    ///   - NullValue
    ///   - EnumValue
    ///   - ListValue[?Const]
    ///   - ObjectValue[?Const]
    ///
    /// BooleanValue : one of `true` `false`
    ///
    /// NullValue : `null`
    ///
    /// EnumValue : Name but not `true`, `false` or `null`
    /// ```
    ///
    /// The `if (isConst)` inside the `$` case of the reference is the guard of
    /// the first `$` arm here.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that starts no
    /// value, or of a variable where `is_const` admits none.
    pub fn parse_value_literal(&mut self, is_const: bool) -> Result<ValueNode, Box<GraphQLError>> {
        let token = self.lexer.token;
        match self.token(token).kind {
            TokenKind::BracketL => Ok(self.parse_list(is_const)?.into()),
            TokenKind::BraceL => Ok(self.parse_object(is_const)?.into()),
            TokenKind::Int => {
                self.advance_lexer()?;
                Ok(IntValueNode {
                    loc: self.node(token),
                    value: self.value_string(token),
                }
                .into())
            }
            TokenKind::Float => {
                self.advance_lexer()?;
                Ok(FloatValueNode {
                    loc: self.node(token),
                    value: self.value_string(token),
                }
                .into())
            }
            TokenKind::String | TokenKind::BlockString => Ok(self.parse_string_literal()?.into()),
            TokenKind::Name => {
                self.advance_lexer()?;
                match self.value_str(token) {
                    "true" => Ok(BooleanValueNode {
                        loc: self.node(token),
                        value: true,
                    }
                    .into()),
                    "false" => Ok(BooleanValueNode {
                        loc: self.node(token),
                        value: false,
                    }
                    .into()),
                    "null" => Ok(NullValueNode {
                        loc: self.node(token),
                    }
                    .into()),
                    _ => Ok(EnumValueNode {
                        loc: self.node(token),
                        value: self.value_string(token),
                    }
                    .into()),
                }
            }
            TokenKind::Dollar if is_const => {
                self.expect_token(TokenKind::Dollar)?;
                if self.token(self.lexer.token).kind == TokenKind::Name {
                    let var_name = self.value_str(self.lexer.token);
                    return Err(Box::new(syntax_error(
                        &self.lexer.source,
                        widen(self.token(token).start),
                        format!("Unexpected variable \"${var_name}\" in constant value."),
                    )));
                }
                Err(self.unexpected(Some(token)))
            }
            TokenKind::Dollar => Ok(self.parse_variable()?.into()),
            _ => Err(self.unexpected(None)),
        }
    }

    /// Parses a constant value literal.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that starts no
    /// value, or of a variable.
    pub fn parse_const_value_literal(&mut self) -> Result<ConstValueNode, Box<GraphQLError>> {
        self.parse_value_literal(true)
    }

    /// Parses a string or block string token into a string value node.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer.
    pub fn parse_string_literal(&mut self) -> Result<StringValueNode, Box<GraphQLError>> {
        let token = self.lexer.token;
        self.advance_lexer()?;
        Ok(StringValueNode {
            loc: self.node(token),
            value: self.value_string(token),
            block: Some(self.token(token).kind == TokenKind::BlockString),
        })
    }

    /// Parses a list value.
    ///
    /// ```text
    /// ListValue[Const] :
    ///   - [ ]
    ///   - [ Value[?Const]+ ]
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production, and the stop of a bound that refuses the list.
    pub fn parse_list(&mut self, is_const: bool) -> Result<ListValueNode, Box<GraphQLError>> {
        self.open_value(true)?;
        let item = |parser: &mut Self| parser.parse_value_literal(is_const);
        let start = self.lexer.token;
        let values = self.any(TokenKind::BracketL, item, TokenKind::BracketR)?;
        self.close_value(true);
        Ok(ListValueNode {
            loc: self.node(start),
            values,
        })
    }

    /// Parses an object value.
    ///
    /// ```text
    /// ObjectValue[Const] :
    ///   - { }
    ///   - { ObjectField[?Const]+ }
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production, and the stop of a bound that refuses the object.
    pub fn parse_object(&mut self, is_const: bool) -> Result<ObjectValueNode, Box<GraphQLError>> {
        self.open_value(false)?;
        let item = |parser: &mut Self| parser.parse_object_field(is_const);
        let start = self.lexer.token;
        let fields = self.any(TokenKind::BraceL, item, TokenKind::BraceR)?;
        self.close_value(false);
        Ok(ObjectValueNode {
            loc: self.node(start),
            fields,
        })
    }

    /// Parses `ObjectField[Const] : Name : Value[?Const]`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_object_field(
        &mut self,
        is_const: bool,
    ) -> Result<ObjectFieldNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let name = self.parse_name()?;
        self.expect_token(TokenKind::Colon)?;
        let value = self.parse_value_literal(is_const)?;
        Ok(ObjectFieldNode {
            loc: self.node(start),
            name,
            value,
        })
    }

    // Implements the parsing rules in the Directives section.

    /// Parses `Directives[Const] : Directive[?Const]+`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_directives(
        &mut self,
        is_const: bool,
    ) -> Result<Vec<DirectiveNode>, Box<GraphQLError>> {
        let mut directives = Vec::new();
        while self.peek(TokenKind::At) {
            directives.push(self.parse_directive(is_const)?);
        }
        Ok(directives)
    }

    /// Parses constant directives.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production, and a variable is one.
    pub fn parse_const_directives(&mut self) -> Result<Vec<ConstDirectiveNode>, Box<GraphQLError>> {
        self.parse_directives(true)
    }

    /// Parses `Directive[Const] : @ Name Arguments[?Const]?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_directive(&mut self, is_const: bool) -> Result<DirectiveNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_token(TokenKind::At)?;
        let name = self.parse_name()?;
        let arguments = self.parse_arguments(is_const)?;
        Ok(DirectiveNode {
            loc: self.node(start),
            name,
            arguments: Some(arguments),
        })
    }

    // Implements the parsing rules in the Types section.

    /// Parses a type reference.
    ///
    /// ```text
    /// Type :
    ///   - NamedType
    ///   - ListType
    ///   - NonNullType
    /// ```
    ///
    /// The local `type` of the reference is a named or a list type, which is
    /// the [`NullableTypeNode`] a non null type wraps.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_type_reference(&mut self) -> Result<TypeNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let r#type: NullableTypeNode = if self.expect_optional_token(TokenKind::BracketL)? {
            let inner_type = self.parse_type_reference()?;
            self.expect_token(TokenKind::BracketR)?;
            ListTypeNode {
                loc: self.node(start),
                r#type: Box::new(inner_type),
            }
            .into()
        } else {
            self.parse_named_type()?.into()
        };

        if self.expect_optional_token(TokenKind::Bang)? {
            return Ok(NonNullTypeNode {
                loc: self.node(start),
                r#type,
            }
            .into());
        }

        Ok(r#type.into())
    }

    /// Parses `NamedType : Name`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that is not a name.
    pub fn parse_named_type(&mut self) -> Result<NamedTypeNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let name = self.parse_name()?;
        Ok(NamedTypeNode {
            loc: self.node(start),
            name,
        })
    }

    // Implements the parsing rules in the Type Definition section.

    /// Determines if the next token starts a description.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    #[must_use]
    pub fn peek_description(&self) -> bool {
        self.peek(TokenKind::String) || self.peek(TokenKind::BlockString)
    }

    /// Parses `Description : StringValue` when one is next.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer.
    pub fn parse_description(&mut self) -> Result<Option<StringValueNode>, Box<GraphQLError>> {
        if self.peek_description() {
            return Ok(Some(self.parse_string_literal()?));
        }
        Ok(None)
    }

    /// Parses `SchemaDefinition : Description? schema Directives[Const]? { OperationTypeDefinition+ }`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_schema_definition(&mut self) -> Result<SchemaDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("schema")?;
        let directives = self.parse_const_directives()?;
        let operation_types = self.many(
            TokenKind::BraceL,
            Self::parse_operation_type_definition,
            TokenKind::BraceR,
        )?;
        Ok(SchemaDefinitionNode {
            loc: self.node(start),
            description,
            directives: Some(directives),
            operation_types,
        })
    }

    /// Parses `OperationTypeDefinition : OperationType : NamedType`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_operation_type_definition(
        &mut self,
    ) -> Result<OperationTypeDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let operation = self.parse_operation_type()?;
        self.expect_token(TokenKind::Colon)?;
        let r#type = self.parse_named_type()?;
        Ok(OperationTypeDefinitionNode {
            loc: self.node(start),
            operation,
            r#type,
        })
    }

    /// Parses `ScalarTypeDefinition : Description? scalar Name Directives[Const]?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_scalar_type_definition(
        &mut self,
    ) -> Result<ScalarTypeDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("scalar")?;
        let name = self.parse_name()?;
        let directives = self.parse_const_directives()?;
        Ok(ScalarTypeDefinitionNode {
            loc: self.node(start),
            description,
            name,
            directives: Some(directives),
        })
    }

    /// Parses an object type definition.
    ///
    /// ```text
    /// ObjectTypeDefinition :
    ///   Description?
    ///   type Name ImplementsInterfaces? Directives[Const]? FieldsDefinition?
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_object_type_definition(
        &mut self,
    ) -> Result<ObjectTypeDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("type")?;
        let name = self.parse_name()?;
        let interfaces = self.parse_implements_interfaces()?;
        let directives = self.parse_const_directives()?;
        let fields = self.parse_fields_definition()?;
        Ok(ObjectTypeDefinitionNode {
            loc: self.node(start),
            description,
            name,
            interfaces: Some(interfaces),
            directives: Some(directives),
            fields: Some(fields),
        })
    }

    /// Parses the implemented interfaces of a type.
    ///
    /// ```text
    /// ImplementsInterfaces :
    ///   - implements `&`? NamedType
    ///   - ImplementsInterfaces & NamedType
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_implements_interfaces(&mut self) -> Result<Vec<NamedTypeNode>, Box<GraphQLError>> {
        if self.expect_optional_keyword("implements")? {
            self.delimited_many(TokenKind::Amp, Self::parse_named_type)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parses `FieldsDefinition : { FieldDefinition+ }`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_fields_definition(
        &mut self,
    ) -> Result<Vec<FieldDefinitionNode>, Box<GraphQLError>> {
        self.optional_many(
            TokenKind::BraceL,
            Self::parse_field_definition,
            TokenKind::BraceR,
        )
    }

    /// Parses `FieldDefinition : Description? Name ArgumentsDefinition? : Type Directives[Const]?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_field_definition(&mut self) -> Result<FieldDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        let name = self.parse_name()?;
        let args = self.parse_argument_defs()?;
        self.expect_token(TokenKind::Colon)?;
        let r#type = self.parse_type_reference()?;
        let directives = self.parse_const_directives()?;
        Ok(FieldDefinitionNode {
            loc: self.node(start),
            description,
            name,
            arguments: Some(args),
            r#type,
            directives: Some(directives),
        })
    }

    /// Parses `ArgumentsDefinition : ( InputValueDefinition+ )`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_argument_defs(
        &mut self,
    ) -> Result<Vec<InputValueDefinitionNode>, Box<GraphQLError>> {
        self.optional_many(
            TokenKind::ParenL,
            Self::parse_input_value_def,
            TokenKind::ParenR,
        )
    }

    /// Parses `InputValueDefinition : Description? Name : Type DefaultValue? Directives[Const]?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_input_value_def(&mut self) -> Result<InputValueDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        let name = self.parse_name()?;
        self.expect_token(TokenKind::Colon)?;
        let r#type = self.parse_type_reference()?;
        let mut default_value = None;
        if self.expect_optional_token(TokenKind::Equals)? {
            default_value = Some(self.parse_const_value_literal()?);
        }
        let directives = self.parse_const_directives()?;
        Ok(InputValueDefinitionNode {
            loc: self.node(start),
            description,
            name,
            r#type,
            default_value,
            directives: Some(directives),
        })
    }

    /// Parses `InterfaceTypeDefinition : Description? interface Name Directives[Const]? FieldsDefinition?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_interface_type_definition(
        &mut self,
    ) -> Result<InterfaceTypeDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("interface")?;
        let name = self.parse_name()?;
        let interfaces = self.parse_implements_interfaces()?;
        let directives = self.parse_const_directives()?;
        let fields = self.parse_fields_definition()?;
        Ok(InterfaceTypeDefinitionNode {
            loc: self.node(start),
            description,
            name,
            interfaces: Some(interfaces),
            directives: Some(directives),
            fields: Some(fields),
        })
    }

    /// Parses `UnionTypeDefinition : Description? union Name Directives[Const]? UnionMemberTypes?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_union_type_definition(
        &mut self,
    ) -> Result<UnionTypeDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("union")?;
        let name = self.parse_name()?;
        let directives = self.parse_const_directives()?;
        let types = self.parse_union_member_types()?;
        Ok(UnionTypeDefinitionNode {
            loc: self.node(start),
            description,
            name,
            directives: Some(directives),
            types: Some(types),
        })
    }

    /// Parses the member types of a union.
    ///
    /// ```text
    /// UnionMemberTypes :
    ///   - = `|`? NamedType
    ///   - UnionMemberTypes | NamedType
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_union_member_types(&mut self) -> Result<Vec<NamedTypeNode>, Box<GraphQLError>> {
        if self.expect_optional_token(TokenKind::Equals)? {
            self.delimited_many(TokenKind::Pipe, Self::parse_named_type)
        } else {
            Ok(Vec::new())
        }
    }

    /// Parses `EnumTypeDefinition : Description? enum Name Directives[Const]? EnumValuesDefinition?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_enum_type_definition(
        &mut self,
    ) -> Result<EnumTypeDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("enum")?;
        let name = self.parse_name()?;
        let directives = self.parse_const_directives()?;
        let values = self.parse_enum_values_definition()?;
        Ok(EnumTypeDefinitionNode {
            loc: self.node(start),
            description,
            name,
            directives: Some(directives),
            values: Some(values),
        })
    }

    /// Parses `EnumValuesDefinition : { EnumValueDefinition+ }`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_enum_values_definition(
        &mut self,
    ) -> Result<Vec<EnumValueDefinitionNode>, Box<GraphQLError>> {
        self.optional_many(
            TokenKind::BraceL,
            Self::parse_enum_value_definition,
            TokenKind::BraceR,
        )
    }

    /// Parses `EnumValueDefinition : Description? EnumValue Directives[Const]?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_enum_value_definition(
        &mut self,
    ) -> Result<EnumValueDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        let name = self.parse_enum_value_name()?;
        let directives = self.parse_const_directives()?;
        Ok(EnumValueDefinitionNode {
            loc: self.node(start),
            description,
            name,
            directives: Some(directives),
        })
    }

    /// Parses an enum value name, a `Name` but not `true`, `false` or `null`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that is not a name,
    /// or of one of the three reserved names.
    pub fn parse_enum_value_name(&mut self) -> Result<NameNode, Box<GraphQLError>> {
        let token = self.lexer.token;
        if matches!(self.value_str(token), "true" | "false" | "null") {
            return Err(Box::new(syntax_error(
                &self.lexer.source,
                widen(self.token(token).start),
                format!(
                    "{} is reserved and cannot be used for an enum value.",
                    get_token_desc(self.token(token), &self.lexer.source)
                ),
            )));
        }
        self.parse_name()
    }

    /// Parses `InputObjectTypeDefinition : Description? input Name Directives[Const]? InputFieldsDefinition?`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_input_object_type_definition(
        &mut self,
    ) -> Result<InputObjectTypeDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("input")?;
        let name = self.parse_name()?;
        let directives = self.parse_const_directives()?;
        let fields = self.parse_input_fields_definition()?;
        Ok(InputObjectTypeDefinitionNode {
            loc: self.node(start),
            description,
            name,
            directives: Some(directives),
            fields: Some(fields),
        })
    }

    /// Parses `InputFieldsDefinition : { InputValueDefinition+ }`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_input_fields_definition(
        &mut self,
    ) -> Result<Vec<InputValueDefinitionNode>, Box<GraphQLError>> {
        self.optional_many(
            TokenKind::BraceL,
            Self::parse_input_value_def,
            TokenKind::BraceR,
        )
    }

    /// Parses a type system extension.
    ///
    /// ```text
    /// TypeSystemExtension :
    ///   - SchemaExtension
    ///   - TypeExtension
    ///
    /// TypeExtension :
    ///   - ScalarTypeExtension
    ///   - ObjectTypeExtension
    ///   - InterfaceTypeExtension
    ///   - UnionTypeExtension
    ///   - EnumTypeExtension
    ///   - InputObjectTypeDefinition
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, or of the token after `extend`
    /// when it is no extension keyword.
    // Out of line, so a host that parses only requests compiles no parser of an extension.
    #[inline(never)]
    pub fn parse_type_system_extension(
        &mut self,
    ) -> Result<TypeSystemExtensionNode, Box<GraphQLError>> {
        let keyword_token = self.lexer.lookahead()?;

        if self.token(keyword_token).kind == TokenKind::Name {
            match self.value_str(keyword_token) {
                "schema" => return Ok(self.parse_schema_extension()?.into()),
                "scalar" => return Ok(self.parse_scalar_type_extension()?.into()),
                "type" => return Ok(self.parse_object_type_extension()?.into()),
                "interface" => return Ok(self.parse_interface_type_extension()?.into()),
                "union" => return Ok(self.parse_union_type_extension()?.into()),
                "enum" => return Ok(self.parse_enum_type_extension()?.into()),
                "input" => return Ok(self.parse_input_object_type_extension()?.into()),
                _ => {}
            }
        }

        Err(self.unexpected(Some(keyword_token)))
    }

    /// Parses a schema extension.
    ///
    /// ```text
    /// SchemaExtension :
    ///  - extend schema Directives[Const]? { OperationTypeDefinition+ }
    ///  - extend schema Directives[Const]
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that does not fit
    /// the production, or of an extension that extends nothing.
    pub fn parse_schema_extension(&mut self) -> Result<SchemaExtensionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_keyword("extend")?;
        self.expect_keyword("schema")?;
        let directives = self.parse_const_directives()?;
        let operation_types = self.optional_many(
            TokenKind::BraceL,
            Self::parse_operation_type_definition,
            TokenKind::BraceR,
        )?;
        if directives.is_empty() && operation_types.is_empty() {
            return Err(self.unexpected(None));
        }
        Ok(SchemaExtensionNode {
            loc: self.node(start),
            directives: Some(directives),
            operation_types: Some(operation_types),
        })
    }

    /// Parses `ScalarTypeExtension : extend scalar Name Directives[Const]`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that does not fit
    /// the production, or of an extension that extends nothing.
    pub fn parse_scalar_type_extension(
        &mut self,
    ) -> Result<ScalarTypeExtensionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_keyword("extend")?;
        self.expect_keyword("scalar")?;
        let name = self.parse_name()?;
        let directives = self.parse_const_directives()?;
        if directives.is_empty() {
            return Err(self.unexpected(None));
        }
        Ok(ScalarTypeExtensionNode {
            loc: self.node(start),
            name,
            directives: Some(directives),
        })
    }

    /// Parses an object type extension.
    ///
    /// ```text
    /// ObjectTypeExtension :
    ///  - extend type Name ImplementsInterfaces? Directives[Const]? FieldsDefinition
    ///  - extend type Name ImplementsInterfaces? Directives[Const]
    ///  - extend type Name ImplementsInterfaces
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that does not fit
    /// the production, or of an extension that extends nothing.
    pub fn parse_object_type_extension(
        &mut self,
    ) -> Result<ObjectTypeExtensionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_keyword("extend")?;
        self.expect_keyword("type")?;
        let name = self.parse_name()?;
        let interfaces = self.parse_implements_interfaces()?;
        let directives = self.parse_const_directives()?;
        let fields = self.parse_fields_definition()?;
        if interfaces.is_empty() && directives.is_empty() && fields.is_empty() {
            return Err(self.unexpected(None));
        }
        Ok(ObjectTypeExtensionNode {
            loc: self.node(start),
            name,
            interfaces: Some(interfaces),
            directives: Some(directives),
            fields: Some(fields),
        })
    }

    /// Parses an interface type extension.
    ///
    /// ```text
    /// InterfaceTypeExtension :
    ///  - extend interface Name ImplementsInterfaces? Directives[Const]? FieldsDefinition
    ///  - extend interface Name ImplementsInterfaces? Directives[Const]
    ///  - extend interface Name ImplementsInterfaces
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that does not fit
    /// the production, or of an extension that extends nothing.
    pub fn parse_interface_type_extension(
        &mut self,
    ) -> Result<InterfaceTypeExtensionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_keyword("extend")?;
        self.expect_keyword("interface")?;
        let name = self.parse_name()?;
        let interfaces = self.parse_implements_interfaces()?;
        let directives = self.parse_const_directives()?;
        let fields = self.parse_fields_definition()?;
        if interfaces.is_empty() && directives.is_empty() && fields.is_empty() {
            return Err(self.unexpected(None));
        }
        Ok(InterfaceTypeExtensionNode {
            loc: self.node(start),
            name,
            interfaces: Some(interfaces),
            directives: Some(directives),
            fields: Some(fields),
        })
    }

    /// Parses a union type extension.
    ///
    /// ```text
    /// UnionTypeExtension :
    ///   - extend union Name Directives[Const]? UnionMemberTypes
    ///   - extend union Name Directives[Const]
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that does not fit
    /// the production, or of an extension that extends nothing.
    pub fn parse_union_type_extension(
        &mut self,
    ) -> Result<UnionTypeExtensionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_keyword("extend")?;
        self.expect_keyword("union")?;
        let name = self.parse_name()?;
        let directives = self.parse_const_directives()?;
        let types = self.parse_union_member_types()?;
        if directives.is_empty() && types.is_empty() {
            return Err(self.unexpected(None));
        }
        Ok(UnionTypeExtensionNode {
            loc: self.node(start),
            name,
            directives: Some(directives),
            types: Some(types),
        })
    }

    /// Parses an enum type extension.
    ///
    /// ```text
    /// EnumTypeExtension :
    ///   - extend enum Name Directives[Const]? EnumValuesDefinition
    ///   - extend enum Name Directives[Const]
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that does not fit
    /// the production, or of an extension that extends nothing.
    pub fn parse_enum_type_extension(
        &mut self,
    ) -> Result<EnumTypeExtensionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_keyword("extend")?;
        self.expect_keyword("enum")?;
        let name = self.parse_name()?;
        let directives = self.parse_const_directives()?;
        let values = self.parse_enum_values_definition()?;
        if directives.is_empty() && values.is_empty() {
            return Err(self.unexpected(None));
        }
        Ok(EnumTypeExtensionNode {
            loc: self.node(start),
            name,
            directives: Some(directives),
            values: Some(values),
        })
    }

    /// Parses an input object type extension.
    ///
    /// ```text
    /// InputObjectTypeExtension :
    ///   - extend input Name Directives[Const]? InputFieldsDefinition
    ///   - extend input Name Directives[Const]
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that does not fit
    /// the production, or of an extension that extends nothing.
    pub fn parse_input_object_type_extension(
        &mut self,
    ) -> Result<InputObjectTypeExtensionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        self.expect_keyword("extend")?;
        self.expect_keyword("input")?;
        let name = self.parse_name()?;
        let directives = self.parse_const_directives()?;
        let fields = self.parse_input_fields_definition()?;
        if directives.is_empty() && fields.is_empty() {
            return Err(self.unexpected(None));
        }
        Ok(InputObjectTypeExtensionNode {
            loc: self.node(start),
            name,
            directives: Some(directives),
            fields: Some(fields),
        })
    }

    /// Parses a directive definition.
    ///
    /// ```text
    /// DirectiveDefinition :
    ///   - Description? directive @ Name ArgumentsDefinition? `repeatable`? on DirectiveLocations
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_directive_definition(
        &mut self,
    ) -> Result<DirectiveDefinitionNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let description = self.parse_description()?;
        self.expect_keyword("directive")?;
        self.expect_token(TokenKind::At)?;
        let name = self.parse_name()?;
        let args = self.parse_argument_defs()?;
        let repeatable = self.expect_optional_keyword("repeatable")?;
        self.expect_keyword("on")?;
        let locations = self.parse_directive_locations()?;
        Ok(DirectiveDefinitionNode {
            loc: self.node(start),
            description,
            name,
            arguments: Some(args),
            repeatable,
            locations,
        })
    }

    /// Parses the locations of a directive definition.
    ///
    /// ```text
    /// DirectiveLocations :
    ///   - `|`? DirectiveLocation
    ///   - DirectiveLocations | DirectiveLocation
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_directive_locations(&mut self) -> Result<Vec<NameNode>, Box<GraphQLError>> {
        self.delimited_many(TokenKind::Pipe, Self::parse_directive_location)
    }

    /// Parses one directive location.
    ///
    /// ```text
    /// DirectiveLocation :
    ///   - ExecutableDirectiveLocation
    ///   - TypeSystemDirectiveLocation
    ///
    /// ExecutableDirectiveLocation : one of
    ///   `QUERY`
    ///   `MUTATION`
    ///   `SUBSCRIPTION`
    ///   `FIELD`
    ///   `FRAGMENT_DEFINITION`
    ///   `FRAGMENT_SPREAD`
    ///   `INLINE_FRAGMENT`
    ///
    /// TypeSystemDirectiveLocation : one of
    ///   `SCHEMA`
    ///   `SCALAR`
    ///   `OBJECT`
    ///   `FIELD_DEFINITION`
    ///   `ARGUMENT_DEFINITION`
    ///   `INTERFACE`
    ///   `UNION`
    ///   `ENUM`
    ///   `ENUM_VALUE`
    ///   `INPUT_OBJECT`
    ///   `INPUT_FIELD_DEFINITION`
    /// ```
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a token that is not a name,
    /// or of a name that is no directive location.
    pub fn parse_directive_location(&mut self) -> Result<NameNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let name = self.parse_name()?;
        if is_directive_location(name.value()) {
            return Ok(name);
        }
        Err(self.unexpected(Some(start)))
    }

    // Schema Coordinates

    /// Parses a schema coordinate.
    ///
    /// ```text
    /// SchemaCoordinate :
    ///   - Name
    ///   - Name . Name
    ///   - Name . Name ( Name : )
    ///   - @ Name
    ///   - @ Name ( Name : )
    /// ```
    ///
    /// The reference tests `memberName` and `argumentName` for truthiness, and
    /// a node object is true while `undefined` is false, which `is_some`
    /// reads. The `else if` of the reference follows a branch that returns on
    /// every path, so it is a plain `if` here.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of a token that does not fit
    /// the production.
    pub fn parse_schema_coordinate(&mut self) -> Result<SchemaCoordinateNode, Box<GraphQLError>> {
        let start = self.lexer.token;
        let of_directive = self.expect_optional_token(TokenKind::At)?;
        let name = self.parse_name()?;
        let mut member_name: Option<NameNode> = None;
        if !of_directive && self.expect_optional_token(TokenKind::Dot)? {
            member_name = Some(self.parse_name()?);
        }
        let mut argument_name: Option<NameNode> = None;
        if (of_directive || member_name.is_some())
            && self.expect_optional_token(TokenKind::ParenL)?
        {
            argument_name = Some(self.parse_name()?);
            self.expect_token(TokenKind::Colon)?;
            self.expect_token(TokenKind::ParenR)?;
        }

        if of_directive {
            if let Some(argument_name) = argument_name {
                return Ok(DirectiveArgumentCoordinateNode {
                    loc: self.node(start),
                    name,
                    argument_name,
                }
                .into());
            }
            return Ok(DirectiveCoordinateNode {
                loc: self.node(start),
                name,
            }
            .into());
        }
        if let Some(member_name) = member_name {
            if let Some(argument_name) = argument_name {
                return Ok(ArgumentCoordinateNode {
                    loc: self.node(start),
                    name,
                    field_name: member_name,
                    argument_name,
                }
                .into());
            }
            return Ok(MemberCoordinateNode {
                loc: self.node(start),
                name,
                member_name,
            }
            .into());
        }

        Ok(TypeCoordinateNode {
            loc: self.node(start),
            name,
        }
        .into())
    }

    // Core parsing utility functions

    /// Returns the location a node built from `start_token` carries, or
    /// `None` when locations are off.
    ///
    /// The reference sets the `loc` of a finished node from the start token
    /// and the last token of the lexer, unless `noLocation` is exactly `true`.
    /// Here the caller puts the result into the `loc` field of the node it
    /// builds, after it parsed every child, which is when the reference calls
    /// `node`.
    ///
    /// # Panics
    ///
    /// Panics when `start_token` was minted by another arena than the one of
    /// the lexer of this parser.
    #[must_use]
    pub fn node(&self, start_token: TokenId) -> Option<Location> {
        if self.options.no_location {
            return None;
        }
        Some(Location::new(
            start_token,
            self.lexer.last_token,
            &self.lexer.tokens,
            Arc::clone(&self.lexer.source),
        ))
    }

    /// Determines if the next token is of a given kind.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    #[must_use]
    pub fn peek(&self, kind: TokenKind) -> bool {
        self.token(self.lexer.token).kind == kind
    }

    /// If the next token is of the given kind, return that token after
    /// advancing the lexer.
    ///
    /// Otherwise, do not change the parser state and throw an error.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or the `Expected` error of a
    /// token of another kind.
    pub fn expect_token(&mut self, kind: TokenKind) -> Result<TokenId, Box<GraphQLError>> {
        let token = self.lexer.token;
        if self.token(token).kind == kind {
            self.advance_lexer()?;
            return Ok(token);
        }

        Err(Box::new(syntax_error(
            &self.lexer.source,
            widen(self.token(token).start),
            format!(
                "Expected {}, found {}.",
                get_token_kind_desc(kind),
                get_token_desc(self.token(token), &self.lexer.source)
            ),
        )))
    }

    /// If the next token is of the given kind, return `true` after advancing
    /// the lexer.
    ///
    /// Otherwise, do not change the parser state and return `false`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer.
    pub fn expect_optional_token(&mut self, kind: TokenKind) -> Result<bool, Box<GraphQLError>> {
        let token = self.lexer.token;
        if self.token(token).kind == kind {
            self.advance_lexer()?;
            return Ok(true);
        }
        Ok(false)
    }

    /// If the next token is a given keyword, advance the lexer.
    ///
    /// Otherwise, do not change the parser state and throw an error.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or the `Expected` error of a
    /// token that is not the keyword.
    pub fn expect_keyword(&mut self, value: &str) -> Result<(), Box<GraphQLError>> {
        let token = self.lexer.token;
        if self.token(token).kind == TokenKind::Name && self.value_str(token) == value {
            self.advance_lexer()
        } else {
            Err(Box::new(syntax_error(
                &self.lexer.source,
                widen(self.token(token).start),
                format!(
                    "Expected \"{value}\", found {}.",
                    get_token_desc(self.token(token), &self.lexer.source)
                ),
            )))
        }
    }

    /// If the next token is a given keyword, return `true` after advancing
    /// the lexer.
    ///
    /// Otherwise, do not change the parser state and return `false`.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer.
    pub fn expect_optional_keyword(&mut self, value: &str) -> Result<bool, Box<GraphQLError>> {
        let token = self.lexer.token;
        if self.token(token).kind == TokenKind::Name && self.value_str(token) == value {
            self.advance_lexer()?;
            return Ok(true);
        }
        Ok(false)
    }

    /// Helper function for creating an error when an unexpected lexed token
    /// is encountered.
    ///
    /// `at_token` names the token to report, and `None` reports the current
    /// token, which is the `??` of the reference.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Panics
    ///
    /// Panics when `at_token` was minted by another arena than the one of the
    /// lexer of this parser.
    #[must_use]
    pub fn unexpected(&self, at_token: Option<TokenId>) -> Box<GraphQLError> {
        let token = at_token.unwrap_or(self.lexer.token);
        Box::new(syntax_error(
            &self.lexer.source,
            widen(self.token(token).start),
            format!(
                "Unexpected {}.",
                get_token_desc(self.token(token), &self.lexer.source)
            ),
        ))
    }

    /// Returns a possibly empty list of parse nodes, determined by `parse_fn`.
    ///
    /// This list begins with a lex token of `open_kind` and ends with a lex
    /// token of `close_kind`. Advances the parser to the next lex token after
    /// the closing token.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a missing opening token, or
    /// of an item.
    pub fn any<T>(
        &mut self,
        open_kind: TokenKind,
        mut parse_fn: impl FnMut(&mut Self) -> Result<T, Box<GraphQLError>>,
        close_kind: TokenKind,
    ) -> Result<Vec<T>, Box<GraphQLError>> {
        self.expect_token(open_kind)?;
        let mut nodes = Vec::new();
        while !self.expect_optional_token(close_kind)? {
            nodes.push(parse_fn(self)?);
        }
        Ok(nodes)
    }

    /// Returns a list of parse nodes, determined by `parse_fn`.
    ///
    /// It can be empty only if open token is missing otherwise it will always
    /// return non-empty list that begins with a lex token of `open_kind` and
    /// ends with a lex token of `close_kind`. Advances the parser to the next
    /// lex token after the closing token.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of an item.
    pub fn optional_many<T>(
        &mut self,
        open_kind: TokenKind,
        mut parse_fn: impl FnMut(&mut Self) -> Result<T, Box<GraphQLError>>,
        close_kind: TokenKind,
    ) -> Result<Vec<T>, Box<GraphQLError>> {
        if self.expect_optional_token(open_kind)? {
            let mut nodes = Vec::new();
            loop {
                nodes.push(parse_fn(self)?);
                if self.expect_optional_token(close_kind)? {
                    break;
                }
            }
            return Ok(nodes);
        }
        Ok(Vec::new())
    }

    /// Returns a non-empty list of parse nodes, determined by `parse_fn`.
    ///
    /// This list begins with a lex token of `open_kind` and ends with a lex
    /// token of `close_kind`. Advances the parser to the next lex token after
    /// the closing token.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of a missing opening token, or
    /// of an item.
    pub fn many<T>(
        &mut self,
        open_kind: TokenKind,
        mut parse_fn: impl FnMut(&mut Self) -> Result<T, Box<GraphQLError>>,
        close_kind: TokenKind,
    ) -> Result<Vec<T>, Box<GraphQLError>> {
        self.expect_token(open_kind)?;
        let mut nodes = Vec::new();
        loop {
            nodes.push(parse_fn(self)?);
            if self.expect_optional_token(close_kind)? {
                break;
            }
        }
        Ok(nodes)
    }

    /// Returns a non-empty list of parse nodes, determined by `parse_fn`.
    ///
    /// This list may begin with a lex token of `delimiter_kind` followed by
    /// items separated by lex tokens of that kind. Advances the parser to the
    /// next lex token after last item in the list.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer or of an item.
    pub fn delimited_many<T>(
        &mut self,
        delimiter_kind: TokenKind,
        mut parse_fn: impl FnMut(&mut Self) -> Result<T, Box<GraphQLError>>,
    ) -> Result<Vec<T>, Box<GraphQLError>> {
        self.expect_optional_token(delimiter_kind)?;

        let mut nodes = Vec::new();
        loop {
            nodes.push(parse_fn(self)?);
            if !self.expect_optional_token(delimiter_kind)? {
                break;
            }
        }
        Ok(nodes)
    }

    /// Advances the lexer by one token and counts it against `max_tokens`.
    ///
    /// The end of file token is not counted, and the budget is checked after
    /// the count, so a document of exactly `max_tokens` tokens passes.
    ///
    /// Derived from graphql-js 16.12.0, src/language/parser.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, or the `Document contains more
    /// that` error at the first token past the budget.
    pub fn advance_lexer(&mut self) -> Result<(), Box<GraphQLError>> {
        let max_tokens = self.options.max_tokens;
        let token = self.lexer.advance()?;

        if self.token(token).kind != TokenKind::Eof {
            self.token_counter += 1;
            if let Some(max_tokens) = max_tokens
                && js_count(self.token_counter) > max_tokens
            {
                // `${maxTokens}` is `ToString` of the number the caller
                // passed, which is `Number::toString`.
                let max_tokens = number_to_string(max_tokens);
                return Err(Box::new(syntax_error(
                    &self.lexer.source,
                    widen(self.token(token).start),
                    format!("Document contains more that {max_tokens} tokens. Parsing aborted."),
                )));
            }
        }
        Ok(())
    }

    /// Returns the token of the lexer arena that `id` names.
    fn token(&self, id: TokenId) -> &Token {
        &self.lexer.tokens[id]
    }

    /// Counts one more open list value, or object value where `list` is
    /// false, against the bound of [`parse_value_within`], which a parser of
    /// another entry does not hold.
    ///
    /// # Errors
    ///
    /// Returns the error that stops the parse where the bound refuses the
    /// value, which [`parse_value_within`] answers as
    /// [`ParseValueError::Nesting`].
    fn open_value(&mut self, list: bool) -> Result<(), Box<GraphQLError>> {
        let Some(bound) = self.bound.as_mut() else {
            return Ok(());
        };
        if list {
            bound.open.lists = bound.open.lists.saturating_add(1);
        } else {
            bound.open.objects = bound.open.objects.saturating_add(1);
            bound.open.object_opened = true;
        }
        if (bound.ends)(&bound.open) {
            bound.ended = true;
            return Err(Box::new(GraphQLError::new(
                "The nesting of the value passes the bound of the parse.",
                GraphQLErrorOptions::default(),
            )));
        }
        Ok(())
    }

    /// Counts a list value, or an object value where `list` is false, closed
    /// again, the end of what [`Parser::open_value`] counted.
    fn close_value(&mut self, list: bool) {
        if let Some(bound) = self.bound.as_mut() {
            if list {
                bound.open.lists = bound.open.lists.saturating_sub(1);
            } else {
                bound.open.objects = bound.open.objects.saturating_sub(1);
            }
        }
    }

    /// Returns the value of the token `id` names, or an empty string when it
    /// carries none.
    ///
    /// The reference reads `token.value`, which is `undefined` on a
    /// punctuation token, `<SOF>` and `<EOF>`, and a string on every other
    /// kind, set by the `createToken` calls of `readComment`, `readNumber`,
    /// `readString`, `readBlockString` and `readName`. The parser compares a
    /// value with a keyword, quotes the name of a variable, or copies the
    /// value into a node through [`Parser::value_string`] and into a name
    /// without a location through [`NameNode::new`]. No keyword is empty, so
    /// the empty string compares as `undefined` does. The text is read through
    /// [`Token::value_in`], from the span of the token or from the value it
    /// owns, and no string is built.
    fn value_str(&self, id: TokenId) -> &str {
        self.token(id)
            .value_in(&self.lexer.source)
            .unwrap_or_default()
    }

    /// Returns the value of the token `id` names as the string a node keeps.
    ///
    /// The text is read through [`Parser::value_str`] and copied, which is the
    /// one string built for a number, an enum value and a string without an
    /// escape. A value the token owns, a string with its escapes resolved or a
    /// block string, is copied as well, and the token keeps it. A caller that
    /// holds the node can pass its start token to [`Parser::unexpected`], and
    /// the reference describes that token with its value.
    fn value_string(&self, id: TokenId) -> String {
        String::from(self.value_str(id))
    }

    /// Parses what `parse_fn` reads between the start of file and the end of
    /// file token.
    ///
    /// These are the `expectToken(TokenKind.SOF)`, the parse and the
    /// `expectToken(TokenKind.EOF)` that `parseValue`, `parseConstValue` and
    /// `parseType` of the reference share.
    ///
    /// # Errors
    ///
    /// Returns the syntax error of the lexer, of `parse_fn`, or of a token
    /// that is not the one its step expects.
    fn enclosed<T>(
        &mut self,
        parse_fn: impl FnOnce(&mut Self) -> Result<T, Box<GraphQLError>>,
    ) -> Result<T, Box<GraphQLError>> {
        self.expect_token(TokenKind::Sof)?;
        let node = parse_fn(self)?;
        self.expect_token(TokenKind::Eof)?;
        Ok(node)
    }
}

/// Returns the reserve the budget `max_tokens` allows the arena of a parse,
/// or `None` when the budget sets no bound.
///
/// The parser counts every token [`Lexer::advance`] returns other than
/// `<EOF>`, and `advance` returns neither `<SOF>` nor a comment, so neither of
/// them counts. The parser aborts at the first count above the budget. The
/// reserve is the whole part of the budget plus two, room for `<SOF>`, the
/// counted tokens up to the budget and `<EOF>` or the token that aborts. A
/// negative budget aborts at the first token, and a NaN budget never aborts.
/// The reserve is not the most tokens the arena reaches. A comment and a token
/// read ahead, which the parser does not count, grow the arena past it, as a
/// document denser than the ratio grows past the reserve its length gives.
fn budget_capacity(max_tokens: Option<f64>) -> Option<usize> {
    let budget = max_tokens?;
    if budget.is_nan() {
        return None;
    }
    // A cast from a double to an integer drops the fraction and saturates at
    // the bounds of the integer, so a negative budget, which aborts at the
    // first token, gives zero and a budget past usize::MAX gives usize::MAX.
    // NaN, which the cast reads as zero, has returned above.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = budget as usize;
    Some(whole.saturating_add(2))
}

/// Returns the count of tokens as the JavaScript number the reference compares.
///
/// `this._tokenCounter > maxTokens` compares two doubles, and a comparison
/// with NaN is false, so a NaN budget never aborts. A count is bounded by the
/// length of a document, which stays far below 2^53, so the widening to a
/// double is exact.
const fn js_count(count: usize) -> f64 {
    // The cast is exact below 2^53, which a count of tokens never reaches.
    #[allow(clippy::cast_precision_loss)]
    let number = count as f64;
    number
}

/// Determines if `value` is one of the directive locations.
///
/// The reference asks whether the `DirectiveLocation` enum object has an own
/// property of that name, and a string enum compiles to an object whose own
/// keys are exactly its members. A name a prototype supplies, such as
/// `constructor`, is not an own property, so it is no location, which the
/// fixed list reads the same.
fn is_directive_location(value: &str) -> bool {
    matches!(
        value,
        "QUERY"
            | "MUTATION"
            | "SUBSCRIPTION"
            | "FIELD"
            | "FRAGMENT_DEFINITION"
            | "FRAGMENT_SPREAD"
            | "INLINE_FRAGMENT"
            | "VARIABLE_DEFINITION"
            | "SCHEMA"
            | "SCALAR"
            | "OBJECT"
            | "FIELD_DEFINITION"
            | "ARGUMENT_DEFINITION"
            | "INTERFACE"
            | "UNION"
            | "ENUM"
            | "ENUM_VALUE"
            | "INPUT_OBJECT"
            | "INPUT_FIELD_DEFINITION"
    )
}

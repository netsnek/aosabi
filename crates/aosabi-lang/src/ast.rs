// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the abstract syntax tree, the token and the location of a
 * node.
 *
 * Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
 */

//! The abstract syntax tree of the language layer.
//!
//! Every interface of the reference is a struct here, every union is an enum,
//! and each keeps its reference name. A node carries `loc`, its
//! [`Location`], as an `Option` the way the reference marks it optional, and
//! the `kind` discriminant of the reference is the Rust type itself, readable
//! through [`AstNode::kind`] and the `kind` method of each union.
//!
//! Six shapes of the reference read differently in Rust.
//!
//! - A [`Token`] links to its neighbours through `prev` and `next`.
//! - The `value` of a token is a string in the reference. A token here names
//!   the value of a name, a number, a comment and a string without an escape
//!   by its span through a [`TokenValue`]. It owns the value of a string with
//!   an escape and of every block string, which the lexer builds, and
//!   [`Token::value_in`] reads the string of the reference.
//! - The `value` of a [`NameNode`] is a string in the reference. A name the
//!   parser reads with a location names its text by the span of its token in
//!   the body of that location, and [`NameNode::value`] reads the string of
//!   the reference.
//! - The `Const` interfaces, such as `ConstValueNode`, name the same runtime
//!   shape as their non const twins and differ only in what TypeScript admits
//!   as a child, and the parser casts one into the other. Each is a type alias
//!   of its twin here.
//! - `QueryDocumentKeys` and `isNode` serve the visitor, which walks a node
//!   by its property names, and the runtime kind check. A Rust visitor walks
//!   the fields of a struct directly and the type system is the kind check,
//!   so neither has an item here.
//! - A node of the reference is an object every holder shares, so a copy of
//!   a node that keeps a child shares that child. The `selections` of a
//!   [`SelectionSetNode`] is a [`SharedSlice`] here, so a clone of a field,
//!   of a fragment or of an operation copies its head and shares the
//!   selections below it by one increment, and a field node found inside a
//!   clone keeps the address it has inside the document. A site that edits
//!   the selections of a set writes a new slice, or edits the elements of
//!   its own copy through `make_mut`.
//!
//! Every position is a UTF-16 code unit index, see the crate documentation. A
//! token keeps its positions, its line and its column as `u32` and links
//! through ids of `u32`, which fit every body the lexer reads (see
//! [`crate::source`]), and a [`Location`] widens its offsets to `usize`.

use alloc::{
    boxed::Box,
    string::String,
    sync::Arc,
    vec::Vec, //
};
use core::{
    fmt,
    ops::{
        Index,
        IndexMut, //
    }, //
};

use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::{
    SharedSlice,
    kinds::{
        Kind,
        TokenKind, //
    },
    location::js_number,
    source::{
        Source,
        narrow,
        widen, //
    }, //
};

/// The index of a [`Token`] in its [`TokenArena`].
///
/// An id is minted by [`TokenArena::push`] and is only meaningful in the
/// arena that minted it. It is the Rust reading of the `prev` and `next`
/// object references of the reference token. The index is a `u32`, which
/// names every token of a body the lexer reads, so a link takes half the
/// room of a `usize` on a 64-bit host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TokenId(u32);

impl TokenId {
    /// Returns the position of the token in its arena.
    #[must_use]
    pub const fn index(self) -> usize {
        widen(self.0)
    }
}

/// Represents a range of characters represented by a lexical token within a
/// Source.
///
/// `value` is `undefined` for punctuation tokens in the reference and typed as
/// a string there for convenience in the parser. Here it is a [`TokenValue`],
/// which names most values by their span in the body, and [`Token::value_in`]
/// reads the string of the reference from it.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
///
/// The positions, the line and the column are numbers in the reference and
/// `u32` here, which holds each of them for every body the lexer reads (see
/// [`crate::source`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token {
    /// The kind of Token.
    pub kind: TokenKind,
    /// The character offset at which this Node begins.
    pub start: u32,
    /// The character offset at which this Node ends.
    pub end: u32,
    /// The 1-indexed line number on which this Token appears.
    pub line: u32,
    /// The 1-indexed column number at which this Token begins.
    pub column: u32,
    /// For non-punctuation tokens, represents the interpreted value of the token.
    pub value: TokenValue,
    /// The previous token in the double linked list amongst all tokens,
    /// including ignored tokens. `<SOF>` is always the first node.
    pub prev: Option<TokenId>,
    /// The next token in the double linked list amongst all tokens,
    /// including ignored tokens. `<EOF>` is always the last node.
    pub next: Option<TokenId>,
}

impl Token {
    /// Creates a token with no neighbours.
    ///
    /// The arguments are the fields of the same name, and both links start out
    /// empty, which is the reference constructor.
    #[must_use]
    pub const fn new(
        kind: TokenKind,
        start: u32,
        end: u32,
        line: u32,
        column: u32,
        value: TokenValue,
    ) -> Self {
        Self {
            kind,
            start,
            end,
            line,
            column,
            value,
            prev: None,
            next: None,
        }
    }

    /// Returns the value of the token, the `value` property of the reference.
    ///
    /// `source` is the source the lexer read the token from, which holds the
    /// text of a value the token names by its span. A punctuator, `<SOF>` and
    /// `<EOF>` have no value, which is `undefined` in the reference.
    #[must_use]
    pub fn value_in<'a>(&'a self, source: &'a Source) -> Option<&'a str> {
        match &self.value {
            TokenValue::None => None,
            TokenValue::Span { start, end } => Some(source.slice(widen(*start), widen(*end))),
            TokenValue::Owned(value) => Some(value),
        }
    }

    /// Builds the `{kind, value, line, column}` object of the reference `toJSON`.
    ///
    /// `source` is the source the lexer read the token from. A missing value
    /// is inserted as the absent value, which the JSON writer leaves out, so
    /// the key order is that of the reference either way.
    #[must_use]
    pub fn to_json(&self, source: &Source) -> JsObject {
        let mut object = JsObject::with_capacity(4);
        object.insert("kind", self.kind.as_str());
        object.insert(
            "value",
            self.value_in(source)
                .map_or(JsValue::Undefined, JsValue::from),
        );
        object.insert("line", js_number(widen(self.line)));
        object.insert("column", js_number(widen(self.column)));
        object
    }
}

/// Where the value of a [`Token`] is kept.
///
/// The reference gives a name, a number, a string, a block string and a
/// comment a string value and leaves the value of a punctuator, `<SOF>` and
/// `<EOF>` undefined. The value of a name, a number, a comment and a string
/// without an escape is text of the body, so the token names it by its span
/// and no string is built for it. The lexer builds the value of a string with
/// an escape by resolving its escapes, and the value of every block string
/// from its dedented lines, even where that value equals text of the body, so
/// the token owns it. The parser copies it into the node that keeps it, and
/// the token keeps it as well, so a description of the token reads its value
/// as the reference does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenValue {
    /// No value, which is `undefined` in the reference.
    None,
    /// The text of the body between two byte offsets.
    ///
    /// Both offsets stand on a character boundary and the text between them
    /// holds no lone surrogate, because every reader that names a span stops
    /// at a surrogate without its partner. The offsets are `u32` as the
    /// positions of the token are.
    Span {
        /// The byte offset of the first byte of the value.
        start: u32,
        /// The byte offset just past the last byte of the value.
        end: u32,
    },
    /// A value the lexer builds, a string with its escapes resolved or a
    /// block string from its dedented lines.
    Owned(String),
}

/// The tokens of one source, in the order the lexer read them.
///
/// The reference links tokens into a double linked list as the lexer reads
/// ahead, including comments, so that the parser can walk back from any token.
/// The arena owns every token and the links are ids into it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TokenArena {
    /// The tokens, indexed by [`TokenId`].
    tokens: Vec<Token>,
}

impl TokenArena {
    /// Creates an empty arena.
    #[must_use]
    pub const fn new() -> Self {
        Self { tokens: Vec::new() }
    }

    /// Creates an empty arena with room for `capacity` tokens.
    ///
    /// The lexer reserves the arena of a body from its length before the
    /// first token, so a document of the usual density of tokens fills it
    /// without growing it.
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            tokens: Vec::with_capacity(capacity),
        }
    }

    /// Appends `token` and returns its id.
    ///
    /// An id is a `u32`, and the lexer pushes at most two tokens more than
    /// the bytes of a body it reads, which ids of `u32` name one by one (see
    /// [`crate::source`]). Every push after the first `u32::MAX + 1` tokens
    /// answers the id `u32::MAX` again, which only a caller outside the
    /// lexer can reach.
    pub fn push(&mut self, token: Token) -> TokenId {
        let id = TokenId(narrow(self.tokens.len()));
        self.tokens.push(token);
        id
    }

    /// Returns the token with `id`, when the arena holds it.
    #[must_use]
    pub fn get(&self, id: TokenId) -> Option<&Token> {
        self.tokens.get(id.index())
    }

    /// Returns the token with `id` for writing, when the arena holds it.
    pub fn get_mut(&mut self, id: TokenId) -> Option<&mut Token> {
        self.tokens.get_mut(id.index())
    }

    /// Returns the number of tokens.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tokens.len()
    }

    /// Returns whether the arena holds no token.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    /// Iterates over the tokens in reading order.
    pub fn iter(&self) -> impl Iterator<Item = &Token> {
        self.tokens.iter()
    }
}

impl Index<TokenId> for TokenArena {
    type Output = Token;

    /// # Panics
    ///
    /// Panics when `id` was minted by another arena, which is the only way an
    /// id can be out of range.
    fn index(&self, id: TokenId) -> &Token {
        &self.tokens[id.index()]
    }
}

impl IndexMut<TokenId> for TokenArena {
    /// # Panics
    ///
    /// Panics when `id` was minted by another arena, which is the only way an
    /// id can be out of range.
    fn index_mut(&mut self, id: TokenId) -> &mut Token {
        &mut self.tokens[id.index()]
    }
}

impl<'a> IntoIterator for &'a TokenArena {
    type Item = &'a Token;
    type IntoIter = core::slice::Iter<'a, Token>;

    fn into_iter(self) -> Self::IntoIter {
        self.tokens.iter()
    }
}

/// Contains a range of UTF-16 code unit offsets and token references that
/// identify the region of the source from which the AST derived.
///
/// The source is shared through an `Arc` because every node of a document
/// refers to the same one, as every reference location refers to the same
/// `Source` object.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct Location {
    /// The character offset at which this Node begins.
    pub start: usize,
    /// The character offset at which this Node ends.
    pub end: usize,
    /// The Token at which this Node begins.
    pub start_token: TokenId,
    /// The Token at which this Node ends.
    pub end_token: TokenId,
    /// The Source document the AST represents.
    pub source: Arc<Source>,
}

impl Location {
    /// Creates the location spanning `start_token` to `end_token`.
    ///
    /// `tokens` is the arena both ids belong to, and `source` is the document
    /// the tokens were read from. The offsets are the start of the first token
    /// and the end of the last, which is the reference constructor.
    ///
    /// # Panics
    ///
    /// Panics when either id was minted by another arena.
    #[must_use]
    pub fn new(
        start_token: TokenId,
        end_token: TokenId,
        tokens: &TokenArena,
        source: Arc<Source>,
    ) -> Self {
        Self {
            start: widen(tokens[start_token].start),
            end: widen(tokens[end_token].end),
            start_token,
            end_token,
            source,
        }
    }

    /// Builds the `{start, end}` object of the reference `toJSON`.
    ///
    /// The key order is that of the reference.
    #[must_use]
    pub fn to_json(&self) -> JsObject {
        let mut object = JsObject::with_capacity(2);
        object.insert("start", js_number(self.start));
        object.insert("end", js_number(self.end));
        object
    }
}

/// The operation of an operation definition or of an operation type definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OperationTypeNode {
    /// `query`.
    Query,
    /// `mutation`.
    Mutation,
    /// `subscription`.
    Subscription,
}

impl OperationTypeNode {
    /// Returns the string the reference assigns to this operation.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Query => "query",
            Self::Mutation => "mutation",
            Self::Subscription => "subscription",
        }
    }
}

impl fmt::Display for OperationTypeNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Declares a union of nodes as an enum, with its kind, its location and its
/// conversions.
///
/// The variant of a node carries the node's name without the `Node` suffix,
/// which is the name every union uses for that node, so a union converts into
/// any wider union named in `into` by renaming nothing. Each entry names the
/// variant, the node struct and the [`Kind`] the node carries.
macro_rules! node_union {
    (
        $(#[$meta:meta])*
        $name:ident, into [$($wider:ident),*] {
            $($variant:ident($node:ident) => $kind:ident),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq)]
        pub enum $name {
            $(
                #[doc = concat!("A [`", stringify!($node), "`].")]
                $variant($node),
            )+
        }

        impl $name {
            /// Returns the kind of the wrapped node.
            #[must_use]
            pub const fn kind(&self) -> Kind {
                match self {
                    $(Self::$variant(_) => Kind::$kind,)+
                }
            }

            /// Returns the location of the wrapped node, when it carries one.
            #[must_use]
            pub const fn loc(&self) -> Option<&Location> {
                match self {
                    $(Self::$variant(node) => node.loc.as_ref(),)+
                }
            }
        }

        $(
            impl From<$node> for $name {
                fn from(node: $node) -> Self {
                    Self::$variant(node)
                }
            }
        )+

        node_union_into!($name => [$($wider),*] { $($variant),+ });
    };
}

/// Converts a union into each wider union of its list, one impl per step.
///
/// A `macro_rules` repetition cannot pair the variant list with the wider
/// list in one expansion, so this helper peels one wider union at a time and
/// recurses on the rest. The variant names are the same in both unions,
/// which is the rule [`node_union`] states.
macro_rules! node_union_into {
    ($name:ident => [] { $($variant:ident),+ }) => {};
    ($name:ident => [$wider:ident $(, $rest:ident)*] { $($variant:ident),+ }) => {
        impl From<$name> for $wider {
            fn from(node: $name) -> Self {
                match node {
                    $($name::$variant(inner) => $wider::$variant(inner),)+
                }
            }
        }

        node_union_into!($name => [$($rest),*] { $($variant),+ });
    };
}

node_union! {
    /// The list of all possible AST node types.
    ///
    /// Every union below converts into this one, and so does every node.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    AstNode, into [] {
        Name(NameNode) => Name,
        Document(DocumentNode) => Document,
        OperationDefinition(OperationDefinitionNode) => OperationDefinition,
        VariableDefinition(VariableDefinitionNode) => VariableDefinition,
        Variable(VariableNode) => Variable,
        SelectionSet(SelectionSetNode) => SelectionSet,
        Field(FieldNode) => Field,
        Argument(ArgumentNode) => Argument,
        FragmentSpread(FragmentSpreadNode) => FragmentSpread,
        InlineFragment(InlineFragmentNode) => InlineFragment,
        FragmentDefinition(FragmentDefinitionNode) => FragmentDefinition,
        IntValue(IntValueNode) => Int,
        FloatValue(FloatValueNode) => Float,
        StringValue(StringValueNode) => String,
        BooleanValue(BooleanValueNode) => Boolean,
        NullValue(NullValueNode) => Null,
        EnumValue(EnumValueNode) => Enum,
        ListValue(ListValueNode) => List,
        ObjectValue(ObjectValueNode) => Object,
        ObjectField(ObjectFieldNode) => ObjectField,
        Directive(DirectiveNode) => Directive,
        NamedType(NamedTypeNode) => NamedType,
        ListType(ListTypeNode) => ListType,
        NonNullType(NonNullTypeNode) => NonNullType,
        SchemaDefinition(SchemaDefinitionNode) => SchemaDefinition,
        OperationTypeDefinition(OperationTypeDefinitionNode) => OperationTypeDefinition,
        ScalarTypeDefinition(ScalarTypeDefinitionNode) => ScalarTypeDefinition,
        ObjectTypeDefinition(ObjectTypeDefinitionNode) => ObjectTypeDefinition,
        FieldDefinition(FieldDefinitionNode) => FieldDefinition,
        InputValueDefinition(InputValueDefinitionNode) => InputValueDefinition,
        InterfaceTypeDefinition(InterfaceTypeDefinitionNode) => InterfaceTypeDefinition,
        UnionTypeDefinition(UnionTypeDefinitionNode) => UnionTypeDefinition,
        EnumTypeDefinition(EnumTypeDefinitionNode) => EnumTypeDefinition,
        EnumValueDefinition(EnumValueDefinitionNode) => EnumValueDefinition,
        InputObjectTypeDefinition(InputObjectTypeDefinitionNode) => InputObjectTypeDefinition,
        DirectiveDefinition(DirectiveDefinitionNode) => DirectiveDefinition,
        SchemaExtension(SchemaExtensionNode) => SchemaExtension,
        ScalarTypeExtension(ScalarTypeExtensionNode) => ScalarTypeExtension,
        ObjectTypeExtension(ObjectTypeExtensionNode) => ObjectTypeExtension,
        InterfaceTypeExtension(InterfaceTypeExtensionNode) => InterfaceTypeExtension,
        UnionTypeExtension(UnionTypeExtensionNode) => UnionTypeExtension,
        EnumTypeExtension(EnumTypeExtensionNode) => EnumTypeExtension,
        InputObjectTypeExtension(InputObjectTypeExtensionNode) => InputObjectTypeExtension,
        TypeCoordinate(TypeCoordinateNode) => TypeCoordinate,
        MemberCoordinate(MemberCoordinateNode) => MemberCoordinate,
        ArgumentCoordinate(ArgumentCoordinateNode) => ArgumentCoordinate,
        DirectiveCoordinate(DirectiveCoordinateNode) => DirectiveCoordinate,
        DirectiveArgumentCoordinate(DirectiveArgumentCoordinateNode) => DirectiveArgumentCoordinate,
    }
}

/* Name */

/// A name.
///
/// The `value` of the reference is a string. A name the parser reads with a
/// location names its text by the span of its token in the body that location
/// holds, so it owns no string and reads its text through the handle of the
/// source its location already keeps. A name of a parse with `noLocation`, and
/// every name built through [`NameNode::new`], owns its string.
/// [`NameNode::value`] answers the text either way, and [`NameNode::loc`] the
/// location.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
///
/// No field is public, so that a span never stands without the location
/// whose body it indexes. Two names are equal when their locations are equal
/// and their texts are, whichever way each keeps its text, and the `Debug`
/// of a name prints its text as the field `value`, so a name compares and
/// prints as a name that owns its string does.
pub struct NameNode {
    /// The region of the source this node derives from.
    ///
    /// The unions of this module and the [`NodeRef`](crate::visitor::NodeRef)
    /// of the visitor read this field as they read the `loc` of every other
    /// node, so the crate sees it.
    pub(crate) loc: Option<Location>,
    /// The name, a span of the body of `loc` or a string of its own.
    value: NameValue,
}

/// Where the text of a [`NameNode`] is kept.
#[derive(Clone)]
enum NameValue {
    /// The text of the body of the location of the name between two byte
    /// offsets, the span of its token (see [`TokenValue::Span`]).
    Span {
        /// The byte offset of the first byte of the name.
        start: u32,
        /// The byte offset just past the last byte of the name.
        end: u32,
    },
    /// A string the name owns.
    Owned(String),
}

// A name takes the room of a location and a string on a 64-bit host, since
// the two offsets of a span fit in the room of the string.
#[cfg(target_pointer_width = "64")]
const _: () = assert!(size_of::<NameNode>() == 56);

impl NameNode {
    /// Creates a name that owns `value` as its text.
    ///
    /// `loc` is the region of the source the name derives from, `None` for a
    /// name built outside a parse.
    #[must_use]
    pub fn new(loc: Option<Location>, value: impl Into<String>) -> Self {
        Self {
            loc,
            value: NameValue::Owned(value.into()),
        }
    }

    /// Creates a name whose text is the span `start..end` of the body of `loc`.
    ///
    /// `start` and `end` are the byte offsets of the span of a name token
    /// of the source `loc` holds, which stand on character boundaries.
    pub(crate) const fn spanned(loc: Location, start: u32, end: u32) -> Self {
        Self {
            loc: Some(loc),
            value: NameValue::Span { start, end },
        }
    }

    /// Returns the location of the name, when it carries one.
    #[must_use]
    pub const fn loc(&self) -> Option<&Location> {
        self.loc.as_ref()
    }

    /// Returns the text of the name, the `value` property of the reference.
    ///
    /// A name the parser read with a location answers the span of the body
    /// of that location through [`Source::slice`], which checks the two ends
    /// of the span, and any other name answers the string it owns.
    // Every reader of a name in the workspace calls this. Inlined into each
    // call, the match and the slice cost more code than a call does.
    #[must_use]
    #[inline(never)]
    pub fn value(&self) -> &str {
        match &self.value {
            NameValue::Owned(value) => value,
            NameValue::Span { start, end } => self
                .loc
                .as_ref()
                .map_or("", |loc| loc.source.slice(widen(*start), widen(*end))),
        }
    }
}

impl PartialEq for NameNode {
    fn eq(&self, other: &Self) -> bool {
        self.loc == other.loc && self.value() == other.value()
    }
}

impl Clone for NameNode {
    // A clone copies the location and either the span or the string. Inlined
    // into the clone of every node that holds a name, the two paths cost more
    // code than a call does.
    #[inline(never)]
    fn clone(&self) -> Self {
        Self {
            loc: self.loc.clone(),
            value: self.value.clone(),
        }
    }
}

impl fmt::Debug for NameNode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NameNode")
            .field("loc", &self.loc)
            .field("value", &self.value())
            .finish()
    }
}

/* Document */

/// A document, the root of a parse.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct DocumentNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The definitions, in source order.
    pub definitions: Vec<DefinitionNode>,
    /// The number of tokens the parser consumed, when it counted them.
    pub token_count: Option<usize>,
}

node_union! {
    /// A definition of a document.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    DefinitionNode, into [AstNode] {
        OperationDefinition(OperationDefinitionNode) => OperationDefinition,
        FragmentDefinition(FragmentDefinitionNode) => FragmentDefinition,
        SchemaDefinition(SchemaDefinitionNode) => SchemaDefinition,
        ScalarTypeDefinition(ScalarTypeDefinitionNode) => ScalarTypeDefinition,
        ObjectTypeDefinition(ObjectTypeDefinitionNode) => ObjectTypeDefinition,
        InterfaceTypeDefinition(InterfaceTypeDefinitionNode) => InterfaceTypeDefinition,
        UnionTypeDefinition(UnionTypeDefinitionNode) => UnionTypeDefinition,
        EnumTypeDefinition(EnumTypeDefinitionNode) => EnumTypeDefinition,
        InputObjectTypeDefinition(InputObjectTypeDefinitionNode) => InputObjectTypeDefinition,
        DirectiveDefinition(DirectiveDefinitionNode) => DirectiveDefinition,
        SchemaExtension(SchemaExtensionNode) => SchemaExtension,
        ScalarTypeExtension(ScalarTypeExtensionNode) => ScalarTypeExtension,
        ObjectTypeExtension(ObjectTypeExtensionNode) => ObjectTypeExtension,
        InterfaceTypeExtension(InterfaceTypeExtensionNode) => InterfaceTypeExtension,
        UnionTypeExtension(UnionTypeExtensionNode) => UnionTypeExtension,
        EnumTypeExtension(EnumTypeExtensionNode) => EnumTypeExtension,
        InputObjectTypeExtension(InputObjectTypeExtensionNode) => InputObjectTypeExtension,
    }
}

node_union! {
    /// An executable definition, an operation or a fragment.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    ExecutableDefinitionNode, into [DefinitionNode, AstNode] {
        OperationDefinition(OperationDefinitionNode) => OperationDefinition,
        FragmentDefinition(FragmentDefinitionNode) => FragmentDefinition,
    }
}

/// An operation definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationDefinitionNode {
    /// The description, when the operation carries one.
    pub description: Option<StringValueNode>,
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The operation.
    pub operation: OperationTypeNode,
    /// The name, when the operation is named.
    pub name: Option<NameNode>,
    /// The variable definitions.
    pub variable_definitions: Option<Vec<VariableDefinitionNode>>,
    /// The directives.
    pub directives: Option<Vec<DirectiveNode>>,
    /// The selection set.
    pub selection_set: SelectionSetNode,
}

/// A variable definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableDefinitionNode {
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The variable.
    pub variable: VariableNode,
    /// The type.
    pub r#type: TypeNode,
    /// The default value, when the definition carries one.
    pub default_value: Option<ConstValueNode>,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
}

/// A variable.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct VariableNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
}

/// A selection set.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct SelectionSetNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The selections, in source order, which every clone of the node shares.
    pub selections: SharedSlice<SelectionNode>,
}

node_union! {
    /// A selection, a field or a spread.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    SelectionNode, into [AstNode] {
        Field(FieldNode) => Field,
        FragmentSpread(FragmentSpreadNode) => FragmentSpread,
        InlineFragment(InlineFragmentNode) => InlineFragment,
    }
}

/// A field.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The alias, when the field is aliased.
    pub alias: Option<NameNode>,
    /// The name.
    pub name: NameNode,
    /// The arguments.
    pub arguments: Option<Vec<ArgumentNode>>,
    /// The directives.
    pub directives: Option<Vec<DirectiveNode>>,
    /// The selection set, when the field has one.
    pub selection_set: Option<SelectionSetNode>,
}

/// An argument.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgumentNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The value.
    pub value: ValueNode,
}

/// An argument whose value is constant.
pub type ConstArgumentNode = ArgumentNode;

/* Fragments */

/// A fragment spread.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentSpreadNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name of the fragment.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<DirectiveNode>>,
}

/// An inline fragment.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct InlineFragmentNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The type condition, when the fragment carries one.
    pub type_condition: Option<NamedTypeNode>,
    /// The directives.
    pub directives: Option<Vec<DirectiveNode>>,
    /// The selection set.
    pub selection_set: SelectionSetNode,
}

/// A fragment definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct FragmentDefinitionNode {
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The variable definitions, which the reference deprecates and keeps
    /// until v17.
    pub variable_definitions: Option<Vec<VariableDefinitionNode>>,
    /// The type condition.
    pub type_condition: NamedTypeNode,
    /// The directives.
    pub directives: Option<Vec<DirectiveNode>>,
    /// The selection set.
    pub selection_set: SelectionSetNode,
}

/* Values */

node_union! {
    /// A value.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    ValueNode, into [AstNode] {
        Variable(VariableNode) => Variable,
        IntValue(IntValueNode) => Int,
        FloatValue(FloatValueNode) => Float,
        StringValue(StringValueNode) => String,
        BooleanValue(BooleanValueNode) => Boolean,
        NullValue(NullValueNode) => Null,
        EnumValue(EnumValueNode) => Enum,
        ListValue(ListValueNode) => List,
        ObjectValue(ObjectValueNode) => Object,
    }
}

/// A value with no variable in it.
///
/// It is the same type here, and the parser's `isConst` flag is what keeps the
/// variable out.
pub type ConstValueNode = ValueNode;

/// An integer value, kept as the digits of the source.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct IntValueNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The digits as written.
    pub value: String,
}

/// A float value, kept as the digits of the source.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct FloatValueNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The digits as written.
    pub value: String,
}

/// A string value.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct StringValueNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The decoded string.
    pub value: String,
    /// Whether the source wrote a block string.
    pub block: Option<bool>,
}

/// A boolean value.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct BooleanValueNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The value.
    pub value: bool,
}

/// The null value.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct NullValueNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
}

/// An enum value.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumValueNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name of the value.
    pub value: String,
}

/// A list value.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ListValueNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The values, in source order.
    pub values: Vec<ValueNode>,
}

/// A list value with no variable in it.
pub type ConstListValueNode = ListValueNode;

/// An object value.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectValueNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The fields, in source order.
    pub fields: Vec<ObjectFieldNode>,
}

/// An object value with no variable in it.
pub type ConstObjectValueNode = ObjectValueNode;

/// A field of an object value.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectFieldNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The value.
    pub value: ValueNode,
}

/// A field of an object value with no variable in it.
pub type ConstObjectFieldNode = ObjectFieldNode;

/* Directives */

/// A directive.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The arguments.
    pub arguments: Option<Vec<ArgumentNode>>,
}

/// A directive whose arguments are constant.
pub type ConstDirectiveNode = DirectiveNode;

/* Type Reference */

node_union! {
    /// A type reference.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    TypeNode, into [AstNode] {
        NamedType(NamedTypeNode) => NamedType,
        ListType(ListTypeNode) => ListType,
        NonNullType(NonNullTypeNode) => NonNullType,
    }
}

node_union! {
    /// A type reference that a non null wrapper may wrap.
    NullableTypeNode, into [TypeNode, AstNode] {
        NamedType(NamedTypeNode) => NamedType,
        ListType(ListTypeNode) => ListType,
    }
}

/// A named type.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedTypeNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
}

/// A list type.
///
/// The item type is boxed because a list type may wrap a list type without
/// bound.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ListTypeNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The item type.
    pub r#type: Box<TypeNode>,
}

/// A non null type.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct NonNullTypeNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The wrapped type.
    pub r#type: NullableTypeNode,
}

/* Type System Definition */

node_union! {
    /// A type system definition.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    TypeSystemDefinitionNode, into [DefinitionNode, AstNode] {
        SchemaDefinition(SchemaDefinitionNode) => SchemaDefinition,
        ScalarTypeDefinition(ScalarTypeDefinitionNode) => ScalarTypeDefinition,
        ObjectTypeDefinition(ObjectTypeDefinitionNode) => ObjectTypeDefinition,
        InterfaceTypeDefinition(InterfaceTypeDefinitionNode) => InterfaceTypeDefinition,
        UnionTypeDefinition(UnionTypeDefinitionNode) => UnionTypeDefinition,
        EnumTypeDefinition(EnumTypeDefinitionNode) => EnumTypeDefinition,
        InputObjectTypeDefinition(InputObjectTypeDefinitionNode) => InputObjectTypeDefinition,
        DirectiveDefinition(DirectiveDefinitionNode) => DirectiveDefinition,
    }
}

/// A schema definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The operation types.
    pub operation_types: Vec<OperationTypeDefinitionNode>,
}

/// An operation type definition of a schema.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationTypeDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The operation.
    pub operation: OperationTypeNode,
    /// The root type of that operation.
    pub r#type: NamedTypeNode,
}

/* Type Definition */

node_union! {
    /// A type definition.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    TypeDefinitionNode, into [TypeSystemDefinitionNode, DefinitionNode, AstNode] {
        ScalarTypeDefinition(ScalarTypeDefinitionNode) => ScalarTypeDefinition,
        ObjectTypeDefinition(ObjectTypeDefinitionNode) => ObjectTypeDefinition,
        InterfaceTypeDefinition(InterfaceTypeDefinitionNode) => InterfaceTypeDefinition,
        UnionTypeDefinition(UnionTypeDefinitionNode) => UnionTypeDefinition,
        EnumTypeDefinition(EnumTypeDefinitionNode) => EnumTypeDefinition,
        InputObjectTypeDefinition(InputObjectTypeDefinitionNode) => InputObjectTypeDefinition,
    }
}

/// A scalar type definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarTypeDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
}

/// An object type definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectTypeDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The interfaces the type implements.
    pub interfaces: Option<Vec<NamedTypeNode>>,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The fields.
    pub fields: Option<Vec<FieldDefinitionNode>>,
}

/// A field definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The arguments.
    pub arguments: Option<Vec<InputValueDefinitionNode>>,
    /// The type.
    pub r#type: TypeNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
}

/// An input value definition, an argument or an input field.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct InputValueDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The type.
    pub r#type: TypeNode,
    /// The default value, when the definition carries one.
    pub default_value: Option<ConstValueNode>,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
}

/// An interface type definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceTypeDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The interfaces the type implements.
    pub interfaces: Option<Vec<NamedTypeNode>>,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The fields.
    pub fields: Option<Vec<FieldDefinitionNode>>,
}

/// A union type definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionTypeDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The member types.
    pub types: Option<Vec<NamedTypeNode>>,
}

/// An enum type definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumTypeDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The values.
    pub values: Option<Vec<EnumValueDefinitionNode>>,
}

/// An enum value definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumValueDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
}

/// An input object type definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct InputObjectTypeDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The fields.
    pub fields: Option<Vec<InputValueDefinitionNode>>,
}

/* Directive Definitions */

/// A directive definition.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveDefinitionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The description, when the definition carries one.
    pub description: Option<StringValueNode>,
    /// The name.
    pub name: NameNode,
    /// The arguments.
    pub arguments: Option<Vec<InputValueDefinitionNode>>,
    /// Whether the directive may appear more than once at a location.
    pub repeatable: bool,
    /// The locations the directive may appear at.
    pub locations: Vec<NameNode>,
}

/* Type System Extensions */

node_union! {
    /// A type system extension.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    TypeSystemExtensionNode, into [DefinitionNode, AstNode] {
        SchemaExtension(SchemaExtensionNode) => SchemaExtension,
        ScalarTypeExtension(ScalarTypeExtensionNode) => ScalarTypeExtension,
        ObjectTypeExtension(ObjectTypeExtensionNode) => ObjectTypeExtension,
        InterfaceTypeExtension(InterfaceTypeExtensionNode) => InterfaceTypeExtension,
        UnionTypeExtension(UnionTypeExtensionNode) => UnionTypeExtension,
        EnumTypeExtension(EnumTypeExtensionNode) => EnumTypeExtension,
        InputObjectTypeExtension(InputObjectTypeExtensionNode) => InputObjectTypeExtension,
    }
}

/// A schema extension.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct SchemaExtensionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The operation types.
    pub operation_types: Option<Vec<OperationTypeDefinitionNode>>,
}

/* Type Extensions */

node_union! {
    /// A type extension.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    TypeExtensionNode, into [TypeSystemExtensionNode, DefinitionNode, AstNode] {
        ScalarTypeExtension(ScalarTypeExtensionNode) => ScalarTypeExtension,
        ObjectTypeExtension(ObjectTypeExtensionNode) => ObjectTypeExtension,
        InterfaceTypeExtension(InterfaceTypeExtensionNode) => InterfaceTypeExtension,
        UnionTypeExtension(UnionTypeExtensionNode) => UnionTypeExtension,
        EnumTypeExtension(EnumTypeExtensionNode) => EnumTypeExtension,
        InputObjectTypeExtension(InputObjectTypeExtensionNode) => InputObjectTypeExtension,
    }
}

/// A scalar type extension.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ScalarTypeExtensionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
}

/// An object type extension.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ObjectTypeExtensionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The interfaces the extension adds.
    pub interfaces: Option<Vec<NamedTypeNode>>,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The fields the extension adds.
    pub fields: Option<Vec<FieldDefinitionNode>>,
}

/// An interface type extension.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceTypeExtensionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The interfaces the extension adds.
    pub interfaces: Option<Vec<NamedTypeNode>>,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The fields the extension adds.
    pub fields: Option<Vec<FieldDefinitionNode>>,
}

/// A union type extension.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionTypeExtensionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The member types the extension adds.
    pub types: Option<Vec<NamedTypeNode>>,
}

/// An enum type extension.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct EnumTypeExtensionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The values the extension adds.
    pub values: Option<Vec<EnumValueDefinitionNode>>,
}

/// An input object type extension.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct InputObjectTypeExtensionNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The name.
    pub name: NameNode,
    /// The directives.
    pub directives: Option<Vec<ConstDirectiveNode>>,
    /// The fields the extension adds.
    pub fields: Option<Vec<InputValueDefinitionNode>>,
}

/* Schema Coordinates */

node_union! {
    /// A schema coordinate.
    ///
    /// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
    SchemaCoordinateNode, into [AstNode] {
        TypeCoordinate(TypeCoordinateNode) => TypeCoordinate,
        MemberCoordinate(MemberCoordinateNode) => MemberCoordinate,
        ArgumentCoordinate(ArgumentCoordinateNode) => ArgumentCoordinate,
        DirectiveCoordinate(DirectiveCoordinateNode) => DirectiveCoordinate,
        DirectiveArgumentCoordinate(DirectiveArgumentCoordinateNode) => DirectiveArgumentCoordinate,
    }
}

/// A coordinate naming a type.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeCoordinateNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The type name.
    pub name: NameNode,
}

/// A coordinate naming a field or an enum value of a type.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct MemberCoordinateNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The type name.
    pub name: NameNode,
    /// The member name.
    pub member_name: NameNode,
}

/// A coordinate naming an argument of a field.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct ArgumentCoordinateNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The type name.
    pub name: NameNode,
    /// The field name.
    pub field_name: NameNode,
    /// The argument name.
    pub argument_name: NameNode,
}

/// A coordinate naming a directive.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveCoordinateNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The directive name.
    pub name: NameNode,
}

/// A coordinate naming an argument of a directive.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectiveArgumentCoordinateNode {
    /// The region of the source this node derives from.
    pub loc: Option<Location>,
    /// The directive name.
    pub name: NameNode,
    /// The argument name.
    pub argument_name: NameNode,
}

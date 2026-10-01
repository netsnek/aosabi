// SPDX-License-Identifier: MIT
/*
 * aosabi-infer: the descriptor graph, an owned arena of the types an
 * application describes, with its two constructors and its JSON dump.
 *
 * No file of the reference is translated here.
 */

//! The descriptor graph.
//!
//! A [`Graph`] holds one node per described type, the three roots and the
//! declarations of the application crate. It has two constructors.
//! [`Graph::from_roots`] walks the static descriptors of a linked
//! application, and [`Graph::from_js_value`] reads the dump
//! [`Graph::to_js_value`] writes, which is the artifact `descriptors.json`.
//! The contract between the two is that a graph read back from its dump
//! infers the same intermediate representation as the graph itself.
//!
//! # The walk
//!
//! `from_roots` follows the links from the roots in the order Query, Mutation,
//! Subscription, fields before methods and parameters before the return, and
//! makes one node per `TypeLink::id`. A node is reserved before its members
//! are walked, so a type that refers to itself terminates. A `Result` and a
//! smart pointer have no node and resolve to the node of their inner link.
//! Then every declaration of the application crate is appended in `SourceKey`
//! order, because the order of the declaration list is the order of the scan
//! of the module tree, which is no source file order of the reference.
//!
//! A `TypeExpr::Callable` becomes a node of its own, a member with one call
//! signature, whose parameters and return the walk follows as it follows
//! those of a method. A struct with a field under `#[aosabi(spread)]` is the
//! twin of an object literal with a spread, and once every node is walked,
//! each such field is replaced by the members of its type through
//! [`spread_order`], as the checker presents a spread type.
//!
//! # The spread order
//!
//! An object literal with spreads has the members of its spread type, which
//! the checker builds segment by segment: a run of own members, or one spread.
//! The members come in the segments in reverse source order, each in its own
//! order, and a name in two segments stands at the place and with the value of
//! its later occurrence. [`spread_order`] is that rule, and two callers use
//! it, the root of a `graphql!` fragment and a struct with spread fields. The
//! members a spread of a struct brings are the public fields of its type, its
//! own and those of an embedded base, and the `__typename` literal it carries,
//! because the checker skips a private member and a method of a class when it
//! spreads an object. A private field of the spread type, its own or one of an
//! embedded base, brings no member and hides the member of its name in every
//! segment before the spread, while a segment after the spread that holds the
//! name brings it again, because the checker drops a name it skips as private
//! on the right of a spread from the members of the left. A struct that
//! declares a typename of its own keeps it. A spread whose type describes no
//! object, and a spread whose type spreads back to the struct, refuse the
//! build.
//!
//! # The roots of an entry
//!
//! A root of `graphql!` and a root `compose` merges have no type of their own.
//! The walk makes one node of the object literal the checker would see, a root
//! node whose methods are the entries, which presents as a root struct does.
//! The entries of a fragment come in the order of the spread type,
//! [`spread_order`] over its segments. A merged root lists the members of the
//! parent and then each member of the child whose name the parent lacks, the
//! order of the properties of an intersection. An entry takes the description
//! of its own comment and the parameters of the function it reaches, whose
//! comments carry the `@param` text of that function. An entry under
//! `mutation(...)` returns the payload of the reference's return type, a
//! promise of the intersection of an anonymous object whose fields are the
//! fields of the returned object, each nullable, and the one anonymous object
//! `{userErrors: UserError[]}` every payload shares, whose members the schema
//! prints as one type with `userErrors: [UserError!]!` last. The entries that
//! answer one returned type answer one payload, as the checker instantiates
//! the mapped type of the wrapper once for one result type. An anonymous
//! struct is the twin of an object literal, which the checker types afresh at
//! each literal, so the payload of one follows the function whose body holds
//! the literal. The entries that reference one such function answer one
//! payload, and an inline closure, which one entry owns, answers a payload of
//! its own.
//!
//! # The dump
//!
//! The dump is `{"version":1,"crate":..,"roots":{..},"types":[..],
//! "declarations":[..]}`. A node refers to another by its position in
//! `types`. Every node carries `expr` and `rust`, a wrapper carries `of`, a
//! primitive carries `primitive`, and a named node carries the fields of the
//! `TypeInfo` descriptor in camel case. The documentation of a type or a
//! member is its comment under `docs` and its tags under `tags`, and its
//! `Site` is an object under `site`. A dump without `tags`, `site` or the
//! `docs` of a parameter reads as an item with no tag, no site and no
//! comment, so a dump written before those existed reads the same. A root
//! node of an entry carries `fragment`, an entry its `entry` and a function
//! its `decorators` and its `guards`, an embedded base the kind `embedded`
//! with its `field`, a field a spread brings the `spread` it came through,
//! with the name and the site of the spread field, a callable node the
//! expression `callable` with its `params` and its `returns`, and an
//! intersection the expression `intersection` with its `members`. Each key is
//! written only where it holds something, so the dump of a twin that uses
//! none of them is the bytes it was. The dump holds the fields of a struct
//! after the spread fields are replaced, so a graph read back presents the
//! same members with no spread left to replace. The file exists so the
//! inference runs with no application linked.

use core::{
    any::TypeId,
    fmt, //
};

use aosabi_core::descriptor::{
    Declaration,
    DecoratorFact,
    Docs,
    EntryInfo,
    EntryKind,
    FragmentRoot,
    GuardFact,
    HeritageInfo,
    HeritageKind,
    MethodInfo,
    Primitive,
    RootInfo,
    RootsInfo,
    Segment,
    Site,
    SourceKey,
    TypeExpr,
    TypeInfo,
    TypeKind,
    TypeLink,
    UserError,
    Visibility,
    callable::CallableInfo, //
};
use aosabi_json::{
    JsObject,
    JsValue,
    map::IndexMap,
    value::KeyHasherBuilder, //
};

use super::authoring::{
    AuthoringGuard,
    AuthoringGuardCall, //
};
use crate::error::InferError;

/// The three primitives of the descriptor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PrimitiveKind {
    /// `string`.
    String,
    /// `number`.
    Number,
    /// `boolean`.
    Boolean,
}

/// The kind of a named type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum NamedKind {
    /// A struct, which presents as a class.
    Object,
    /// An enum whose variants carry no data, which presents as a union of
    /// string literals.
    Enum,
    /// An enum whose variants carry no data under `#[aosabi(enum)]`, which
    /// presents as a TypeScript `enum` declaration.
    EnumDeclaration,
    /// An enum whose variants each carry one type, which presents as a union
    /// of those types.
    Union,
    /// A scalar newtype.
    Scalar,
}

/// Where a declaration stands, the `SourceKey` of the descriptor.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct SourcePosition {
    /// The source file.
    pub(super) file: String,
    /// The line of the derive.
    pub(super) line: u32,
    /// The column of the derive.
    pub(super) column: u32,
}

/// The documentation of a node or a member, the `Docs` of the descriptor
/// owned.
#[derive(Debug, Clone, Default)]
pub(super) struct DocsNode {
    /// The comment.
    pub(super) comment: String,
    /// The tags in source order.
    pub(super) tags: Vec<TagNode>,
}

/// Where an item stands and the documentation block it carries, the `Site`
/// of the descriptor owned.
#[derive(Debug, Clone, Default)]
pub(super) struct SiteNode {
    /// The value of `file!()` at the macro.
    pub(super) file: String,
    /// The source line of the name of the item.
    pub(super) line: u32,
    /// The lines of the last documentation block as written.
    pub(super) docs: Vec<String>,
    /// The number of documentation blocks.
    pub(super) blocks: u32,
}

/// One tag of a documentation comment.
#[derive(Debug, Clone)]
pub(super) struct TagNode {
    /// The tag name without the `@`.
    pub(super) name: String,
    /// The tag text.
    pub(super) text: String,
}

/// One field of a named node.
#[derive(Debug, Clone)]
pub(super) struct FieldNode {
    /// The field name.
    pub(super) name: String,
    /// The documentation of the field.
    pub(super) docs: DocsNode,
    /// Where the field stands.
    pub(super) site: SiteNode,
    /// Whether the field is private.
    pub(super) is_private: bool,
    /// The node of the field type.
    pub(super) r#type: u32,
    /// The spread the field came through, `None` for a field the struct
    /// declares.
    pub(super) spread: Option<SpreadNode>,
}

/// The spread a member of a struct came through, the field under
/// `#[aosabi(spread)]` that stands in its place in the source.
#[derive(Debug, Clone)]
pub(super) struct SpreadNode {
    /// The name of the spread field.
    pub(super) field: String,
    /// Where the spread field stands.
    pub(super) site: SiteNode,
}

/// One parameter of a method.
#[derive(Debug, Clone)]
pub(super) struct ParamNode {
    /// The parameter name.
    pub(super) name: String,
    /// The node of the parameter type.
    pub(super) r#type: u32,
    /// Whether the parameter carries a default.
    pub(super) optional: bool,
    /// The documentation of the parameter.
    pub(super) docs: DocsNode,
    /// Where the parameter stands.
    pub(super) site: SiteNode,
}

/// One method of a named node.
#[derive(Debug, Clone)]
pub(super) struct MethodNode {
    /// The method name.
    pub(super) name: String,
    /// The documentation of the method.
    pub(super) docs: DocsNode,
    /// Where the method stands.
    pub(super) site: SiteNode,
    /// Whether the method is private.
    pub(super) is_private: bool,
    /// Whether the method is an `async fn`.
    pub(super) is_async: bool,
    /// The parameters after the receiver, in declaration order.
    pub(super) params: Vec<ParamNode>,
    /// The node of the return type.
    pub(super) returns: u32,
    /// What the method is when it is an entry of `graphql!`.
    pub(super) entry: Option<EntryNode>,
    /// The decorators of the entry and of the function it reaches, in source
    /// order.
    pub(super) decorators: Vec<DecoratorNode>,
    /// The guards of the leading run of the body of the function the method
    /// or the entry reaches, in source order.
    pub(super) guards: Vec<AuthoringGuard>,
}

/// One variant of an enum.
#[derive(Debug, Clone)]
pub(super) struct VariantNode {
    /// The variant identifier.
    pub(super) name: String,
    /// The enum value the variant stands for.
    pub(super) value: String,
    /// The documentation of the variant.
    pub(super) docs: DocsNode,
    /// Where the variant stands.
    pub(super) site: SiteNode,
}

/// One heritage entry of a named node or of a declaration.
#[derive(Debug, Clone)]
pub(super) struct HeritageNode {
    /// Whether the entry is an `extends` and not an `implements` relation,
    /// an embedded base included.
    pub(super) is_extends: bool,
    /// The field that holds an embedded base, whose members the node presents
    /// after its own, or `None` for a base stated on the type.
    pub(super) embedded: Option<String>,
    /// The node of the base type.
    pub(super) base: u32,
}

/// What an entry of `graphql!` is, for the authoring map.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct EntryNode {
    /// `inline`, `reference` or `shorthand`.
    pub(super) kind: &'static str,
    /// Whether the entry stands under `mutation(...)`.
    pub(super) mutation: bool,
}

/// One decorator of a function, the `DecoratorFact` of the descriptor owned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DecoratorNode {
    /// The expression as written.
    pub(super) text: String,
    /// The source line of the attribute.
    pub(super) line: u32,
}

/// A named type, the `TypeInfo` of the descriptor with links as node
/// positions.
#[derive(Debug, Clone)]
pub(super) struct NamedNode {
    /// The kind of the type.
    pub(super) kind: NamedKind,
    /// The declared name.
    pub(super) name: String,
    /// The crate that declares the type.
    pub(super) krate: String,
    /// Where the type is declared.
    pub(super) source: SourcePosition,
    /// The documentation of the type.
    pub(super) docs: DocsNode,
    /// Where the type stands.
    pub(super) site: SiteNode,
    /// The `__typename` literal, which overrides the name.
    pub(super) typename: Option<String>,
    /// Whether the type presents with no name of its own.
    pub(super) anonymous: bool,
    /// The fields in declaration order.
    pub(super) fields: Vec<FieldNode>,
    /// The methods in declaration order.
    pub(super) methods: Vec<MethodNode>,
    /// The variants of an enum.
    pub(super) variants: Vec<VariantNode>,
    /// The member nodes of a union.
    pub(super) members: Vec<u32>,
    /// The nodes of the type arguments.
    pub(super) type_arguments: Vec<u32>,
    /// The heritage in attribute order.
    pub(super) heritage: Vec<HeritageNode>,
    /// The constant `graphql!` declares, for the root node of an entry, and
    /// the empty string for the root node of a merge of structs alone.
    pub(super) fragment: Option<String>,
}

/// A function valued member, the `CallableInfo` of the descriptor with links
/// as node positions.
#[derive(Debug, Clone)]
pub(super) struct CallableNode {
    /// The parameters of the call signature, in declaration order.
    pub(super) params: Vec<ParamNode>,
    /// The node of the return type.
    pub(super) returns: u32,
}

/// What a node is, the `TypeExpr` of the descriptor with links as node
/// positions.
#[derive(Debug, Clone)]
pub(super) enum Expr {
    /// A primitive.
    Primitive(PrimitiveKind),
    /// `()`.
    Void,
    /// The any marker.
    Any,
    /// The never marker.
    Never,
    /// `Option<T>`.
    Nullable(u32),
    /// A list of `T`.
    List(u32),
    /// A promise of `T`.
    Promise(u32),
    /// A repeater of `T`.
    Repeater(u32),
    /// A named type.
    Named(Box<NamedNode>),
    /// A function valued member, `(a: A) => R`.
    Callable(Box<CallableNode>),
    /// An intersection of object types, `A & B`, its members in source
    /// order, which the payload of `mutation(...)` is.
    Intersection(Vec<u32>),
}

/// One type of the graph.
#[derive(Debug, Clone)]
pub(super) struct Node {
    /// The Rust name of the type, for messages and the dump alone.
    pub(super) rust: String,
    /// What the node is.
    pub(super) expr: Expr,
}

/// One declaration of the application crate.
#[derive(Debug, Clone)]
pub(super) struct DeclarationNode {
    /// The crate that declares the type.
    pub(super) krate: String,
    /// Where the type is declared.
    pub(super) source: SourcePosition,
    /// The declared name.
    pub(super) name: String,
    /// The node of the type, absent for a generic declaration.
    pub(super) link: Option<u32>,
    /// The heritage in attribute order.
    pub(super) heritage: Vec<HeritageNode>,
}

/// The types an application describes, as an owned arena.
#[derive(Debug, Clone)]
pub struct Graph {
    /// The application crate, which is the crate of the first present root.
    pub(super) krate: Option<String>,
    /// The nodes of the roots, in the order Query, Mutation, Subscription.
    pub(super) roots: [Option<u32>; 3],
    /// The nodes in walk order.
    pub(super) types: Vec<Node>,
    /// The declarations of the application crate in `SourceKey` order.
    pub(super) declarations: Vec<DeclarationNode>,
}

/// Says why a value is no descriptor dump.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphError {
    /// The path of the offending value, such as `types[3].fields[0].type`,
    /// and the empty string for the root.
    pub path: String,
    /// What the reader expected there.
    pub expected: &'static str,
}

impl fmt::Display for GraphError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            return write!(
                f,
                "expected {} at the root of the descriptor dump",
                self.expected
            );
        }
        write!(
            f,
            "expected {} at {} of the descriptor dump",
            self.expected, self.path
        )
    }
}

impl core::error::Error for GraphError {}

/// Returns the position the next pushed element of a list of `len` elements
/// takes, saturated to the range of a node position.
fn position(len: usize) -> u32 {
    u32::try_from(len).unwrap_or(u32::MAX)
}

/// The file of the reference that declares the wrapper of a mutation, which
/// the two objects of a payload name as their source.
const MUTATION_TS: &str = "aosabi/core/mutation.ts";

/// The walk of [`Graph::from_roots`], which makes one node per type identity.
struct Walk {
    /// The nodes made so far.
    types: Vec<Node>,
    /// The node of each type identity.
    ids: IndexMap<TypeId, u32, KeyHasherBuilder>,
    /// The fields of each struct with a spread field in source order, keyed
    /// by its node, until [`Walk::expand`] replaces its spread fields.
    layouts: IndexMap<u32, Vec<Layout>, KeyHasherBuilder>,
    /// The node of the object `{userErrors: UserError[]}` that the payload
    /// of every entry under `mutation(...)` shares, once one is walked.
    user_errors: Option<u32>,
    /// The node of each payload the entries under `mutation(...)` share,
    /// keyed by the type they return, or by the function they reference
    /// where it returns an anonymous struct.
    payloads: IndexMap<PayloadKey, u32, KeyHasherBuilder>,
}

/// What [`Walk::payloads`] keys a payload by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum PayloadKey {
    /// The node of a returned type that is no anonymous struct.
    Returned(u32),
    /// The function an entry references where it returns an anonymous
    /// struct, by the file, the line and the name of its site.
    Function(&'static str, u32, &'static str),
}

/// One segment of the fields of a struct with a spread field.
enum Layout {
    /// A run of fields the struct declares, in source order.
    Own(Vec<FieldNode>),
    /// One field under `#[aosabi(spread)]`.
    Spread {
        /// The spread field, its name and its site.
        spread: SpreadNode,
        /// The node of the type of the field.
        node: u32,
    },
}

/// One member of the spread type of a struct.
enum SpreadMember {
    /// A field.
    Field(FieldNode),
    /// The `__typename` literal of a spread type.
    Typename(String),
    /// A private field of a spread type, which brings no member and hides
    /// the member of its name in every segment before the spread, as the
    /// `skippedPrivateMembers` of the checker's `getSpreadType` do.
    Skipped,
}

/// Returns the members of an object literal with spreads, given as its
/// `segments` in source order, in the order the checker gives its spread
/// type.
///
/// The segments come in reverse source order, each in its own order, and a
/// name in two segments stands at the place and with the value of its later
/// occurrence. The root of a `graphql!` fragment and a struct with spread
/// fields both present their members through it. A member that only hides its
/// name, as a private field of a spread type does, takes its place by the same
/// rule and its caller drops it, so the name leaves every earlier segment and
/// stays in every later one.
fn spread_order<T>(segments: Vec<Vec<(String, T)>>) -> Vec<(String, T)> {
    let mut members: Vec<(String, T)> = Vec::new();
    for segment in segments.into_iter().rev() {
        for (name, member) in segment {
            if !members.iter().any(|(known, _)| *known == name) {
                members.push((name, member));
            }
        }
    }
    members
}

impl Walk {
    /// Returns the node of `link`, walking the type when it is new.
    ///
    /// The node is reserved before its members are walked, so a link back to
    /// the type finds it. A `Result` and a smart pointer resolve to the node
    /// of their inner link.
    fn node_of(&mut self, link: TypeLink) -> u32 {
        let id = link.id();

        if let Some(index) = self.ids.get(&id) {
            return *index;
        }

        if let TypeExpr::Fallible(inner) | TypeExpr::Boxed(inner) = link.resolve() {
            let index = self.node_of(inner);
            self.ids.insert(id, index);
            return index;
        }

        let index = position(self.types.len());
        self.ids.insert(id, index);
        self.types.push(Node {
            rust: String::from(link.rust_name()),
            expr: Expr::Any,
        });

        let expr = self.expr_of(index, link.resolve());

        if let Some(node) = usize::try_from(index)
            .ok()
            .and_then(|i| self.types.get_mut(i))
        {
            node.expr = expr;
        }

        index
    }

    /// Returns what the node at `index` of the expression `expr` is.
    ///
    /// The descriptor enum is open, and a variant this crate does not know
    /// yet presents as the any marker until its view lands with it.
    fn expr_of(&mut self, index: u32, expr: TypeExpr) -> Expr {
        match expr {
            TypeExpr::Primitive(Primitive::String) => Expr::Primitive(PrimitiveKind::String),
            TypeExpr::Primitive(Primitive::Number) => Expr::Primitive(PrimitiveKind::Number),
            TypeExpr::Primitive(Primitive::Boolean) => Expr::Primitive(PrimitiveKind::Boolean),
            TypeExpr::Void => Expr::Void,
            TypeExpr::Never => Expr::Never,
            TypeExpr::Nullable(inner) => Expr::Nullable(self.node_of(inner)),
            TypeExpr::List(inner) => Expr::List(self.node_of(inner)),
            TypeExpr::Promise(inner) => Expr::Promise(self.node_of(inner)),
            TypeExpr::Repeater(inner) => Expr::Repeater(self.node_of(inner)),
            TypeExpr::Named(info) => Expr::Named(Box::new(self.named_of(index, info))),
            TypeExpr::Callable(info) => Expr::Callable(Box::new(self.callable_of(info))),
            _ => Expr::Any,
        }
    }

    /// Walks a function valued member, parameters before the return, as a
    /// method is walked.
    fn callable_of(&mut self, info: &'static CallableInfo) -> CallableNode {
        let params = info
            .params
            .iter()
            .map(|param| ParamNode {
                name: String::from(param.name),
                r#type: self.node_of(param.r#type),
                optional: param.optional,
                docs: docs_of(&param.docs),
                site: site_of(&param.site),
            })
            .collect();

        CallableNode {
            params,
            returns: self.node_of(info.returns),
        }
    }

    /// Walks the named type at `index`, fields before methods, then the
    /// members, the type arguments and the heritage.
    ///
    /// The kind enum is open, and a kind this crate does not know yet
    /// presents as an object. A struct with a spread field keeps its own
    /// fields and records its layout, whose spread fields [`Walk::expand`]
    /// replaces once every node is walked.
    fn named_of(&mut self, index: u32, info: &'static TypeInfo) -> NamedNode {
        let has_spread = info.fields.iter().any(|field| field.spread);
        let mut fields = Vec::new();
        let mut layout = Vec::new();
        for field in info.fields {
            let site = site_of(&field.site);
            let r#type = self.node_of(field.r#type);
            if field.spread {
                let spread = SpreadNode {
                    field: String::from(field.name),
                    site,
                };
                layout.push(Layout::Spread {
                    spread,
                    node: r#type,
                });
                continue;
            }
            let node = FieldNode {
                name: String::from(field.name),
                docs: docs_of(&field.docs),
                site,
                is_private: field.visibility == Visibility::Private,
                r#type,
                spread: None,
            };
            if has_spread {
                match layout.last_mut() {
                    Some(Layout::Own(run)) => run.push(node.clone()),
                    _ => layout.push(Layout::Own(vec![node.clone()])),
                }
            }
            fields.push(node);
        }
        if has_spread {
            self.layouts.insert(index, layout);
        }
        let methods = info
            .methods
            .iter()
            .map(|method| self.method_of(method))
            .collect();

        NamedNode {
            kind: match info.kind {
                TypeKind::Enum => NamedKind::Enum,
                TypeKind::EnumDeclaration => NamedKind::EnumDeclaration,
                TypeKind::Union => NamedKind::Union,
                TypeKind::Scalar => NamedKind::Scalar,
                _ => NamedKind::Object,
            },
            name: String::from(info.name),
            krate: String::from(info.krate),
            source: source_of(&info.source),
            docs: docs_of(&info.docs),
            site: site_of(&info.site),
            typename: info.typename.map(String::from),
            anonymous: info.anonymous,
            fields,
            methods,
            variants: info
                .variants
                .iter()
                .map(|variant| VariantNode {
                    name: String::from(variant.name),
                    value: String::from(variant.value),
                    docs: docs_of(&variant.docs),
                    site: site_of(&variant.site),
                })
                .collect(),
            members: info.members.iter().map(|m| self.node_of(*m)).collect(),
            type_arguments: info
                .type_arguments
                .iter()
                .map(|a| self.node_of(*a))
                .collect(),
            heritage: self.heritage_of(info.heritage),
            fragment: None,
        }
    }

    /// Walks a method, parameters before the return.
    fn method_of(&mut self, method: &MethodInfo) -> MethodNode {
        let params = method
            .params
            .iter()
            .map(|param| ParamNode {
                name: String::from(param.name),
                r#type: self.node_of(param.r#type),
                optional: param.optional,
                docs: docs_of(&param.docs),
                site: site_of(&param.site),
            })
            .collect();

        MethodNode {
            name: String::from(method.name),
            docs: docs_of(&method.docs),
            site: site_of(&method.site),
            is_private: method.visibility == Visibility::Private,
            is_async: method.is_async,
            params,
            returns: self.node_of(method.returns),
            entry: None,
            decorators: decorators_of(method.decorators),
            guards: guards_of(method.guards),
        }
    }

    /// Walks the base types of a heritage list.
    fn heritage_of(&mut self, heritage: &[HeritageInfo]) -> Vec<HeritageNode> {
        heritage
            .iter()
            .map(|entry| HeritageNode {
                is_extends: entry.kind.is_extends(),
                embedded: match entry.kind {
                    HeritageKind::Embedded { field } => Some(String::from(field)),
                    HeritageKind::Extends | HeritageKind::Implements => None,
                },
                base: self.node_of(entry.base),
            })
            .collect()
    }

    /// Pushes `node` as a node of its own and returns its position.
    fn push(&mut self, rust: String, expr: Expr) -> u32 {
        let index = position(self.types.len());
        self.types.push(Node { rust, expr });
        index
    }

    /// Returns the node of `root`, walking a struct and making the node of
    /// the object literal of an entry, or `None` for an absent root.
    ///
    /// `name` is the name of the root, `Query`, `Mutation` or `Subscription`.
    ///
    /// # Errors
    ///
    /// Returns the refusal of a spread of a struct root that [`Walk::expand`]
    /// refuses.
    fn root_of(&mut self, root: &RootInfo, name: &str) -> Result<Option<u32>, InferError> {
        Ok(match root {
            RootInfo::Absent => None,
            RootInfo::Type(link) => Some(self.node_of(*link)),
            RootInfo::Fragment(_) | RootInfo::Merged(..) => {
                let members = self.root_members(root)?;
                let origin = origin_of(root);
                let mut fields = Vec::new();
                let mut methods = Vec::new();
                for (_, member) in members {
                    match member {
                        Member::Entry(entry) => methods.push(self.entry_of(entry)),
                        Member::Type { link, name } => {
                            let owner = self.node_of(link);
                            self.expand(owner, &mut Vec::new())?;
                            let Some(Expr::Named(named)) = usize::try_from(owner)
                                .ok()
                                .and_then(|index| self.types.get(index))
                                .map(|node| &node.expr)
                            else {
                                continue;
                            };
                            if let Some(field) =
                                named.fields.iter().find(|field| field.name == name)
                            {
                                fields.push(field.clone());
                            } else if let Some(method) =
                                named.methods.iter().find(|method| method.name == name)
                            {
                                methods.push(method.clone());
                            }
                        }
                    }
                }
                let named = NamedNode {
                    kind: NamedKind::Object,
                    name: String::from(name),
                    krate: origin.krate,
                    site: SiteNode {
                        file: origin.source.file.clone(),
                        line: origin.source.line,
                        docs: Vec::new(),
                        blocks: 0,
                    },
                    source: origin.source,
                    docs: DocsNode::default(),
                    typename: None,
                    anonymous: false,
                    fields,
                    methods,
                    variants: Vec::new(),
                    members: Vec::new(),
                    type_arguments: Vec::new(),
                    heritage: Vec::new(),
                    fragment: Some(origin.fragment.unwrap_or_default()),
                };
                let rust = match named.fragment.as_deref() {
                    Some("") | None => format!("compose().{name}"),
                    Some(fragment) => format!("graphql!({fragment}).{name}"),
                };
                Some(self.push(rust, Expr::Named(Box::new(named))))
            }
        })
    }

    /// Returns the members of `root` by name, in the order the checker lists
    /// the properties of the object literal, the spread type or the
    /// intersection.
    ///
    /// The members of a struct are its fields and then its methods. A struct
    /// with a spread field is walked and its spreads replaced first, so it
    /// lists the members its spreads bring.
    ///
    /// # Errors
    ///
    /// Returns the refusal of a spread of a struct root that [`Walk::expand`]
    /// refuses.
    fn root_members(&mut self, root: &RootInfo) -> Result<Vec<(String, Member)>, InferError> {
        Ok(match root {
            RootInfo::Absent => Vec::new(),
            RootInfo::Type(link) => {
                let TypeExpr::Named(info) = link.resolve() else {
                    return Ok(Vec::new());
                };
                let names: Vec<String> = if info.fields.iter().any(|field| field.spread) {
                    let owner = self.node_of(*link);
                    self.expand(owner, &mut Vec::new())?;
                    match self.expr_at(owner) {
                        Some(Expr::Named(named)) => named
                            .fields
                            .iter()
                            .map(|field| field.name.clone())
                            .chain(named.methods.iter().map(|method| method.name.clone()))
                            .collect(),
                        _ => Vec::new(),
                    }
                } else {
                    info.fields
                        .iter()
                        .map(|field| String::from(field.name))
                        .chain(info.methods.iter().map(|method| String::from(method.name)))
                        .collect()
                };
                names
                    .into_iter()
                    .map(|name| {
                        let member = Member::Type {
                            link: *link,
                            name: name.clone(),
                        };
                        (name, member)
                    })
                    .collect()
            }
            RootInfo::Fragment(fragment) => fragment_members(fragment),
            RootInfo::Merged(parent, child) => {
                let mut members = self.root_members(parent)?;
                for (name, member) in self.root_members(child)? {
                    if !members.iter().any(|(known, _)| *known == name) {
                        members.push((name, member));
                    }
                }
                members
            }
        })
    }

    /// Replaces the spread fields of the struct at `owner` by the members of
    /// their types, in the order of [`spread_order`], once.
    ///
    /// The spreads of a spread type are replaced first, and `stack` holds the
    /// structs whose spreads are being replaced, so a spread that reaches one
    /// of them again is a cycle. A struct with no spread field, or one already
    /// replaced, is left as it is.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::SpreadNotDescribed`] for a spread whose type is
    /// no named object, and [`InferError::SpreadCycle`] for a spread whose
    /// type spreads back to a struct on `stack`.
    fn expand(&mut self, owner: u32, stack: &mut Vec<u32>) -> Result<(), InferError> {
        let Some(layout) = self.layouts.shift_remove(&owner) else {
            return Ok(());
        };
        stack.push(owner);
        let mut segments = Vec::new();
        for segment in layout {
            match segment {
                Layout::Own(fields) => segments.push(
                    fields
                        .into_iter()
                        .map(|field| (field.name.clone(), SpreadMember::Field(field)))
                        .collect(),
                ),
                Layout::Spread { spread, node } => {
                    if stack.contains(&node) {
                        return Err(InferError::SpreadCycle {
                            owner: self.name_at(owner),
                            field: spread.field,
                        });
                    }
                    self.expand(node, stack)?;
                    let Some(members) =
                        self.object_members(node, &spread, stack, &mut Vec::new())?
                    else {
                        return Err(InferError::SpreadNotDescribed {
                            owner: self.name_at(owner),
                            field: spread.field,
                        });
                    };
                    segments.push(members);
                }
            }
        }
        stack.pop();

        let mut typename = None;
        let fields: Vec<FieldNode> = spread_order(segments)
            .into_iter()
            .filter_map(|(_, member)| match member {
                SpreadMember::Field(field) => Some(field),
                SpreadMember::Typename(literal) => {
                    typename = Some(literal);
                    None
                }
                SpreadMember::Skipped => None,
            })
            .collect();
        if let Some(Node {
            expr: Expr::Named(named),
            ..
        }) = usize::try_from(owner)
            .ok()
            .and_then(|index| self.types.get_mut(index))
        {
            named.fields = fields;
            if named.typename.is_none() {
                named.typename = typename;
            }
        }
        Ok(())
    }

    /// Returns the members a spread of the named object at `node` brings
    /// through `spread`, or `None` when the node is no named object.
    ///
    /// They are its fields, each public one marked with the spread it came
    /// through and each private one a [`SpreadMember::Skipped`] of its name,
    /// then its `__typename` literal, then the members of each embedded base
    /// whose name the list lacks, the order the checker lists the properties
    /// of a derived class in. The stack is the one of [`Walk::expand`], and
    /// `bases` holds the nodes whose members are being listed, which ends a
    /// base that embeds itself.
    ///
    /// # Errors
    ///
    /// Returns the refusal of a spread of an embedded base that
    /// [`Walk::expand`] refuses.
    fn object_members(
        &mut self,
        node: u32,
        spread: &SpreadNode,
        stack: &mut Vec<u32>,
        bases: &mut Vec<u32>,
    ) -> Result<Option<Vec<(String, SpreadMember)>>, InferError> {
        let Some(Expr::Named(named)) = self.expr_at(node) else {
            return Ok(None);
        };
        if named.kind != NamedKind::Object || bases.contains(&node) {
            return Ok(None);
        }
        let mut members: Vec<(String, SpreadMember)> = named
            .fields
            .iter()
            .map(|field| {
                let member = if field.is_private {
                    SpreadMember::Skipped
                } else {
                    let mut field = field.clone();
                    field.spread = Some(spread.clone());
                    SpreadMember::Field(field)
                };
                (field.name.clone(), member)
            })
            .collect();
        if let Some(literal) = &named.typename {
            members.push((
                String::from("__typename"),
                SpreadMember::Typename(literal.clone()),
            ));
        }
        let embedded: Vec<u32> = named
            .heritage
            .iter()
            .filter(|entry| entry.embedded.is_some())
            .map(|entry| entry.base)
            .collect();
        bases.push(node);
        for base in embedded {
            self.expand(base, stack)?;
            for (name, member) in self
                .object_members(base, spread, stack, bases)?
                .unwrap_or_default()
            {
                if !members.iter().any(|(known, _)| *known == name) {
                    members.push((name, member));
                }
            }
        }
        bases.pop();
        Ok(Some(members))
    }

    /// Returns the declared name of the named node at `index`, for a refusal.
    fn name_at(&self, index: u32) -> String {
        match self.expr_at(index) {
            Some(Expr::Named(named)) => named.name.clone(),
            _ => String::new(),
        }
    }

    /// Returns the method of one entry of `graphql!`.
    ///
    /// The entry names the field and carries its description, and the
    /// function it reaches gives the parameters, whose documentation carries
    /// the `@param` text of that function, and the return.
    fn entry_of(&mut self, entry: &'static EntryInfo) -> MethodNode {
        let method = entry.kind.method();
        let params = method
            .params
            .iter()
            .map(|param| ParamNode {
                name: String::from(param.name),
                r#type: self.node_of(param.r#type),
                optional: param.optional,
                docs: docs_of(&param.docs),
                site: site_of(&param.site),
            })
            .collect();
        let mutation = entry.kind.is_mutation();
        let returns = if mutation {
            self.payload_of(entry, method)
        } else {
            self.node_of(method.returns)
        };
        let kind = match entry.kind.unwrapped() {
            EntryKind::Reference(_) => "reference",
            EntryKind::Shorthand(_) => "shorthand",
            EntryKind::Inline(_) | EntryKind::Mutation(_) => "inline",
        };
        let mut decorators = decorators_of(entry.decorators);
        if kind != "inline" {
            decorators.extend(decorators_of(method.decorators));
        }
        MethodNode {
            name: String::from(entry.name),
            docs: docs_of(&entry.docs),
            site: site_of(&entry.site),
            is_private: false,
            is_async: method.is_async || mutation,
            params,
            returns,
            entry: Some(EntryNode { kind, mutation }),
            decorators,
            guards: guards_of(method.guards),
        }
    }

    /// Returns the node of the payload of `entry`, an entry under
    /// `mutation(...)` that calls `method`, a promise of the intersection of
    /// the mapped object and the object of `userErrors`.
    ///
    /// The reference types the wrapper as
    /// `Promise<{[K in keyof Awaited<R>]: Awaited<R>[K] | null} & {userErrors: UserError[]}>`.
    /// The mapped object holds the fields of the returned object, each
    /// nullable with its documentation, and has no name of its own. The object
    /// of `userErrors` names no type parameter of the wrapper, so the checker
    /// gives every payload the same one, [`Walk::user_errors_object`]. The
    /// inference names the intersection after its field and then each of its
    /// members, so at the first payload the mapped object, unless it holds no
    /// field, and the object of `userErrors` take names of the counter of that
    /// field, which a later type of that name skips.
    ///
    /// The checker instantiates the mapped type once for one result type, so
    /// the entries that return one type answer one payload, which
    /// [`Walk::payloads`] keys by the node of that type as the source path
    /// keys the payload of one struct, and the inference names it after the
    /// field of the first of them. An anonymous struct is the twin of an
    /// object literal, which the checker types afresh at each literal, and the
    /// literal stands in the body of the function an entry calls. So the
    /// entries that reference one function that returns one answer one
    /// payload, which [`Walk::payloads`] keys by that function, and an inline
    /// closure, which one entry owns, answers a payload of its own. The walk
    /// tells a function by the file, the line and the name of its site, since
    /// an entry reaches the descriptor of a function it references through a
    /// constant, which the language gives no fixed address.
    fn payload_of(&mut self, entry: &EntryInfo, method: &MethodInfo) -> u32 {
        let returns = self.node_of(method.returns);
        let inner = match self.expr_at(returns) {
            Some(Expr::Promise(of)) => *of,
            _ => returns,
        };
        let literal = matches!(self.expr_at(inner), Some(Expr::Named(named)) if named.anonymous);
        let key = match (literal, entry.kind.unwrapped()) {
            (false, _) => Some(PayloadKey::Returned(inner)),
            (true, EntryKind::Inline(_)) => None,
            (true, _) => Some(PayloadKey::Function(
                method.site.file,
                method.site.line,
                method.name,
            )),
        };
        if let Some(node) = key.and_then(|key| self.payloads.get(&key).copied()) {
            return node;
        }
        let label = if literal {
            format!("mutation({})", method.name)
        } else {
            let rust = self
                .types
                .get(usize::try_from(inner).unwrap_or(usize::MAX))
                .map_or("", |node| node.rust.as_str());
            format!("mutation(-> {rust})")
        };
        let mut fields: Vec<FieldNode> = match self.expr_at(inner) {
            Some(Expr::Named(named)) if named.kind == NamedKind::Object => named.fields.clone(),
            _ => Vec::new(),
        };
        for field in &mut fields {
            field.r#type = self.nullable_of(field.r#type);
        }
        let mapped = NamedNode {
            kind: NamedKind::Object,
            name: format!("{}Payload", entry.name),
            krate: String::from("aosabi-core"),
            source: SourcePosition {
                file: String::from(MUTATION_TS),
                line: 78,
                column: 3,
            },
            docs: DocsNode::default(),
            site: SiteNode::default(),
            typename: None,
            anonymous: true,
            fields,
            methods: Vec::new(),
            variants: Vec::new(),
            members: Vec::new(),
            type_arguments: Vec::new(),
            heritage: Vec::new(),
            fragment: None,
        };
        let mapped = self.push(format!("{label} fields"), Expr::Named(Box::new(mapped)));
        let user_errors = self.user_errors_object();
        let payload = self.push(
            format!("{label} payload"),
            Expr::Intersection(vec![mapped, user_errors]),
        );
        let promise = self.push(format!("Promise<{label} payload>"), Expr::Promise(payload));
        if let Some(key) = key {
            self.payloads.insert(key, promise);
        }
        promise
    }

    /// Returns the node of the object `{userErrors: UserError[]}` of the
    /// return type of the wrapper, which the payload of every entry under
    /// `mutation(...)` shares, made at its first use.
    fn user_errors_object(&mut self) -> u32 {
        if let Some(node) = self.user_errors {
            return node;
        }
        let r#type = self.node_of(TypeLink::of::<Vec<UserError>>());
        let object = NamedNode {
            kind: NamedKind::Object,
            name: String::from("UserErrors"),
            krate: String::from("aosabi-core"),
            source: SourcePosition {
                file: String::from(MUTATION_TS),
                line: 78,
                column: 53,
            },
            docs: DocsNode::default(),
            site: SiteNode::default(),
            typename: None,
            anonymous: true,
            fields: vec![FieldNode {
                name: String::from("userErrors"),
                docs: DocsNode::default(),
                site: SiteNode::default(),
                is_private: false,
                r#type,
                spread: None,
            }],
            methods: Vec::new(),
            variants: Vec::new(),
            members: Vec::new(),
            type_arguments: Vec::new(),
            heritage: Vec::new(),
            fragment: None,
        };
        let node = self.push(
            String::from("{userErrors: UserError[]}"),
            Expr::Named(Box::new(object)),
        );
        self.user_errors = Some(node);
        node
    }

    /// Returns the node of `T | null` over the node `inner`, which is `inner`
    /// itself when it is nullable already.
    fn nullable_of(&mut self, inner: u32) -> u32 {
        if matches!(self.expr_at(inner), Some(Expr::Nullable(_))) {
            return inner;
        }
        let existing = self
            .types
            .iter()
            .position(|node| matches!(node.expr, Expr::Nullable(of) if of == inner));
        if let Some(index) = existing {
            return position(index);
        }
        let rust = self
            .types
            .get(usize::try_from(inner).unwrap_or(usize::MAX))
            .map_or_else(String::new, |node| format!("Option<{}>", node.rust));
        self.push(rust, Expr::Nullable(inner))
    }

    /// Returns what the node at `index` is.
    fn expr_at(&self, index: u32) -> Option<&Expr> {
        self.types
            .get(usize::try_from(index).ok()?)
            .map(|node| &node.expr)
    }
}

/// Where the node of a root of an entry comes from.
#[derive(Default)]
struct Origin {
    /// The crate of the fragment or of the struct.
    krate: String,
    /// The source key of the `graphql!` or of the derive.
    source: SourcePosition,
    /// The constant of the fragment, `None` for a struct.
    fragment: Option<String>,
}

/// One member of the node of a root of an entry.
enum Member {
    /// An entry of `graphql!`.
    Entry(&'static EntryInfo),
    /// A field or a method of a struct that is a root of a merge.
    Type {
        /// The struct.
        link: TypeLink,
        /// The name of the member.
        name: String,
    },
}

/// Returns the crate, the source and the constant of the first fragment
/// or struct a root reaches, which the node of the root carries.
fn origin_of(root: &RootInfo) -> Origin {
    match root {
        RootInfo::Absent => Origin::default(),
        RootInfo::Type(link) => match link.resolve() {
            TypeExpr::Named(info) => Origin {
                krate: String::from(info.krate),
                source: source_of(&info.source),
                fragment: None,
            },
            _ => Origin::default(),
        },
        RootInfo::Fragment(fragment) => Origin {
            krate: String::from(fragment.krate),
            source: source_of(&fragment.source),
            fragment: Some(String::from(fragment.fragment)),
        },
        RootInfo::Merged(parent, child) => {
            if parent.is_present() {
                origin_of(parent)
            } else {
                origin_of(child)
            }
        }
    }
}

/// Returns the entries of a root of `graphql!` in the order of the spread
/// type, [`spread_order`] over its segments, a spread segment being the
/// entries of the root it spreads.
fn fragment_members(fragment: &'static FragmentRoot) -> Vec<(String, Member)> {
    spread_order(
        fragment
            .segments
            .iter()
            .map(|segment| match segment {
                Segment::Entries(entries) => entries
                    .iter()
                    .map(|entry| (String::from(entry.name), Member::Entry(entry)))
                    .collect(),
                Segment::Spread(spread) => fragment_members(spread()),
            })
            .collect(),
    )
}

/// Copies the decorators of a descriptor into owned nodes.
fn decorators_of(decorators: &[DecoratorFact]) -> Vec<DecoratorNode> {
    decorators
        .iter()
        .map(|decorator| DecoratorNode {
            text: String::from(decorator.text),
            line: decorator.line,
        })
        .collect()
}

/// Copies the guards of a descriptor into owned nodes, the guards of the
/// authoring map.
fn guards_of(guards: &[GuardFact]) -> Vec<AuthoringGuard> {
    guards
        .iter()
        .map(|guard| AuthoringGuard {
            kind: guard.kind.name(),
            args: guard
                .args
                .map(|args| args.iter().copied().map(String::from).collect()),
            calls: guard
                .calls
                .iter()
                .map(|call| AuthoringGuardCall {
                    name: String::from(call.name),
                    args: call.args.iter().copied().map(String::from).collect(),
                })
                .collect(),
            others: guard.others,
            line: guard.line,
        })
        .collect()
}

/// Copies the documentation of a descriptor into an owned node.
fn docs_of(docs: &Docs) -> DocsNode {
    DocsNode {
        comment: String::from(docs.comment),
        tags: docs
            .tags
            .iter()
            .map(|tag| TagNode {
                name: String::from(tag.name),
                text: String::from(tag.text),
            })
            .collect(),
    }
}

/// Copies the site of a descriptor into an owned node.
fn site_of(site: &Site) -> SiteNode {
    SiteNode {
        file: String::from(site.file),
        line: site.line,
        docs: site.docs.iter().copied().map(String::from).collect(),
        blocks: site.blocks,
    }
}

/// Copies a `SourceKey` into an owned position.
fn source_of(source: &SourceKey) -> SourcePosition {
    SourcePosition {
        file: String::from(source.file),
        line: source.line,
        column: source.column,
    }
}

impl Graph {
    /// Builds the graph of a linked application from its roots and from the
    /// declarations the derives emitted.
    ///
    /// `declarations` is the declaration list the roots carry, and the graph
    /// keeps those of the application crate alone, sorted by their
    /// `SourceKey`. Once every node is walked, the spread fields of every
    /// struct are replaced by the members of their types.
    ///
    /// # Errors
    ///
    /// Returns [`InferError::SpreadNotDescribed`] for a field under
    /// `#[aosabi(spread)]` whose type is no named object, and
    /// [`InferError::SpreadCycle`] for one whose type spreads back to the
    /// struct that declares it.
    pub fn from_roots(roots: &RootsInfo, declarations: &[Declaration]) -> Result<Self, InferError> {
        let mut walk = Walk {
            types: Vec::new(),
            ids: IndexMap::default(),
            layouts: IndexMap::default(),
            user_errors: None,
            payloads: IndexMap::default(),
        };

        let mut nodes = [None; 3];
        for (slot, (root, name)) in nodes.iter_mut().zip([
            (roots.query, "Query"),
            (roots.mutation, "Mutation"),
            (roots.subscription, "Subscription"),
        ]) {
            *slot = walk.root_of(&root, name)?;
        }
        let roots = nodes;

        let krate = roots
            .iter()
            .flatten()
            .next()
            .and_then(|index| usize::try_from(*index).ok())
            .and_then(|index| walk.types.get(index))
            .and_then(|node| match &node.expr {
                Expr::Named(named) => Some(named.krate.clone()),
                _ => None,
            });

        let mut own: Vec<&Declaration> = declarations
            .iter()
            .filter(|declaration| Some(declaration.krate) == krate.as_deref())
            .collect();
        own.sort_by_key(|declaration| declaration.source);

        let declarations = own
            .into_iter()
            .map(|declaration| DeclarationNode {
                krate: String::from(declaration.krate),
                source: source_of(&declaration.source),
                name: String::from(declaration.name),
                link: declaration.link.map(|link| walk.node_of(link)),
                heritage: walk.heritage_of(declaration.heritage),
            })
            .collect();

        while let Some(owner) = walk.layouts.keys().next().copied() {
            walk.expand(owner, &mut Vec::new())?;
        }

        Ok(Self {
            krate,
            roots,
            types: walk.types,
            declarations,
        })
    }

    /// Returns the application crate, which is the crate of the first
    /// present root in the order Query, Mutation, Subscription.
    #[must_use]
    pub fn app_crate(&self) -> Option<&str> {
        self.krate.as_deref()
    }

    /// Returns whether the declarations hold the one of the first present
    /// root.
    ///
    /// The roots reach every type through links, and the declarations come
    /// from the list the build script of the application crate writes. A root
    /// whose own declaration is missing tells a build that the crate has no
    /// such build script. The declaration of a struct links it, and the one of
    /// a `graphql!` fragment, which declares no type, stands at the source key
    /// of the fragment, which the node of its root carries, as the scan of the
    /// build script lists a `graphql!` at the top level of a module. The node
    /// of a merged root carries the source key of the first fragment or struct
    /// it reaches.
    #[must_use]
    pub fn has_declaration_of_root(&self) -> bool {
        let Some(root) = self.roots.iter().flatten().next() else {
            return false;
        };
        let fragment_source = usize::try_from(*root)
            .ok()
            .and_then(|index| self.types.get(index))
            .and_then(|node| match &node.expr {
                Expr::Named(named) if named.fragment.is_some() => Some(&named.source),
                _ => None,
            });

        self.declarations.iter().any(|declaration| {
            declaration.link == Some(*root) || Some(&declaration.source) == fragment_source
        })
    }

    /// Returns the dump of the graph as a JavaScript value.
    #[must_use]
    pub fn to_js_value(&self) -> JsValue {
        let mut roots = JsObject::new();
        for (name, root) in ["Query", "Mutation", "Subscription"]
            .into_iter()
            .zip(self.roots)
        {
            roots.insert(name, root.map_or(JsValue::Null, JsValue::from));
        }

        let mut object = JsObject::new();
        object.insert("version", 1_u32);
        object.insert(
            "crate",
            self.krate.as_deref().map_or(JsValue::Null, JsValue::from),
        );
        object.insert("roots", roots);
        object.insert(
            "types",
            self.types.iter().map(write_node).collect::<Vec<JsValue>>(),
        );
        object.insert(
            "declarations",
            self.declarations
                .iter()
                .map(write_declaration)
                .collect::<Vec<JsValue>>(),
        );

        JsValue::Object(object)
    }

    /// Reads a graph back from its dump.
    ///
    /// The keys may stand in any order. Every node position is checked
    /// against the length of `types`, so a graph that was read holds no
    /// dangling position.
    ///
    /// # Errors
    ///
    /// Returns a [`GraphError`] with the path of the first value that has the
    /// wrong type, is missing, names no node or is a string with a lone
    /// surrogate, and for a `version` other than `1`.
    pub fn from_js_value(value: &JsValue) -> Result<Self, GraphError> {
        let object = read_object(value, "")?;

        if !matches!(object.get("version"), Some(JsValue::Number(version)) if version.to_bits() == 1.0_f64.to_bits())
        {
            return Err(expected("version", "the number 1"));
        }

        let nodes = read_array(read_member(object, "", "types")?, "types")?;
        let reader = Reader {
            count: position(nodes.len()),
        };

        let krate = Reader::optional_string(read_member(object, "", "crate")?, "crate")?;
        let roots_object = read_object(read_member(object, "", "roots")?, "roots")?;
        let mut roots = [None; 3];
        for (root, name) in roots.iter_mut().zip(["Query", "Mutation", "Subscription"]) {
            let path = format!("roots.{name}");
            *root = reader.optional_index(read_member(roots_object, "roots", name)?, &path)?;
        }

        let types = nodes
            .iter()
            .enumerate()
            .map(|(i, node)| reader.node(node, &format!("types[{i}]")))
            .collect::<Result<Vec<Node>, GraphError>>()?;

        let declarations = read_array(read_member(object, "", "declarations")?, "declarations")?
            .iter()
            .enumerate()
            .map(|(i, declaration)| reader.declaration(declaration, &format!("declarations[{i}]")))
            .collect::<Result<Vec<DeclarationNode>, GraphError>>()?;

        Ok(Self {
            krate,
            roots,
            types,
            declarations,
        })
    }
}

/// Returns the dump of a source position.
fn write_source(source: &SourcePosition) -> JsValue {
    let mut object = JsObject::new();

    object.insert("file", source.file.as_str());
    object.insert("line", source.line);
    object.insert("column", source.column);

    JsValue::Object(object)
}

/// Returns the dump word of a visibility.
fn write_visibility(is_private: bool) -> &'static str {
    if is_private { "private" } else { "public" }
}

/// Returns the dump of a list of node positions.
fn write_indices(indices: &[u32]) -> Vec<JsValue> {
    indices.iter().map(|index| JsValue::from(*index)).collect()
}

/// Returns the dump of a heritage list.
fn write_heritage(heritage: &[HeritageNode]) -> Vec<JsValue> {
    heritage
        .iter()
        .map(|entry| {
            let mut object = JsObject::new();

            if let Some(field) = &entry.embedded {
                object.insert("kind", "embedded");
                object.insert("field", field.as_str());
            } else {
                let kind = if entry.is_extends {
                    "extends"
                } else {
                    "implements"
                };
                object.insert("kind", kind);
            }
            object.insert("base", entry.base);

            JsValue::Object(object)
        })
        .collect()
}

/// Writes a documentation record into `object`, the comment under `docs` and
/// the tags under `tags`.
fn write_docs(object: &mut JsObject, docs: &DocsNode) {
    object.insert("docs", docs.comment.as_str());
    object.insert(
        "tags",
        docs.tags
            .iter()
            .map(|tag| {
                let mut object = JsObject::new();

                object.insert("name", tag.name.as_str());
                object.insert("text", tag.text.as_str());

                JsValue::Object(object)
            })
            .collect::<Vec<JsValue>>(),
    );
}

/// Returns the dump of a site, `{"file", "line", "docs", "blocks"}`.
fn write_site(site: &SiteNode) -> JsValue {
    let mut object = JsObject::new();

    object.insert("file", site.file.as_str());
    object.insert("line", site.line);
    object.insert(
        "docs",
        site.docs
            .iter()
            .map(|line| JsValue::from(line.as_str()))
            .collect::<Vec<JsValue>>(),
    );
    object.insert("blocks", site.blocks);

    JsValue::Object(object)
}

/// Returns the dump of the parameters of a method or of a callable.
fn write_params(params: &[ParamNode]) -> Vec<JsValue> {
    params
        .iter()
        .map(|param| {
            let mut object = JsObject::new();

            object.insert("name", param.name.as_str());
            object.insert("type", param.r#type);
            object.insert("optional", param.optional);
            write_docs(&mut object, &param.docs);
            object.insert("site", write_site(&param.site));

            JsValue::Object(object)
        })
        .collect()
}

/// Returns the dump of a method.
fn write_method(method: &MethodNode) -> JsValue {
    let mut object = JsObject::new();

    object.insert("name", method.name.as_str());
    write_docs(&mut object, &method.docs);
    object.insert("site", write_site(&method.site));
    object.insert("visibility", write_visibility(method.is_private));
    object.insert("isAsync", method.is_async);
    object.insert("params", write_params(&method.params));
    object.insert("returns", method.returns);
    if let Some(entry) = &method.entry {
        let mut fact = JsObject::new();
        fact.insert("kind", entry.kind);
        fact.insert("mutation", entry.mutation);
        object.insert("entry", fact);
    }
    if !method.decorators.is_empty() {
        object.insert(
            "decorators",
            method
                .decorators
                .iter()
                .map(|decorator| {
                    let mut object = JsObject::new();
                    object.insert("text", decorator.text.as_str());
                    object.insert("line", decorator.line);
                    JsValue::Object(object)
                })
                .collect::<Vec<JsValue>>(),
        );
    }
    if !method.guards.is_empty() {
        object.insert(
            "guards",
            method
                .guards
                .iter()
                .map(AuthoringGuard::to_js_value)
                .collect::<Vec<JsValue>>(),
        );
    }

    JsValue::Object(object)
}

/// Writes the keys of a named node into `object`, in the field order of the
/// `TypeInfo` descriptor.
fn write_named(object: &mut JsObject, named: &NamedNode) {
    object.insert(
        "kind",
        match named.kind {
            NamedKind::Object => "object",
            NamedKind::Enum => "enum",
            NamedKind::EnumDeclaration => "enumDeclaration",
            NamedKind::Union => "union",
            NamedKind::Scalar => "scalar",
        },
    );
    object.insert("name", named.name.as_str());
    object.insert("krate", named.krate.as_str());
    object.insert("source", write_source(&named.source));
    write_docs(object, &named.docs);
    object.insert("site", write_site(&named.site));
    object.insert(
        "typename",
        named
            .typename
            .as_deref()
            .map_or(JsValue::Null, JsValue::from),
    );
    object.insert("anonymous", named.anonymous);
    object.insert(
        "fields",
        named
            .fields
            .iter()
            .map(|field| {
                let mut object = JsObject::new();

                object.insert("name", field.name.as_str());
                write_docs(&mut object, &field.docs);
                object.insert("site", write_site(&field.site));
                object.insert("visibility", write_visibility(field.is_private));
                object.insert("type", field.r#type);
                if let Some(spread) = &field.spread {
                    let mut origin = JsObject::new();
                    origin.insert("field", spread.field.as_str());
                    origin.insert("site", write_site(&spread.site));
                    object.insert("spread", origin);
                }

                JsValue::Object(object)
            })
            .collect::<Vec<JsValue>>(),
    );
    object.insert(
        "methods",
        named
            .methods
            .iter()
            .map(write_method)
            .collect::<Vec<JsValue>>(),
    );
    object.insert(
        "variants",
        named
            .variants
            .iter()
            .map(|variant| {
                let mut object = JsObject::new();

                object.insert("name", variant.name.as_str());
                object.insert("value", variant.value.as_str());
                write_docs(&mut object, &variant.docs);
                object.insert("site", write_site(&variant.site));

                JsValue::Object(object)
            })
            .collect::<Vec<JsValue>>(),
    );
    object.insert("members", write_indices(&named.members));
    object.insert("typeArguments", write_indices(&named.type_arguments));
    object.insert("heritage", write_heritage(&named.heritage));
    if let Some(fragment) = &named.fragment {
        object.insert("fragment", fragment.as_str());
    }
}

/// Returns the dump of a node.
fn write_node(node: &Node) -> JsValue {
    let mut object = JsObject::new();
    let (expr, of) = match &node.expr {
        Expr::Primitive(_) => ("primitive", None),
        Expr::Void => ("void", None),
        Expr::Any => ("any", None),
        Expr::Never => ("never", None),
        Expr::Nullable(of) => ("nullable", Some(*of)),
        Expr::List(of) => ("list", Some(*of)),
        Expr::Promise(of) => ("promise", Some(*of)),
        Expr::Repeater(of) => ("repeater", Some(*of)),
        Expr::Named(_) => ("named", None),
        Expr::Callable(_) => ("callable", None),
        Expr::Intersection(_) => ("intersection", None),
    };

    object.insert("expr", expr);
    object.insert("rust", node.rust.as_str());

    if let Some(of) = of {
        object.insert("of", of);
    }

    match &node.expr {
        Expr::Primitive(primitive) => {
            object.insert(
                "primitive",
                match primitive {
                    PrimitiveKind::String => "string",
                    PrimitiveKind::Number => "number",
                    PrimitiveKind::Boolean => "boolean",
                },
            );
        }
        Expr::Named(named) => write_named(&mut object, named),
        Expr::Callable(callable) => {
            object.insert("params", write_params(&callable.params));
            object.insert("returns", callable.returns);
        }
        Expr::Intersection(members) => {
            object.insert("members", write_indices(members));
        }
        _ => {}
    }

    JsValue::Object(object)
}

/// Returns the dump of a declaration.
fn write_declaration(declaration: &DeclarationNode) -> JsValue {
    let mut object = JsObject::new();

    object.insert("krate", declaration.krate.as_str());
    object.insert("source", write_source(&declaration.source));
    object.insert("name", declaration.name.as_str());
    object.insert(
        "link",
        declaration.link.map_or(JsValue::Null, JsValue::from),
    );
    object.insert("heritage", write_heritage(&declaration.heritage));

    JsValue::Object(object)
}

/// Builds the error of a value at `path` that is not `expected`.
fn expected(path: &str, expected: &'static str) -> GraphError {
    GraphError {
        path: String::from(path),
        expected,
    }
}

/// Returns `path` extended by the key `key`.
fn join(path: &str, key: &str) -> String {
    if path.is_empty() {
        return String::from(key);
    }

    format!("{path}.{key}")
}

/// Reads `value` as an object.
fn read_object<'a>(value: &'a JsValue, path: &str) -> Result<&'a JsObject, GraphError> {
    match value {
        JsValue::Object(object) => Ok(object),
        _ => Err(expected(path, "an object")),
    }
}

/// Reads `value` as an array.
fn read_array<'a>(value: &'a JsValue, path: &str) -> Result<&'a [JsValue], GraphError> {
    match value {
        JsValue::Array(array) => Ok(array),
        _ => Err(expected(path, "an array")),
    }
}

/// Reads the member `key` of `object`, which has to exist.
fn read_member<'a>(object: &'a JsObject, path: &str, key: &str) -> Result<&'a JsValue, GraphError> {
    object
        .get(key)
        .ok_or_else(|| expected(&join(path, key), "a value"))
}

/// Reads `value` as a well formed string.
fn read_string(value: &JsValue, path: &str) -> Result<String, GraphError> {
    match value {
        JsValue::String(string) => string
            .as_str()
            .map(String::from)
            .ok_or_else(|| expected(path, "a well formed string")),
        _ => Err(expected(path, "a string")),
    }
}

/// Reads `value` as a boolean.
fn read_bool(value: &JsValue, path: &str) -> Result<bool, GraphError> {
    match value {
        JsValue::Bool(boolean) => Ok(*boolean),
        _ => Err(expected(path, "a boolean")),
    }
}

/// Reads `value` as an integer in the range of a `u32`.
fn read_u32(value: &JsValue, path: &str) -> Result<u32, GraphError> {
    let JsValue::Number(number) = value else {
        return Err(expected(path, "an unsigned integer"));
    };

    if !(0.0..=f64::from(u32::MAX)).contains(number) || number.fract() > 0.0 {
        return Err(expected(path, "an unsigned integer"));
    }

    // The range test and the integer test above leave a whole number the
    // target holds, so the cast drops nothing.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the value is a whole number in the range of the target"
    )]
    let integer = *number as u32;

    Ok(integer)
}

/// Reads the members of a dump, checking every node position against the
/// number of nodes.
struct Reader {
    /// The number of nodes of the dump.
    count: u32,
}

impl Reader {
    /// Reads `value` as the position of a node.
    fn index(&self, value: &JsValue, path: &str) -> Result<u32, GraphError> {
        let index = read_u32(value, path).map_err(|_| expected(path, "a position in types"))?;

        if index >= self.count {
            return Err(expected(path, "a position in types"));
        }

        Ok(index)
    }

    /// Reads `value` as the position of a node or as `null`.
    fn optional_index(&self, value: &JsValue, path: &str) -> Result<Option<u32>, GraphError> {
        match value {
            JsValue::Null => Ok(None),
            _ => self.index(value, path).map(Some),
        }
    }

    /// Reads `value` as a string or as `null`.
    fn optional_string(value: &JsValue, path: &str) -> Result<Option<String>, GraphError> {
        match value {
            JsValue::Null => Ok(None),
            _ => read_string(value, path).map(Some),
        }
    }

    /// Reads the member `key` of `object` as a list and maps each element
    /// with `read`, which receives the element and its path.
    fn list<T>(
        object: &JsObject,
        path: &str,
        key: &str,
        read: impl Fn(&JsValue, &str) -> Result<T, GraphError>,
    ) -> Result<Vec<T>, GraphError> {
        let path = join(path, key);

        read_array(
            object.get(key).ok_or_else(|| expected(&path, "a value"))?,
            &path,
        )?
        .iter()
        .enumerate()
        .map(|(i, element)| read(element, &format!("{path}[{i}]")))
        .collect()
    }

    /// Reads a source position.
    fn source(object: &JsObject, path: &str) -> Result<SourcePosition, GraphError> {
        let path = join(path, "source");
        let source = read_object(
            object
                .get("source")
                .ok_or_else(|| expected(&path, "a value"))?,
            &path,
        )?;

        Ok(SourcePosition {
            file: read_string(read_member(source, &path, "file")?, &join(&path, "file"))?,
            line: read_u32(read_member(source, &path, "line")?, &join(&path, "line"))?,
            column: read_u32(
                read_member(source, &path, "column")?,
                &join(&path, "column"),
            )?,
        })
    }

    /// Reads the member `key` of `object` as a string.
    fn string(object: &JsObject, path: &str, key: &str) -> Result<String, GraphError> {
        read_string(read_member(object, path, key)?, &join(path, key))
    }

    /// Reads the documentation record of `object`, the comment under `docs`
    /// and the tags under `tags`.
    ///
    /// A dump written before the tags and the sites existed has neither
    /// `tags` nor `site`, and reads as an item with no tag.
    fn docs(object: &JsObject, path: &str) -> Result<DocsNode, GraphError> {
        let tags = if object.get("tags").is_some() {
            Self::list(object, path, "tags", |value, path| {
                let tag = read_object(value, path)?;

                Ok(TagNode {
                    name: Self::string(tag, path, "name")?,
                    text: Self::string(tag, path, "text")?,
                })
            })?
        } else {
            Vec::new()
        };

        Ok(DocsNode {
            comment: Self::string(object, path, "docs")?,
            tags,
        })
    }

    /// Reads the member `site` of `object` as a site record, and an absent
    /// one as the empty site of a dump written before sites existed.
    fn site(object: &JsObject, path: &str) -> Result<SiteNode, GraphError> {
        let Some(site) = object.get("site") else {
            return Ok(SiteNode::default());
        };
        let path = join(path, "site");
        let site = read_object(site, &path)?;

        Ok(SiteNode {
            file: Self::string(site, &path, "file")?,
            line: read_u32(read_member(site, &path, "line")?, &join(&path, "line"))?,
            docs: Self::list(site, &path, "docs", read_string)?,
            blocks: read_u32(read_member(site, &path, "blocks")?, &join(&path, "blocks"))?,
        })
    }

    /// Reads the member `key` of `object` as a boolean.
    fn boolean(object: &JsObject, path: &str, key: &str) -> Result<bool, GraphError> {
        read_bool(read_member(object, path, key)?, &join(path, key))
    }

    /// Reads the member `key` of `object` as the position of a node.
    fn member_index(&self, object: &JsObject, path: &str, key: &str) -> Result<u32, GraphError> {
        self.index(read_member(object, path, key)?, &join(path, key))
    }

    /// Reads the member `visibility` of `object` and returns whether it says
    /// private.
    fn is_private(object: &JsObject, path: &str) -> Result<bool, GraphError> {
        match Self::string(object, path, "visibility")?.as_str() {
            "public" => Ok(false),
            "private" => Ok(true),
            _ => Err(expected(
                &join(path, "visibility"),
                "one of \"public\" and \"private\"",
            )),
        }
    }

    /// Reads a heritage list.
    fn heritage(&self, object: &JsObject, path: &str) -> Result<Vec<HeritageNode>, GraphError> {
        Self::list(object, path, "heritage", |value, path| {
            let entry = read_object(value, path)?;
            let (is_extends, embedded) = match Self::string(entry, path, "kind")?.as_str() {
                "extends" => (true, None),
                "implements" => (false, None),
                "embedded" => (true, Some(Self::string(entry, path, "field")?)),
                _ => {
                    return Err(expected(
                        &join(path, "kind"),
                        "one of \"extends\", \"implements\" and \"embedded\"",
                    ));
                }
            };

            Ok(HeritageNode {
                is_extends,
                embedded,
                base: self.member_index(entry, path, "base")?,
            })
        })
    }

    /// Reads the parameters of a method or of a callable.
    fn params(&self, object: &JsObject, path: &str) -> Result<Vec<ParamNode>, GraphError> {
        Self::list(object, path, "params", |value, path| {
            let param = read_object(value, path)?;

            Ok(ParamNode {
                name: Self::string(param, path, "name")?,
                r#type: self.member_index(param, path, "type")?,
                optional: Self::boolean(param, path, "optional")?,
                // A parameter carried no documentation before the
                // parameter comments existed.
                docs: if param.get("docs").is_some() {
                    Self::docs(param, path)?
                } else {
                    DocsNode::default()
                },
                site: Self::site(param, path)?,
            })
        })
    }

    /// Reads the spread a field came through, which a field the struct
    /// declares does not carry.
    fn spread(field: &JsObject, path: &str) -> Result<Option<SpreadNode>, GraphError> {
        let Some(value) = field.get("spread") else {
            return Ok(None);
        };
        let path = join(path, "spread");
        let spread = read_object(value, &path)?;

        Ok(Some(SpreadNode {
            field: Self::string(spread, &path, "field")?,
            site: Self::site(spread, &path)?,
        }))
    }

    /// Reads a method.
    fn method(&self, value: &JsValue, path: &str) -> Result<MethodNode, GraphError> {
        let method = read_object(value, path)?;

        Ok(MethodNode {
            name: Self::string(method, path, "name")?,
            docs: Self::docs(method, path)?,
            site: Self::site(method, path)?,
            is_private: Self::is_private(method, path)?,
            is_async: Self::boolean(method, path, "isAsync")?,
            params: self.params(method, path)?,
            returns: self.member_index(method, path, "returns")?,
            entry: match method.get("entry") {
                None => None,
                Some(value) => Some(Self::entry(value, &join(path, "entry"))?),
            },
            decorators: if method.get("decorators").is_some() {
                Self::list(method, path, "decorators", |value, path| {
                    let decorator = read_object(value, path)?;
                    Ok(DecoratorNode {
                        text: Self::string(decorator, path, "text")?,
                        line: read_u32(read_member(decorator, path, "line")?, &join(path, "line"))?,
                    })
                })?
            } else {
                Vec::new()
            },
            guards: if method.get("guards").is_some() {
                Self::list(method, path, "guards", Self::guard)?
            } else {
                Vec::new()
            },
        })
    }

    /// Reads one guard of a method.
    fn guard(value: &JsValue, path: &str) -> Result<AuthoringGuard, GraphError> {
        let guard = read_object(value, path)?;
        let kind = match Self::string(guard, path, "kind")?.as_str() {
            "require_role" => "require_role",
            "authorize" => "authorize",
            _ => {
                return Err(expected(
                    &join(path, "kind"),
                    "one of \"require_role\" and \"authorize\"",
                ));
            }
        };
        let args = match read_member(guard, path, "args")? {
            JsValue::Null => None,
            _ => Some(Self::list(guard, path, "args", read_string)?),
        };

        Ok(AuthoringGuard {
            kind,
            args,
            calls: Self::list(guard, path, "calls", |value, path| {
                let call = read_object(value, path)?;
                Ok(AuthoringGuardCall {
                    name: Self::string(call, path, "name")?,
                    args: Self::list(call, path, "args", read_string)?,
                })
            })?,
            others: read_u32(read_member(guard, path, "others")?, &join(path, "others"))?,
            line: read_u32(read_member(guard, path, "line")?, &join(path, "line"))?,
        })
    }

    /// Reads the fact of an entry of `graphql!`.
    fn entry(value: &JsValue, path: &str) -> Result<EntryNode, GraphError> {
        let entry = read_object(value, path)?;
        let kind = match Self::string(entry, path, "kind")?.as_str() {
            "inline" => "inline",
            "reference" => "reference",
            "shorthand" => "shorthand",
            _ => {
                return Err(expected(
                    &join(path, "kind"),
                    "one of \"inline\", \"reference\" and \"shorthand\"",
                ));
            }
        };
        Ok(EntryNode {
            kind,
            mutation: Self::boolean(entry, path, "mutation")?,
        })
    }

    /// Reads the keys of a named node.
    fn named(&self, object: &JsObject, path: &str) -> Result<NamedNode, GraphError> {
        let kind = match Self::string(object, path, "kind")?.as_str() {
            "object" => NamedKind::Object,
            "enum" => NamedKind::Enum,
            "enumDeclaration" => NamedKind::EnumDeclaration,
            "union" => NamedKind::Union,
            "scalar" => NamedKind::Scalar,
            _ => {
                return Err(expected(
                    &join(path, "kind"),
                    "one of \"object\", \"enum\", \"enumDeclaration\", \"union\" and \"scalar\"",
                ));
            }
        };

        Ok(NamedNode {
            kind,
            name: Self::string(object, path, "name")?,
            krate: Self::string(object, path, "krate")?,
            source: Self::source(object, path)?,
            docs: Self::docs(object, path)?,
            site: Self::site(object, path)?,
            typename: Self::optional_string(
                read_member(object, path, "typename")?,
                &join(path, "typename"),
            )?,
            anonymous: Self::boolean(object, path, "anonymous")?,
            fields: Self::list(object, path, "fields", |value, path| {
                let field = read_object(value, path)?;

                Ok(FieldNode {
                    name: Self::string(field, path, "name")?,
                    docs: Self::docs(field, path)?,
                    site: Self::site(field, path)?,
                    is_private: Self::is_private(field, path)?,
                    r#type: self.member_index(field, path, "type")?,
                    spread: Self::spread(field, path)?,
                })
            })?,
            methods: Self::list(object, path, "methods", |value, path| {
                self.method(value, path)
            })?,
            variants: Self::list(object, path, "variants", |value, path| {
                let variant = read_object(value, path)?;

                Ok(VariantNode {
                    name: Self::string(variant, path, "name")?,
                    value: Self::string(variant, path, "value")?,
                    docs: Self::docs(variant, path)?,
                    site: Self::site(variant, path)?,
                })
            })?,
            members: Self::list(object, path, "members", |value, path| {
                self.index(value, path)
            })?,
            type_arguments: Self::list(object, path, "typeArguments", |value, path| {
                self.index(value, path)
            })?,
            heritage: self.heritage(object, path)?,
            fragment: match object.get("fragment") {
                None => None,
                Some(value) => Some(read_string(value, &join(path, "fragment"))?),
            },
        })
    }

    /// Reads a node.
    fn node(&self, value: &JsValue, path: &str) -> Result<Node, GraphError> {
        let object = read_object(value, path)?;
        let of = |key: &str| self.member_index(object, path, key);

        let expr = match Self::string(object, path, "expr")?.as_str() {
            "primitive" => {
                Expr::Primitive(match Self::string(object, path, "primitive")?.as_str() {
                    "string" => PrimitiveKind::String,
                    "number" => PrimitiveKind::Number,
                    "boolean" => PrimitiveKind::Boolean,
                    _ => {
                        return Err(expected(
                            &join(path, "primitive"),
                            "one of \"string\", \"number\" and \"boolean\"",
                        ));
                    }
                })
            }
            "void" => Expr::Void,
            "any" => Expr::Any,
            "never" => Expr::Never,
            "nullable" => Expr::Nullable(of("of")?),
            "list" => Expr::List(of("of")?),
            "promise" => Expr::Promise(of("of")?),
            "repeater" => Expr::Repeater(of("of")?),
            "named" => Expr::Named(Box::new(self.named(object, path)?)),
            "callable" => Expr::Callable(Box::new(CallableNode {
                params: self.params(object, path)?,
                returns: of("returns")?,
            })),
            "intersection" => {
                Expr::Intersection(Self::list(object, path, "members", |value, path| {
                    self.index(value, path)
                })?)
            }
            _ => {
                return Err(expected(&join(path, "expr"), "the name of a node kind"));
            }
        };

        Ok(Node {
            rust: Self::string(object, path, "rust")?,
            expr,
        })
    }

    /// Reads a declaration.
    fn declaration(&self, value: &JsValue, path: &str) -> Result<DeclarationNode, GraphError> {
        let object = read_object(value, path)?;

        Ok(DeclarationNode {
            krate: Self::string(object, path, "krate")?,
            source: Self::source(object, path)?,
            name: Self::string(object, path, "name")?,
            link: self.optional_index(read_member(object, path, "link")?, &join(path, "link"))?,
            heritage: self.heritage(object, path)?,
        })
    }
}

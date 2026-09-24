// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the depth first walk that applies the edits its hooks answer.
 *
 * Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
 */

//! The walk of the language layer that edits a tree, keeping, removing or
//! replacing each node a hook answers for.
//!
//! [`visit_edit`] walks a document depth first in the order of the
//! `QueryDocumentKeys` table, as [󰀂](crate::visitor::visit) does, and a hook
//! of an [`EditVisitor`] answers an [`Edit`]. A hook that answers
//! [`Edit::Remove`] deletes its node from the list that holds it, or clears
//! the optional property, and one that answers [`Edit::Replace`] puts another
//! node in its place. A replacement on enter is walked in place of the node,
//! so its children meet the hooks. The original tree is never modified, and
//! the walk returns the edited tree. [`visit_edit_with_keys`] walks any node
//! with a table of keys of its own, the third parameter `visitorKeys` of the
//! reference, which the delegation of Federation passes to walk a part of a
//! document.
//!
//! # The shape of the port
//!
//! The reference holds the edits of each level of the walk in a list of
//! `[key, value]` pairs and applies them when it leaves the level, to a
//! shallow copy of the node or of the array the level walks. A node here owns
//! its children, so a copy of a node clones the node with every child it
//! keeps, its `loc` included, and then puts each edited child in its place. A
//! level that records no edit copies nothing, and the walk clones the root
//! once where no hook edits.
//!
//! The reference walks with an explicit stack and descends into a
//! replacement it holds by reference. A replacement here is a value the walk
//! owns while it visits the children of that value, so the walk descends by
//! recursion, one call per level, as the printer folds the tree (see
//! [`crate::printer`]). The depth of the recursion is the depth of the tree.
//!
//! A hook of the reference may answer any value, and the walk stores it under
//! the key whatever the node there may hold. A property of a node here holds
//! one type, so an edit that offers a value its property cannot hold, a node
//! of another kind or `null` for a property that is not optional, ends the
//! walk with an [`EditError`], where the reference stores the value and
//! builds a tree of a shape `ast.ts` does not describe.
//!
//! A hook that answers [`Edit::Break`] stops the walk, and the reference then
//! returns the last edit of the level it stopped in, or the original root
//! where that level recorded none. [`visit_edit_with_keys`] keeps that answer
//! where it is a node or a removal, and [`visit_edit`] keeps it only where it
//! is a document. Each ends with [`EditError::Root`] otherwise.

use alloc::{
    boxed::Box,
    vec::Vec, //
};
use core::fmt;

use crate::{
    SharedSlice,
    ast::{
        ArgumentNode,
        AstNode,
        DefinitionNode,
        DirectiveNode,
        DocumentNode,
        EnumValueDefinitionNode,
        FieldDefinitionNode,
        InputValueDefinitionNode,
        NameNode,
        NamedTypeNode,
        NullableTypeNode,
        ObjectFieldNode,
        OperationTypeDefinitionNode,
        SelectionNode,
        SelectionSetNode,
        StringValueNode,
        TypeNode,
        ValueNode,
        VariableDefinitionNode,
        VariableNode, //
    },
    kinds::Kind,
    visitor::{
        Key,
        ListRef,
        NodeOrList,
        NodeRef,
        child_of,
        query_document_keys, //
    }, //
};

/// What a hook answers: keep the node, skip it, stop the walk, remove the
/// node from its list, or replace it.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
#[derive(Debug, Clone, PartialEq)]
pub enum Edit {
    /// No action, the `undefined` of the reference.
    Keep,
    /// On enter, skip the children of this node and its leave hook, the
    /// `false` of the reference. On leave, no action, and the edits of the
    /// children of the node are dropped, as the reference records none for a
    /// leave that answers `false`.
    Skip,
    /// Stop the whole walk, the `BREAK` of the reference.
    Break,
    /// Delete this node from the list that holds it, or clear the optional
    /// property that holds it, the `null` of the reference.
    Remove,
    /// Replace this node with the given one, which the walk visits in its
    /// place when the hook is an enter hook.
    ///
    /// The node is boxed, because a node takes several hundred bytes and
    /// every other answer none.
    Replace(Box<AstNode>),
}

impl Edit {
    /// Returns the answer that replaces the node with `node`.
    pub fn replace(node: impl Into<AstNode>) -> Self {
        Self::Replace(Box::new(node.into()))
    }
}

/// The property names of the children of each node kind the walk visits, in
/// the order it visits them.
///
/// The default is [`query_document_keys`], and a kind the table answers with
/// no name has no children to the walk. A name the kind does not hold reads
/// as an absent child, as `parent[key]` reads `undefined`.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
pub type VisitorKeys = fn(Kind) -> &'static [&'static str];

/// The position of the visited node, the four trailing arguments of a hook.
///
/// The parent and the ancestors are the nodes and the arrays the walk
/// descended through, the replacement of an enter hook in place of the node
/// it replaced, and never a copy the edits of their children made.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
#[derive(Debug, Clone, Copy)]
pub struct EditContext<'w> {
    /// The index or key to this node from the parent node or array, absent
    /// at the root.
    pub key: Option<Key>,
    /// The parent immediately above this node, which may be an array, absent
    /// at the root.
    pub parent: Option<NodeOrList<'w>>,
    /// The key path to get to this node from the root node.
    pub path: &'w [Key],
    /// All nodes and arrays visited before reaching the parent of this node,
    /// which includes the arrays that hold the parent.
    pub ancestors: Ancestors<'w>,
}

/// The nodes and arrays above the parent of a visited node, the `ancestors`
/// array of the reference.
///
/// The walk keeps one link of the chain on the stack of each level it
/// descends into, so a hook reads the chain from its nearest end.
#[derive(Debug, Clone, Copy, Default)]
pub struct Ancestors<'w> {
    /// The nearest ancestor, the last entry of the array of the reference.
    last: Option<&'w Ancestor<'w>>,
}

/// One link of [`Ancestors`].
#[derive(Debug)]
struct Ancestor<'w> {
    /// The node or the array.
    node: NodeOrList<'w>,
    /// The number of ancestors up to and including this one.
    depth: usize,
    /// The ancestor above this one.
    up: Option<&'w Ancestor<'w>>,
}

impl<'w> Ancestor<'w> {
    /// Returns the link that puts `node` below the chain `up`.
    fn new(node: NodeOrList<'w>, up: Ancestors<'w>) -> Self {
        Self {
            node,
            depth: up.len() + 1,
            up: up.last,
        }
    }
}

impl<'w> Ancestors<'w> {
    /// Returns the chain whose nearest end is `link`.
    const fn ending_at(link: &'w Ancestor<'w>) -> Self {
        Self { last: Some(link) }
    }

    /// Returns how many ancestors there are.
    #[must_use]
    pub fn len(self) -> usize {
        self.last.map_or(0, |link| link.depth)
    }

    /// Returns whether there is no ancestor, which holds at the root and at
    /// the children of the root.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.last.is_none()
    }

    /// Returns the ancestor at `index`, counted from the root as the array of
    /// the reference counts.
    #[must_use]
    pub fn get(self, index: usize) -> Option<NodeOrList<'w>> {
        let steps = self.len().checked_sub(index + 1)?;
        self.iter().nth(steps)
    }

    /// Iterates over the ancestors from the nearest to the root.
    pub fn iter(self) -> impl Iterator<Item = NodeOrList<'w>> {
        core::iter::successors(self.last, |link| link.up).map(|link| link.node)
    }
}

/// The hooks of a walk that edits, called at every node the walk reaches.
///
/// A hook for one kind matches on the [`NodeRef`] it receives, which is what
/// `getEnterLeaveForKind` resolves for a visitor of the kind form. The leave
/// hook receives the node with the edits of its children applied.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
pub trait EditVisitor {
    /// Enters `node`, before its children.
    ///
    /// Answers [`Edit::Keep`] by default, which visits the children.
    fn enter(&mut self, _node: NodeRef<'_>, _context: &EditContext<'_>) -> Edit {
        Edit::Keep
    }

    /// Leaves `node`, after its children.
    ///
    /// Answers [`Edit::Keep`] by default, which keeps the node as the edits
    /// of its children left it.
    fn leave(&mut self, _node: NodeRef<'_>, _context: &EditContext<'_>) -> Edit {
        Edit::Keep
    }
}

/// What an edit offers a property of a node.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offered {
    /// A node of the kind.
    Node(Kind),
    /// No node, the `null` of the reference.
    Null,
    /// An array of nodes.
    List,
}

impl fmt::Display for Offered {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Node(kind) => write!(f, "a node of the kind {kind}"),
            Self::Null => f.write_str("null"),
            Self::List => f.write_str("a list"),
        }
    }
}

/// Why a walk that edits ends without a tree.
///
/// The reference stores any value an edit offers, so neither error has a
/// message of the reference, and the texts are Aosabi's own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditError {
    /// An edit offers a value the property of its parent cannot hold.
    Property {
        /// The kind of the node whose property the edit writes.
        parent: Kind,
        /// The property, or the index into the list the property holds.
        key: Key,
        /// What the edit offers.
        offered: Offered,
    },
    /// The walk ends on a value of another shape than its root.
    Root {
        /// The kind of the root the walk was given.
        root: Kind,
        /// What the walk ends on.
        offered: Offered,
    },
}

impl fmt::Display for EditError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Property {
                parent,
                key: Key::Name(name),
                offered,
            } => write!(
                f,
                "The edit of `{name}` of a node of the kind {parent} offers {offered}, which that property cannot hold."
            ),
            Self::Property {
                parent,
                key: Key::Index(index),
                offered,
            } => write!(
                f,
                "The edit of element {index} of a list of a node of the kind {parent} offers {offered}, which that list cannot hold."
            ),
            Self::Root { root, offered } => write!(
                f,
                "The walk of a node of the kind {root} ends on {offered}, which is no node of that kind."
            ),
        }
    }
}

/// Walks a document depth first, calling the visitor's enter hook at each
/// node and its leave hook after the children of that node, and returns the
/// document with the edits the hooks answer applied.
///
/// `root` is the document and `visitor` receives the hooks. The children of a
/// node are visited in the order of [`query_document_keys`].
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
///
/// # Errors
///
/// Returns the [`EditError`] of an edit that offers a value its property
/// cannot hold. Returns [`EditError::Root`] when a hook removes the document
/// or replaces it with a node of another kind, and when a hook answers
/// [`Edit::Break`] on a level whose last edit is a list, a removal or a node
/// of another kind.
pub fn visit_edit<V>(root: &DocumentNode, visitor: &mut V) -> Result<DocumentNode, EditError>
where
    V: EditVisitor + ?Sized,
{
    let offered = match visit_edit_with_keys(root, visitor, query_document_keys)? {
        Some(AstNode::Document(document)) => return Ok(document),
        Some(node) => Offered::Node(node.kind()),
        None => Offered::Null,
    };
    Err(EditError::Root {
        root: Kind::Document,
        offered,
    })
}

/// Walks any node depth first through the children `visitor_keys` names for
/// each kind, and returns the node with the edits the hooks answer applied.
///
/// `root` is the node and `visitor` receives the hooks. The walk answers
/// `None` where a hook removed the root, which the reference answers as
/// `null`, and otherwise the root with its edits or what a hook put in its
/// place.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
///
/// # Errors
///
/// Returns the [`EditError`] of an edit that offers a value its property
/// cannot hold, and [`EditError::Root`] when the walk stops on a list, which
/// the reference returns as the edited array.
pub fn visit_edit_with_keys<'a, V>(
    root: impl Into<NodeRef<'a>>,
    visitor: &mut V,
    visitor_keys: VisitorKeys,
) -> Result<Option<AstNode>, EditError>
where
    V: EditVisitor + ?Sized,
{
    let root = root.into();
    let mut walk = Walk {
        visitor,
        visitor_keys,
        path: Vec::new(),
    };
    let mut edits = Edits::new();
    let last = match walk.visit_node(root, None, None, Ancestors::default(), &mut edits) {
        Ok(()) => last_edit(&mut edits),
        Err(Stop::Break(value)) => value,
        Err(Stop::Fail(error)) => return Err(error),
    };
    match last {
        None => Ok(Some(owned(root))),
        Some(EditValue::Node(node)) => Ok(Some(*node)),
        Some(EditValue::Null) => Ok(None),
        Some(EditValue::List(_)) => Err(EditError::Root {
            root: root.kind(),
            offered: Offered::List,
        }),
    }
}

/// The value an edit records under a key, the second entry of a pair of the
/// `edits` list of the reference.
enum EditValue {
    /// A node, a replacement or a copy with the edits of its children.
    Node(Box<AstNode>),
    /// No node, the `null` of a removal.
    Null,
    /// The copy of an array with the edits of its elements.
    List(Vec<AstNode>),
}

/// The edits of one level of the walk, in the order the walk records them.
///
/// The key of an edit at the root is the index 0 of the one entry list
/// `[root]` the reference counts through.
type Edits = Vec<(Key, EditValue)>;

/// Returns the last edit of a level the walk leaves for good.
fn last_edit(edits: &mut Edits) -> Option<EditValue> {
    edits.pop().map(|(_, value)| value)
}

/// Why the walk stops before its end.
enum Stop {
    /// A hook answered [`Edit::Break`], and the walk ends on the last edit of
    /// the level it stopped in, or on the root.
    Break(Option<EditValue>),
    /// An edit offers a value its property cannot hold.
    Fail(EditError),
}

/// The state of one walk, the variables of `visit` that outlive a level.
struct Walk<'v, V: ?Sized> {
    /// The hooks.
    visitor: &'v mut V,
    /// The keys of the children of each kind.
    visitor_keys: VisitorKeys,
    /// The key path to the visited node.
    path: Vec<Key>,
}

impl<V: EditVisitor + ?Sized> Walk<'_, V> {
    /// Visits `node`, found under `key` of `parent`, and records its edits in
    /// `edits`, the list of the level that holds it.
    ///
    /// This is one turn of the loop of the reference that enters a node, with
    /// the descent into its children, and the turn that leaves it. The
    /// replacement of the enter hook is recorded before the edit of the leave
    /// hook, as the reference records it before it descends.
    fn visit_node(
        &mut self,
        node: NodeRef<'_>,
        key: Option<Key>,
        parent: Option<NodeOrList<'_>>,
        ancestors: Ancestors<'_>,
        edits: &mut Edits,
    ) -> Result<(), Stop> {
        let edit_key = key.unwrap_or(Key::Index(0));
        let context = EditContext {
            key,
            parent,
            path: &self.path,
            ancestors,
        };
        let replacement = match self.visitor.enter(node, &context) {
            Edit::Keep => None,
            Edit::Skip => return Ok(()),
            Edit::Break => return Err(Stop::Break(last_edit(edits))),
            Edit::Remove => {
                edits.push((edit_key, EditValue::Null));
                return Ok(());
            }
            Edit::Replace(replacement) => Some(replacement),
        };

        let current = replacement.as_deref().map_or(node, NodeRef::from);
        let children = self.visit_children(current, parent, ancestors)?;
        let copy = if children.is_empty() {
            None
        } else {
            Some(Box::new(rebuild(current, children).map_err(Stop::Fail)?))
        };

        let leaving = copy.as_deref().map_or(current, NodeRef::from);
        let context = EditContext {
            key,
            parent,
            path: &self.path,
            ancestors,
        };
        let left = self.visitor.leave(leaving, &context);
        let entered = replacement.map(EditValue::Node);
        let value = match left {
            Edit::Break => return Err(Stop::Break(entered.or_else(|| last_edit(edits)))),
            Edit::Keep => copy.map(EditValue::Node),
            Edit::Skip => None,
            Edit::Remove => Some(EditValue::Null),
            Edit::Replace(node) => Some(EditValue::Node(node)),
        };
        edits.extend(entered.map(|entered| (edit_key, entered)));
        edits.extend(value.map(|value| (edit_key, value)));
        Ok(())
    }

    /// Visits the children of `node` and returns the edits they record.
    ///
    /// `parent` and `ancestors` are those of `node`, so the children see
    /// `node` as their parent and the parent of `node` as their nearest
    /// ancestor. A child the node does not hold is passed over, as the
    /// reference passes over `null` and `undefined`.
    fn visit_children(
        &mut self,
        node: NodeRef<'_>,
        parent: Option<NodeOrList<'_>>,
        ancestors: Ancestors<'_>,
    ) -> Result<Edits, Stop> {
        let link = parent.map(|parent| Ancestor::new(parent, ancestors));
        let ancestors = link.as_ref().map_or(ancestors, Ancestors::ending_at);
        let mut edits = Edits::new();
        for &name in (self.visitor_keys)(node.kind()) {
            let key = Key::Name(name);
            let Some(child) = child_of(NodeOrList::Node(node), key) else {
                continue;
            };
            self.path.push(key);
            let visited = match child {
                NodeOrList::Node(child) => self.visit_node(
                    child,
                    Some(key),
                    Some(NodeOrList::Node(node)),
                    ancestors,
                    &mut edits,
                ),
                NodeOrList::List(items) => self.visit_list(items, key, node, ancestors, &mut edits),
            };
            self.path.pop();
            visited?;
        }
        Ok(edits)
    }

    /// Visits the elements of the array `items`, held under `key` of `owner`,
    /// and records the edited array in `edits` when an element was edited.
    ///
    /// An array is a level of its own with no hook, whose parent is `owner`.
    /// `ancestors` are those of the children of `owner`.
    fn visit_list(
        &mut self,
        items: ListRef<'_>,
        key: Key,
        owner: NodeRef<'_>,
        ancestors: Ancestors<'_>,
        edits: &mut Edits,
    ) -> Result<(), Stop> {
        let link = Ancestor::new(NodeOrList::Node(owner), ancestors);
        let ancestors = Ancestors::ending_at(&link);
        let parent = Some(NodeOrList::List(items));
        let mut item_edits = Edits::new();
        for (index, item) in items.iter().enumerate() {
            let item_key = Key::Index(index);
            self.path.push(item_key);
            let visited = self.visit_node(item, Some(item_key), parent, ancestors, &mut item_edits);
            self.path.pop();
            visited?;
        }
        if !item_edits.is_empty() {
            let copy = rebuild_list(items, item_edits, owner.kind(), key).map_err(Stop::Fail)?;
            edits.push((key, EditValue::List(copy)));
        }
        Ok(())
    }
}

/// Returns a copy of `node` with `edits` applied in the order they were
/// recorded, the `{ ...node }` and the assignments of the reference.
///
/// The copy clones the node with its `loc` and every child, and each edit
/// then replaces or clears one child.
// The copy clones the children an edit replaces as well, since a node offers
// no clone that leaves a property out, and it clones the children the edit
// keeps, which the shallow copy of the reference shares. The selections of a
// selection set are the one child the copy shares, by their `SharedSlice`.
// Sharing the others needs each behind a shared pointer or a walk over an
// owned tree that moves them.
fn rebuild(node: NodeRef<'_>, edits: Edits) -> Result<AstNode, EditError> {
    let mut copy = owned(node);
    for (key, value) in edits {
        set_child(&mut copy, key, value)?;
    }
    Ok(copy)
}

/// Returns a copy of the array `items` with `edits` applied, the `slice` and
/// the `splice` of the reference.
///
/// A removal shifts the elements after it, so the index of every later edit
/// is moved back by the removals before it, as the offset of the reference
/// moves it. `owner` and `key` name the property that holds the array, for
/// the error of an edit an element cannot hold.
fn rebuild_list(
    items: ListRef<'_>,
    edits: Edits,
    owner: Kind,
    key: Key,
) -> Result<Vec<AstNode>, EditError> {
    let mut copy = owned_list(items);
    let mut edit_offset = 0;
    for (edit_key, value) in edits {
        let Key::Index(index) = edit_key else {
            return Err(EditError::Property {
                parent: owner,
                key,
                offered: offered(&value),
            });
        };
        let array_key = index.saturating_sub(edit_offset);
        match value {
            EditValue::Null => {
                if array_key < copy.len() {
                    copy.remove(array_key);
                }
                edit_offset += 1;
            }
            EditValue::Node(node) => {
                if let Some(slot) = copy.get_mut(array_key) {
                    *slot = *node;
                }
            }
            EditValue::List(_) => {
                return Err(EditError::Property {
                    parent: owner,
                    key: edit_key,
                    offered: Offered::List,
                });
            }
        }
    }
    Ok(copy)
}

/// Returns what `value` offers, for an error.
fn offered(value: &EditValue) -> Offered {
    match value {
        EditValue::Node(node) => Offered::Node(node.kind()),
        EditValue::Null => Offered::Null,
        EditValue::List(_) => Offered::List,
    }
}

/// Clones a borrowed node into a node the walk owns.
macro_rules! owned_node {
    ($node:expr; $($variant:ident),+ $(,)?) => {
        match $node {
            $(NodeRef::$variant(inner) => AstNode::$variant(inner.clone()),)+
        }
    };
}

/// Returns a clone of the node `node` views.
fn owned(node: NodeRef<'_>) -> AstNode {
    owned_node!(node;
        Name, Document, OperationDefinition, VariableDefinition, Variable, SelectionSet, Field,
        Argument, FragmentSpread, InlineFragment, FragmentDefinition, IntValue, FloatValue,
        StringValue, BooleanValue, NullValue, EnumValue, ListValue, ObjectValue, ObjectField,
        Directive, NamedType, ListType, NonNullType, SchemaDefinition, OperationTypeDefinition,
        ScalarTypeDefinition, ObjectTypeDefinition, FieldDefinition, InputValueDefinition,
        InterfaceTypeDefinition, UnionTypeDefinition, EnumTypeDefinition, EnumValueDefinition,
        InputObjectTypeDefinition, DirectiveDefinition, SchemaExtension, ScalarTypeExtension,
        ObjectTypeExtension, InterfaceTypeExtension, UnionTypeExtension, EnumTypeExtension,
        InputObjectTypeExtension, TypeCoordinate, MemberCoordinate, ArgumentCoordinate,
        DirectiveCoordinate, DirectiveArgumentCoordinate,
    )
}

/// Clones a borrowed array into nodes the walk owns.
macro_rules! owned_items {
    ($items:expr; $($variant:ident),+ $(,)?) => {
        match $items {
            $(ListRef::$variant(items) => items.iter().cloned().map(AstNode::from).collect(),)+
        }
    };
}

/// Returns a clone of every element of the array `items` views.
fn owned_list(items: ListRef<'_>) -> Vec<AstNode> {
    owned_items!(items;
        Definition, VariableDefinition, Directive, Selection, Argument, InputValueDefinition,
        Value, ObjectField, FieldDefinition, OperationTypeDefinition, NamedType,
        EnumValueDefinition, Name,
    )
}

/// A type a property of a node holds, read back from a node the walk owns.
trait Narrow: Sized {
    /// Returns the node as this type, or the kind of the node when it is of
    /// another kind.
    fn narrow(node: AstNode) -> Result<Self, Kind>;
}

/// Implements [`Narrow`] for node structs, each the payload of one variant.
macro_rules! narrow_node {
    ($($node:ident => $variant:ident;)+) => {
        $(
            impl Narrow for $node {
                fn narrow(node: AstNode) -> Result<Self, Kind> {
                    match node {
                        AstNode::$variant(inner) => Ok(inner),
                        other => Err(other.kind()),
                    }
                }
            }
        )+
    };
}

narrow_node! {
    NameNode => Name;
    StringValueNode => StringValue;
    VariableNode => Variable;
    SelectionSetNode => SelectionSet;
    NamedTypeNode => NamedType;
    ArgumentNode => Argument;
    VariableDefinitionNode => VariableDefinition;
    DirectiveNode => Directive;
    ObjectFieldNode => ObjectField;
    OperationTypeDefinitionNode => OperationTypeDefinition;
    FieldDefinitionNode => FieldDefinition;
    InputValueDefinitionNode => InputValueDefinition;
    EnumValueDefinitionNode => EnumValueDefinition;
}

/// Implements [`Narrow`] for unions, whose variants share their names with
/// those of [`AstNode`].
macro_rules! narrow_union {
    ($($union:ident { $($variant:ident),+ $(,)? })+) => {
        $(
            impl Narrow for $union {
                fn narrow(node: AstNode) -> Result<Self, Kind> {
                    match node {
                        $(AstNode::$variant(inner) => Ok(Self::$variant(inner)),)+
                        other => Err(other.kind()),
                    }
                }
            }
        )+
    };
}

narrow_union! {
    DefinitionNode {
        OperationDefinition, FragmentDefinition, SchemaDefinition, ScalarTypeDefinition,
        ObjectTypeDefinition, InterfaceTypeDefinition, UnionTypeDefinition, EnumTypeDefinition,
        InputObjectTypeDefinition, DirectiveDefinition, SchemaExtension, ScalarTypeExtension,
        ObjectTypeExtension, InterfaceTypeExtension, UnionTypeExtension, EnumTypeExtension,
        InputObjectTypeExtension,
    }
    SelectionNode { Field, FragmentSpread, InlineFragment }
    ValueNode {
        Variable, IntValue, FloatValue, StringValue, BooleanValue, NullValue, EnumValue, ListValue,
        ObjectValue,
    }
    TypeNode { NamedType, ListType, NonNullType }
    NullableTypeNode { NamedType, ListType }
}

/// The property an edit writes, for the error of a value it cannot hold.
#[derive(Clone, Copy)]
struct At {
    /// The kind of the node that holds the property.
    parent: Kind,
    /// The property.
    key: Key,
}

impl At {
    /// Returns the error of `value`, which the property cannot hold.
    fn refuse(self, value: &EditValue) -> EditError {
        EditError::Property {
            parent: self.parent,
            key: self.key,
            offered: offered(value),
        }
    }
}

/// Returns `value` as a `T`, which a property that holds one node takes.
fn narrow<T: Narrow>(at: At, value: EditValue) -> Result<T, EditError> {
    match value {
        EditValue::Node(node) => T::narrow(*node).map_err(|kind| EditError::Property {
            parent: at.parent,
            key: at.key,
            offered: Offered::Node(kind),
        }),
        other => Err(at.refuse(&other)),
    }
}

/// Returns `value` as an array of `T`, which a property that holds an array
/// takes.
///
/// The array is built by a loop into a new vector, and the function stays out
/// of line. Over `VariableDefinitionNode`, the SLP vectorizer of the LLVM of
/// rustc 1.95.0 crashes under fat link time optimization for
/// `aarch64-unknown-linux-gnu` both on the collection of the mapped iterator,
/// which reuses the allocation of the nodes in place, and on this loop
/// inlined into `set_optional_list`, so a release build of a binary that
/// links the walk would not compile otherwise.
#[inline(never)]
fn narrow_list<T: Narrow>(at: At, value: EditValue) -> Result<Vec<T>, EditError> {
    let EditValue::List(items) = value else {
        return Err(at.refuse(&value));
    };
    let mut narrowed = Vec::with_capacity(items.len());
    for node in items {
        narrowed.push(T::narrow(node).map_err(|kind| EditError::Property {
            parent: at.parent,
            key: at.key,
            offered: Offered::Node(kind),
        })?);
    }
    Ok(narrowed)
}

/// Writes `value` into a property that holds one node.
fn set<T: Narrow>(slot: &mut T, at: At, value: EditValue) -> Result<(), EditError> {
    *slot = narrow(at, value)?;
    Ok(())
}

/// Writes `value` into a property that holds an optional node, where `null`
/// clears it.
fn set_optional<T: Narrow>(
    slot: &mut Option<T>,
    at: At,
    value: EditValue,
) -> Result<(), EditError> {
    *slot = match value {
        EditValue::Null => None,
        value => Some(narrow(at, value)?),
    };
    Ok(())
}

/// Writes `value` into a property that holds an array.
fn set_list<T: Narrow>(slot: &mut Vec<T>, at: At, value: EditValue) -> Result<(), EditError> {
    *slot = narrow_list(at, value)?;
    Ok(())
}

/// Writes `value` into a property that holds a shared array, as a new slice.
///
/// The slice the property held stays with the nodes that share it, the node
/// the walk copies from among them.
fn set_shared_list<T: Narrow>(
    slot: &mut SharedSlice<T>,
    at: At,
    value: EditValue,
) -> Result<(), EditError> {
    *slot = SharedSlice::from(narrow_list(at, value)?);
    Ok(())
}

/// Writes `value` into a property that holds an optional array, where `null`
/// clears it.
fn set_optional_list<T: Narrow>(
    slot: &mut Option<Vec<T>>,
    at: At,
    value: EditValue,
) -> Result<(), EditError> {
    *slot = match value {
        EditValue::Null => None,
        value => Some(narrow_list(at, value)?),
    };
    Ok(())
}

/// Writes `value` into the property `key` of `node`, the
/// `node[editKey] = editValue` of the reference.
///
/// The arms are those of the table of children the walk reads (see
/// [`child_of`]), one per kind and property, so every property an edit can
/// reach takes it.
// The function is a lookup table with one arm per property of the reference
// table, and a table split over several functions hides the row a reader
// looks for.
#[expect(clippy::too_many_lines)]
fn set_child(node: &mut AstNode, key: Key, value: EditValue) -> Result<(), EditError> {
    let at = At {
        parent: node.kind(),
        key,
    };
    let Key::Name(name) = key else {
        return Err(at.refuse(&value));
    };
    match (node, name) {
        (AstNode::Document(n), "definitions") => set_list(&mut n.definitions, at, value),
        (AstNode::OperationDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::OperationDefinition(n), "name") => set_optional(&mut n.name, at, value),
        (AstNode::OperationDefinition(n), "variableDefinitions") => {
            set_optional_list(&mut n.variable_definitions, at, value)
        }
        (AstNode::OperationDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::OperationDefinition(n), "selectionSet") => set(&mut n.selection_set, at, value),
        (AstNode::VariableDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::VariableDefinition(n), "variable") => set(&mut n.variable, at, value),
        (AstNode::VariableDefinition(n), "type") => set(&mut n.r#type, at, value),
        (AstNode::VariableDefinition(n), "defaultValue") => {
            set_optional(&mut n.default_value, at, value)
        }
        (AstNode::VariableDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::Variable(n), "name") => set(&mut n.name, at, value),
        (AstNode::SelectionSet(n), "selections") => set_shared_list(&mut n.selections, at, value),
        (AstNode::Field(n), "alias") => set_optional(&mut n.alias, at, value),
        (AstNode::Field(n), "name") => set(&mut n.name, at, value),
        (AstNode::Field(n), "arguments") => set_optional_list(&mut n.arguments, at, value),
        (AstNode::Field(n), "directives") => set_optional_list(&mut n.directives, at, value),
        (AstNode::Field(n), "selectionSet") => set_optional(&mut n.selection_set, at, value),
        (AstNode::Argument(n), "name") => set(&mut n.name, at, value),
        (AstNode::Argument(n), "value") => set(&mut n.value, at, value),
        (AstNode::FragmentSpread(n), "name") => set(&mut n.name, at, value),
        (AstNode::FragmentSpread(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InlineFragment(n), "typeCondition") => {
            set_optional(&mut n.type_condition, at, value)
        }
        (AstNode::InlineFragment(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InlineFragment(n), "selectionSet") => set(&mut n.selection_set, at, value),
        (AstNode::FragmentDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::FragmentDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::FragmentDefinition(n), "variableDefinitions") => {
            set_optional_list(&mut n.variable_definitions, at, value)
        }
        (AstNode::FragmentDefinition(n), "typeCondition") => set(&mut n.type_condition, at, value),
        (AstNode::FragmentDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::FragmentDefinition(n), "selectionSet") => set(&mut n.selection_set, at, value),
        (AstNode::ListValue(n), "values") => set_list(&mut n.values, at, value),
        (AstNode::ObjectValue(n), "fields") => set_list(&mut n.fields, at, value),
        (AstNode::ObjectField(n), "name") => set(&mut n.name, at, value),
        (AstNode::ObjectField(n), "value") => set(&mut n.value, at, value),
        (AstNode::Directive(n), "name") => set(&mut n.name, at, value),
        (AstNode::Directive(n), "arguments") => set_optional_list(&mut n.arguments, at, value),
        (AstNode::NamedType(n), "name") => set(&mut n.name, at, value),
        (AstNode::ListType(n), "type") => set(&mut *n.r#type, at, value),
        (AstNode::NonNullType(n), "type") => set(&mut n.r#type, at, value),
        (AstNode::SchemaDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::SchemaDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::SchemaDefinition(n), "operationTypes") => {
            set_list(&mut n.operation_types, at, value)
        }
        (AstNode::OperationTypeDefinition(n), "type") => set(&mut n.r#type, at, value),
        (AstNode::ScalarTypeDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::ScalarTypeDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::ScalarTypeDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::ObjectTypeDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::ObjectTypeDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::ObjectTypeDefinition(n), "interfaces") => {
            set_optional_list(&mut n.interfaces, at, value)
        }
        (AstNode::ObjectTypeDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::ObjectTypeDefinition(n), "fields") => set_optional_list(&mut n.fields, at, value),
        (AstNode::FieldDefinition(n), "description") => set_optional(&mut n.description, at, value),
        (AstNode::FieldDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::FieldDefinition(n), "arguments") => {
            set_optional_list(&mut n.arguments, at, value)
        }
        (AstNode::FieldDefinition(n), "type") => set(&mut n.r#type, at, value),
        (AstNode::FieldDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InputValueDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::InputValueDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::InputValueDefinition(n), "type") => set(&mut n.r#type, at, value),
        (AstNode::InputValueDefinition(n), "defaultValue") => {
            set_optional(&mut n.default_value, at, value)
        }
        (AstNode::InputValueDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InterfaceTypeDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::InterfaceTypeDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::InterfaceTypeDefinition(n), "interfaces") => {
            set_optional_list(&mut n.interfaces, at, value)
        }
        (AstNode::InterfaceTypeDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InterfaceTypeDefinition(n), "fields") => {
            set_optional_list(&mut n.fields, at, value)
        }
        (AstNode::UnionTypeDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::UnionTypeDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::UnionTypeDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::UnionTypeDefinition(n), "types") => set_optional_list(&mut n.types, at, value),
        (AstNode::EnumTypeDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::EnumTypeDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::EnumTypeDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::EnumTypeDefinition(n), "values") => set_optional_list(&mut n.values, at, value),
        (AstNode::EnumValueDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::EnumValueDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::EnumValueDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InputObjectTypeDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::InputObjectTypeDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::InputObjectTypeDefinition(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InputObjectTypeDefinition(n), "fields") => {
            set_optional_list(&mut n.fields, at, value)
        }
        (AstNode::DirectiveDefinition(n), "description") => {
            set_optional(&mut n.description, at, value)
        }
        (AstNode::DirectiveDefinition(n), "name") => set(&mut n.name, at, value),
        (AstNode::DirectiveDefinition(n), "arguments") => {
            set_optional_list(&mut n.arguments, at, value)
        }
        (AstNode::DirectiveDefinition(n), "locations") => set_list(&mut n.locations, at, value),
        (AstNode::SchemaExtension(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::SchemaExtension(n), "operationTypes") => {
            set_optional_list(&mut n.operation_types, at, value)
        }
        (AstNode::ScalarTypeExtension(n), "name") => set(&mut n.name, at, value),
        (AstNode::ScalarTypeExtension(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::ObjectTypeExtension(n), "name") => set(&mut n.name, at, value),
        (AstNode::ObjectTypeExtension(n), "interfaces") => {
            set_optional_list(&mut n.interfaces, at, value)
        }
        (AstNode::ObjectTypeExtension(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::ObjectTypeExtension(n), "fields") => set_optional_list(&mut n.fields, at, value),
        (AstNode::InterfaceTypeExtension(n), "name") => set(&mut n.name, at, value),
        (AstNode::InterfaceTypeExtension(n), "interfaces") => {
            set_optional_list(&mut n.interfaces, at, value)
        }
        (AstNode::InterfaceTypeExtension(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InterfaceTypeExtension(n), "fields") => {
            set_optional_list(&mut n.fields, at, value)
        }
        (AstNode::UnionTypeExtension(n), "name") => set(&mut n.name, at, value),
        (AstNode::UnionTypeExtension(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::UnionTypeExtension(n), "types") => set_optional_list(&mut n.types, at, value),
        (AstNode::EnumTypeExtension(n), "name") => set(&mut n.name, at, value),
        (AstNode::EnumTypeExtension(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::EnumTypeExtension(n), "values") => set_optional_list(&mut n.values, at, value),
        (AstNode::InputObjectTypeExtension(n), "name") => set(&mut n.name, at, value),
        (AstNode::InputObjectTypeExtension(n), "directives") => {
            set_optional_list(&mut n.directives, at, value)
        }
        (AstNode::InputObjectTypeExtension(n), "fields") => {
            set_optional_list(&mut n.fields, at, value)
        }
        (AstNode::TypeCoordinate(n), "name") => set(&mut n.name, at, value),
        (AstNode::MemberCoordinate(n), "name") => set(&mut n.name, at, value),
        (AstNode::MemberCoordinate(n), "memberName") => set(&mut n.member_name, at, value),
        (AstNode::ArgumentCoordinate(n), "name") => set(&mut n.name, at, value),
        (AstNode::ArgumentCoordinate(n), "fieldName") => set(&mut n.field_name, at, value),
        (AstNode::ArgumentCoordinate(n), "argumentName") => set(&mut n.argument_name, at, value),
        (AstNode::DirectiveCoordinate(n), "name") => set(&mut n.name, at, value),
        (AstNode::DirectiveArgumentCoordinate(n), "name") => set(&mut n.name, at, value),
        (AstNode::DirectiveArgumentCoordinate(n), "argumentName") => {
            set(&mut n.argument_name, at, value)
        }
        _ => Err(at.refuse(&value)),
    }
}

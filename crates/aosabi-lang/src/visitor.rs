// SPDX-License-Identifier: MIT
/*
 * aosabi-lang: the depth first walk over the abstract syntax tree.
 *
 * Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
 * Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
 */

//! The visitor of the language layer.
//!
//! [`visit`] walks a tree depth first, calls the `enter` hook of a [`Visitor`]
//! at each node before its children and the `leave` hook after them, and visits
//! the children of a node in the order the `QueryDocumentKeys` table lists
//! them. The order matters beyond the printer, because the validation rules
//! report their errors in the order the walk reaches the nodes, and that order
//! is part of the byte contract.
//!
//! The reference passes every node as an `ASTNode` object reference, and a
//! visitor keeps such a reference as long as it likes. [`NodeRef`] is that
//! reading here, a borrowed view over any node that carries the lifetime of
//! the tree, and [`ListRef`] is the same view over an array of nodes, which
//! the walk hands a visitor as the parent of an array element and as an
//! ancestor. The predicates and the printer take the same view, so a node
//! reached by the walk passes to them without a copy.
//!
//! [`visit_in_parallel`] runs a list of visitors in one walk, and the list
//! holds each as an [`EnterLeave`] object, whose two methods call the hook
//! the visitor holds for the kind of the node.
//!
//! The reference lets a hook edit the tree by returning `null` or a replacement
//! node. The walk of this module is read only, and a hook answers with a
//! [`VisitResult`], which is the `undefined`, `false` and `BREAK` of the
//! reference. The walk that edits is [󰀅](crate::visitor_edit::visit_edit),
//! whose hooks answer an [󰀆](crate::visitor_edit::Edit) and which returns the
//! edited tree, as the transforms and the delegation of Federation edit a
//! document. Both walks read the children of a node through one table, the
//! `QueryDocumentKeys` of the reference.

use alloc::{
    vec,
    vec::Vec, //
};

use crate::{
    ast::{
        ArgumentCoordinateNode,
        ArgumentNode,
        AstNode,
        BooleanValueNode,
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
        ExecutableDefinitionNode,
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
        ScalarTypeDefinitionNode,
        ScalarTypeExtensionNode,
        SchemaCoordinateNode,
        SchemaDefinitionNode,
        SchemaExtensionNode,
        SelectionNode,
        SelectionSetNode,
        StringValueNode,
        TypeCoordinateNode,
        TypeDefinitionNode,
        TypeExtensionNode,
        TypeNode,
        TypeSystemDefinitionNode,
        TypeSystemExtensionNode,
        UnionTypeDefinitionNode,
        UnionTypeExtensionNode,
        ValueNode,
        VariableDefinitionNode,
        VariableNode, //
    },
    kinds::Kind, //
};

/// Declares the borrowed node view, the visitor trait and the two hook
/// dispatchers from one list of node kinds.
///
/// Each entry names the variant, the node struct, the [`Kind`] the node
/// carries, and the two hook names a [`Visitor`] gets for it. One list keeps
/// the view, the trait and the dispatch in step, because a kind that is
/// missing from any of the three would silently skip its hooks.
macro_rules! node_kinds {
    ($($variant:ident($node:ident) => $kind:ident, $enter:ident, $leave:ident;)+) => {
        /// A borrowed view over any node of the tree.
        ///
        /// The reference passes an `ASTNode` by object reference to every
        /// hook, predicate and printer. This enum is that reference in Rust,
        /// one variant per node kind, each holding a shared borrow with the
        /// lifetime of the tree. It converts from a borrow of any node struct
        /// and of any union of [`crate::ast`], so a caller passes `&node`
        /// through `into()`.
        #[derive(Debug, Clone, Copy)]
        pub enum NodeRef<'a> {
            $(
                #[doc = concat!("A borrowed [`", stringify!($node), "`].")]
                $variant(&'a $node),
            )+
        }

        impl<'a> NodeRef<'a> {
            /// Returns the kind of the viewed node.
            #[must_use]
            pub const fn kind(self) -> Kind {
                match self {
                    $(Self::$variant(_) => Kind::$kind,)+
                }
            }

            /// Returns the location of the viewed node, when it carries one.
            #[must_use]
            pub const fn loc(self) -> Option<&'a Location> {
                match self {
                    $(Self::$variant(node) => node.loc.as_ref(),)+
                }
            }

            /// Returns the identity of the viewed node, which is what the
            /// reference compares with `===`.
            fn identity(self) -> NodeIdentity {
                let addr = match self {
                    $(Self::$variant(node) => core::ptr::from_ref(node).addr(),)+
                };
                NodeIdentity { kind: self.kind(), addr }
            }
        }

        $(
            impl<'a> From<&'a $node> for NodeRef<'a> {
                fn from(node: &'a $node) -> Self {
                    Self::$variant(node)
                }
            }
        )+

        impl<'a> From<&'a AstNode> for NodeRef<'a> {
            fn from(node: &'a AstNode) -> Self {
                match node {
                    $(AstNode::$variant(inner) => Self::$variant(inner),)+
                }
            }
        }

        /// The hooks of a walk, called at every node the walk reaches.
        ///
        /// The reference accepts three shapes of visitor, a generic
        /// `{ enter, leave }` pair, a kind entry `{ Kind: { enter, leave } }`
        /// and a kind function `{ Kind() {} }`, and `getEnterLeaveForKind`
        /// picks the kind entry when one exists and the generic pair otherwise.
        /// Here every kind has an enter and a leave method whose default
        /// forwards to [`Visitor::enter`] and [`Visitor::leave`], so a method
        /// left alone is the generic pair and an overridden one is the kind
        /// entry for that direction. A kind entry of the reference silences the
        /// generic hook of the other direction as well, so a visitor that wants
        /// the `{ Kind() {} }` form overrides the leave method of that kind
        /// too.
        ///
        /// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
        ///
        /// Every hook receives the node it is called for and a
        /// [`VisitContext`], which carries the key, the parent, the path and
        /// the ancestors of the reference signature. The lifetime `'a` is
        /// the tree, so a hook may keep the node or an ancestor.
        pub trait Visitor<'a> {
            /// Enters any node for which no kind method is overridden.
            ///
            /// Answers [`VisitResult::Continue`] by default, which visits the
            /// children.
            fn enter(&mut self, _node: NodeRef<'a>, _context: &VisitContext<'a, '_>) -> VisitResult {
                VisitResult::Continue
            }

            /// Leaves any node for which no kind method is overridden.
            ///
            /// Answers [`VisitResult::Continue`] by default, which carries on
            /// with the next node.
            fn leave(&mut self, _node: NodeRef<'a>, _context: &VisitContext<'a, '_>) -> VisitResult {
                VisitResult::Continue
            }

            $(
                #[doc = concat!("Enters a [`", stringify!($node), "`], the enter half of the `", stringify!($variant), "` entry.")]
                ///
                /// Forwards to [`Visitor::enter`] by default.
                fn $enter(&mut self, node: &'a $node, context: &VisitContext<'a, '_>) -> VisitResult {
                    Visitor::enter(self, NodeRef::$variant(node), context)
                }

                #[doc = concat!("Leaves a [`", stringify!($node), "`], the leave half of the `", stringify!($variant), "` entry.")]
                ///
                /// Forwards to [`Visitor::leave`] by default.
                fn $leave(&mut self, node: &'a $node, context: &VisitContext<'a, '_>) -> VisitResult {
                    Visitor::leave(self, NodeRef::$variant(node), context)
                }
            )+
        }

        /// Calls the enter hook a visitor holds for the kind of `node`.
        ///
        /// The type layer wraps a visitor with its type information through
        /// this function, as the reference does with `visitWithTypeInfo`.
        ///
        /// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
        pub fn enter_for_kind<'a, V>(
            visitor: &mut V,
            node: NodeRef<'a>,
            context: &VisitContext<'a, '_>,
        ) -> VisitResult
        where
            V: Visitor<'a> + ?Sized,
        {
            match node {
                $(NodeRef::$variant(node) => visitor.$enter(node, context),)+
            }
        }

        /// Calls the leave hook a visitor holds for the kind of `node`.
        ///
        /// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
        pub fn leave_for_kind<'a, V>(
            visitor: &mut V,
            node: NodeRef<'a>,
            context: &VisitContext<'a, '_>,
        ) -> VisitResult
        where
            V: Visitor<'a> + ?Sized,
        {
            match node {
                $(NodeRef::$variant(node) => visitor.$leave(node, context),)+
            }
        }
    };
}

node_kinds! {
    Name(NameNode) => Name, enter_name, leave_name;
    Document(DocumentNode) => Document, enter_document, leave_document;
    OperationDefinition(OperationDefinitionNode) => OperationDefinition, enter_operation_definition, leave_operation_definition;
    VariableDefinition(VariableDefinitionNode) => VariableDefinition, enter_variable_definition, leave_variable_definition;
    Variable(VariableNode) => Variable, enter_variable, leave_variable;
    SelectionSet(SelectionSetNode) => SelectionSet, enter_selection_set, leave_selection_set;
    Field(FieldNode) => Field, enter_field, leave_field;
    Argument(ArgumentNode) => Argument, enter_argument, leave_argument;
    FragmentSpread(FragmentSpreadNode) => FragmentSpread, enter_fragment_spread, leave_fragment_spread;
    InlineFragment(InlineFragmentNode) => InlineFragment, enter_inline_fragment, leave_inline_fragment;
    FragmentDefinition(FragmentDefinitionNode) => FragmentDefinition, enter_fragment_definition, leave_fragment_definition;
    IntValue(IntValueNode) => Int, enter_int_value, leave_int_value;
    FloatValue(FloatValueNode) => Float, enter_float_value, leave_float_value;
    StringValue(StringValueNode) => String, enter_string_value, leave_string_value;
    BooleanValue(BooleanValueNode) => Boolean, enter_boolean_value, leave_boolean_value;
    NullValue(NullValueNode) => Null, enter_null_value, leave_null_value;
    EnumValue(EnumValueNode) => Enum, enter_enum_value, leave_enum_value;
    ListValue(ListValueNode) => List, enter_list_value, leave_list_value;
    ObjectValue(ObjectValueNode) => Object, enter_object_value, leave_object_value;
    ObjectField(ObjectFieldNode) => ObjectField, enter_object_field, leave_object_field;
    Directive(DirectiveNode) => Directive, enter_directive, leave_directive;
    NamedType(NamedTypeNode) => NamedType, enter_named_type, leave_named_type;
    ListType(ListTypeNode) => ListType, enter_list_type, leave_list_type;
    NonNullType(NonNullTypeNode) => NonNullType, enter_non_null_type, leave_non_null_type;
    SchemaDefinition(SchemaDefinitionNode) => SchemaDefinition, enter_schema_definition, leave_schema_definition;
    OperationTypeDefinition(OperationTypeDefinitionNode) => OperationTypeDefinition, enter_operation_type_definition, leave_operation_type_definition;
    ScalarTypeDefinition(ScalarTypeDefinitionNode) => ScalarTypeDefinition, enter_scalar_type_definition, leave_scalar_type_definition;
    ObjectTypeDefinition(ObjectTypeDefinitionNode) => ObjectTypeDefinition, enter_object_type_definition, leave_object_type_definition;
    FieldDefinition(FieldDefinitionNode) => FieldDefinition, enter_field_definition, leave_field_definition;
    InputValueDefinition(InputValueDefinitionNode) => InputValueDefinition, enter_input_value_definition, leave_input_value_definition;
    InterfaceTypeDefinition(InterfaceTypeDefinitionNode) => InterfaceTypeDefinition, enter_interface_type_definition, leave_interface_type_definition;
    UnionTypeDefinition(UnionTypeDefinitionNode) => UnionTypeDefinition, enter_union_type_definition, leave_union_type_definition;
    EnumTypeDefinition(EnumTypeDefinitionNode) => EnumTypeDefinition, enter_enum_type_definition, leave_enum_type_definition;
    EnumValueDefinition(EnumValueDefinitionNode) => EnumValueDefinition, enter_enum_value_definition, leave_enum_value_definition;
    InputObjectTypeDefinition(InputObjectTypeDefinitionNode) => InputObjectTypeDefinition, enter_input_object_type_definition, leave_input_object_type_definition;
    DirectiveDefinition(DirectiveDefinitionNode) => DirectiveDefinition, enter_directive_definition, leave_directive_definition;
    SchemaExtension(SchemaExtensionNode) => SchemaExtension, enter_schema_extension, leave_schema_extension;
    ScalarTypeExtension(ScalarTypeExtensionNode) => ScalarTypeExtension, enter_scalar_type_extension, leave_scalar_type_extension;
    ObjectTypeExtension(ObjectTypeExtensionNode) => ObjectTypeExtension, enter_object_type_extension, leave_object_type_extension;
    InterfaceTypeExtension(InterfaceTypeExtensionNode) => InterfaceTypeExtension, enter_interface_type_extension, leave_interface_type_extension;
    UnionTypeExtension(UnionTypeExtensionNode) => UnionTypeExtension, enter_union_type_extension, leave_union_type_extension;
    EnumTypeExtension(EnumTypeExtensionNode) => EnumTypeExtension, enter_enum_type_extension, leave_enum_type_extension;
    InputObjectTypeExtension(InputObjectTypeExtensionNode) => InputObjectTypeExtension, enter_input_object_type_extension, leave_input_object_type_extension;
    TypeCoordinate(TypeCoordinateNode) => TypeCoordinate, enter_type_coordinate, leave_type_coordinate;
    MemberCoordinate(MemberCoordinateNode) => MemberCoordinate, enter_member_coordinate, leave_member_coordinate;
    ArgumentCoordinate(ArgumentCoordinateNode) => ArgumentCoordinate, enter_argument_coordinate, leave_argument_coordinate;
    DirectiveCoordinate(DirectiveCoordinateNode) => DirectiveCoordinate, enter_directive_coordinate, leave_directive_coordinate;
    DirectiveArgumentCoordinate(DirectiveArgumentCoordinateNode) => DirectiveArgumentCoordinate, enter_directive_argument_coordinate, leave_directive_argument_coordinate;
}

/// Converts a borrow of a union of [`crate::ast`] into a [`NodeRef`].
///
/// The variant names of a union and of the view are the same, which is the
/// rule the `node_union` macro of the tree states, so each conversion is a
/// rename of nothing.
macro_rules! node_ref_from_union {
    ($($union:ident { $($variant:ident),+ $(,)? })+) => {
        $(
            impl<'a> From<&'a $union> for NodeRef<'a> {
                fn from(node: &'a $union) -> Self {
                    match node {
                        $($union::$variant(inner) => Self::$variant(inner),)+
                    }
                }
            }
        )+
    };
}

node_ref_from_union! {
    DefinitionNode {
        OperationDefinition, FragmentDefinition, SchemaDefinition, ScalarTypeDefinition,
        ObjectTypeDefinition, InterfaceTypeDefinition, UnionTypeDefinition, EnumTypeDefinition,
        InputObjectTypeDefinition, DirectiveDefinition, SchemaExtension, ScalarTypeExtension,
        ObjectTypeExtension, InterfaceTypeExtension, UnionTypeExtension, EnumTypeExtension,
        InputObjectTypeExtension,
    }
    ExecutableDefinitionNode { OperationDefinition, FragmentDefinition }
    SelectionNode { Field, FragmentSpread, InlineFragment }
    ValueNode {
        Variable, IntValue, FloatValue, StringValue, BooleanValue, NullValue, EnumValue, ListValue,
        ObjectValue,
    }
    TypeNode { NamedType, ListType, NonNullType }
    NullableTypeNode { NamedType, ListType }
    TypeSystemDefinitionNode {
        SchemaDefinition, ScalarTypeDefinition, ObjectTypeDefinition, InterfaceTypeDefinition,
        UnionTypeDefinition, EnumTypeDefinition, InputObjectTypeDefinition, DirectiveDefinition,
    }
    TypeDefinitionNode {
        ScalarTypeDefinition, ObjectTypeDefinition, InterfaceTypeDefinition, UnionTypeDefinition,
        EnumTypeDefinition, InputObjectTypeDefinition,
    }
    TypeSystemExtensionNode {
        SchemaExtension, ScalarTypeExtension, ObjectTypeExtension, InterfaceTypeExtension,
        UnionTypeExtension, EnumTypeExtension, InputObjectTypeExtension,
    }
    TypeExtensionNode {
        ScalarTypeExtension, ObjectTypeExtension, InterfaceTypeExtension, UnionTypeExtension,
        EnumTypeExtension, InputObjectTypeExtension,
    }
    SchemaCoordinateNode {
        TypeCoordinate, MemberCoordinate, ArgumentCoordinate, DirectiveCoordinate,
        DirectiveArgumentCoordinate,
    }
}

/// Declares the borrowed list view from one list of element types.
///
/// Each entry names the variant and the element type of one array the
/// `QueryDocumentKeys` table reaches. The variant is named after the element,
/// because `interfaces` and `types` share one element type and so do the
/// `arguments` of a definition and the `fields` of an input object.
macro_rules! list_kinds {
    ($($variant:ident($element:ident);)+) => {
        /// A borrowed view over an array of nodes.
        ///
        /// The reference hands a hook the array that holds the visited node as
        /// its `parent`, and it lists arrays among the `ancestors`. This enum
        /// is that array in Rust, one variant per element type, each holding a
        /// shared borrow of the slice with the lifetime of the tree.
        #[derive(Debug, Clone, Copy)]
        pub enum ListRef<'a> {
            $(
                #[doc = concat!("A borrowed slice of [`", stringify!($element), "`].")]
                $variant(&'a [$element]),
            )+
        }

        impl<'a> ListRef<'a> {
            /// Returns the number of elements.
            #[must_use]
            pub const fn len(self) -> usize {
                match self {
                    $(Self::$variant(items) => items.len(),)+
                }
            }

            /// Returns whether the list holds no element.
            #[must_use]
            pub const fn is_empty(self) -> bool {
                self.len() == 0
            }

            /// Returns the element at `index`, when there is one.
            #[must_use]
            pub fn get(self, index: usize) -> Option<NodeRef<'a>> {
                match self {
                    $(Self::$variant(items) => items.get(index).map(NodeRef::from),)+
                }
            }

            /// Returns the elements in order.
            pub fn iter(self) -> impl Iterator<Item = NodeRef<'a>> {
                (0..self.len()).filter_map(move |index| self.get(index))
            }
        }

        $(
            impl<'a> From<&'a [$element]> for ListRef<'a> {
                fn from(items: &'a [$element]) -> Self {
                    Self::$variant(items)
                }
            }

            impl<'a> From<&'a Vec<$element>> for ListRef<'a> {
                fn from(items: &'a Vec<$element>) -> Self {
                    Self::$variant(items)
                }
            }
        )+
    };
}

list_kinds! {
    Definition(DefinitionNode);
    VariableDefinition(VariableDefinitionNode);
    Directive(DirectiveNode);
    Selection(SelectionNode);
    Argument(ArgumentNode);
    InputValueDefinition(InputValueDefinitionNode);
    Value(ValueNode);
    ObjectField(ObjectFieldNode);
    FieldDefinition(FieldDefinitionNode);
    OperationTypeDefinition(OperationTypeDefinitionNode);
    NamedType(NamedTypeNode);
    EnumValueDefinition(EnumValueDefinitionNode);
    Name(NameNode);
}

/// A node or an array of nodes, the `ASTNode | ReadonlyArray<ASTNode>` a
/// hook receives as its parent and among its ancestors.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
#[derive(Debug, Clone, Copy)]
pub enum NodeOrList<'a> {
    /// A node.
    Node(NodeRef<'a>),
    /// An array of nodes.
    List(ListRef<'a>),
}

/// The key of a node in its parent, a property name in a node or an index
/// in an array.
///
/// The property name is the camel case name of the reference, because the
/// `path` a hook receives is the path the reference reports.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    /// A property name of a node, such as `selectionSet`.
    Name(&'static str),
    /// An index into an array.
    Index(usize),
}

/// The position of the visited node, the four trailing arguments of a hook.
///
/// The lifetime `'a` is the tree and `'c` is the call, because the path and the
/// ancestors are the working lists of the walk, which change between calls.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
#[derive(Debug, Clone, Copy)]
pub struct VisitContext<'a, 'c> {
    /// The index or key to this node from the parent node or array, absent
    /// at the root.
    pub key: Option<Key>,
    /// The parent immediately above this node, which may be an array, absent
    /// at the root.
    pub parent: Option<NodeOrList<'a>>,
    /// The key path to get to this node from the root node.
    pub path: &'c [Key],
    /// All nodes and arrays visited before reaching the parent of this node,
    /// which includes the arrays that hold the parent.
    pub ancestors: &'c [NodeOrList<'a>],
}

/// The answer of a hook, which steers the walk.
///
/// The two editing answers of the reference, `null` and a replacement node,
/// have no reading in a read only walk.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VisitResult {
    /// No action, the `undefined` of the reference.
    Continue,
    /// On enter, skip the children of this node and its leave hook, the
    /// `false` of the reference. On leave, no action.
    Skip,
    /// Stop the whole walk, the `BREAK` of the reference.
    Break,
}

/// The answer that stops the whole walk.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
pub const BREAK: VisitResult = VisitResult::Break;

/// Returns the property names of the children of a node kind, in the order
/// the walk visits them.
///
/// The names are the camel case property names of the reference, and they are
/// the keys [`VisitContext::path`] reports.
///
/// Derived from graphql-js 16.12.0, src/language/ast.ts, MIT.
// One arm per kind keeps the table in the shape of the reference, where each
// kind lists its own keys, so a reader checks it row by row.
#[allow(clippy::match_same_arms)]
#[must_use]
pub const fn query_document_keys(kind: Kind) -> &'static [&'static str] {
    match kind {
        Kind::Name => &[],
        Kind::Document => &["definitions"],
        Kind::OperationDefinition => &[
            "description",
            "name",
            "variableDefinitions",
            "directives",
            "selectionSet",
        ],
        Kind::VariableDefinition => &[
            "description",
            "variable",
            "type",
            "defaultValue",
            "directives",
        ],
        Kind::Variable => &["name"],
        Kind::SelectionSet => &["selections"],
        Kind::Field => &["alias", "name", "arguments", "directives", "selectionSet"],
        Kind::Argument => &["name", "value"],
        Kind::FragmentSpread => &["name", "directives"],
        Kind::InlineFragment => &["typeCondition", "directives", "selectionSet"],
        Kind::FragmentDefinition => &[
            "description",
            "name",
            "variableDefinitions",
            "typeCondition",
            "directives",
            "selectionSet",
        ],
        Kind::Int => &[],
        Kind::Float => &[],
        Kind::String => &[],
        Kind::Boolean => &[],
        Kind::Null => &[],
        Kind::Enum => &[],
        Kind::List => &["values"],
        Kind::Object => &["fields"],
        Kind::ObjectField => &["name", "value"],
        Kind::Directive => &["name", "arguments"],
        Kind::NamedType => &["name"],
        Kind::ListType => &["type"],
        Kind::NonNullType => &["type"],
        Kind::SchemaDefinition => &["description", "directives", "operationTypes"],
        Kind::OperationTypeDefinition => &["type"],
        Kind::ScalarTypeDefinition => &["description", "name", "directives"],
        Kind::ObjectTypeDefinition => {
            &["description", "name", "interfaces", "directives", "fields"]
        }
        Kind::FieldDefinition => &["description", "name", "arguments", "type", "directives"],
        Kind::InputValueDefinition => {
            &["description", "name", "type", "defaultValue", "directives"]
        }
        Kind::InterfaceTypeDefinition => {
            &["description", "name", "interfaces", "directives", "fields"]
        }
        Kind::UnionTypeDefinition => &["description", "name", "directives", "types"],
        Kind::EnumTypeDefinition => &["description", "name", "directives", "values"],
        Kind::EnumValueDefinition => &["description", "name", "directives"],
        Kind::InputObjectTypeDefinition => &["description", "name", "directives", "fields"],
        Kind::DirectiveDefinition => &["description", "name", "arguments", "locations"],
        Kind::SchemaExtension => &["directives", "operationTypes"],
        Kind::ScalarTypeExtension => &["name", "directives"],
        Kind::ObjectTypeExtension => &["name", "interfaces", "directives", "fields"],
        Kind::InterfaceTypeExtension => &["name", "interfaces", "directives", "fields"],
        Kind::UnionTypeExtension => &["name", "directives", "types"],
        Kind::EnumTypeExtension => &["name", "directives", "values"],
        Kind::InputObjectTypeExtension => &["name", "directives", "fields"],
        Kind::TypeCoordinate => &["name"],
        Kind::MemberCoordinate => &["name", "memberName"],
        Kind::ArgumentCoordinate => &["name", "fieldName", "argumentName"],
        Kind::DirectiveCoordinate => &["name"],
        Kind::DirectiveArgumentCoordinate => &["name", "argumentName"],
    }
}

/// Views an optional child node the way `parent[key]` reads it.
fn optional<'a, T>(value: Option<&'a T>) -> Option<NodeOrList<'a>>
where
    NodeRef<'a>: From<&'a T>,
{
    value.map(|node| NodeOrList::Node(NodeRef::from(node)))
}

/// Views a child node the way `parent[key]` reads it.
fn node<'a, T>(value: &'a T) -> Option<NodeOrList<'a>>
where
    NodeRef<'a>: From<&'a T>,
{
    optional(Some(value))
}

/// Views an optional child array the way `parent[key]` reads it.
fn optional_list<'a, T>(value: Option<&'a [T]>) -> Option<NodeOrList<'a>>
where
    ListRef<'a>: From<&'a [T]>,
{
    value.map(|items| NodeOrList::List(ListRef::from(items)))
}

/// Views a child array the way `parent[key]` reads it.
fn list<'a, T>(value: &'a [T]) -> Option<NodeOrList<'a>>
where
    ListRef<'a>: From<&'a [T]>,
{
    optional_list(Some(value))
}

/// Returns the child of a node under a property name, and `None` where the
/// reference reads `null` or `undefined`.
///
/// This is `parent[key]` of the walk over the properties the
/// `QueryDocumentKeys` table names, one arm per kind and key.
// The function is a lookup table with one arm per property of the reference
// table, and a table split over several functions hides the row a reader
// looks for.
#[allow(clippy::too_many_lines)]
fn child<'a>(parent: NodeRef<'a>, key: &str) -> Option<NodeOrList<'a>> {
    match (parent, key) {
        (NodeRef::Document(n), "definitions") => list(&n.definitions),
        (NodeRef::OperationDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::OperationDefinition(n), "name") => optional(n.name.as_ref()),
        (NodeRef::OperationDefinition(n), "variableDefinitions") => {
            optional_list(n.variable_definitions.as_deref())
        }
        (NodeRef::OperationDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::OperationDefinition(n), "selectionSet") => node(&n.selection_set),
        (NodeRef::VariableDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::VariableDefinition(n), "variable") => node(&n.variable),
        (NodeRef::VariableDefinition(n), "type") => node(&n.r#type),
        (NodeRef::VariableDefinition(n), "defaultValue") => optional(n.default_value.as_ref()),
        (NodeRef::VariableDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::Variable(n), "name") => node(&n.name),
        (NodeRef::SelectionSet(n), "selections") => list(&n.selections),
        (NodeRef::Field(n), "alias") => optional(n.alias.as_ref()),
        (NodeRef::Field(n), "name") => node(&n.name),
        (NodeRef::Field(n), "arguments") => optional_list(n.arguments.as_deref()),
        (NodeRef::Field(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::Field(n), "selectionSet") => optional(n.selection_set.as_ref()),
        (NodeRef::Argument(n), "name") => node(&n.name),
        (NodeRef::Argument(n), "value") => node(&n.value),
        (NodeRef::FragmentSpread(n), "name") => node(&n.name),
        (NodeRef::FragmentSpread(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::InlineFragment(n), "typeCondition") => optional(n.type_condition.as_ref()),
        (NodeRef::InlineFragment(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::InlineFragment(n), "selectionSet") => node(&n.selection_set),
        (NodeRef::FragmentDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::FragmentDefinition(n), "name") => node(&n.name),
        (NodeRef::FragmentDefinition(n), "variableDefinitions") => {
            optional_list(n.variable_definitions.as_deref())
        }
        (NodeRef::FragmentDefinition(n), "typeCondition") => node(&n.type_condition),
        (NodeRef::FragmentDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::FragmentDefinition(n), "selectionSet") => node(&n.selection_set),
        (NodeRef::ListValue(n), "values") => list(&n.values),
        (NodeRef::ObjectValue(n), "fields") => list(&n.fields),
        (NodeRef::ObjectField(n), "name") => node(&n.name),
        (NodeRef::ObjectField(n), "value") => node(&n.value),
        (NodeRef::Directive(n), "name") => node(&n.name),
        (NodeRef::Directive(n), "arguments") => optional_list(n.arguments.as_deref()),
        (NodeRef::NamedType(n), "name") => node(&n.name),
        (NodeRef::ListType(n), "type") => node(&*n.r#type),
        (NodeRef::NonNullType(n), "type") => node(&n.r#type),
        (NodeRef::SchemaDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::SchemaDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::SchemaDefinition(n), "operationTypes") => list(&n.operation_types),
        (NodeRef::OperationTypeDefinition(n), "type") => node(&n.r#type),
        (NodeRef::ScalarTypeDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::ScalarTypeDefinition(n), "name") => node(&n.name),
        (NodeRef::ScalarTypeDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::ObjectTypeDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::ObjectTypeDefinition(n), "name") => node(&n.name),
        (NodeRef::ObjectTypeDefinition(n), "interfaces") => optional_list(n.interfaces.as_deref()),
        (NodeRef::ObjectTypeDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::ObjectTypeDefinition(n), "fields") => optional_list(n.fields.as_deref()),
        (NodeRef::FieldDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::FieldDefinition(n), "name") => node(&n.name),
        (NodeRef::FieldDefinition(n), "arguments") => optional_list(n.arguments.as_deref()),
        (NodeRef::FieldDefinition(n), "type") => node(&n.r#type),
        (NodeRef::FieldDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::InputValueDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::InputValueDefinition(n), "name") => node(&n.name),
        (NodeRef::InputValueDefinition(n), "type") => node(&n.r#type),
        (NodeRef::InputValueDefinition(n), "defaultValue") => optional(n.default_value.as_ref()),
        (NodeRef::InputValueDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::InterfaceTypeDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::InterfaceTypeDefinition(n), "name") => node(&n.name),
        (NodeRef::InterfaceTypeDefinition(n), "interfaces") => {
            optional_list(n.interfaces.as_deref())
        }
        (NodeRef::InterfaceTypeDefinition(n), "directives") => {
            optional_list(n.directives.as_deref())
        }
        (NodeRef::InterfaceTypeDefinition(n), "fields") => optional_list(n.fields.as_deref()),
        (NodeRef::UnionTypeDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::UnionTypeDefinition(n), "name") => node(&n.name),
        (NodeRef::UnionTypeDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::UnionTypeDefinition(n), "types") => optional_list(n.types.as_deref()),
        (NodeRef::EnumTypeDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::EnumTypeDefinition(n), "name") => node(&n.name),
        (NodeRef::EnumTypeDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::EnumTypeDefinition(n), "values") => optional_list(n.values.as_deref()),
        (NodeRef::EnumValueDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::EnumValueDefinition(n), "name") => node(&n.name),
        (NodeRef::EnumValueDefinition(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::InputObjectTypeDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::InputObjectTypeDefinition(n), "name") => node(&n.name),
        (NodeRef::InputObjectTypeDefinition(n), "directives") => {
            optional_list(n.directives.as_deref())
        }
        (NodeRef::InputObjectTypeDefinition(n), "fields") => optional_list(n.fields.as_deref()),
        (NodeRef::DirectiveDefinition(n), "description") => optional(n.description.as_ref()),
        (NodeRef::DirectiveDefinition(n), "name") => node(&n.name),
        (NodeRef::DirectiveDefinition(n), "arguments") => optional_list(n.arguments.as_deref()),
        (NodeRef::DirectiveDefinition(n), "locations") => list(&n.locations),
        (NodeRef::SchemaExtension(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::SchemaExtension(n), "operationTypes") => {
            optional_list(n.operation_types.as_deref())
        }
        (NodeRef::ScalarTypeExtension(n), "name") => node(&n.name),
        (NodeRef::ScalarTypeExtension(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::ObjectTypeExtension(n), "name") => node(&n.name),
        (NodeRef::ObjectTypeExtension(n), "interfaces") => optional_list(n.interfaces.as_deref()),
        (NodeRef::ObjectTypeExtension(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::ObjectTypeExtension(n), "fields") => optional_list(n.fields.as_deref()),
        (NodeRef::InterfaceTypeExtension(n), "name") => node(&n.name),
        (NodeRef::InterfaceTypeExtension(n), "interfaces") => {
            optional_list(n.interfaces.as_deref())
        }
        (NodeRef::InterfaceTypeExtension(n), "directives") => {
            optional_list(n.directives.as_deref())
        }
        (NodeRef::InterfaceTypeExtension(n), "fields") => optional_list(n.fields.as_deref()),
        (NodeRef::UnionTypeExtension(n), "name") => node(&n.name),
        (NodeRef::UnionTypeExtension(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::UnionTypeExtension(n), "types") => optional_list(n.types.as_deref()),
        (NodeRef::EnumTypeExtension(n), "name") => node(&n.name),
        (NodeRef::EnumTypeExtension(n), "directives") => optional_list(n.directives.as_deref()),
        (NodeRef::EnumTypeExtension(n), "values") => optional_list(n.values.as_deref()),
        (NodeRef::InputObjectTypeExtension(n), "name") => node(&n.name),
        (NodeRef::InputObjectTypeExtension(n), "directives") => {
            optional_list(n.directives.as_deref())
        }
        (NodeRef::InputObjectTypeExtension(n), "fields") => optional_list(n.fields.as_deref()),
        (NodeRef::TypeCoordinate(n), "name") => node(&n.name),
        (NodeRef::MemberCoordinate(n), "name") => node(&n.name),
        (NodeRef::MemberCoordinate(n), "memberName") => node(&n.member_name),
        (NodeRef::ArgumentCoordinate(n), "name") => node(&n.name),
        (NodeRef::ArgumentCoordinate(n), "fieldName") => node(&n.field_name),
        (NodeRef::ArgumentCoordinate(n), "argumentName") => node(&n.argument_name),
        (NodeRef::DirectiveCoordinate(n), "name") => node(&n.name),
        (NodeRef::DirectiveArgumentCoordinate(n), "name") => node(&n.name),
        (NodeRef::DirectiveArgumentCoordinate(n), "argumentName") => node(&n.argument_name),
        _ => None,
    }
}

/// Returns the child of a node or an array under a key, the `parent[key]`
/// of the walk with an index into an array or a property name into a node.
///
/// The walk that edits reads the children of a node through it as well (see
/// [`crate::visitor_edit`]).
pub(crate) fn child_of(parent: NodeOrList<'_>, key: Key) -> Option<NodeOrList<'_>> {
    match (parent, key) {
        (NodeOrList::List(items), Key::Index(index)) => items.get(index).map(NodeOrList::Node),
        (NodeOrList::Node(parent), Key::Name(name)) => child(parent, name),
        _ => None,
    }
}

/// The keys the walk counts through at one level, the `keys` variable of
/// the reference.
///
/// At the root the reference counts through `[root]`, a list of one entry that
/// is never read by key because the root has no parent.
#[derive(Debug, Clone, Copy)]
enum Keys<'a> {
    /// The one entry list that holds the root.
    Root,
    /// The property names of a node.
    Names(&'static [&'static str]),
    /// The elements of an array, keyed by index.
    Items(ListRef<'a>),
}

impl Keys<'_> {
    /// Returns how many keys the level holds.
    const fn len(self) -> usize {
        match self {
            Self::Root => 1,
            Self::Names(names) => names.len(),
            Self::Items(items) => items.len(),
        }
    }

    /// Returns the key at `index`, which is below [`Keys::len`] whenever the
    /// walk reads one.
    fn key_at(self, index: usize) -> Key {
        match self {
            Self::Root | Self::Items(_) => Key::Index(index),
            Self::Names(names) => Key::Name(names[index]),
        }
    }
}

/// The saved state of one level of the walk, the `stack` object of the
/// reference without its edits.
#[derive(Debug, Clone, Copy)]
struct Frame<'a> {
    /// The index the level was at when the walk descended.
    index: usize,
    /// The keys of the level.
    keys: Keys<'a>,
}

/// Walks through an AST using a depth first traversal, calling the visitor's
/// enter hook at each node in the traversal, and calling the leave hook
/// after visiting that node and all of its child nodes.
///
/// `root` is any node or union of the tree, and `visitor` receives the hooks.
/// By returning different values from the enter and leave hooks, the behavior
/// of the visitor can be altered, including skipping over a sub tree of the AST
/// with [`VisitResult::Skip`], or stopping the whole traversal with
/// [`VisitResult::Break`].
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
///
/// The children of a node are visited in the order of
/// [`query_document_keys`], and an array of nodes is a level of its own
/// that has no hook but appears as a parent and an ancestor, as in the
/// reference.
pub fn visit<'a, V>(root: impl Into<NodeRef<'a>>, visitor: &mut V)
where
    V: Visitor<'a> + ?Sized,
{
    let root = root.into();
    let mut stack: Vec<Frame<'a>> = Vec::new();
    let mut keys = Keys::Root;
    // The reference starts the index at -1 and counts up at the head of
    // every turn. A `usize` has no -1, so the count starts one wrap below
    // zero and the first turn lands on zero.
    let mut index = usize::MAX;
    let mut node = NodeOrList::Node(root);
    let mut key: Option<Key> = None;
    let mut parent: Option<NodeOrList<'a>> = None;
    let mut path: Vec<Key> = Vec::new();
    let mut ancestors: Vec<NodeOrList<'a>> = Vec::new();

    loop {
        index = index.wrapping_add(1);
        let is_leaving = index == keys.len();
        if is_leaving {
            key = if ancestors.is_empty() {
                None
            } else {
                path.last().copied()
            };
            if let Some(current) = parent {
                node = current;
            }
            parent = ancestors.pop();
            let Some(frame) = stack.pop() else { break };
            index = frame.index;
            keys = frame.keys;
        } else if let Some(current) = parent {
            let next = keys.key_at(index);
            key = Some(next);
            let Some(found) = child_of(current, next) else {
                continue;
            };
            node = found;
            path.push(next);
        }

        if let NodeOrList::Node(current) = node {
            let context = VisitContext {
                key,
                parent,
                path: &path,
                ancestors: &ancestors,
            };
            let result = if is_leaving {
                leave_for_kind(visitor, current, &context)
            } else {
                enter_for_kind(visitor, current, &context)
            };
            if result == VisitResult::Break {
                break;
            }
            if result == VisitResult::Skip && !is_leaving {
                path.pop();
                continue;
            }
        }

        if is_leaving {
            path.pop();
        } else {
            stack.push(Frame { index, keys });
            keys = match node {
                NodeOrList::List(items) => Keys::Items(items),
                NodeOrList::Node(current) => Keys::Names(query_document_keys(current.kind())),
            };
            index = usize::MAX;
            if let Some(current) = parent {
                ancestors.push(current);
            }
            parent = Some(node);
        }
        // A `continue` above runs inside a descent, where the stack holds the
        // frame of the parent, so this check is the loop condition of the
        // reference for every turn that reaches it.
        if stack.is_empty() {
            break;
        }
    }
}

/// The two hooks of a visitor resolved per node, the object a list of
/// visitors of different types holds.
///
/// Every [`Visitor`] implements the trait through [`enter_for_kind`] and
/// [`leave_for_kind`], so a visitor keeps its kind methods and dispatches to
/// them as it does in a walk of its own.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
///
/// A [`Visitor`] object carries one method for each direction of every
/// kind, while an object of this trait carries two, so a list such as the
/// rules of the validation layer holds its visitors as `dyn EnterLeave`,
/// and the dispatch over the kinds is compiled once per visitor type.
///
/// [`enter`]: EnterLeave::enter
/// [`leave`]: EnterLeave::leave
pub trait EnterLeave<'a> {
    /// Calls the enter hook the visitor holds for the kind of `node`.
    fn enter(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult;

    /// Calls the leave hook the visitor holds for the kind of `node`.
    fn leave(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult;
}

impl<'a, V> EnterLeave<'a> for V
where
    V: Visitor<'a> + ?Sized,
{
    fn enter(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult {
        enter_for_kind(self, node, context)
    }

    fn leave(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult {
        leave_for_kind(self, node, context)
    }
}

/// The identity of a node of a tree, the object identity of the reference.
///
/// The reference tells two nodes apart with `===`, which compares object
/// references. The address of a node struct is not that identity on its own,
/// because the first field of a struct sits at the address of the struct, so a
/// `DirectiveNode` and the `NameNode` it holds in `name` answer the same
/// address. A Rust type cannot hold a value of its own type inline, so no two
/// nodes of one kind share an address, and the kind beside the address tells
/// every node of a tree from every other.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct NodeIdentity {
    /// The kind of the node.
    kind: Kind,
    /// The address of the node struct.
    addr: usize,
}

/// What one visitor of a parallel walk is skipping, the `skipping` entry of
/// the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Skipping {
    /// Nothing, the `null` of the reference.
    Nothing,
    /// The sub tree of the node with this identity, until its leave.
    Node(NodeIdentity),
    /// Everything, the `BREAK` of the reference.
    Break,
}

/// A visitor that delegates to many visitors run in parallel.
///
/// Built by [`visit_in_parallel`]. Each visitor is visited for each node before
/// moving on, and a visitor that skips a sub tree or breaks does so for itself
/// alone.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
pub struct ParallelVisitor<'a, 'v> {
    /// The visitors, in the order they are called.
    visitors: Vec<&'v mut dyn EnterLeave<'a>>,
    /// What each visitor is skipping, indexed like `visitors`.
    skipping: Vec<Skipping>,
}

/// Creates a new visitor instance which delegates to many visitors to run in
/// parallel.
///
/// Each visitor will be visited for each node before moving on. `visitors` are
/// the visitors, in the order they are called. The reference builds a kind
/// entry only for a kind some visitor has a hook for, and forwards every other
/// kind to nothing. The merged visitor here forwards every kind through
/// [`EnterLeave`], where a visitor without a hook answers
/// [`VisitResult::Continue`], which is the same outcome.
///
/// Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
#[must_use]
pub fn visit_in_parallel<'a, 'v>(
    visitors: Vec<&'v mut dyn EnterLeave<'a>>,
) -> ParallelVisitor<'a, 'v> {
    let skipping = vec![Skipping::Nothing; visitors.len()];
    ParallelVisitor { visitors, skipping }
}

impl<'a> Visitor<'a> for ParallelVisitor<'a, '_> {
    fn enter(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult {
        for (visitor, skipping) in self.visitors.iter_mut().zip(self.skipping.iter_mut()) {
            if *skipping != Skipping::Nothing {
                continue;
            }
            // The reference returns any other answer as an edit, which a
            // read only walk cannot produce.
            match visitor.enter(node, context) {
                VisitResult::Skip => *skipping = Skipping::Node(node.identity()),
                VisitResult::Break => *skipping = Skipping::Break,
                VisitResult::Continue => {}
            }
        }
        VisitResult::Continue
    }

    fn leave(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult {
        for (visitor, skipping) in self.visitors.iter_mut().zip(self.skipping.iter_mut()) {
            if *skipping == Skipping::Nothing {
                if visitor.leave(node, context) == VisitResult::Break {
                    *skipping = Skipping::Break;
                }
            } else if *skipping == Skipping::Node(node.identity()) {
                *skipping = Skipping::Nothing;
            }
        }
        VisitResult::Continue
    }
}

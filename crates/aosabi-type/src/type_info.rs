// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the type information of a walk over a document, which keeps
 * the current type, parent type, input type, field, directive, argument and
 * enum value at every node the walk reaches.
 *
 * Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
 */

//! The type information of the type layer.
//!
//! [`TypeInfo`] follows a recursive descent over a document through
//! [`TypeInfo::enter()`] and [`TypeInfo::leave()`], and answers at any node
//! which output type, composite parent type, input type, field definition,
//! default value, directive, argument and enum value the schema assigns to
//! that position. Every validation rule reads the schema through it, so its
//! stacks hold what the reference holds at each node, and a lookup that
//! finds nothing pushes nothing, as the reference pushes `undefined`.
//! [`visit_with_type_info()`] wraps a visitor so the type information moves
//! with the walk.
//!
//! # The shape of the port
//!
//! The reference holds a field definition by reference into the schema. A
//! field of this crate lives inside the object or interface type that
//! declares it, and a stack that holds the field while the walk goes on
//! cannot borrow from a type it also holds, so [`FieldDef`] names the field
//! by its owner and its position and resolves it on every read, and an
//! argument and an enum value are named the same way. The three meta
//! fields are the shared definitions of [`crate::introspection`], so the
//! field the type information answers is the one the executor resolves.
//!
//! A field thunk of the schema may fail, so [`TypeInfo::enter()`] returns
//! that failure where the reference throws it, and the wrapping visitor
//! stops the walk and keeps the error for its caller. The wrapping visitor
//! and the visitor it wraps share the type information through a `RefCell`,
//! because the reference shares one object between them.

use alloc::{
    sync::Arc,
    vec::Vec, //
};
use core::cell::RefCell;

use aosabi_json::JsValue;
use aosabi_lang::{
    ast::{
        FieldNode,
        NamedTypeNode, //
    },
    visitor::{
        NodeRef,
        VisitContext,
        VisitResult,
        Visitor,
        enter_for_kind,
        leave_for_kind, //
    }, //
};

use crate::{
    definition::{
        Argument,
        DefinitionError,
        EnumType,
        EnumValue,
        Field,
        InterfaceType,
        NamedType,
        ObjectType,
        Type,
        get_named_type,
        get_nullable_type,
        is_composite_type,
        is_input_type,
        is_object_type,
        is_output_type, //
    },
    directives::Directive,
    introspection::{
        schema_meta_field_def,
        type_meta_field_def,
        type_name_meta_field_def, //
    },
    schema::Schema,
    type_from_ast::type_from_ast, //
};

/// A field definition the type information holds, named by the type that
/// declares it.
///
/// The `GraphQLField` a stack of the reference holds by reference. A field
/// of an object or an interface type is the entry at a position of the
/// field map of that type, and a meta field is borrowed from the static
/// that holds it, because it belongs to no type.
#[derive(Clone, Debug)]
pub enum FieldDef {
    /// The field at the position of the field map of an object type.
    Object(Arc<ObjectType>, usize),
    /// The field at the position of the field map of an interface type.
    Interface(Arc<InterfaceType>, usize),
    /// A meta field, `__schema`, `__type` or `__typename`.
    Meta(&'static Field),
}

impl FieldDef {
    /// Returns the field.
    ///
    /// The field map of the owner resolved when the definition was made, so
    /// the read finds it again, and `None` answers only a position the map
    /// does not hold.
    #[must_use]
    pub fn get(&self) -> Option<&Field> {
        match self {
            Self::Object(object, index) => object
                .get_fields()
                .ok()?
                .get_index(*index)
                .map(|(_, field)| field),
            Self::Interface(interface, index) => interface
                .get_fields()
                .ok()?
                .get_index(*index)
                .map(|(_, field)| field),
            Self::Meta(field) => Some(field),
        }
    }
}

/// An argument definition, named by the directive or the field that
/// declares it.
#[derive(Clone, Debug)]
enum ArgumentDef {
    /// The argument at the position of the argument list of a directive.
    Directive(Arc<Directive>, usize),
    /// The argument at the position of the argument list of a field.
    Field(FieldDef, usize),
}

impl ArgumentDef {
    /// Returns the argument.
    fn get(&self) -> Option<&Argument> {
        match self {
            Self::Directive(directive, index) => directive.args.get(*index),
            Self::Field(field_def, index) => field_def.get()?.args.get(*index),
        }
    }
}

/// An enum value, named by the enum type that declares it.
#[derive(Clone, Debug)]
struct EnumValueDef {
    /// The enum type.
    enum_type: Arc<EnumType>,
    /// The position of the value in the value list of the type.
    index: usize,
}

impl EnumValueDef {
    /// Returns the enum value.
    fn get(&self) -> Option<&EnumValue> {
        self.enum_type.get_values().ok()?.get(self.index)
    }
}

/// The lookup of a field definition, `GetFieldDefFn` of the reference.
///
/// The parent type is the composite type the type information holds, because
/// that is what the one caller passes.
///
/// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
pub type GetFieldDefFn =
    fn(&Schema, &NamedType, &FieldNode) -> Result<Option<FieldDef>, DefinitionError>;

/// `TypeInfo` is a utility class which, given a GraphQL schema, can keep track
/// of the current field and type definitions at any point in a GraphQL
/// document AST during a recursive descent by calling `enter(node)` and
/// `leave(node)`.
///
/// The lifetime is the schema the information reads.
///
/// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
#[derive(Debug)]
pub struct TypeInfo<'s> {
    /// The schema.
    schema: &'s Schema,
    /// The output type at each enclosing field or fragment.
    type_stack: Vec<Option<Type>>,
    /// The composite type at each enclosing selection set.
    parent_type_stack: Vec<Option<NamedType>>,
    /// The input type at each enclosing argument, list, object field or
    /// variable definition.
    input_type_stack: Vec<Option<Type>>,
    /// The field definition at each enclosing field.
    field_def_stack: Vec<Option<FieldDef>>,
    /// The default value at each enclosing argument, list or object field.
    default_value_stack: Vec<JsValue>,
    /// The current directive.
    directive: Option<Arc<Directive>>,
    /// The current argument.
    argument: Option<ArgumentDef>,
    /// The current enum value.
    enum_value: Option<EnumValueDef>,
    /// The field definition lookup.
    get_field_def: GetFieldDefFn,
}

impl<'s> TypeInfo<'s> {
    /// Creates the type information of `schema`.
    ///
    /// `initial_type` may be provided in rare cases to facilitate traversals
    /// beginning somewhere other than documents, and `get_field_def_fn`
    /// replaces [`get_field_def()`], which the reference deprecates.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn new(
        schema: &'s Schema,
        initial_type: Option<&Type>,
        get_field_def_fn: Option<GetFieldDefFn>,
    ) -> Self {
        let mut type_info = Self {
            schema,
            type_stack: Vec::new(),
            parent_type_stack: Vec::new(),
            input_type_stack: Vec::new(),
            field_def_stack: Vec::new(),
            default_value_stack: Vec::new(),
            directive: None,
            argument: None,
            enum_value: None,
            get_field_def: get_field_def_fn.unwrap_or(get_field_def),
        };
        if let Some(initial_type) = initial_type {
            if is_input_type(initial_type) {
                type_info.input_type_stack.push(Some(initial_type.clone()));
            }
            if is_composite_type(initial_type) {
                type_info.parent_type_stack.push(initial_type.as_named());
            }
            if is_output_type(initial_type) {
                type_info.type_stack.push(Some(initial_type.clone()));
            }
        }
        type_info
    }

    /// Returns the current output type.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_type(&self) -> Option<&Type> {
        self.type_stack.last()?.as_ref()
    }

    /// Returns the current composite parent type.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_parent_type(&self) -> Option<&NamedType> {
        self.parent_type_stack.last()?.as_ref()
    }

    /// Returns the current input type.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_input_type(&self) -> Option<&Type> {
        self.input_type_stack.last()?.as_ref()
    }

    /// Returns the input type enclosing the current one.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_parent_input_type(&self) -> Option<&Type> {
        let index = self.input_type_stack.len().checked_sub(2)?;
        self.input_type_stack.get(index)?.as_ref()
    }

    /// Returns the current field definition.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_field_def(&self) -> Option<&Field> {
        self.field_def_stack.last()?.as_ref()?.get()
    }

    /// Returns the current default value, which is undefined where the
    /// position has none.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_default_value(&self) -> Option<&JsValue> {
        self.default_value_stack.last()
    }

    /// Returns the current directive.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_directive(&self) -> Option<&Arc<Directive>> {
        self.directive.as_ref()
    }

    /// Returns the current argument.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_argument(&self) -> Option<&Argument> {
        self.argument.as_ref()?.get()
    }

    /// Returns the current enum value.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    #[must_use]
    pub fn get_enum_value(&self) -> Option<&EnumValue> {
        self.enum_value.as_ref()?.get()
    }

    /// Enters `node` and pushes what the schema assigns to its position.
    ///
    /// Note: many of the types below are checked at run time to drop any
    /// assumptions of a valid schema, since `TypeInfo` is used as part of
    /// validation which occurs before guarantees of schema and document
    /// validity.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns the error of the field lookup, of a field thunk of the input
    /// object type an object field is read from, or of the value thunk of
    /// the enum type an enum value is read from.
    pub fn enter(&mut self, node: NodeRef<'_>) -> Result<(), DefinitionError> {
        let schema = self.schema;
        match node {
            NodeRef::SelectionSet(_) => {
                let named_type = self.get_type().map(get_named_type);
                self.parent_type_stack.push(
                    named_type.filter(|named_type| is_composite_type(&Type::from(named_type))),
                );
            }
            NodeRef::Field(node) => {
                let parent_type = self.get_parent_type();
                let mut field_def = None;
                let mut field_type = None;
                if let Some(parent_type) = parent_type {
                    field_def = (self.get_field_def)(schema, parent_type, node)?;
                    if let Some(field) = field_def.as_ref().and_then(FieldDef::get) {
                        field_type = Some(field.r#type.clone());
                    }
                }
                self.field_def_stack.push(field_def);
                self.type_stack.push(field_type.filter(is_output_type));
            }
            NodeRef::Directive(node) => {
                self.directive = schema.get_directive(node.name.value()).cloned();
            }
            NodeRef::OperationDefinition(node) => {
                let root_type = schema.get_root_type(node.operation).map(Type::from);
                self.type_stack.push(root_type.filter(is_object_type));
            }
            NodeRef::InlineFragment(node) => self.enter_fragment(node.type_condition.as_ref()),
            NodeRef::FragmentDefinition(node) => self.enter_fragment(Some(&node.type_condition)),
            NodeRef::VariableDefinition(node) => {
                let input_type = type_from_ast(schema, &node.r#type);
                self.input_type_stack.push(input_type.filter(is_input_type));
            }
            NodeRef::Argument(node) => {
                let arg_def = self.find_argument(node.name.value());
                let arg_type = arg_def
                    .as_ref()
                    .and_then(ArgumentDef::get)
                    .map(|arg_def| arg_def.r#type.clone());
                let default_value = arg_def
                    .as_ref()
                    .and_then(ArgumentDef::get)
                    .map_or(JsValue::Undefined, |arg_def| arg_def.default_value.clone());
                self.argument = arg_def;
                self.default_value_stack.push(default_value);
                self.input_type_stack.push(arg_type.filter(is_input_type));
            }
            NodeRef::ListValue(_) => {
                let list_type = self.get_input_type().map(get_nullable_type);
                let item_type = list_type
                    .map(|list_type| list_type.as_list().map_or(list_type, |list| &list.of_type))
                    .filter(|item_type| is_input_type(item_type))
                    .cloned();
                // List positions never have a default value.
                self.default_value_stack.push(JsValue::Undefined);
                self.input_type_stack.push(item_type);
            }
            NodeRef::ObjectField(node) => {
                let object_type = self.get_input_type().map(get_named_type);
                let input_field = match object_type.as_ref().and_then(NamedType::as_input_object) {
                    Some(input_object) => input_object.get_fields()?.get(node.name.value()),
                    None => None,
                };
                self.default_value_stack.push(
                    input_field.map_or(JsValue::Undefined, |input_field| {
                        input_field.default_value.clone()
                    }),
                );
                self.input_type_stack.push(
                    input_field
                        .map(|input_field| input_field.r#type.clone())
                        .filter(is_input_type),
                );
            }
            NodeRef::EnumValue(node) => {
                let enum_type = self.get_input_type().map(get_named_type);
                let mut enum_value = None;
                if let Some(enum_type) = enum_type.as_ref().and_then(NamedType::as_enum) {
                    enum_value = enum_type
                        .get_values()?
                        .iter()
                        .position(|value| value.name == node.value)
                        .map(|index| EnumValueDef {
                            enum_type: Arc::clone(enum_type),
                            index,
                        });
                }
                self.enum_value = enum_value;
            }
            // Ignore other nodes.
            _ => {}
        }
        Ok(())
    }

    /// Enters an inline fragment or a fragment definition, whose type is
    /// its type condition or, without one, the named type of the current
    /// type.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    fn enter_fragment(&mut self, type_condition: Option<&NamedTypeNode>) {
        let output_type = match type_condition {
            Some(type_condition_ast) => named_type_from_ast(self.schema, type_condition_ast),
            None => self.get_type().map(get_named_type).map(Type::from),
        };
        self.type_stack.push(output_type.filter(is_output_type));
    }

    /// Returns the argument named `name` of the current directive, or of the
    /// current field when no directive is current.
    fn find_argument(&self, name: &str) -> Option<ArgumentDef> {
        if let Some(directive) = &self.directive {
            let index = directive.args.iter().position(|arg| arg.name == name)?;
            return Some(ArgumentDef::Directive(Arc::clone(directive), index));
        }
        let field_def = self.field_def_stack.last()?.as_ref()?;
        let index = field_def
            .get()?
            .args
            .iter()
            .position(|arg| arg.name == name)?;
        Some(ArgumentDef::Field(field_def.clone(), index))
    }

    /// Leaves `node` and pops what its enter pushed.
    ///
    /// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
    pub fn leave(&mut self, node: NodeRef<'_>) {
        match node {
            NodeRef::SelectionSet(_) => {
                self.parent_type_stack.pop();
            }
            NodeRef::Field(_) => {
                self.field_def_stack.pop();
                self.type_stack.pop();
            }
            NodeRef::Directive(_) => {
                self.directive = None;
            }
            NodeRef::OperationDefinition(_)
            | NodeRef::InlineFragment(_)
            | NodeRef::FragmentDefinition(_) => {
                self.type_stack.pop();
            }
            NodeRef::VariableDefinition(_) => {
                self.input_type_stack.pop();
            }
            NodeRef::Argument(_) => {
                self.argument = None;
                self.default_value_stack.pop();
                self.input_type_stack.pop();
            }
            NodeRef::ListValue(_) | NodeRef::ObjectField(_) => {
                self.default_value_stack.pop();
                self.input_type_stack.pop();
            }
            NodeRef::EnumValue(_) => {
                self.enum_value = None;
            }
            // Ignore other nodes.
            _ => {}
        }
    }
}

/// The name of the `__schema` meta field.
const SCHEMA_META_FIELD_NAME: &str = "__schema";

/// The name of the `__type` meta field.
const TYPE_META_FIELD_NAME: &str = "__type";

/// The name of the `__typename` meta field.
const TYPE_NAME_META_FIELD_NAME: &str = "__typename";

/// Returns the field definition of `field_node` on `parent_type`.
///
/// Not exactly the same as the executor's definition of getFieldDef, in this
/// statically evaluated environment we do not always have an Object type, and
/// need to handle Interface and Union types. The
/// `schema.getQueryType() === parentType` of the reference is the identity of
/// the query type of `schema` and `parent_type`, which a [`NamedType`]
/// compares by.
///
/// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
///
/// # Errors
///
/// Returns the error of the field thunk of `parent_type`, or of a meta
/// field definition.
pub fn get_field_def(
    schema: &Schema,
    parent_type: &NamedType,
    field_node: &FieldNode,
) -> Result<Option<FieldDef>, DefinitionError> {
    let name = field_node.name.value();
    if name == SCHEMA_META_FIELD_NAME && is_query_type(schema, parent_type) {
        return Ok(Some(FieldDef::Meta(schema_meta_field_def()?)));
    }
    if name == TYPE_META_FIELD_NAME && is_query_type(schema, parent_type) {
        return Ok(Some(FieldDef::Meta(type_meta_field_def()?)));
    }
    if name == TYPE_NAME_META_FIELD_NAME && is_composite_type(&Type::from(parent_type)) {
        return Ok(Some(FieldDef::Meta(type_name_meta_field_def())));
    }
    match parent_type {
        NamedType::Object(object) => Ok(object
            .get_fields()?
            .get_index_of(name)
            .map(|index| FieldDef::Object(Arc::clone(object), index))),
        NamedType::Interface(interface) => Ok(interface
            .get_fields()?
            .get_index_of(name)
            .map(|index| FieldDef::Interface(Arc::clone(interface), index))),
        _ => Ok(None),
    }
}

/// Returns whether `parent_type` is the query type of `schema`, by
/// identity.
fn is_query_type(schema: &Schema, parent_type: &NamedType) -> bool {
    schema.get_query_type() == Some(parent_type)
}

/// Returns the type of `schema` that a type condition names.
fn named_type_from_ast(schema: &Schema, node: &NamedTypeNode) -> Option<Type> {
    schema.get_type(node.name.value()).map(Type::from)
}

/// A visitor which maintains a provided [`TypeInfo`] instance along with a
/// visiting visitor.
///
/// Built by [`visit_with_type_info()`]. The reference leaves the type
/// information again when a hook answers anything but `undefined`, because the
/// walk then skips the leave of that node, and re-enters an edited node, which
/// a read only walk never produces.
///
/// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
///
/// The type information of the reference is one object that this visitor moves
/// and that the visiting visitor reads inside its hooks, through the
/// validation context that holds it. A `RefCell` behind a shared borrow is
/// that one object here. This visitor borrows it mutably around each
/// [`TypeInfo::enter()`] and [`TypeInfo::leave()`] and holds no borrow while a
/// hook runs, so a hook reads it with `borrow()`. A failing enter, and a
/// borrow a hook kept across its return, stop the walk, and the error waits in
/// [`VisitWithTypeInfo::into_error()`].
pub struct VisitWithTypeInfo<'t, 's, V: ?Sized> {
    /// The type information that moves with the walk.
    type_info: &'t RefCell<TypeInfo<'s>>,
    /// The visiting visitor.
    visitor: &'t mut V,
    /// The error that stopped the walk, when one did.
    error: Option<DefinitionError>,
}

impl<V: ?Sized> VisitWithTypeInfo<'_, '_, V> {
    /// Returns the error that stopped the walk, when one did.
    #[must_use]
    pub fn into_error(self) -> Option<DefinitionError> {
        self.error
    }

    /// Enters `node` in the type information.
    fn enter_type_info(&self, node: NodeRef<'_>) -> Result<(), DefinitionError> {
        self.type_info
            .try_borrow_mut()
            .map_err(|_| type_info_in_use())?
            .enter(node)
    }

    /// Leaves `node` in the type information.
    fn leave_type_info(&self, node: NodeRef<'_>) -> Result<(), DefinitionError> {
        self.type_info
            .try_borrow_mut()
            .map_err(|_| type_info_in_use())?
            .leave(node);
        Ok(())
    }

    /// Keeps `result` of a type information step, and answers whether the
    /// walk stops.
    fn stops(&mut self, result: Result<(), DefinitionError>) -> bool {
        match result {
            Ok(()) => false,
            Err(error) => {
                self.error = Some(error);
                true
            }
        }
    }
}

/// Returns the error of a hook that kept its borrow of the type information
/// across its return, which leaves this visitor unable to move it.
///
/// The visitor of the walk that edits refuses the same way (see
/// [`crate::type_info_edit`]).
pub(crate) fn type_info_in_use() -> DefinitionError {
    DefinitionError::plain("The type information is borrowed by a visitor hook that returned.")
}

impl<'a, V> Visitor<'a> for VisitWithTypeInfo<'_, '_, V>
where
    V: Visitor<'a> + ?Sized,
{
    fn enter(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult {
        let entered = self.enter_type_info(node);
        if self.stops(entered) {
            return VisitResult::Break;
        }
        let result = enter_for_kind(&mut *self.visitor, node, context);
        if result != VisitResult::Continue {
            let left = self.leave_type_info(node);
            if self.stops(left) {
                return VisitResult::Break;
            }
        }
        result
    }

    fn leave(&mut self, node: NodeRef<'a>, context: &VisitContext<'a, '_>) -> VisitResult {
        let result = leave_for_kind(&mut *self.visitor, node, context);
        let left = self.leave_type_info(node);
        if self.stops(left) {
            return VisitResult::Break;
        }
        result
    }
}

/// Creates a new visitor instance which maintains a provided [`TypeInfo`]
/// instance along with visiting visitor.
///
/// `type_info` is entered before and left after every hook of `visitor`, and
/// `visitor` reads it inside a hook through the same cell.
///
/// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
///
/// # Examples
///
/// ```ignore
/// let type_info = RefCell::new(TypeInfo::new(&schema, None, None));
/// let mut rule = Rule { type_info: &type_info };
/// let mut visitor = visit_with_type_info(&type_info, &mut rule);
/// visit(&document, &mut visitor);
/// if let Some(error) = visitor.into_error() {
///     return Err(error);
/// }
/// ```
pub fn visit_with_type_info<'t, 's, V: ?Sized>(
    type_info: &'t RefCell<TypeInfo<'s>>,
    visitor: &'t mut V,
) -> VisitWithTypeInfo<'t, 's, V> {
    VisitWithTypeInfo {
        type_info,
        visitor,
        error: None,
    }
}

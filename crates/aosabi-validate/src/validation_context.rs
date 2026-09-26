// SPDX-License-Identifier: MIT
/*
 * aosabi-validate: the validation contexts, through which every rule reads
 * the document, the schema and the type information and reports its errors.
 *
 * Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
 */

//! The validation contexts of the validation layer.
//!
//! [`AstValidationContext`] is the base every rule reports to and reads the
//! document and its fragments through, [`SdlValidationContext`] adds the
//! schema a schema document extends, and [`ValidationContext`] adds the
//! schema, the variable usages and the type information of the walk. A rule
//! is a visitor built over one of them, and [`Rule`] is the shape every rule
//! file follows.
//!
//! # The shape of the port
//!
//! The two derived classes of the reference extend the base class. Each
//! derived context here holds the base and dereferences to it, so a rule
//! calls [`AstValidationContext::report_error()`] on any of the three, and a
//! borrow of a derived context passes where a rule asks for the base. A rule
//! the reference types as `ValidationContext | SDLValidationContext` takes
//! [`SdlOrValidationContext`], which is that union.
//!
//! The reference shares one context between every rule of a walk, and the
//! context fills its caches and reports errors behind that shared reference.
//! Every method here takes `&self` for the same reason, and the caches, the
//! error callback and the stop state sit in cells. The lifetime `'a` is the
//! validation, which is the document, the schema, the type information and
//! the error callback together, so a rule keeps a node or the schema as
//! long as it lives.
//!
//! A `Map` of the reference that is keyed by a node finds a key by object
//! identity. A node never moves while a document is borrowed, so the address
//! of a node is its identity here, as it is in the parallel visitor of the
//! language layer.
//!
//! An exception of the reference leaves the walk at once. A hook returns a
//! [`VisitResult`] here and cannot unwind, so the context keeps a stop state
//! in the place of the exception. [`AstValidationContext::report_error()`]
//! sets it when the error callback aborts, a rule sets it through
//! [`AstValidationContext::throw_error()`] with the error a thunk of the
//! schema returns, no error is reported once it is set, and the walk of
//! [`crate::validate()`] breaks at the next hook. The errors a caller
//! receives are therefore the ones the reference collects before its
//! exception.

use alloc::{
    boxed::Box,
    rc::Rc,
    vec::Vec, //
};
use core::{
    cell::{
        Cell,
        OnceCell,
        Ref,
        RefCell, //
    },
    fmt,
    ops::Deref, //
};

use aosabi_json::{
    JsValue,
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    GraphQLError,
    ast::{
        DefinitionNode,
        DocumentNode,
        FragmentDefinitionNode,
        FragmentSpreadNode,
        OperationDefinitionNode,
        SelectionNode,
        SelectionSetNode,
        VariableDefinitionNode,
        VariableNode, //
    },
    visitor::{
        EnterLeave,
        NodeRef,
        VisitContext,
        VisitResult,
        Visitor,
        visit, //
    }, //
};
use aosabi_type::{
    Argument,
    DefinitionError,
    Directive,
    EnumValue,
    Field,
    NamedType,
    Schema,
    Type,
    TypeInfo,
    visit_with_type_info, //
};

/// A map in insertion order with the hasher of the workspace, the carrier of
/// every `Map` and null prototype object of a context.
type OrderedMap<K, V> = IndexMap<K, V, KeyHasherBuilder>;

/// Returns the address of `node`, which is the identity the reference
/// compares a `Map` key by.
pub(crate) fn address_of<T>(node: &T) -> usize {
    core::ptr::from_ref(node).addr()
}

/// The answer of an error callback that aborts the validation, the
/// `abortObj` the reference throws.
///
/// Derived from graphql-js 16.12.0, src/validation/validate.ts, MIT.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidationAbort;

/// The error callback of a context, the `onError` of the reference.
///
/// The callback of the reference either returns or throws, and `Err` is the
/// throw.
///
/// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
pub type OnError<'a> = Box<dyn FnMut(GraphQLError) -> Result<(), ValidationAbort> + 'a>;

/// An operation or a fragment definition, the `NodeWithSelectionSet` of the
/// reference.
///
/// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
#[derive(Debug, Clone, Copy)]
pub enum NodeWithSelectionSet<'a> {
    /// An operation definition.
    OperationDefinition(&'a OperationDefinitionNode),
    /// A fragment definition.
    FragmentDefinition(&'a FragmentDefinitionNode),
}

impl<'a> NodeWithSelectionSet<'a> {
    /// Returns the identity of the node, which keeps the kind beside the
    /// address so that the two kinds never share a key.
    fn key(self) -> NodeKey {
        match self {
            Self::OperationDefinition(node) => NodeKey::OperationDefinition(address_of(node)),
            Self::FragmentDefinition(node) => NodeKey::FragmentDefinition(address_of(node)),
        }
    }

    /// Returns the node as the view the walk takes.
    fn node_ref(self) -> NodeRef<'a> {
        match self {
            Self::OperationDefinition(node) => NodeRef::from(node),
            Self::FragmentDefinition(node) => NodeRef::from(node),
        }
    }
}

impl<'a> From<&'a OperationDefinitionNode> for NodeWithSelectionSet<'a> {
    fn from(node: &'a OperationDefinitionNode) -> Self {
        Self::OperationDefinition(node)
    }
}

impl<'a> From<&'a FragmentDefinitionNode> for NodeWithSelectionSet<'a> {
    fn from(node: &'a FragmentDefinitionNode) -> Self {
        Self::FragmentDefinition(node)
    }
}

/// The identity of a [`NodeWithSelectionSet`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum NodeKey {
    /// The address of an operation definition.
    OperationDefinition(usize),
    /// The address of a fragment definition.
    FragmentDefinition(usize),
}

/// One usage of a variable, with what the type information holds there.
///
/// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
#[derive(Debug, Clone)]
pub struct VariableUsage<'a> {
    /// The variable.
    pub node: &'a VariableNode,
    /// The input type the position expects, when the schema knows one.
    pub r#type: Option<Type>,
    /// The default value of the position, absent where the type information
    /// holds none and `undefined` where the position declares none.
    pub default_value: Option<JsValue>,
    /// The input type of the enclosing input position, when there is one.
    pub parent_type: Option<Type>,
}

/// An instance of this class is passed as the "this" context to all
/// validators, allowing access to commonly useful contextual information
/// from within a validation rule.
///
/// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
pub struct AstValidationContext<'a> {
    /// The document.
    ast: &'a DocumentNode,
    /// The error callback.
    on_error: RefCell<OnError<'a>>,
    /// The fragment definitions by name, built on the first lookup.
    fragments: OnceCell<OrderedMap<&'a str, &'a FragmentDefinitionNode>>,
    /// The fragment spreads below a selection set, by its address.
    fragment_spreads: RefCell<OrderedMap<usize, Rc<[&'a FragmentSpreadNode]>>>,
    /// The fragments an operation reaches, by its address.
    recursively_referenced_fragments: RefCell<OrderedMap<usize, Rc<[&'a FragmentDefinitionNode]>>>,
    /// Whether the error callback aborted the validation.
    aborted: Cell<bool>,
    /// The error a rule threw, when one did before the callback aborted.
    thrown: RefCell<Option<DefinitionError>>,
}

impl<'a> AstValidationContext<'a> {
    /// Creates the context of `ast`, which reports to `on_error`.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn new(ast: &'a DocumentNode, on_error: OnError<'a>) -> Self {
        Self {
            ast,
            on_error: RefCell::new(on_error),
            fragments: OnceCell::new(),
            fragment_spreads: RefCell::new(OrderedMap::default()),
            recursively_referenced_fragments: RefCell::new(OrderedMap::default()),
            aborted: Cell::new(false),
            thrown: RefCell::new(None),
        }
    }

    /// Reports `error` to the error callback.
    ///
    /// A callback that answers [`ValidationAbort`] stops the validation, and
    /// a stopped validation drops `error`, because the exception of the
    /// reference leaves no hook behind that reports.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    ///
    /// # Panics
    ///
    /// Panics when the error callback reports to the context it belongs to.
    pub fn report_error(&self, error: GraphQLError) {
        if self.is_stopped() {
            return;
        }
        let reported = (self.on_error.borrow_mut())(error);
        if reported.is_err() {
            self.aborted.set(true);
        }
    }

    /// Stops the validation with `error`, the `throw` of a rule.
    ///
    /// A thunk of the schema throws in the reference and returns a
    /// [`DefinitionError`] here. A hook that receives one hands it to this
    /// method and returns the answer, which is [`VisitResult::Break`]. The
    /// first stop wins, as the first exception of the reference does, so
    /// `error` is dropped when the validation is stopped already.
    pub fn throw_error(&self, error: DefinitionError) -> VisitResult {
        if !self.is_stopped() {
            *self.thrown.borrow_mut() = Some(error);
        }
        VisitResult::Break
    }

    /// Returns whether the error callback aborted or a rule threw.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.aborted.get() || self.thrown.borrow().is_some()
    }

    /// Takes the error a rule threw, which the driver of the walk returns
    /// where the reference lets the exception pass.
    #[must_use]
    pub fn take_thrown(&self) -> Option<DefinitionError> {
        self.thrown.borrow_mut().take()
    }

    /// Returns the document.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub const fn get_document(&self) -> &'a DocumentNode {
        self.ast
    }

    /// Returns the fragment definition named `name`.
    ///
    /// The reference assigns every fragment definition to a property of a
    /// null prototype object, so of two definitions with one name the later
    /// one is the fragment, as an assignment to an existing property replaces
    /// its value.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_fragment(&self, name: &str) -> Option<&'a FragmentDefinitionNode> {
        let fragments = self.fragments.get_or_init(|| {
            let mut fragments = OrderedMap::default();
            for def_node in &self.get_document().definitions {
                if let DefinitionNode::FragmentDefinition(def_node) = def_node {
                    fragments.insert(def_node.name.value(), def_node);
                }
            }
            fragments
        });
        fragments.get(name).copied()
    }

    /// Returns every fragment spread below the selection set `node`, the
    /// ones inside nested fields and inline fragments included.
    ///
    /// The sets to visit are a stack, so the spreads below a later sibling
    /// come before the spreads below an earlier one, which is the order the
    /// rules report in.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_fragment_spreads(&self, node: &'a SelectionSetNode) -> Rc<[&'a FragmentSpreadNode]> {
        let key = address_of(node);
        if let Some(spreads) = self.fragment_spreads.borrow().get(&key) {
            return Rc::clone(spreads);
        }
        let mut spreads: Vec<&'a FragmentSpreadNode> = Vec::new();
        let mut sets_to_visit: Vec<&'a SelectionSetNode> = alloc::vec![node];
        while let Some(set) = sets_to_visit.pop() {
            for selection in set.selections.iter() {
                match selection {
                    SelectionNode::FragmentSpread(selection) => spreads.push(selection),
                    // The reference tests `selection.selectionSet` for
                    // truthiness, and an object is truthy where `undefined`
                    // is not.
                    SelectionNode::Field(selection) => {
                        sets_to_visit.extend(selection.selection_set.as_ref());
                    }
                    SelectionNode::InlineFragment(selection) => {
                        sets_to_visit.push(&selection.selection_set);
                    }
                }
            }
        }
        let spreads: Rc<[&'a FragmentSpreadNode]> = Rc::from(spreads);
        self.fragment_spreads
            .borrow_mut()
            .insert(key, Rc::clone(&spreads));
        spreads
    }

    /// Returns every fragment `operation` reaches through its spreads, each
    /// once, in the order the spreads reach them.
    ///
    /// A spread of an unknown fragment collects its name and nothing else.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_recursively_referenced_fragments(
        &self,
        operation: &'a OperationDefinitionNode,
    ) -> Rc<[&'a FragmentDefinitionNode]> {
        let key = address_of(operation);
        if let Some(fragments) = self.recursively_referenced_fragments.borrow().get(&key) {
            return Rc::clone(fragments);
        }
        let mut fragments: Vec<&'a FragmentDefinitionNode> = Vec::new();
        let mut collected_names: OrderedMap<&'a str, bool> = OrderedMap::default();
        let mut nodes_to_visit: Vec<&'a SelectionSetNode> = alloc::vec![&operation.selection_set];
        while let Some(node) = nodes_to_visit.pop() {
            for spread in self.get_fragment_spreads(node).iter() {
                let frag_name = spread.name.value();
                if collected_names.get(frag_name) != Some(&true) {
                    collected_names.insert(frag_name, true);
                    let fragment = self.get_fragment(frag_name);
                    if let Some(fragment) = fragment {
                        fragments.push(fragment);
                        nodes_to_visit.push(&fragment.selection_set);
                    }
                }
            }
        }
        let fragments: Rc<[&'a FragmentDefinitionNode]> = Rc::from(fragments);
        self.recursively_referenced_fragments
            .borrow_mut()
            .insert(key, Rc::clone(&fragments));
        fragments
    }
}

impl fmt::Debug for AstValidationContext<'_> {
    /// Writes the `Symbol.toStringTag` of the reference, translation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ASTValidationContext")
            .finish_non_exhaustive()
    }
}

/// The context of an SDL validation rule.
///
/// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
pub struct SdlValidationContext<'a> {
    /// The base the reference extends.
    base: AstValidationContext<'a>,
    /// The schema the document extends, when it extends one.
    schema: Option<&'a Schema>,
}

impl<'a> SdlValidationContext<'a> {
    /// Creates the context of the schema document `ast`, which extends
    /// `schema` when one is given and reports to `on_error`.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn new(ast: &'a DocumentNode, schema: Option<&'a Schema>, on_error: OnError<'a>) -> Self {
        Self {
            base: AstValidationContext::new(ast, on_error),
            schema,
        }
    }

    /// Returns the schema the document extends, when it extends one.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub const fn get_schema(&self) -> Option<&'a Schema> {
        self.schema
    }
}

impl<'a> Deref for SdlValidationContext<'a> {
    type Target = AstValidationContext<'a>;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl fmt::Debug for SdlValidationContext<'_> {
    /// Writes the `Symbol.toStringTag` of the reference, translation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SDLValidationContext")
            .finish_non_exhaustive()
    }
}

/// The context of a validation rule over an executable document.
///
/// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
///
/// The type information is the one cell the walk moves through
/// [`visit_with_type_info()`], which holds no borrow of it while a hook
/// runs. An accessor here borrows it for the length of the call and answers
/// with a clone where a clone is a reference count, which is a type, a
/// named type and a directive. A field, an argument and an enum value own
/// their definition node, so their accessors answer with a [`Ref`], which a
/// hook drops before it returns.
pub struct ValidationContext<'a> {
    /// The base the reference extends.
    base: AstValidationContext<'a>,
    /// The schema.
    schema: &'a Schema,
    /// The type information the walk moves.
    type_info: &'a RefCell<TypeInfo<'a>>,
    /// The variable usages below an operation or a fragment definition, by
    /// its identity.
    variable_usages: RefCell<OrderedMap<NodeKey, Rc<[VariableUsage<'a>]>>>,
    /// The variable usages of an operation and of every fragment it reaches,
    /// by its address.
    recursive_variable_usages: RefCell<OrderedMap<usize, Rc<[VariableUsage<'a>]>>>,
}

impl<'a> ValidationContext<'a> {
    /// Creates the context of the document `ast` under `schema`, which reads
    /// `type_info` and reports to `on_error`.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn new(
        schema: &'a Schema,
        ast: &'a DocumentNode,
        type_info: &'a RefCell<TypeInfo<'a>>,
        on_error: OnError<'a>,
    ) -> Self {
        Self {
            base: AstValidationContext::new(ast, on_error),
            schema,
            type_info,
            variable_usages: RefCell::new(OrderedMap::default()),
            recursive_variable_usages: RefCell::new(OrderedMap::default()),
        }
    }

    /// Returns the schema.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub const fn get_schema(&self) -> &'a Schema {
        self.schema
    }

    /// Returns every usage of a variable below `node`, with the type
    /// information of each.
    ///
    /// The walk runs under a type information of its own and skips the
    /// variable definitions, so the variable a definition declares is no
    /// usage.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what a thunk of the schema throws while the type information
    /// enters a node.
    pub fn get_variable_usages(
        &self,
        node: impl Into<NodeWithSelectionSet<'a>>,
    ) -> Result<Rc<[VariableUsage<'a>]>, DefinitionError> {
        let node = node.into();
        if let Some(usages) = self.variable_usages.borrow().get(&node.key()) {
            return Ok(Rc::clone(usages));
        }
        let type_info = RefCell::new(TypeInfo::new(self.schema, None, None));
        let mut collector = VariableUsageCollector {
            type_info: &type_info,
            new_usages: Vec::new(),
        };
        let mut visitor = visit_with_type_info(&type_info, &mut collector);
        visit(node.node_ref(), &mut visitor);
        if let Some(error) = visitor.into_error() {
            return Err(error);
        }
        let usages: Rc<[VariableUsage<'a>]> = Rc::from(collector.new_usages);
        self.variable_usages
            .borrow_mut()
            .insert(node.key(), Rc::clone(&usages));
        Ok(usages)
    }

    /// Returns every usage of a variable below `operation` and below every
    /// fragment it reaches.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    ///
    /// # Errors
    ///
    /// Returns what [`ValidationContext::get_variable_usages()`] returns.
    pub fn get_recursive_variable_usages(
        &self,
        operation: &'a OperationDefinitionNode,
    ) -> Result<Rc<[VariableUsage<'a>]>, DefinitionError> {
        let key = address_of(operation);
        if let Some(usages) = self.recursive_variable_usages.borrow().get(&key) {
            return Ok(Rc::clone(usages));
        }
        let mut usages: Vec<VariableUsage<'a>> = self.get_variable_usages(operation)?.to_vec();
        for frag in self.get_recursively_referenced_fragments(operation).iter() {
            usages.extend_from_slice(&self.get_variable_usages(*frag)?);
        }
        let usages: Rc<[VariableUsage<'a>]> = Rc::from(usages);
        self.recursive_variable_usages
            .borrow_mut()
            .insert(key, Rc::clone(&usages));
        Ok(usages)
    }

    /// Returns the type information the walk moves, borrowed.
    ///
    /// A hook reads through it what no accessor of the context answers,
    /// and drops the borrow before it returns.
    ///
    /// # Panics
    ///
    /// Panics when called outside a hook while the walk moves the type
    /// information.
    #[must_use]
    pub fn type_info(&self) -> Ref<'_, TypeInfo<'a>> {
        self.type_info.borrow()
    }

    /// Returns the current output type.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_type(&self) -> Option<Type> {
        self.type_info().get_type().cloned()
    }

    /// Returns the current composite parent type.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_parent_type(&self) -> Option<NamedType> {
        self.type_info().get_parent_type().cloned()
    }

    /// Returns the current input type.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_input_type(&self) -> Option<Type> {
        self.type_info().get_input_type().cloned()
    }

    /// Returns the input type of the enclosing input position.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_parent_input_type(&self) -> Option<Type> {
        self.type_info().get_parent_input_type().cloned()
    }

    /// Returns the current field definition, borrowed from the type
    /// information.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_field_def(&self) -> Option<Ref<'_, Field>> {
        Ref::filter_map(self.type_info(), TypeInfo::get_field_def).ok()
    }

    /// Returns the current directive.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_directive(&self) -> Option<alloc::sync::Arc<Directive>> {
        self.type_info().get_directive().cloned()
    }

    /// Returns the current argument, borrowed from the type information.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_argument(&self) -> Option<Ref<'_, Argument>> {
        Ref::filter_map(self.type_info(), TypeInfo::get_argument).ok()
    }

    /// Returns the current enum value, borrowed from the type information.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    #[must_use]
    pub fn get_enum_value(&self) -> Option<Ref<'_, EnumValue>> {
        Ref::filter_map(self.type_info(), TypeInfo::get_enum_value).ok()
    }
}

impl<'a> Deref for ValidationContext<'a> {
    type Target = AstValidationContext<'a>;

    fn deref(&self) -> &Self::Target {
        &self.base
    }
}

impl fmt::Debug for ValidationContext<'_> {
    /// Writes the `Symbol.toStringTag` of the reference, translation.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ValidationContext").finish_non_exhaustive()
    }
}

/// The visitor of [`ValidationContext::get_variable_usages()`], the object
/// literal.
struct VariableUsageCollector<'t, 'a> {
    /// The type information of this walk.
    type_info: &'t RefCell<TypeInfo<'a>>,
    /// The usages the walk reaches.
    new_usages: Vec<VariableUsage<'a>>,
}

impl<'a> Visitor<'a> for VariableUsageCollector<'_, 'a> {
    fn enter_variable_definition(
        &mut self,
        _node: &'a VariableDefinitionNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        VisitResult::Skip
    }

    fn enter_variable(
        &mut self,
        variable: &'a VariableNode,
        _context: &VisitContext<'a, '_>,
    ) -> VisitResult {
        let type_info = self.type_info.borrow();
        self.new_usages.push(VariableUsage {
            node: variable,
            r#type: type_info.get_input_type().cloned(),
            default_value: type_info.get_default_value().cloned(),
            parent_type: type_info.get_parent_input_type().cloned(),
        });
        VisitResult::Continue
    }
}

/// A validation or an SDL validation context, the
/// `ValidationContext | SDLValidationContext` of the reference.
///
/// The union dereferences to the base, and
/// [`SdlOrValidationContext::get_schema()`] is the `getSchema` both members
/// share, whose answer may be absent.
///
/// Derived from graphql-js 16.12.0, src/validation/rules/KnownTypeNamesRule.ts, MIT.
#[derive(Debug, Clone, Copy)]
pub enum SdlOrValidationContext<'c, 'a> {
    /// The context of a schema document.
    Sdl(&'c SdlValidationContext<'a>),
    /// The context of an executable document.
    Validation(&'c ValidationContext<'a>),
}

impl<'a> SdlOrValidationContext<'_, 'a> {
    /// Returns the schema, which a schema document that extends nothing has
    /// none of.
    #[must_use]
    pub const fn get_schema(self) -> Option<&'a Schema> {
        match self {
            Self::Sdl(context) => context.get_schema(),
            Self::Validation(context) => Some(context.get_schema()),
        }
    }
}

impl<'a> Deref for SdlOrValidationContext<'_, 'a> {
    type Target = AstValidationContext<'a>;

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Sdl(context) => context,
            Self::Validation(context) => context,
        }
    }
}

impl<'c, 'a> From<&'c SdlValidationContext<'a>> for SdlOrValidationContext<'c, 'a> {
    fn from(context: &'c SdlValidationContext<'a>) -> Self {
        Self::Sdl(context)
    }
}

impl<'c, 'a> From<&'c ValidationContext<'a>> for SdlOrValidationContext<'c, 'a> {
    fn from(context: &'c ValidationContext<'a>) -> Self {
        Self::Validation(context)
    }
}

/// A validation rule, a visitor built over a context.
///
/// The reference writes a rule as a function that takes the context and
/// returns a visitor object whose hooks close over the context and over the
/// state of the rule. A rule here is a struct that holds the context and that
/// state, its [`Visitor`] implementation holds the hooks of the visitor
/// object, and [`Rule::new()`] is the body of the function above its
/// `return`. `'c` is the borrow of the context and `'a` the validation.
///
/// Every rule file pairs the struct with a function that carries the name
/// of the reference in snake case and answers [`Rule::boxed()`], because a
/// function whose lifetimes are its own is what a rule list holds. The box
/// holds the rule as an [`EnterLeave`] object, whose two methods dispatch
/// over the kinds of the nodes to the hooks of the rule.
///
/// # Examples
///
/// ```ignore
/// pub struct ScalarLeafsRule<'c, 'a> {
///     context: &'c ValidationContext<'a>,
/// }
///
/// impl<'c, 'a> Rule<'c, 'a> for ScalarLeafsRule<'c, 'a> {
///     type Context = &'c ValidationContext<'a>;
///
///     fn new(context: Self::Context) -> Self {
///         Self { context }
///     }
/// }
///
/// impl<'a> Visitor<'a> for ScalarLeafsRule<'_, 'a> {
///     fn enter_field(&mut self, node: &'a FieldNode, _: &VisitContext<'a, '_>) -> VisitResult {
///         self.check(node).unwrap_or_else(|error| self.context.throw_error(error))
///     }
/// }
///
/// pub fn scalar_leafs_rule<'c, 'a>(
///     context: &'c ValidationContext<'a>,
/// ) -> Box<dyn EnterLeave<'a> + 'c> {
///     ScalarLeafsRule::boxed(context)
/// }
/// ```
pub trait Rule<'c, 'a>: Visitor<'a> + Sized + 'c {
    /// The context the rule takes, which is a borrow of one of the three
    /// contexts for `'c`, or [`SdlOrValidationContext`].
    type Context;

    /// Creates the rule over `context`.
    fn new(context: Self::Context) -> Self;

    /// Creates the rule over `context` as the boxed visitor a rule list
    /// holds, an [`EnterLeave`] object of two methods.
    #[must_use]
    fn boxed(context: Self::Context) -> Box<dyn EnterLeave<'a> + 'c> {
        Box::new(Self::new(context))
    }
}

/// Declares a rule value, the function of the reference with its `name`.
///
/// A function of the reference carries its declared name as the `name`
/// property, and the harness picks a rule by it. A function pointer carries
/// none, so a rule value is the pointer beside the name.
macro_rules! rule_value {
    ($(#[$meta:meta])* $rule:ident, $context:ident) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $rule {
            /// The name of the rule function in the reference.
            pub name: &'static str,
            /// The rule function, which builds the visitor over a context.
            pub create: for<'c, 'a> fn(&'c $context<'a>) -> Box<dyn EnterLeave<'a> + 'c>,
        }
    };
}

rule_value! {
    /// A rule over any document, the `ASTValidationRule` of the reference.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    AstValidationRule, AstValidationContext
}

rule_value! {
    /// A rule over a schema document, the `SDLValidationRule` of the
    /// reference.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    SdlValidationRule, SdlValidationContext
}

rule_value! {
    /// A rule over an executable document, the `ValidationRule` of the
    /// reference.
    ///
    /// Derived from graphql-js 16.12.0, src/validation/ValidationContext.ts, MIT.
    ValidationRule, ValidationContext
}

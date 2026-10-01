// SPDX-License-Identifier: MIT
/*
 * aosabi-infer: the descriptor source, the first implementation of the
 * TypeSource seam, which answers the questions of the inference from a
 * descriptor graph.
 *
 * No file of the reference is translated here.
 */

//! The descriptor source.
//!
//! The five translated modules never import this module.
//!
//! | File | Holds |
//! | --- | --- |
//! | `graph.rs` | the arena, its two constructors and its dump |
//! | `present.rs` | the checker view of each node kind |
//! | `assignable.rs` | `is_type_assignable_to`, the structural comparison |
//! | `heritage.rs` | `get_heritage_declarations` |
//! | `authoring.rs` | the authoring map of a build, the Rust half of `12_authoring_parity` |
//!
//! # Handles
//!
//! A type, a symbol and a signature of this source are `u32` positions in
//! three interning tables. The tables hold more than the nodes of the graph,
//! because the checker view needs types no descriptor names, such as
//! `undefined`, the literals `false` and `true`, the string literal of an
//! enum value, the function type of a method and a union the inference asks
//! the source to build. Interning by value gives each of them one handle,
//! which is the identity the reference gets from the checker.
//!
//! The tables grow behind a `RefCell` while the source is shared, because a
//! method of the seam takes `&self`. No borrow outlives the helper that
//! takes it, so two borrows never meet.

mod assignable;
mod authoring;
mod graph;
mod heritage;
mod present;

use std::cell::RefCell;

use aosabi_ir::RootName;
use aosabi_json::{
    map::IndexSet,
    value::KeyHasherBuilder, //
};

use self::present::{
    SignatureData,
    SymbolData,
    TypeData, //
};
pub use self::{
    authoring::{
        AuthoringGuard,
        AuthoringGuardCall,
        AuthoringHeritage,
        AuthoringNode, //
    },
    graph::{
        Graph,
        GraphError, //
    },
};
use crate::type_source::{
    HeritageDeclaration,
    JsDocTag,
    ModifierFlags,
    ObjectFlags,
    SymbolFlags,
    TupleElement,
    TypeFlags,
    TypeSource, //
};

/// The handle of a type of a [`DescriptorSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorType(u32);

/// The handle of a symbol of a [`DescriptorSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorSymbol(u32);

/// The handle of a call signature of a [`DescriptorSource`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DescriptorSignature(u32);

/// An interning table, which gives each distinct value one position.
#[derive(Debug)]
struct Interner<T> {
    /// The values in the order they were first seen.
    values: RefCell<IndexSet<T, KeyHasherBuilder>>,
}

impl<T: Clone + Eq + core::hash::Hash> Interner<T> {
    /// Creates an empty table.
    fn new() -> Self {
        Self {
            values: RefCell::new(IndexSet::default()),
        }
    }

    /// Returns the position of `value`, adding it when it is new.
    fn intern(&self, value: T) -> u32 {
        let (index, _) = self.values.borrow_mut().insert_full(value);

        u32::try_from(index).unwrap_or(u32::MAX)
    }

    /// Returns the value at `position`.
    fn get(&self, position: u32) -> Option<T> {
        let index = usize::try_from(position).ok()?;

        self.values.borrow().get_index(index).cloned()
    }
}

/// Answers the questions of the inference from a descriptor graph.
#[derive(Debug)]
pub struct DescriptorSource {
    /// The graph the answers come from.
    graph: Graph,
    /// The types handed out so far.
    types: Interner<TypeData>,
    /// The symbols handed out so far.
    symbols: Interner<SymbolData>,
    /// The signatures handed out so far.
    signatures: Interner<SignatureData>,
}

impl DescriptorSource {
    /// Creates a source over `graph`.
    #[must_use]
    pub fn new(graph: Graph) -> Self {
        Self {
            graph,
            types: Interner::new(),
            symbols: Interner::new(),
            signatures: Interner::new(),
        }
    }

    /// Returns the graph the source runs over.
    #[must_use]
    pub fn graph(&self) -> &Graph {
        &self.graph
    }

    /// Returns the position in the graph of the node the handle `type`
    /// stands for, `None` for a type no single node is, a union the
    /// inference built or a literal among them.
    #[must_use]
    pub fn node_of(&self, r#type: DescriptorType) -> Option<u32> {
        match self.type_data(r#type) {
            TypeData::Node(index) => Some(index),
            _ => None,
        }
    }

    /// Returns the handle of the type `data` describes.
    fn type_of(&self, data: TypeData) -> DescriptorType {
        DescriptorType(self.types.intern(data))
    }

    /// Returns what the handle `type` stands for. A handle of another source
    /// reads as the never type.
    fn type_data(&self, r#type: DescriptorType) -> TypeData {
        self.types.get(r#type.0).unwrap_or(TypeData::Never)
    }

    /// Returns the handle of the symbol `data` describes.
    fn symbol_of(&self, data: SymbolData) -> DescriptorSymbol {
        DescriptorSymbol(self.symbols.intern(data))
    }

    /// Returns what the handle `symbol` stands for.
    fn symbol_data(&self, symbol: DescriptorSymbol) -> Option<SymbolData> {
        self.symbols.get(symbol.0)
    }
}

impl TypeSource for DescriptorSource {
    type Type = DescriptorType;
    type Symbol = DescriptorSymbol;
    type Signature = DescriptorSignature;

    fn type_of_root(&self, root: RootName) -> Option<Self::Type> {
        let slot = match root {
            RootName::Query => 0,
            RootName::Mutation => 1,
            RootName::Subscription => 2,
        };
        let node = self.graph.roots.get(slot).copied().flatten()?;

        Some(self.type_of(TypeData::Node(node)))
    }

    fn type_to_string(&self, r#type: Self::Type) -> String {
        self.print_type(r#type, 0)
    }

    fn get_flags(&self, r#type: Self::Type) -> TypeFlags {
        self.flags_of(r#type)
    }

    fn get_object_flags(&self, r#type: Self::Type) -> ObjectFlags {
        self.object_flags_of(r#type)
    }

    fn get_symbol(&self, r#type: Self::Type) -> Option<Self::Symbol> {
        self.symbol_of_type(r#type)
    }

    fn get_alias_symbol(&self, r#type: Self::Type) -> Option<Self::Symbol> {
        self.alias_symbol_of_type(r#type)
    }

    fn get_name(&self, symbol: Self::Symbol) -> String {
        self.name_of(symbol)
    }

    fn get_symbol_flags(&self, symbol: Self::Symbol) -> SymbolFlags {
        self.symbol_flags_of(symbol)
    }

    fn get_properties(&self, r#type: Self::Type) -> Vec<Self::Symbol> {
        self.properties_of(r#type)
    }

    fn get_property(&self, r#type: Self::Type, name: &str) -> Option<Self::Symbol> {
        self.properties_of(r#type)
            .into_iter()
            .find(|property| self.name_of(*property) == name)
    }

    fn get_type_of_symbol_at_location(&self, symbol: Self::Symbol) -> Self::Type {
        self.type_of_symbol(symbol)
    }

    fn has_value_declaration(&self, symbol: Self::Symbol) -> bool {
        !matches!(self.symbol_data(symbol), Some(SymbolData::Typename { .. }))
    }

    fn get_combined_modifier_flags(&self, symbol: Self::Symbol) -> ModifierFlags {
        if self.is_private(symbol) {
            ModifierFlags::PRIVATE
        } else {
            ModifierFlags::empty()
        }
    }

    fn get_call_signatures(&self, r#type: Self::Type) -> Vec<Self::Signature> {
        self.call_signatures_of(r#type)
    }

    fn get_parameters(&self, signature: Self::Signature) -> Vec<Self::Symbol> {
        self.parameters_of(signature)
    }

    fn get_return_type(&self, signature: Self::Signature) -> Self::Type {
        self.return_type_of(signature)
    }

    fn is_optional_parameter(&self, parameter: Self::Symbol) -> bool {
        self.is_optional(parameter)
    }

    fn is_rest_parameter(&self, _parameter: Self::Symbol) -> bool {
        false
    }

    fn get_labeled_tuple_elements(
        &self,
        _type: Self::Type,
    ) -> Option<Vec<TupleElement<Self::Type, Self::Symbol>>> {
        None
    }

    fn is_union(&self, r#type: Self::Type) -> bool {
        self.flags_of(r#type).contains(TypeFlags::UNION)
    }

    fn is_intersection(&self, r#type: Self::Type) -> bool {
        self.intersection_members_of(r#type).is_some()
    }

    fn get_types(&self, r#type: Self::Type) -> Vec<Self::Type> {
        self.intersection_members_of(r#type)
            .unwrap_or_else(|| self.members_of(r#type))
    }

    fn get_non_nullable_type(&self, r#type: Self::Type) -> Self::Type {
        self.non_nullable_of(r#type)
    }

    fn get_union_type(&self, types: &[Self::Type]) -> Self::Type {
        self.union_of(types)
    }

    fn is_type_assignable_to(&self, source: Self::Type, target: Self::Type) -> bool {
        self.is_assignable(source, target, &mut Vec::new())
    }

    fn get_base_types(&self, r#type: Self::Type) -> Option<Vec<Self::Type>> {
        self.base_types_of(r#type)
    }

    fn get_target(&self, r#type: Self::Type) -> Option<Self::Type> {
        let declared = self.declared_of(r#type);

        (declared != r#type).then_some(declared)
    }

    fn get_type_arguments(&self, r#type: Self::Type) -> Vec<Self::Type> {
        self.type_arguments_of(r#type)
    }

    fn get_alias_type_arguments(&self, r#type: Self::Type) -> Option<Vec<Self::Type>> {
        self.alias_type_arguments_of(r#type)
    }

    fn get_index_type_of_type(&self, r#type: Self::Type) -> Option<Self::Type> {
        self.index_type_of(r#type)
    }

    fn is_array_or_tuple_type_node(&self, r#type: Self::Type) -> bool {
        self.index_type_of(r#type).is_some()
    }

    fn get_type_node_name(&self, r#type: Self::Type) -> Option<String> {
        self.alias_symbol_of_type(r#type)
            .map(|alias| self.name_of(alias))
    }

    fn get_literal_value(&self, r#type: Self::Type) -> Option<String> {
        match self.type_data(r#type) {
            TypeData::StringLiteral(value) => Some(value),
            _ => None,
        }
    }

    fn get_declaration_name(&self, symbol: Self::Symbol) -> Option<String> {
        self.declaration_name_of(symbol)
    }

    fn is_library_type(&self, _type: Self::Type) -> bool {
        false
    }

    fn is_declared_in_file(&self, symbol: Self::Symbol, needle: &str) -> bool {
        self.file_of(symbol).contains(needle)
    }

    fn get_documentation_comment(&self, symbol: Self::Symbol) -> String {
        self.docs_of(symbol)
    }

    fn get_js_doc_tags(&self, symbol: Self::Symbol) -> Vec<JsDocTag> {
        self.tags_of(symbol)
    }

    fn get_heritage_declarations(&self) -> Vec<HeritageDeclaration<Self::Type>> {
        self.heritage_declarations()
    }
}

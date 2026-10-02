// SPDX-License-Identifier: MIT
/*
 * aosabi-build: the third type source, which answers the questions of the
 * inference about the types of a plain crate.
 *
 * No file of the reference is translated here.
 */

//! The types of a plain crate behind the seam `TypeSource`.
//!
//! The inference of `aosabi-infer` asks every question about a type through the
//! trait [`TypeSource`], whose 39 methods are named after the calls of the
//! TypeScript checker. It answers for the types the roots of a plain crate
//! reach, which the lowering of the module `describe` writes from the reader's
//! model as the nodes of a descriptor graph, and the checker view of the
//! descriptor source presents each node as the TypeScript checker presents the
//! construct that means the same. So a struct answers as a class, a unit enum
//! as a TypeScript `enum`, a payload enum as a union alias, `Option<T>` as a
//! nullable, a list as an array, an `async fn` as a promise and a marker as a
//! branded scalar, and the inference applies its rules unchanged.
//!
//! The rules of the source path live in the lowering and in no answer: the
//! names of the rule of `names`, a Rust number by its width, every unit enum
//! a TypeScript `enum`, every inherent block of a type read for the uses it
//! belongs to, the types of path dependencies read as types of the crate, and
//! no inheritance, since Rust states none, so the sweep of the heritage of the
//! program answers no declaration. The derive path and the source path share
//! the checker view, and so a construct both state answers alike on both.

use aosabi_infer::{
    AuthoringNode, DescriptorSource, Graph, GraphError, HeritageDeclaration, JsDocTag,
    ModifierFlags, ObjectFlags, SymbolFlags, TupleElement, TypeFlags, TypeSource,
    descriptor_source::{
        DescriptorSignature,
        DescriptorSymbol,
        DescriptorType, //
    },
};
use aosabi_ir::{
    RootName,
    SchemaIr, //
};
use aosabi_json::JsValue;

use super::{
    Crate,
    Place,
    Refusal,
    describe, //
};

/// Says why a crate has no types of the source path.
#[derive(Debug)]
pub enum Undescribed {
    /// The source states constructs the builder cannot read, each refused at
    /// the place of the author, in the order the reader and the lowering meet
    /// them.
    Refused(Vec<Refusal>),
    /// The lowering wrote a dump that the descriptor graph does not read,
    /// which is a fault of the builder and of no crate.
    Dump(GraphError),
}

/// The types of a plain crate, the third implementation of [`TypeSource`].
#[derive(Debug)]
pub struct SourceTypes {
    /// The checker view of the lowered nodes.
    view: DescriptorSource,
}

impl SourceTypes {
    /// Returns the types the roots of `krate` reach.
    ///
    /// # Errors
    ///
    /// Returns [`Undescribed::Refused`] with every refusal of the reader and
    /// of the lowering, the crate root that holds no entry among them, and
    /// [`Undescribed::Dump`] when the descriptor graph does not read the dump
    /// the lowering wrote.
    pub fn describe(krate: &Crate) -> Result<Self, Undescribed> {
        let mut refusals = krate.refusals().to_vec();
        if krate.entry().is_none() {
            refusals.push(Refusal {
                place: Place {
                    file: krate.root().to_path_buf(),
                    line: 1,
                },
                message: String::from(
                    "the crate root holds no public function that answers an app, `impl IntoApp`, `App` or a `Result` of either, so the builder finds no root. Write the entry, as `pub fn app() -> impl IntoApp { App::new().query(Query) }`",
                ),
            });
        }
        let entry = krate.entry().map_or("app", |entry| entry.function.as_str());
        let (dump, lowered, _) = describe::lower(
            &krate.resolver(),
            krate.package(),
            entry,
            krate.registrations(),
        );
        refusals.extend(lowered);
        if !refusals.is_empty() {
            return Err(Undescribed::Refused(refusals));
        }
        Self::from_dump(&dump).map_err(Undescribed::Dump)
    }

    /// Returns the types of the dump `dump` the lowering wrote.
    ///
    /// # Errors
    ///
    /// Returns the error of a dump the descriptor graph does not read.
    pub(crate) fn from_dump(dump: &JsValue) -> Result<Self, GraphError> {
        let graph = Graph::from_js_value(dump)?;
        Ok(Self {
            view: DescriptorSource::new(graph),
        })
    }

    /// Returns the position of the node of the dump the type `type` stands
    /// for, `None` for a type no single node is.
    pub(crate) fn node_of(&self, r#type: DescriptorType) -> Option<u32> {
        self.view.node_of(r#type)
    }

    /// Returns the descriptor graph of the types, whose dump is
    /// `descriptors.json`.
    #[must_use]
    pub fn graph(&self) -> &Graph {
        self.view.graph()
    }

    /// Returns the dump of the descriptor graph of the types.
    #[must_use]
    pub fn dump(&self) -> JsValue {
        self.view.graph().to_js_value()
    }

    /// Returns the authoring map of a build of the crate whose intermediate
    /// representation is `ir`, which `authoring.json` writes.
    #[must_use]
    pub fn authoring_map(&self, ir: &SchemaIr) -> Vec<AuthoringNode> {
        self.view.authoring_map(ir)
    }
}

impl TypeSource for SourceTypes {
    type Type = DescriptorType;
    type Symbol = DescriptorSymbol;
    type Signature = DescriptorSignature;

    fn type_of_root(&self, root: RootName) -> Option<Self::Type> {
        self.view.type_of_root(root)
    }

    fn type_to_string(&self, r#type: Self::Type) -> String {
        self.view.type_to_string(r#type)
    }

    fn get_flags(&self, r#type: Self::Type) -> TypeFlags {
        self.view.get_flags(r#type)
    }

    fn get_object_flags(&self, r#type: Self::Type) -> ObjectFlags {
        self.view.get_object_flags(r#type)
    }

    fn get_symbol(&self, r#type: Self::Type) -> Option<Self::Symbol> {
        self.view.get_symbol(r#type)
    }

    fn get_alias_symbol(&self, r#type: Self::Type) -> Option<Self::Symbol> {
        self.view.get_alias_symbol(r#type)
    }

    fn get_name(&self, symbol: Self::Symbol) -> String {
        self.view.get_name(symbol)
    }

    fn get_symbol_flags(&self, symbol: Self::Symbol) -> SymbolFlags {
        self.view.get_symbol_flags(symbol)
    }

    fn get_properties(&self, r#type: Self::Type) -> Vec<Self::Symbol> {
        self.view.get_properties(r#type)
    }

    fn get_property(&self, r#type: Self::Type, name: &str) -> Option<Self::Symbol> {
        self.view.get_property(r#type, name)
    }

    fn get_type_of_symbol_at_location(&self, symbol: Self::Symbol) -> Self::Type {
        self.view.get_type_of_symbol_at_location(symbol)
    }

    fn has_value_declaration(&self, symbol: Self::Symbol) -> bool {
        self.view.has_value_declaration(symbol)
    }

    fn get_combined_modifier_flags(&self, symbol: Self::Symbol) -> ModifierFlags {
        self.view.get_combined_modifier_flags(symbol)
    }

    fn get_call_signatures(&self, r#type: Self::Type) -> Vec<Self::Signature> {
        self.view.get_call_signatures(r#type)
    }

    fn get_parameters(&self, signature: Self::Signature) -> Vec<Self::Symbol> {
        self.view.get_parameters(signature)
    }

    fn get_return_type(&self, signature: Self::Signature) -> Self::Type {
        self.view.get_return_type(signature)
    }

    fn is_optional_parameter(&self, parameter: Self::Symbol) -> bool {
        self.view.is_optional_parameter(parameter)
    }

    fn is_rest_parameter(&self, parameter: Self::Symbol) -> bool {
        self.view.is_rest_parameter(parameter)
    }

    fn get_labeled_tuple_elements(
        &self,
        r#type: Self::Type,
    ) -> Option<Vec<TupleElement<Self::Type, Self::Symbol>>> {
        self.view.get_labeled_tuple_elements(r#type)
    }

    fn is_union(&self, r#type: Self::Type) -> bool {
        self.view.is_union(r#type)
    }

    fn is_intersection(&self, r#type: Self::Type) -> bool {
        self.view.is_intersection(r#type)
    }

    fn get_types(&self, r#type: Self::Type) -> Vec<Self::Type> {
        self.view.get_types(r#type)
    }

    fn get_non_nullable_type(&self, r#type: Self::Type) -> Self::Type {
        self.view.get_non_nullable_type(r#type)
    }

    fn get_union_type(&self, types: &[Self::Type]) -> Self::Type {
        self.view.get_union_type(types)
    }

    fn is_type_assignable_to(&self, source: Self::Type, target: Self::Type) -> bool {
        self.view.is_type_assignable_to(source, target)
    }

    fn get_base_types(&self, r#type: Self::Type) -> Option<Vec<Self::Type>> {
        self.view.get_base_types(r#type)
    }

    fn get_target(&self, r#type: Self::Type) -> Option<Self::Type> {
        self.view.get_target(r#type)
    }

    fn get_type_arguments(&self, r#type: Self::Type) -> Vec<Self::Type> {
        self.view.get_type_arguments(r#type)
    }

    fn get_alias_type_arguments(&self, r#type: Self::Type) -> Option<Vec<Self::Type>> {
        self.view.get_alias_type_arguments(r#type)
    }

    fn get_index_type_of_type(&self, r#type: Self::Type) -> Option<Self::Type> {
        self.view.get_index_type_of_type(r#type)
    }

    fn is_array_or_tuple_type_node(&self, r#type: Self::Type) -> bool {
        self.view.is_array_or_tuple_type_node(r#type)
    }

    fn get_type_node_name(&self, r#type: Self::Type) -> Option<String> {
        self.view.get_type_node_name(r#type)
    }

    fn get_literal_value(&self, r#type: Self::Type) -> Option<String> {
        self.view.get_literal_value(r#type)
    }

    fn get_declaration_name(&self, symbol: Self::Symbol) -> Option<String> {
        self.view.get_declaration_name(symbol)
    }

    fn is_library_type(&self, r#type: Self::Type) -> bool {
        self.view.is_library_type(r#type)
    }

    fn is_declared_in_file(&self, symbol: Self::Symbol, needle: &str) -> bool {
        self.view.is_declared_in_file(symbol, needle)
    }

    fn get_documentation_comment(&self, symbol: Self::Symbol) -> String {
        self.view.get_documentation_comment(symbol)
    }

    fn get_js_doc_tags(&self, symbol: Self::Symbol) -> Vec<JsDocTag> {
        self.view.get_js_doc_tags(symbol)
    }

    // Rust states no inheritance, so a plain crate declares no class or
    // interface with a heritage clause.
    fn get_heritage_declarations(&self) -> Vec<HeritageDeclaration<Self::Type>> {
        Vec::new()
    }
}

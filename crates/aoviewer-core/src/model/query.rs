// SPDX-License-Identifier: MIT
/*
 * aoviewer-core model query: the read only accessors of a finished model.
 *
 * The implementer and referrer tables are computed once when the builder
 * finishes, and these accessors read them.
 */

//! The model accessors.
//!
//! An accessor that takes an id indexes an arena and panics when the id comes
//! from another model. [`Model::field`] also panics on
//! [`FieldId::DIRECTIVE`], the owner every directive argument carries. These
//! are the only ways to reach a panic here.

use super::arena::{
    Arg,
    ArgId,
    Directive,
    DirectiveId,
    EnumValue,
    Field,
    FieldId,
    Model,
    Name,
    Roots,
    Type,
    TypeId,
    ValueId, //
};

/// How a field or an argument names a type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RefKind {
    /// A field of an object or an interface returns the type.
    Field,
    /// An argument of the field takes the type.
    Argument,
    /// An input field takes the type.
    InputField,
}

impl Model {
    /// Returns the description of the schema.
    #[must_use]
    pub fn schema_description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    /// Returns the three operation roots.
    #[must_use]
    pub fn roots(&self) -> &Roots {
        &self.roots
    }

    /// Returns every named type in schema order.
    #[must_use]
    pub fn types(&self) -> &[TypeId] {
        &self.order
    }

    /// Returns the type called `name`, if the schema has one.
    #[must_use]
    pub fn type_by_name(&self, name: &str) -> Option<TypeId> {
        self.by_name
            .binary_search_by(|&id| self.name(self.ty(id).name).as_bytes().cmp(name.as_bytes()))
            .ok()
            .map(|i| self.by_name[i])
    }

    /// Returns the name called `text`, if any element of the schema carries
    /// it.
    ///
    /// The names are numbered in byte order, so the lookup is a binary search
    /// over the name table.
    #[must_use]
    pub fn find_name(&self, text: &str) -> Option<Name> {
        let mut lo = 0;
        let mut hi = self.name_ends.len();
        // The range halves on every iteration, so `usize::BITS` iterations
        // empty it.
        for _ in 0..=usize::BITS {
            if lo >= hi {
                break;
            }
            let mid = lo + (hi - lo) / 2;
            match self.name_at(mid).as_bytes().cmp(text.as_bytes()) {
                core::cmp::Ordering::Less => lo = mid + 1,
                core::cmp::Ordering::Greater => hi = mid,
                core::cmp::Ordering::Equal => return u32::try_from(mid).ok().map(Name),
            }
        }
        None
    }

    /// Returns the type `id`.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn ty(&self, id: TypeId) -> &Type {
        &self.types[id.index()]
    }

    /// Returns the field `id`.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model or is
    /// [`FieldId::DIRECTIVE`].
    #[must_use]
    pub fn field(&self, id: FieldId) -> &Field {
        &self.fields[id.index()]
    }

    /// Returns the argument `id`.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn arg(&self, id: ArgId) -> &Arg {
        &self.args[id.index()]
    }

    /// Returns the enum value `id`.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn value(&self, id: ValueId) -> &EnumValue {
        &self.values[id.index()]
    }

    /// Returns every directive definition in schema order.
    #[must_use]
    pub fn directives(&self) -> &[DirectiveId] {
        &self.directive_order
    }

    /// Returns the directive `id`.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn directive(&self, id: DirectiveId) -> &Directive {
        &self.directives[id.index()]
    }

    /// Returns the text of `name`.
    ///
    /// # Panics
    ///
    /// Panics when `name` comes from another model.
    #[must_use]
    pub fn name(&self, name: Name) -> &str {
        self.name_at(name.index())
    }

    /// Returns the types that implement the interface `id` directly, in
    /// schema order, and nothing for a type of another kind.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn implementers(&self, id: TypeId) -> &[TypeId] {
        let i = id.index();
        &self.implementers[self.implementer_starts[i]..self.implementer_starts[i + 1]]
    }

    /// Returns every field and argument naming the type `id` inside its
    /// wrappers.
    ///
    /// The list follows the fields in schema order, a field before its
    /// arguments, and names a field once per kind of reference. The arguments
    /// of directives are left out, because no field holds them.
    ///
    /// # Panics
    ///
    /// Panics when `id` comes from another model.
    #[must_use]
    pub fn referrers(&self, id: TypeId) -> &[(FieldId, RefKind)] {
        let i = id.index();
        &self.referrers[self.referrer_starts[i]..self.referrer_starts[i + 1]]
    }

    /// Returns the text of the name at position `i` of the name table.
    fn name_at(&self, i: usize) -> &str {
        let start = if i == 0 { 0 } else { self.name_ends[i - 1] };
        &self.name_text[start..self.name_ends[i]]
    }
}

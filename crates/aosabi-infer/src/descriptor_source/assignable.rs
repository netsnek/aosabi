// SPDX-License-Identifier: MIT
/*
 * aosabi-infer: the structural comparison of two types of a descriptor graph,
 * which decides whether two types of one name share that name.
 *
 * No file of the reference is translated here.
 */

//! The assignability of the descriptor source.
//!
//! The reference unifies two types of one name when each is assignable to the
//! other. Over descriptors that is a structural comparison: the same kind of
//! node, the same member names and member types that are assignable in turn.
//! A type is assignable to its `Option`, the any marker is assignable both
//! ways, and a pair that is already under comparison counts as assignable,
//! which is how the checker treats a type that refers to itself.
//!
//! An intersection, the payload of `mutation(...)`, compares by its members
//! and by the properties of all of them viewed as one object.
//!
//! A class with a private member is nominal in TypeScript, so two different
//! nodes of which one has a private member are never assignable. A method and
//! a callable compare by their one call signature, so a method fits a
//! function valued property of the same signature and the other way round.

use super::{
    DescriptorSource,
    DescriptorType,
    graph::{
        Expr,
        NamedKind,
        ParamNode, //
    },
    present::TypeData, //
};
use crate::type_source::TypeFlags;

/// The pairs under comparison, innermost last.
type Comparing = Vec<(DescriptorType, DescriptorType)>;

impl DescriptorSource {
    /// Returns whether `source` is assignable to `target`.
    pub(super) fn is_assignable(
        &self,
        source: DescriptorType,
        target: DescriptorType,
        comparing: &mut Comparing,
    ) -> bool {
        let source = self.declared_of(source);
        let target = self.declared_of(target);

        if source == target || comparing.contains(&(source, target)) {
            return true;
        }

        let source_flags = self.flags_of(source);
        let target_flags = self.flags_of(target);

        if source_flags.contains(TypeFlags::ANY)
            || target_flags.contains(TypeFlags::ANY)
            || source_flags.contains(TypeFlags::NEVER)
        {
            return true;
        }

        comparing.push((source, target));
        let result = self.is_assignable_by_shape(source, target, comparing);
        comparing.pop();

        result
    }

    /// Compares two different types by their shape.
    ///
    /// Every member of a source union has to fit, and one member of a target
    /// union has to accept. The source has to fit every member of a target
    /// intersection, and a source intersection fits when one of its members
    /// fits or when the properties of all its members, viewed as one object,
    /// fit a target object, the order of the checker.
    fn is_assignable_by_shape(
        &self,
        source: DescriptorType,
        target: DescriptorType,
        comparing: &mut Comparing,
    ) -> bool {
        let source_flags = self.flags_of(source);
        let target_flags = self.flags_of(target);

        if source_flags.contains(TypeFlags::UNION) {
            return self
                .members_of(source)
                .into_iter()
                .all(|member| self.is_assignable(member, target, comparing));
        }

        if target_flags.contains(TypeFlags::UNION) {
            return self
                .members_of(target)
                .into_iter()
                .any(|member| self.is_assignable(source, member, comparing));
        }

        if let Some(members) = self.intersection_members_of(target) {
            return members
                .into_iter()
                .all(|member| self.is_assignable(source, member, comparing));
        }

        if let Some(members) = self.intersection_members_of(source) {
            if members
                .into_iter()
                .any(|member| self.is_assignable(member, target, comparing))
            {
                return true;
            }
            let is_object = matches!(
                self.expr_of_type(target),
                Some(Expr::Named(named)) if named.kind == NamedKind::Object
            );
            return is_object && self.is_object_assignable(source, target, comparing);
        }

        if Self::is_primitive_assignable(source_flags, target_flags) {
            return true;
        }

        let source_signature = self.signature_of(source);
        let target_signature = self.signature_of(target);
        if source_signature.is_some() || target_signature.is_some() {
            return match (source_signature, target_signature) {
                (Some(a), Some(b)) => self.is_signature_assignable(a, b, comparing),
                _ => false,
            };
        }

        match (self.type_data(source), self.type_data(target)) {
            (TypeData::Node(_), TypeData::Node(_)) => {
                self.is_node_assignable(source, target, comparing)
            }
            _ => false,
        }
    }

    /// Returns the parameters and the node of the return of the one call
    /// signature of `type`, a method or a callable node, and `None` for every
    /// other type.
    fn signature_of(&self, r#type: DescriptorType) -> Option<(&[ParamNode], u32)> {
        match self.type_data(r#type) {
            TypeData::Method { owner, index } => self
                .method(owner, index)
                .map(|method| (method.params.as_slice(), method.returns)),
            TypeData::Node(index) => self
                .callable(index)
                .map(|callable| (callable.params.as_slice(), callable.returns)),
            _ => None,
        }
    }

    /// Returns whether the flags name a primitive, or a literal of one, that
    /// fits the primitive of the target, and `undefined` that fits `void`.
    fn is_primitive_assignable(source: TypeFlags, target: TypeFlags) -> bool {
        let is_string =
            source.contains(TypeFlags::STRING) || source.contains(TypeFlags::STRING_LITERAL);
        let is_number =
            source.contains(TypeFlags::NUMBER) || source.contains(TypeFlags::NUMBER_LITERAL);

        (is_string && target.contains(TypeFlags::STRING))
            || (is_number && target.contains(TypeFlags::NUMBER))
            || (source.contains(TypeFlags::UNDEFINED) && target.contains(TypeFlags::VOID))
            || (source.contains(TypeFlags::VOID) && target.contains(TypeFlags::VOID))
    }

    /// Compares two nodes, which fit when they are wrappers of one kind
    /// around types that fit, or named types that fit.
    fn is_node_assignable(
        &self,
        source: DescriptorType,
        target: DescriptorType,
        comparing: &mut Comparing,
    ) -> bool {
        match (self.expr_of_type(source), self.expr_of_type(target)) {
            (Some(Expr::List(a)), Some(Expr::List(b)))
            | (Some(Expr::Promise(a)), Some(Expr::Promise(b)))
            | (Some(Expr::Repeater(a)), Some(Expr::Repeater(b))) => {
                self.is_assignable(self.node_type(*a), self.node_type(*b), comparing)
            }
            (Some(Expr::Named(a)), Some(Expr::Named(b))) => match (a.kind, b.kind) {
                (NamedKind::Scalar, NamedKind::Scalar) => a.name == b.name,
                (NamedKind::Object, NamedKind::Object) => {
                    self.is_object_assignable(source, target, comparing)
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Compares two named objects, where the source has to carry every
    /// property of the target with a type that fits.
    fn is_object_assignable(
        &self,
        source: DescriptorType,
        target: DescriptorType,
        comparing: &mut Comparing,
    ) -> bool {
        let source_properties = self.properties_of(source);
        let target_properties = self.properties_of(target);

        if source_properties
            .iter()
            .chain(&target_properties)
            .any(|property| self.is_private(*property))
        {
            return false;
        }

        target_properties.into_iter().all(|target_property| {
            let name = self.name_of(target_property);

            source_properties
                .iter()
                .find(|property| self.name_of(**property) == name)
                .is_some_and(|source_property| {
                    self.is_assignable(
                        self.type_of_symbol(*source_property),
                        self.type_of_symbol(target_property),
                        comparing,
                    )
                })
        })
    }

    /// Compares two call signatures, each given as its parameters and the
    /// node of its return.
    ///
    /// The return type has to fit, a parameter pair has to fit in one of the
    /// two directions, which is the bivariance the checker gives a method,
    /// and the source may not require more parameters than the target has.
    fn is_signature_assignable(
        &self,
        (source_params, source_returns): (&[ParamNode], u32),
        (target_params, target_returns): (&[ParamNode], u32),
        comparing: &mut Comparing,
    ) -> bool {
        let required = source_params.iter().filter(|param| !param.optional).count();

        if required > target_params.len() {
            return false;
        }

        let params_fit = source_params.iter().zip(target_params).all(|(x, y)| {
            let x = self.node_type(x.r#type);
            let y = self.node_type(y.r#type);

            self.is_assignable(x, y, comparing) || self.is_assignable(y, x, comparing)
        });

        params_fit
            && self.is_assignable(
                self.node_type(source_returns),
                self.node_type(target_returns),
                comparing,
            )
    }
}

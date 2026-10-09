// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the transform that applies the argument policies of the
 * patched types.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The transform that forces and filters the arguments, [`ForceArgsTransform`].
//!
//! The transform answers the request it is handed when the federation has no
//! policy. Otherwise it resolves every forced value once per request, a
//! function of the context called with the context of the request and a
//! value that is `undefined` forcing nothing, and walks the document with
//! the type information of the target schema. At a field whose
//! `<Parent>.<field>` has a policy, the first argument the caller supplied
//! that is forced, or that the allowlist leaves out, is refused with a
//! `GraphQLError` whose node is that argument, so the error carries the
//! location of the argument in the client's document, and whose extensions
//! are `code`, `field`, `argument` and, with an allowlist, `allowed` sorted,
//! in that key order. A field that passes gets the forced arguments, each in
//! the place of the caller's argument of its name or after the others.
//!
//! The refusals are original GraphQL errors, so the masking of a front
//! passes them unmasked, and the throw is synchronous inside the delegation,
//! before the executor is asked. The transform runs after the patch
//! transform and before the inline and the needs transforms, so an argument
//! a need writes on a forced field reaches the remote, which `aosabi-compat`
//! registers as `Q79_FEDERATION_FORCE_BEFORE_NEEDS`.

use std::{
    cell::RefCell,
    rc::Rc, //
};

use aosabi_core::Thrown;
use aosabi_execute::{
    errors::thrown_definition_error,
    value::{
        ErrorObject,
        Value, //
    }, //
};
use aosabi_http::Context;
use aosabi_json::{
    JsObject,
    JsValue,
    sort::sort_by_stable, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions,
    ast::{
        ArgumentNode,
        AstNode,
        FieldNode,
        NameNode, //
    },
    visitor::NodeRef,
    visitor_edit::{
        Edit,
        EditContext,
        EditVisitor,
        visit_edit, //
    }, //
};
use aosabi_type::{
    NamedType,
    type_info::TypeInfo,
    type_info_edit::visit_with_type_info_edit, //
};

use crate::{
    ast_from_js::ast_from_js_value,
    policy::{
        Force,
        Policies, //
    },
    transforms::{
        DelegationContext,
        ExecutionRequest,
        Transform, //
    }, //
};

/// The transform that applies the argument policies of the patched types to the outgoing request, `ForceArgsTransform`.
#[derive(Clone, Debug)]
pub struct ForceArgsTransform {
    /// The policies of the federation keyed `<Type>.<field>`, `policies`.
    policies: Option<Rc<Policies>>,
    /// The context of the request, which a forced function takes, `ctx`.
    ctx: Context,
}

impl ForceArgsTransform {
    /// Returns the transform of `policies` over the context `ctx`.
    #[must_use]
    pub const fn new(policies: Option<Rc<Policies>>, ctx: Context) -> Self {
        Self { policies, ctx }
    }

    /// Returns the forced argument nodes of every policy, each function called once against the context.
    ///
    /// A value that is `undefined` forces nothing, and a value is written by
    /// [`ast_from_js_value`] with no collector.
    fn resolve_forced<'p>(&self, policies: &'p Policies) -> Vec<(&'p str, Vec<ArgumentNode>)> {
        policies
            .iter()
            .map(|(key, policy)| {
                let nodes = policy
                    .force
                    .iter()
                    .flatten()
                    .filter_map(|(name, value)| {
                        let resolved = match value {
                            Force::Value(constant) => Some(constant.clone()),
                            Force::Context(function) => function(&self.ctx),
                        };
                        let resolved = resolved.filter(|value| !value.is_undefined())?;
                        Some(ArgumentNode {
                            loc: None,
                            name: NameNode::new(None, name.as_ref()),
                            value: ast_from_js_value(&resolved, None),
                        })
                    })
                    .collect();
                (key, nodes)
            })
            .collect()
    }
}

impl Transform for ForceArgsTransform {
    /// Returns the request with the forced arguments written, or the refusal of a caller's argument.
    ///
    /// # Errors
    ///
    /// Returns the `GraphQLError` `Argument "<name>" on "<key>" is set by
    /// the gateway and cannot be supplied.` with the code
    /// `GATEWAY_ARGUMENT_FORCED` for a forced argument the caller supplied,
    /// and `Argument "<name>" is not allowed on "<key>".` with the code
    /// `GATEWAY_ARGUMENT_NOT_ALLOWED` for one outside the allowlist, and the
    /// error of a type the type information cannot read.
    fn transform_request(
        &self,
        original_request: ExecutionRequest,
        delegation_context: &DelegationContext,
    ) -> Result<ExecutionRequest, Thrown> {
        let Some(policies) = self
            .policies
            .as_deref()
            .filter(|policies| !policies.is_empty())
        else {
            return Ok(original_request);
        };
        let forced = self.resolve_forced(policies);
        let type_info = RefCell::new(TypeInfo::new(&delegation_context.target_schema, None, None));
        let mut apply = ApplyPolicies {
            type_info: &type_info,
            policies,
            forced: &forced,
            refusal: None,
        };
        let mut visitor = visit_with_type_info_edit(&type_info, &mut apply);
        let walked = visit_edit(&original_request.document, &mut visitor);
        if let Some(error) = visitor.into_error() {
            return Err(Thrown(thrown_definition_error(error)));
        }
        if let Some(refusal) = apply.refusal.take() {
            return Err(Thrown(refusal));
        }
        let document = walked.map_err(|error| Thrown::error(&error.to_string()))?;
        Ok(ExecutionRequest {
            document: Rc::new(document),
            ..original_request
        })
    }
}

/// The visitor of [`ForceArgsTransform`], which refuses or forces the arguments of each field a policy covers.
struct ApplyPolicies<'a, 's> {
    /// The type information the walk moves.
    type_info: &'a RefCell<TypeInfo<'s>>,
    /// The policies keyed `<Type>.<field>`.
    policies: &'a Policies,
    /// The forced argument nodes keyed as the policies are.
    forced: &'a [(&'a str, Vec<ArgumentNode>)],
    /// The refusal the walk stopped on.
    refusal: Option<Value>,
}

impl ApplyPolicies<'_, '_> {
    /// Returns the composite type the visited field belongs to, `typeInfo.getParentType()`.
    fn parent_type(&self) -> Option<NamedType> {
        self.type_info
            .try_borrow()
            .ok()
            .and_then(|type_info| type_info.get_parent_type().cloned())
    }

    /// Refuses or forces the arguments of `node`.
    fn field(&mut self, node: &FieldNode) -> Edit {
        let Some(parent) = self.parent_type() else {
            return Edit::Keep;
        };
        let key = format!("{}.{}", parent.name(), node.name.value());
        let Some(policy) = self.policies.get(&key) else {
            return Edit::Keep;
        };
        let forced_nodes: &[ArgumentNode] = self
            .forced
            .iter()
            .find(|(known, _)| *known == key)
            .map_or(&[], |(_, nodes)| nodes);
        // The forced names come from the resolved nodes, so a function that
        // answers `undefined` forces nothing and the caller's value stands.
        let forced_names: Vec<&str> = forced_nodes.iter().map(|arg| arg.name.value()).collect();
        let allowed: Option<Vec<&str>> = policy.args.as_ref().map(|args| {
            let mut set: Vec<&str> = Vec::new();
            for arg in args {
                if !set.contains(&arg.as_ref()) {
                    set.push(arg.as_ref());
                }
            }
            set
        });
        for arg in node.arguments.iter().flatten() {
            let name = arg.name.value();
            let is_forced = forced_names.contains(&name);
            let is_denied = is_forced
                || allowed
                    .as_ref()
                    .is_some_and(|allowed| !allowed.contains(&name));
            if is_denied {
                self.refusal = Some(refusal(arg, &key, is_forced, allowed.as_deref()));
                return Edit::Break;
            }
        }
        if forced_nodes.is_empty() {
            return Edit::Keep;
        }
        let mut merged = node.arguments.clone().unwrap_or_default();
        for arg in forced_nodes {
            match merged
                .iter_mut()
                .find(|known| known.name.value() == arg.name.value())
            {
                // Override, never merge, so a caller's value cannot survive.
                Some(known) => *known = arg.clone(),
                None => merged.push(arg.clone()),
            }
        }
        Edit::replace(FieldNode {
            loc: node.loc.clone(),
            alias: node.alias.clone(),
            name: node.name.clone(),
            arguments: Some(merged),
            directives: node.directives.clone(),
            selection_set: node.selection_set.clone(),
        })
    }
}

impl EditVisitor for ApplyPolicies<'_, '_> {
    fn enter(&mut self, node: NodeRef<'_>, _context: &EditContext<'_>) -> Edit {
        match node {
            NodeRef::Field(node) => self.field(node),
            _ => Edit::Keep,
        }
    }
}

/// Returns the refusal of the caller's argument `arg` on the field `key`.
///
/// The node of the error is the argument, and its extensions are `code`,
/// `field`, `argument` and, when the policy has an allowlist, `allowed`, the
/// allowlist without a repeated name and sorted by UTF-16 code units, as
/// `[...allowed].sort()` sorts it.
fn refusal(arg: &ArgumentNode, key: &str, is_forced: bool, allowed: Option<&[&str]>) -> Value {
    let name = arg.name.value();
    let message = if is_forced {
        format!("Argument \"{name}\" on \"{key}\" is set by the gateway and cannot be supplied.")
    } else {
        format!("Argument \"{name}\" is not allowed on \"{key}\".")
    };
    let code = if is_forced {
        "GATEWAY_ARGUMENT_FORCED"
    } else {
        "GATEWAY_ARGUMENT_NOT_ALLOWED"
    };
    let mut extensions = JsObject::with_capacity(4);
    extensions.insert("code", code);
    extensions.insert("field", key);
    extensions.insert("argument", name);
    if let Some(allowed) = allowed {
        let mut sorted = allowed.to_vec();
        sort_by_stable(&mut sorted, |a, b| a.encode_utf16().cmp(b.encode_utf16()));
        extensions.insert(
            "allowed",
            JsValue::Array(sorted.into_iter().map(JsValue::from).collect()),
        );
    }
    Value::from(ErrorObject::GraphQL(GraphQLError::new(
        message,
        GraphQLErrorOptions {
            nodes: Some(vec![AstNode::Argument(arg.clone())]),
            extensions: Some(extensions),
            ..GraphQLErrorOptions::default()
        },
    )))
}

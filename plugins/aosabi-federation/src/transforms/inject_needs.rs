// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the transform that adds the needs selection to the
 * delegated field.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The transform that injects the `needs` selection, [`InjectNeedsTransform`].
//!
//! The transform answers the request it is handed when the needs object is
//! absent or has no key, and otherwise appends the field selections the
//! needs build to the first field of the document that has a selection set,
//! which is the root field of a delegated request, passing over a field
//! whose name the client selected already at that level, so its nested
//! needs and its arguments are skipped with it. It is the last transform of
//! a request, so the selections it adds carry no `__typename` of the patch
//! transform, which ran before it.

use std::rc::Rc;

use aosabi_core::Thrown;
use aosabi_lang::{
    SharedSlice,
    ast::{
        FieldNode,
        SelectionNode,
        SelectionSetNode, //
    },
    visitor::NodeRef,
    visitor_edit::{
        Edit,
        EditContext,
        EditVisitor,
        visit_edit, //
    }, //
};

use crate::{
    needs::{
        Needs,
        build_selections_from_needs, //
    },
    transforms::{
        DelegationContext,
        ExecutionRequest,
        Transform, //
    }, //
};

/// The transform that adds the `needs` selection to the delegated field, `InjectNeedsTransform`.
#[derive(Clone, Debug)]
pub struct InjectNeedsTransform {
    /// The needs selection of the delegation, `needs`.
    needs: Option<Needs>,
}

impl InjectNeedsTransform {
    /// Returns the transform of the needs selection `needs`.
    #[must_use]
    pub const fn new(needs: Option<Needs>) -> Self {
        Self { needs }
    }
}

impl Transform for InjectNeedsTransform {
    /// Returns the request with the needs selection appended to its first field that has a selection set.
    fn transform_request(
        &self,
        original_request: ExecutionRequest,
        _delegation_context: &DelegationContext,
    ) -> Result<ExecutionRequest, Thrown> {
        let Some(needs) = self.needs.as_ref().filter(|needs| !needs.is_empty()) else {
            return Ok(original_request);
        };
        let mut inject = Inject {
            needs_selections: build_selections_from_needs(needs.nodes()),
            root_field_found: false,
        };
        let document = visit_edit(&original_request.document, &mut inject)
            .map_err(|error| Thrown::error(&error.to_string()))?;
        Ok(ExecutionRequest {
            document: Rc::new(document),
            ..original_request
        })
    }
}

/// The visitor of [`InjectNeedsTransform`], which edits the first field that has a selection set.
struct Inject {
    /// The selections the needs build.
    needs_selections: Vec<SelectionNode>,
    /// Whether the field has been met, `rootFieldFound`.
    root_field_found: bool,
}

impl EditVisitor for Inject {
    fn enter(&mut self, node: NodeRef<'_>, _context: &EditContext<'_>) -> Edit {
        let NodeRef::Field(node) = node else {
            return Edit::Keep;
        };
        if self.root_field_found {
            return Edit::Keep;
        }
        let Some(selection_set) = &node.selection_set else {
            return Edit::Keep;
        };
        self.root_field_found = true;
        let existing_names: Vec<&str> = selection_set
            .selections
            .iter()
            .filter_map(|selection| match selection {
                SelectionNode::Field(field) => Some(field.name.value()),
                _ => None,
            })
            .collect();
        let added: Vec<SelectionNode> = self
            .needs_selections
            .iter()
            .filter(|selection| {
                matches!(selection, SelectionNode::Field(field)
                    if !existing_names.contains(&field.name.value()))
            })
            .cloned()
            .collect();
        let merged_selections: SharedSlice<SelectionNode> = if added.is_empty() {
            SharedSlice::clone(&selection_set.selections)
        } else {
            selection_set
                .selections
                .iter()
                .cloned()
                .chain(added)
                .collect()
        };
        Edit::replace(FieldNode {
            loc: node.loc.clone(),
            alias: node.alias.clone(),
            name: node.name.clone(),
            arguments: node.arguments.clone(),
            directives: node.directives.clone(),
            selection_set: Some(SelectionSetNode {
                loc: selection_set.loc.clone(),
                selections: merged_selections,
            }),
        })
    }
}

// SPDX-License-Identifier: MIT
/*
 * aosabi-type: the walk with type information over the edit path.
 *
 * Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
 * Derived from graphql-js 16.12.0, src/language/visitor.ts, MIT.
 */

//! The walk that keeps the type information of each node while its hooks edit
//! the tree.
//!
//! [`visit_with_type_info_edit()`] wraps an [`EditVisitor`] so that a
//! [`TypeInfo`] moves with the walk of
//! [󰀃](aosabi_lang::visitor_edit::visit_edit), as
//! [󰀄](crate::type_info::visit_with_type_info) wraps a visitor of the read
//! only walk. The transforms of Federation read the input type of a string to
//! turn it into an enum value, and the delegation reads the parent type and
//! the field of each selection it filters, both inside hooks that edit.
//!
//! The reference leaves the type information again when a hook answers
//! anything but `undefined`, because the walk then skips the leave of that
//! node or walks another node in its place, and enters the replacement when
//! the answer is a node. The leave hook of a node receives the node with the
//! edits of its children applied, and the type information leaves that node,
//! which every kind leaves by its kind alone.
//!
//! The type information is one object that this visitor moves and the
//! visiting visitor reads inside its hooks, so the two share it through a
//! `RefCell`, as the visitor of the read only walk does. A failing enter, and
//! a borrow a hook kept across its return, stop the walk with
//! [`Edit::Break`], and the error waits in
//! [`VisitWithTypeInfoEdit::into_error()`].

use core::cell::RefCell;

use aosabi_lang::{
    visitor::NodeRef,
    visitor_edit::{
        Edit,
        EditContext,
        EditVisitor, //
    }, //
};

use crate::{
    definition::DefinitionError,
    type_info::{
        TypeInfo,
        type_info_in_use, //
    }, //
};

/// A visitor which maintains a provided [`TypeInfo`] instance along with a
/// visiting visitor that edits.
///
/// Built by [`visit_with_type_info_edit()`].
///
/// Derived from graphql-js 16.12.0, src/utilities/TypeInfo.ts, MIT.
pub struct VisitWithTypeInfoEdit<'t, 's, V: ?Sized> {
    /// The type information that moves with the walk.
    type_info: &'t RefCell<TypeInfo<'s>>,
    /// The visiting visitor.
    visitor: &'t mut V,
    /// The error that stopped the walk, when one did.
    error: Option<DefinitionError>,
}

impl<V: ?Sized> VisitWithTypeInfoEdit<'_, '_, V> {
    /// Returns the error that stopped the walk, when one did.
    ///
    /// A walk that stops on an error answers what the walk answers on
    /// [`Edit::Break`], so a caller reads this before it uses the tree.
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

impl<V> EditVisitor for VisitWithTypeInfoEdit<'_, '_, V>
where
    V: EditVisitor + ?Sized,
{
    fn enter(&mut self, node: NodeRef<'_>, context: &EditContext<'_>) -> Edit {
        let entered = self.enter_type_info(node);
        if self.stops(entered) {
            return Edit::Break;
        }
        let result = self.visitor.enter(node, context);
        if result != Edit::Keep {
            let left = self.leave_type_info(node);
            if self.stops(left) {
                return Edit::Break;
            }
            if let Edit::Replace(replacement) = &result {
                let entered = self.enter_type_info(NodeRef::from(&**replacement));
                if self.stops(entered) {
                    return Edit::Break;
                }
            }
        }
        result
    }

    fn leave(&mut self, node: NodeRef<'_>, context: &EditContext<'_>) -> Edit {
        let result = self.visitor.leave(node, context);
        let left = self.leave_type_info(node);
        if self.stops(left) {
            return Edit::Break;
        }
        result
    }
}

/// Creates a new visitor instance which maintains a provided [`TypeInfo`]
/// instance along with a visiting visitor that edits.
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
/// let mut enums = StringsToEnums { type_info: &type_info };
/// let mut visitor = visit_with_type_info_edit(&type_info, &mut enums);
/// let document = visit_edit(&document, &mut visitor)?;
/// if let Some(error) = visitor.into_error() {
///     return Err(error);
/// }
/// ```
pub fn visit_with_type_info_edit<'t, 's, V: ?Sized>(
    type_info: &'t RefCell<TypeInfo<'s>>,
    visitor: &'t mut V,
) -> VisitWithTypeInfoEdit<'t, 's, V> {
    VisitWithTypeInfoEdit {
        type_info,
        visitor,
        error: None,
    }
}

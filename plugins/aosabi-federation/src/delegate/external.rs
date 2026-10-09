// SPDX-License-Identifier: MIT
/*
 * aosabi-federation: the annotation of an external object, its subschema and
 * its unpathed errors.
 *
 * The annotation of an external object, kept in a side table keyed by the
 * identity of the object where the reference uses symbols that are not
 * enumerable.
 *
 * Derived from @graphql-tools/delegate 12.0.9, dist/index.cjs, MIT.
 */

//! The annotation of an external object, the object a delegation answered.
//!
//! The reference marks an object a delegation answered with three properties
//! under symbols, its subschema, a map of subschemas per response key and the
//! errors of the result no path placed, and defines them as not enumerable,
//! so no `Object.keys`, no spread and no `JSON.stringify` sees them. A
//! [`Value`] has no symbol keys, so the three live in a side table keyed by
//! the identity of the object, the address of its `Rc`, which the projection
//! of the root resolver and the writer of a response never read.
//!
//! The table belongs to the one runtime thread, as every value of a request
//! does. An entry holds a weak handle of its object, so it lives as long as
//! the object does, which is the life of the request that made it, and an
//! entry whose object is gone is dropped when the table grows. An array is
//! an object in the reference and is annotated the same way, a list here,
//! and a value that is neither has no identity to key on, so annotating it
//! throws the `TypeError` `Object.defineProperties` throws for a primitive.
//!
//! `mergeFields` with `getActualFieldNodes`, its two empty constants and
//! `handleResolverResult` and `executeDelegationStage` below it at
//! `:99-288`, and `getPlanLeftOverFromParent` with its two constants at
//! `:69-76`, belong to the stitching of several subschemas, which the
//! federation never sets up, and are left out with `isPrototypePollutingKey`
//! and its list of keys at `:60-67`, which only they call.

use std::{
    cell::{
        Cell,
        RefCell, //
    },
    rc::{
        Rc,
        Weak, //
    }, //
};

use aosabi_execute::value::{
    ErrorObject,
    ObjectLike,
    Value, //
};
use aosabi_json::map::IndexMap;

use crate::{
    delegate::subschema::SchemaOrSubschemaConfig,
    utils::AddressState, //
};

/// The three properties a delegation puts on an object it answered.
#[derive(Clone)]
pub struct ExternalAnnotation {
    /// The subschema the object came from, `OBJECT_SUBSCHEMA_SYMBOL`.
    pub subschema: SchemaOrSubschemaConfig,
    /// The subschema of a response key that came from another one, `FIELD_SUBSCHEMA_MAP_SYMBOL`.
    pub subschema_map: Vec<(String, SchemaOrSubschemaConfig)>,
    /// The errors of the result no path placed, `UNPATHED_ERRORS_SYMBOL`.
    pub unpathed_errors: Rc<[Value]>,
}

impl ExternalAnnotation {
    /// Returns the subschema a response key came from.
    ///
    /// The subschema the map names for the key, or else the subschema of the
    /// object, the body of `getSubschema` at `index.cjs:93-95` once the
    /// annotation is read.
    #[must_use]
    pub fn subschema_of(&self, response_key: &str) -> SchemaOrSubschemaConfig {
        self.subschema_map
            .iter()
            .find(|(key, _)| key == response_key)
            .map_or_else(
                || self.subschema.clone(),
                |(_, subschema)| subschema.clone(),
            )
    }
}

/// A weak handle of an annotated object, which tells a live entry from a stale one.
enum WeakTarget {
    /// An object.
    Object(Weak<dyn ObjectLike>),
    /// A list.
    List(Weak<Vec<Value>>),
}

/// One entry of the side table.
struct Entry {
    /// The object the annotation belongs to.
    target: WeakTarget,
    /// The annotation.
    annotation: Rc<ExternalAnnotation>,
}

/// The number of entries the table holds before its first sweep of stale entries.
const FIRST_SWEEP: usize = 64;

/// The side table, keyed by the address of the object with the hash of an address.
type Annotations = IndexMap<usize, Entry, AddressState>;

thread_local! {
    /// The annotations of the runtime thread, keyed by the address of the object.
    static ANNOTATIONS: RefCell<Annotations> = RefCell::new(IndexMap::default());
    /// The size of the table at which the next sweep runs.
    static NEXT_SWEEP: Cell<usize> = const { Cell::new(FIRST_SWEEP) };
}

/// Returns whether an object carries the annotation of a delegation.
///
/// Asks for the unpathed errors, and every annotation carries those.
#[must_use]
pub fn is_external_object(data: &Value) -> bool {
    annotation_of(data).is_some()
}

/// Puts the annotation of a delegation on an object and returns the object.
///
/// A second annotation replaces the first, as the writable properties of the
/// reference do.
///
/// # Errors
///
/// Returns the `TypeError` `Object.defineProperties called on non-object`
/// for a value that is no object and no list.
pub fn annotate_external_object(
    object: &Value,
    errors: Rc<[Value]>,
    subschema: SchemaOrSubschemaConfig,
    subschema_map: Vec<(String, SchemaOrSubschemaConfig)>,
) -> Result<Value, Value> {
    let Some((address, target)) = identity(object) else {
        return Err(Value::from(ErrorObject::type_error(
            "Object.defineProperties called on non-object",
        )));
    };
    let annotation = Rc::new(ExternalAnnotation {
        subschema,
        subschema_map,
        unpathed_errors: errors,
    });
    ANNOTATIONS.with(|table| {
        let mut table = table.borrow_mut();
        sweep(&mut table);
        table.insert(address, Entry { target, annotation });
    });
    Ok(object.clone())
}

/// Returns the subschema an object's response key came from.
///
/// The subschema the map names for the key, or else the subschema of the
/// object, and `None` for an object no delegation answered.
#[must_use]
pub fn get_subschema(object: &Value, response_key: &str) -> Option<SchemaOrSubschemaConfig> {
    annotation_of(object).map(|annotation| annotation.subschema_of(response_key))
}

/// Returns the errors of the result no path placed, which an object carries.
#[must_use]
pub fn get_unpathed_errors(object: &Value) -> Option<Rc<[Value]>> {
    annotation_of(object).map(|annotation| Rc::clone(&annotation.unpathed_errors))
}

/// Returns the annotation of an object, `None` when it carries none.
#[must_use]
pub fn annotation_of(object: &Value) -> Option<Rc<ExternalAnnotation>> {
    let (address, _) = identity(object)?;
    ANNOTATIONS.with(|table| {
        let table = table.borrow();
        let entry = table.get(&address)?;
        is_alive(&entry.target, object).then(|| Rc::clone(&entry.annotation))
    })
}

/// Returns the address of an object or a list and a weak handle of it.
fn identity(value: &Value) -> Option<(usize, WeakTarget)> {
    match value {
        Value::Object(object) => Some((
            Rc::as_ptr(object).cast::<()>().addr(),
            WeakTarget::Object(Rc::downgrade(object)),
        )),
        Value::List(list) => Some((
            Rc::as_ptr(list).cast::<()>().addr(),
            WeakTarget::List(Rc::downgrade(list)),
        )),
        _ => None,
    }
}

/// Returns whether the entry of `target` belongs to `object`, which is alive at that address.
fn is_alive(target: &WeakTarget, object: &Value) -> bool {
    match (target, object) {
        (WeakTarget::Object(weak), Value::Object(object)) => weak
            .upgrade()
            .is_some_and(|alive| Rc::ptr_eq(&alive, object)),
        (WeakTarget::List(weak), Value::List(list)) => {
            weak.upgrade().is_some_and(|alive| Rc::ptr_eq(&alive, list))
        }
        _ => false,
    }
}

/// Drops the entries whose object is gone once the table has grown to the next sweep.
fn sweep(table: &mut Annotations) {
    let due = NEXT_SWEEP.with(Cell::get);
    if table.len() < due {
        return;
    }
    let old = core::mem::take(table);
    for (address, entry) in old {
        let alive = match &entry.target {
            WeakTarget::Object(weak) => weak.strong_count() > 0,
            WeakTarget::List(weak) => weak.strong_count() > 0,
        };
        if alive {
            table.insert(address, entry);
        }
    }
    NEXT_SWEEP.with(|next| next.set(FIRST_SWEEP.max(table.len() * 2)));
}

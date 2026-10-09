// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the transform that runs the patches over the objects of
 * a delegated result.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The transform that patches the remote objects, [`PatchTransform`],
//! over the patch table of a federation, [`PatchTable`].
//!
//! The request half appends a `__typename` field to every selection set that
//! holds no field of that name, the root and the fragments among them, so
//! every object of the result names its remote type. The result half walks
//! the result. An error stays as it is, a list is mapped item by item, and
//! an object is copied with every member transformed first and then handed
//! to the patch its `__typename` names. A patch that answers an object is
//! merged over the copy with its members winning, as
//! `{...processedData, ...patchedData}` merges them, so a member under an
//! alias the patch does not name stays, and any other answer replaces the
//! object. The `errors` of a result are walked as its `data` is, and every
//! error in them stays as it is.
//!
//! [`PatchTable`] is the patch map of the configuration with no type over
//! it, a patch per remote type name with the policy `pass` attached to it.
//! The typed patches of a pulled registry, `Patches` with `IntoPatch` and
//! `patches!`, build this table, which `configure` of `gateway.rs` reads.
//!
//! A value that is an object to JavaScript and has no member a spread
//! copies, a `Date`, a promise and a host object, is copied as an object
//! with no member, as `{...value}` copies it. A patch is looked up among the
//! keys of the table, where the reference reads a member of an object, so a
//! `__typename` that names a member of `Object.prototype`, `constructor`
//! among them, finds no patch here and finds that member there. The strict
//! message is text, so a lone surrogate of a `__typename` prints there as
//! the replacement character.

use std::{
    any::Any,
    fmt,
    rc::Rc, //
};

use aosabi_core::Thrown;
use aosabi_execute::value::{
    ErrorObject,
    ObjectLike,
    PlainObject,
    Value,
    to_js_string, //
};
use aosabi_json::JsValue;
use aosabi_lang::{
    SharedSlice,
    ast::{
        FieldNode,
        NameNode,
        SelectionNode,
        SelectionSetNode, //
    },
    js_string::to_string_lossy,
    visitor::NodeRef,
    visitor_edit::{
        Edit,
        EditContext,
        EditVisitor,
        visit_edit, //
    }, //
};

use crate::{
    policy::{
        PatchPolicy,
        Policies,
        collect_policies, //
    },
    transforms::{
        DelegationContext,
        ExecutionRequest,
        Transform, //
    },
    utils::is_truthy, //
};

/// The second argument of a patch, the `api` of `(data, api) => value`.
///
/// The reference hands every patch the context of its federation, an object
/// whose one member is `delegate`, and the federation that builds the
/// transform decides what the handle holds.
pub type PatchApi = Rc<dyn Any>;

/// A patch, `(data, api) => value`, called with the remote object whose members are transformed.
///
/// A patch that throws answers what it threw.
pub type PatchFn = Rc<dyn Fn(Value, &PatchApi) -> Result<Value, Thrown>>;

/// The patches of a federation keyed by remote type name, the patch map of its configuration.
///
/// A type name keeps the place it was first set at, as a key of an object
/// literal does.
#[derive(Clone, Default)]
pub struct PatchTable {
    /// The patches with their type names, in insertion order.
    entries: Vec<PatchEntry>,
}

/// One patch of a [`PatchTable`].
#[derive(Clone)]
struct PatchEntry {
    /// The remote type name, the key.
    type_name: String,
    /// The patch.
    patch: PatchFn,
    /// The policy `pass` attached to the patch.
    policy: Option<PatchPolicy>,
}

impl PatchTable {
    /// Returns a table that holds no patch.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Sets the patch of `type_name` with the policy attached to it, `patches[type_name] = patch`.
    ///
    /// A type name that is set already keeps its place and takes the new
    /// patch and policy.
    pub fn insert(
        &mut self,
        type_name: impl Into<String>,
        patch: PatchFn,
        policy: Option<PatchPolicy>,
    ) {
        let type_name = type_name.into();
        match self
            .entries
            .iter_mut()
            .find(|entry| entry.type_name == type_name)
        {
            Some(entry) => {
                entry.patch = patch;
                entry.policy = policy;
            }
            None => self.entries.push(PatchEntry {
                type_name,
                patch,
                policy,
            }),
        }
    }

    /// Returns the patch of `type_name`, `patches[type_name]`.
    #[must_use]
    pub fn get(&self, type_name: &str) -> Option<&PatchFn> {
        self.entries
            .iter()
            .find(|entry| entry.type_name == type_name)
            .map(|entry| &entry.patch)
    }

    /// Returns the number of patches.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns whether the table holds no patch.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Returns the policies of the patches keyed `<Type>.<field>`, `collectPolicies(patches)`.
    #[must_use]
    pub fn policies(&self) -> Policies {
        collect_policies(
            self.entries
                .iter()
                .map(|entry| (entry.type_name.as_str(), entry.policy.as_ref())),
        )
    }
}

impl fmt::Debug for PatchTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_list()
            .entries(self.entries.iter().map(|entry| &entry.type_name))
            .finish()
    }
}

/// The transform that appends `__typename` to the request and runs the patches over the result.
#[derive(Clone, Debug)]
pub struct PatchTransform {
    /// The patches of the federation, `patches`.
    patches: Option<Rc<PatchTable>>,
    /// The second argument of every patch, `api`.
    api: PatchApi,
    /// Whether an object of a type with no patch is refused, `strict`.
    strict: bool,
}

impl PatchTransform {
    /// Returns the transform of `patches`, which hands every patch `api` and refuses an unpatched type under `strict`.
    #[must_use]
    pub fn new(patches: Option<Rc<PatchTable>>, api: PatchApi, strict: bool) -> Self {
        Self {
            patches,
            api,
            strict,
        }
    }

    /// Returns `data` with the patches applied to every object it holds.
    ///
    /// The members of an object are transformed in the order of its keys
    /// before the object itself, so the strict refusal of a nested object is
    /// thrown before the one of the object that holds it. The reference
    /// copies the object with a spread and then writes each member
    /// transformed, and the copy here takes each member transformed as it
    /// reads it, one read and one key per member in the same order, since no
    /// patch sees the copy before its last member is written.
    fn apply_transforms(&self, data: &Value) -> Result<Value, Thrown> {
        let processed_data = match data {
            Value::Object(object) => {
                let keys = object.own_keys();
                let mut entries = Vec::with_capacity(keys.len());
                for key in keys {
                    let member = Rc::clone(object).get(&key);
                    entries.push((key, self.apply_transforms(&member)?));
                }
                PlainObject::from_entries(entries)
            }
            Value::List(items) => {
                let items: Vec<Value> = items
                    .iter()
                    .map(|item| self.apply_transforms(item))
                    .collect::<Result<_, _>>()?;
                return Ok(Value::from(items));
            }
            Value::Date(_) | Value::Promise(_) | Value::Host(_) => PlainObject::new(),
            // An error is a result and not data, and a value that is falsy
            // or no object has nothing to patch.
            _ => return Ok(data.clone()),
        };
        let type_name = match data {
            Value::Object(object) => Rc::clone(object).get("__typename"),
            _ => Value::Undefined,
        };
        let type_name = is_truthy(&type_name).then(|| to_js_string(&type_name));
        let patch_fn = type_name.as_ref().and_then(|type_name| {
            let patches = self.patches.as_ref()?;
            patches.get(type_name.as_str()?)
        });
        let Some(patch_fn) = patch_fn else {
            return match type_name {
                Some(type_name) if self.strict => {
                    Err(Thrown(strict_refusal(&to_string_lossy(&type_name))))
                }
                _ => Ok(Value::from(processed_data)),
            };
        };
        let patched_data = patch_fn(Value::from(Rc::clone(&processed_data)), &self.api)?;
        if !is_spread_object(&patched_data) {
            return Ok(patched_data);
        }
        let merged = PlainObject::from_entries(
            processed_data
                .own_keys()
                .into_iter()
                .map(|key| {
                    let member = Rc::clone(&processed_data).get(&key);
                    (key, member)
                })
                .collect(),
        );
        for (key, member) in own_enumerable_entries(&patched_data) {
            merged.set(&key, member);
        }
        Ok(Value::from(merged))
    }
}

impl Transform for PatchTransform {
    /// Returns the request with `__typename` appended to every selection set that lacks it.
    ///
    /// A field named `__typename` under an alias counts as present.
    fn transform_request(
        &self,
        original_request: ExecutionRequest,
        _delegation_context: &DelegationContext,
    ) -> Result<ExecutionRequest, Thrown> {
        let document = visit_edit(&original_request.document, &mut AppendTypename)
            .map_err(|error| Thrown::error(&error.to_string()))?;
        Ok(ExecutionRequest {
            document: Rc::new(document),
            ..original_request
        })
    }

    /// Returns the result with the patches applied to every object it holds.
    ///
    /// # Errors
    ///
    /// Returns what a patch throws, and under `strict` the plain `Error` of
    /// an object whose remote type `<T>` has no patch, whose message is the
    /// text below.
    ///
    /// ```text
    /// Gateway (strict): no patch for remote type "<T>", so it would be exposed in full. Add a patch for it, or declare it deliberately with `<T>: passthrough()`.
    /// ```
    fn transform_result(
        &self,
        original_result: Value,
        _delegation_context: &DelegationContext,
    ) -> Result<Value, Thrown> {
        self.apply_transforms(&original_result)
    }
}

/// The visitor of the request half of [`PatchTransform`].
struct AppendTypename;

impl EditVisitor for AppendTypename {
    fn enter(&mut self, node: NodeRef<'_>, _context: &EditContext<'_>) -> Edit {
        let NodeRef::SelectionSet(node) = node else {
            return Edit::Keep;
        };
        let has_typename = node.selections.iter().any(|selection| {
            matches!(selection, SelectionNode::Field(field) if field.name.value() == "__typename")
        });
        if has_typename {
            return Edit::Keep;
        }
        let typename = SelectionNode::Field(FieldNode {
            loc: None,
            alias: None,
            name: NameNode::new(None, "__typename"),
            arguments: None,
            directives: None,
            selection_set: None,
        });
        let selections: SharedSlice<SelectionNode> = node
            .selections
            .iter()
            .cloned()
            .chain(core::iter::once(typename))
            .collect();
        Edit::replace(SelectionSetNode {
            loc: node.loc.clone(),
            selections,
        })
    }
}

/// Returns the strict refusal of an object whose remote type `type_name` has no patch.
///
/// The text of `gateway.ts:756-760`, thrown as a plain `Error`.
fn strict_refusal(type_name: &str) -> Value {
    Value::from(ErrorObject::error(format!(
        "Gateway (strict): no patch for remote type \"{type_name}\", so it would be exposed in full. Add a patch for it, or declare it deliberately with `{type_name}: passthrough()`."
    )))
}

/// Returns whether a patch's answer is merged over the object, `patchedData && typeof patchedData === 'object' && !Array.isArray(patchedData)`.
const fn is_spread_object(value: &Value) -> bool {
    matches!(
        value,
        Value::Object(_) | Value::Error(_) | Value::Date(_) | Value::Promise(_) | Value::Host(_)
    )
}

/// Returns the own enumerable properties of an object value in the order of its keys, what a spread copies of it.
///
/// An object answers its members. A `GraphQLError` answers `message`,
/// `path`, `locations` and `extensions`, the four properties its constructor
/// leaves enumerable, in the order it makes them, with an absent path and
/// absent locations as `undefined`. An error of a class with properties of
/// its own answers those, and every other error, a `Date`, a promise and a
/// host object answer none.
fn own_enumerable_entries(value: &Value) -> Vec<(String, Value)> {
    match value {
        Value::Object(object) => object
            .own_keys()
            .into_iter()
            .map(|key| {
                let member = Rc::clone(object).get(&key);
                (key, member)
            })
            .collect(),
        Value::Error(error) => match error.as_ref() {
            ErrorObject::GraphQL(error) => {
                let path = error.path.as_ref().map_or(Value::Undefined, |path| {
                    Value::from_js(&JsValue::Array(path.iter().map(JsValue::from).collect()))
                });
                let locations = error
                    .locations
                    .as_ref()
                    .map_or(Value::Undefined, |locations| {
                        Value::from_js(&JsValue::Array(
                            locations
                                .iter()
                                .map(|&location| JsValue::from(location))
                                .collect(),
                        ))
                    });
                vec![
                    (
                        String::from("message"),
                        Value::String(error.message.clone()),
                    ),
                    (String::from("path"), path),
                    (String::from("locations"), locations),
                    (
                        String::from("extensions"),
                        Value::from_js(&JsValue::Object(error.extensions.clone())),
                    ),
                ]
            }
            ErrorObject::Custom { props, .. } => props
                .own_keys()
                .map(|(key, member)| (to_string_lossy(key).into_owned(), Value::from_js(member)))
                .collect(),
            _ => Vec::new(),
        },
        _ => Vec::new(),
    }
}

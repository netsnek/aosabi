// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core resolvers wrap_resolver: the wrapping of a value a resolver
 * returns.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/define-pylon.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The resolver wrapping.
//!
//! [`wrap_resolver()`] turns whatever an application returns into what the
//! default field resolver of the executor reads. A primitive, a `Date` and
//! an object nobody selects below pass through. A promise is wrapped when it
//! settles. A function becomes a function over `(args, context, info)` that
//! calls the original with positional arguments. An object is projected onto
//! the fields the document selects, each member wrapped in turn, and the
//! projection is a view of the selection, the type `View` of the module
//! `view`.
//!
//! The branch order is the order of the reference, because the first branch
//! that matches decides, and a value may match more than one.
//!
//! Behind the dev feature `exec-eager-projection`, which no card names, the
//! projection can be the object the reference builds as well, a plain object
//! with a copy of each key, the `PlainObject` of `aosabi-execute`, which a
//! thread selects with `set_eager_projection`. It is the second judge of the
//! view, which a notebook runs beside it to bisect a difference. With the
//! feature `canary-types` the projection at a union or an interface that a
//! type resolver of a canary build resolves is that plain object too, since
//! the type resolver tests every property name of the value, which the
//! projection holds there.
//!
//! With a getter of a host the projection reads a member as often as the
//! reference reads it, once for the test of its truth, once more for the
//! test of its type when the first answer is true and once more for its
//! value, and the function of an aliased field reads the alias once, once
//! more when its value is defined, or else the field name once. An object of
//! a host may run a getter at each read, and a read that throws makes the
//! projection throw, so the wrapping answers the [`Thrown`] value there,
//! which the call that returned the object throws, as `wrapResolver` throws
//! it. A build without the feature `host-reads` of `aosabi-execute` holds no
//! object of a host, and an object of a Rust type answers every read with the
//! same value and no throw, so there the projection takes one read of a
//! member for its tests and its value, and the function of an aliased field
//! one read of the alias, which answer the same bytes with no read that buys
//! no answer ([`HOST_GETTERS`]). Such a build has no [`Thrown`] value and so
//! no path for a throw.

#[cfg(feature = "exec-eager-projection")]
use core::cell::Cell;
use core::cell::OnceCell;
use std::rc::Rc;

#[cfg(any(feature = "exec-eager-projection", feature = "canary-types"))]
use aosabi_execute::value::PlainObject;
use aosabi_execute::{
    execute::{
        FieldNodes,
        ResolveInfo, //
    },
    promise::{
        Agent,
        Promise, //
    },
    value::{
        Completion,
        ErrorObject,
        Function,
        HOST_GETTERS,
        HostValue,
        ObjectLike,
        Read,
        Selected,
        Thrown,
        Value,
        read_member,
        select_members, //
    }, //
};
use aosabi_type::{
    NamedType,
    get_named_type, //
};

use crate::{
    resolve_info::{
        ExecutionContext,
        run_with_execution_context, //
    }, //
    resolvers::{
        get_selected_fields::{
            CallState,
            SelectedField,
            Selection,
            get_selected_fields, //
        },
        view::View, //
    },
};

/// The name of the meta field that carries the concrete type of a value.
const TYPENAME: &str = "__typename";

/// The `name` of an anonymous function.
const ANONYMOUS: &str = "";

/// The selection below the nodes of one field against one type, which every
/// value wrapped for that field and that type reads once it is filled.
type Below = OnceCell<Selection>;

#[cfg(feature = "exec-eager-projection")]
thread_local! {
    /// Whether the projections of this thread are the object of the reference.
    static EAGER: Cell<bool> = const { Cell::new(false) };
}

/// Makes every projection this thread builds the object the reference builds
/// when `eager` holds, and a view of the selection otherwise.
///
/// The switch exists behind the dev feature `exec-eager-projection` alone,
/// which no card names, so a served build always projects into a view.
#[cfg(feature = "exec-eager-projection")]
pub fn set_eager_projection(eager: bool) {
    EAGER.with(|cell| cell.set(eager));
}

/// Returns whether the projections of this thread are the object of the reference.
#[cfg(feature = "exec-eager-projection")]
fn eager_projection() -> bool {
    EAGER.with(Cell::get)
}

/// Returns `value` bound to `parent` when it is a function.
///
/// The reference binds a function valued member so that `this` is the object
/// it was read from. A function an [`ObjectLike`] answers already holds its
/// object, so the binding is the identity, and the function stays as the place
/// the reference calls it.
#[must_use]
pub fn bind_if_method(value: Value, _parent: &Value) -> Value {
    value
}

/// Wraps `resolver`, a value an application returns, for the executor.
///
/// `context` is the store of the resolver call the value comes from,
/// `field_nodes` the nodes whose selection sets apply to the value, `None`
/// where the reference passes `undefined` and falls back to the nodes of
/// `context`, and `parent_type` the named type the value stands for.
///
/// # Errors
///
/// Returns the value a read of the projection threw, at once and with no
/// job, as the reference throws out of `wrapResolver`.
#[must_use = "the wrapped value is what the executor reads, or what a read threw"]
pub fn wrap_resolver(
    agent: &Agent,
    state: &CallState,
    resolver: Value,
    context: &Rc<ExecutionContext>,
    field_nodes: Option<&FieldNodes>,
    parent_type: Option<&NamedType>,
) -> Result<Value, Thrown> {
    let below = Below::new();
    wrap(
        agent,
        state,
        resolver,
        context,
        field_nodes,
        parent_type,
        &below,
    )
}

/// Wraps `resolver` as [`wrap_resolver()`] does, with `below` the selection
/// below `field_nodes` against `parent_type` once a value of the call reads
/// it.
///
/// Every value one wrap reaches with the same nodes and the same type, the
/// items of a list and the member of the same selected field of every object
/// of a selection, reads the selection from `below` and looks it up in the
/// cache of the request once, where the reference looks it up per value and
/// finds the same list.
fn wrap(
    agent: &Agent,
    state: &CallState,
    resolver: Value,
    context: &Rc<ExecutionContext>,
    field_nodes: Option<&FieldNodes>,
    parent_type: Option<&NamedType>,
    below: &Below,
) -> Result<Value, Thrown> {
    match resolver {
        // 1. FAST PATH: Primitives & Nulls
        // 2. LEAF OBJECTS: Dates
        Value::Undefined
        | Value::Null
        | Value::Bool(_)
        | Value::Number(_)
        | Value::String(_)
        | Value::Date(_) => Ok(resolver),
        // 3. ASYNC NODES: Promises
        Value::Promise(promise) => Ok(wrap_promise(
            agent,
            state,
            &promise,
            context,
            field_nodes,
            parent_type,
        )),
        // 4. COLLECTIONS: Arrays
        Value::List(items) => wrap_list(
            agent,
            state,
            &items,
            context,
            field_nodes,
            parent_type,
            below,
        ),
        // 5. EXECUTABLES: Functions
        Value::Function(function) => Ok(Value::Function(wrap_function(
            state,
            function,
            field_nodes.cloned(),
        ))),
        // 6. COMPLEX NODES: Objects
        Value::Object(_) | Value::Error(_) | Value::Host(_) => wrap_object(
            agent,
            state,
            resolver,
            context,
            field_nodes,
            parent_type,
            below,
        ),
    }
}

/// Returns `resolver.then(resolved => wrapResolver(resolved, ...))`.
///
/// One `then` with no rejection handler, so a rejection passes through to the
/// derived promise, and a throw of the wrapping of the settled value rejects
/// it.
fn wrap_promise(
    agent: &Agent,
    state: &CallState,
    promise: &Promise,
    context: &Rc<ExecutionContext>,
    field_nodes: Option<&FieldNodes>,
    parent_type: Option<&NamedType>,
) -> Value {
    let state = state.clone();
    let context = Rc::clone(context);
    let field_nodes = field_nodes.cloned();
    let parent_type = parent_type.cloned();
    let on_fulfilled = move |agent: &Agent, resolved: Value| {
        Completion::from(wrap_resolver(
            agent,
            &state,
            resolved,
            &context,
            field_nodes.as_ref(),
            parent_type.as_ref(),
        ))
    };
    Value::Promise(promise.then(agent, Some(Box::new(on_fulfilled)), None))
}

/// Wraps every item of a list, and gathers the results when one is a promise.
///
/// Every item is read against the same nodes and the same parent type, so the
/// items share `below` and the selection is looked up for the first object of
/// the list alone. An item whose wrapping throws throws the wrapping of the
/// list, as a throw inside the `map` of the reference does.
fn wrap_list(
    agent: &Agent,
    state: &CallState,
    items: &[Value],
    context: &Rc<ExecutionContext>,
    field_nodes: Option<&FieldNodes>,
    parent_type: Option<&NamedType>,
    below: &Below,
) -> Result<Value, Thrown> {
    let mut results: Vec<Value> = Vec::with_capacity(items.len());
    for item in items {
        results.push(wrap(
            agent,
            state,
            item.clone(),
            context,
            field_nodes,
            parent_type,
            below,
        )?);
    }

    // Performance: Check if any result is a Promise
    if results.iter().any(Value::is_thenable) {
        return Ok(Value::Promise(Promise::all(agent, results)));
    }
    Ok(Value::from(results))
}

/// Returns the function over `(args, ctx, info)` that calls `resolver` with
/// positional arguments.
///
/// The store of the call is put in place right before the user function runs
/// and for no longer.
///
/// The function is an arrow function the reference returns, which has no name,
/// because only a definition and an assignment to an identifier name an
/// anonymous function. The value printers write it as `[function]`.
fn wrap_function(
    state: &CallState,
    resolver: Function,
    field_nodes: Option<FieldNodes>,
) -> Function {
    let state = state.clone();
    Function::new(ANONYMOUS, move |agent, arguments| {
        let (Some(Value::Object(args)), Some(Value::Host(HostValue::Info(info)))) =
            (arguments.first(), arguments.get(2))
        else {
            return Completion::Throw(not_a_resolver_call());
        };
        call_with_positional_arguments(agent, &state, &resolver, field_nodes.as_ref(), args, info)
    })
}

/// Calls `resolver` with the arguments of `args` in the order the field
/// declares them, and wraps what it returns.
fn call_with_positional_arguments(
    agent: &Agent,
    state: &CallState,
    resolver: &Function,
    field_nodes: Option<&FieldNodes>,
    args: &Rc<dyn ObjectLike>,
    info: &Rc<ResolveInfo>,
) -> Completion {
    // Evaluate selected properties specifically at this function's scope
    let current_parent_type = get_named_type(&info.return_type);
    let field_nodes = field_nodes.unwrap_or(&info.field_nodes);
    let selected_fields = get_selected_fields(state, info, field_nodes, Some(&current_parent_type));
    let execution_context = Rc::new(ExecutionContext {
        info: Rc::clone(info),
        selected_fields,
    });

    run_with_execution_context(Rc::clone(&execution_context), || {
        let ordered_args = ordered_args(args, info);
        // Recurse on the result
        match resolver.call(agent, &ordered_args) {
            Completion::Normal(value) => Completion::from(wrap_resolver(
                agent,
                state,
                value,
                &execution_context,
                Some(field_nodes),
                Some(&current_parent_type),
            )),
            Completion::Throw(error) => Completion::Throw(error),
        }
    })
}

/// Returns the positional arguments of a call, one per declared argument.
///
/// An argument the request leaves `undefined` takes the default value of its
/// definition, which is `undefined` again for a definition without one. A
/// parent type that does not declare the field gives no argument at all.
fn ordered_args(args: &Rc<dyn ObjectLike>, info: &ResolveInfo) -> Vec<Value> {
    let field_def = info
        .parent_type
        .get_fields()
        .ok()
        .and_then(|fields| fields.get(&info.field_name));
    let Some(field_def) = field_def else {
        return Vec::new();
    };
    field_def
        .args
        .iter()
        .map(|arg| match Rc::clone(args).get(&arg.name) {
            Value::Undefined => Value::from_js(&arg.default_value),
            value => value,
        })
        .collect()
}

/// Returns what a wrapped function throws when it is not called as a resolver.
///
/// The reference reads `info.returnType` and the engine throws a `TypeError`
/// whose text is the engine's own. The executor always calls with
/// `(args, context, info)`, so the branch is not reachable through it, and
/// the error carries no message text.
fn not_a_resolver_call() -> Value {
    Value::from(ErrorObject::type_error(""))
}

/// Projects an object onto the fields the document selects below it.
///
/// The object itself comes back when nothing is selected, which is the case
/// for a leaf. The object learns the members the projection reads before the
/// first read, and a read that throws ends the projection with that throw. The
/// projection is a [`View`] of the selection whose slots the loop of the
/// reference fills, one member per selected field in the order of the
/// selection.
///
/// A field that repeats the name of an earlier one writes its member into
/// the slot of that field and leaves its own slot empty, as the second
/// assignment of the reference to `result[name]` replaces the value of the
/// property in place. Both fields read the same member, so either both hold
/// a property or neither does.
///
/// An object of an object type with no `isTypeOf` and with a static shape
/// whose projection would hand the executor every member the object hands it,
/// a shape source whose selection carries no alias and reads no method and no
/// member that can hold a function or a promise, is its own projection (see
/// `Selection::projects_to_source`), so the executor reads its data members by
/// their slots. The selection answers once per shape, so the objects of a list
/// of one shape pay one test of an address each.
// A `get_unchecked_mut` on the two slots of a repeated name would skip their bounds checks.
fn wrap_object(
    agent: &Agent,
    state: &CallState,
    resolver: Value,
    context: &Rc<ExecutionContext>,
    field_nodes: Option<&FieldNodes>,
    parent_type: Option<&NamedType>,
    below: &Below,
) -> Result<Value, Thrown> {
    let selection = below.get_or_init(|| {
        let field_nodes = field_nodes.unwrap_or(&context.info.field_nodes);
        Selection::of(get_selected_fields(
            state,
            &context.info,
            field_nodes,
            parent_type,
        ))
    });
    let selected_fields = Rc::clone(&selection.fields);
    let repeat = selection.repeat;

    if selected_fields.is_empty() {
        return Ok(resolver);
    }

    if let Value::Object(object) = &resolver {
        select_members(&**object, selected_fields.iter().map(selection_of));
    }

    #[cfg(any(feature = "exec-eager-projection", feature = "canary-types"))]
    if eager_projection_at(parent_type) {
        let result = project_eagerly(agent, state, &resolver, context, &selected_fields)?;
        // The type resolver of a canary build tests every property
        // name of the value, which its projection holds.
        #[cfg(feature = "canary-types")]
        crate::glue::resolve_type::hold_names(&resolver, parent_type, &result);
        return Ok(Value::from(result));
    }

    if let (Some(NamedType::Object(object_type)), Value::Object(object)) = (parent_type, &resolver)
        && object_type.is_type_of.is_none()
        && let Some(shape) = object.shape()
        && selection.projects_to_source(state, &context.info, shape)
    {
        return Ok(resolver);
    }

    let mut slots: Box<[Option<Value>]> = selected_fields
        .iter()
        .map(|selected| project_member(agent, state, &resolver, context, selected))
        .collect::<Result<_, Thrown>>()?;
    if let Some((first, later)) = repeat
        && let Some(member) = slots.get_mut(later).and_then(Option::take)
        && let Some(slot) = slots.get_mut(first)
    {
        *slot = Some(member);
    }
    Ok(Value::Object(Rc::new(View::new(selected_fields, slots))))
}

/// Answers whether the projection at `parent_type` is the object the
/// reference builds in place of a view: on a thread the dev feature
/// `exec-eager-projection` switches, and with the feature `canary-types` at a
/// union or an interface whose projection holds every property name of the
/// value.
#[cfg(any(feature = "exec-eager-projection", feature = "canary-types"))]
fn eager_projection_at(parent_type: Option<&NamedType>) -> bool {
    #[cfg(feature = "canary-types")]
    let held = crate::glue::resolve_type::holds_names(parent_type);
    #[cfg(not(feature = "canary-types"))]
    let held = {
        let _ = parent_type;
        false
    };
    #[cfg(feature = "exec-eager-projection")]
    let eager = eager_projection();
    #[cfg(not(feature = "exec-eager-projection"))]
    let eager = false;
    held || eager
}

/// Projects an object into the object the reference builds, a [`PlainObject`]
/// with one property per selected name the object holds, or the thrown value
/// of a read or a wrapping that throws.
///
/// The second judge of [`View`].
#[cfg(any(feature = "exec-eager-projection", feature = "canary-types"))]
fn project_eagerly(
    agent: &Agent,
    state: &CallState,
    resolver: &Value,
    context: &Rc<ExecutionContext>,
    selected_fields: &[SelectedField],
) -> Result<Rc<PlainObject>, Thrown> {
    let result = PlainObject::new();
    for selected in selected_fields {
        if let Some(member) = project_member(agent, state, resolver, context, selected)? {
            result.set(&selected.name, member);
        }
    }
    Ok(result)
}

/// Returns how the projection reads the member of the selected field
/// `selected`, which the object it projects learns before the first read.
///
/// The member is routed where the alias route of [`project_member`] may take
/// it, and nested where the wrapping of its value may project that value,
/// which a selection set below the field or an abstract type of the field
/// asks for.
fn selection_of(selected: &SelectedField) -> Selected<'_> {
    Selected {
        key: &selected.name,
        routed: selected.name != TYPENAME && selected.aliased,
        nested: selected
            .field_nodes
            .iter()
            .any(|node| node.selection_set.is_some())
            || matches!(
                selected.return_type,
                Some(NamedType::Interface(_) | NamedType::Union(_))
            ),
    }
}

/// What the first read of a member takes: a test where a getter of a host
/// may run at each read, and else the value, which the tests and the fast
/// path of the projection take from that one read ([`HOST_GETTERS`]).
const FIRST_READ: Read = if HOST_GETTERS {
    Read::Test
} else {
    Read::Value
};

/// Returns the member of the projection for one selected field, `None` for a
/// member that is left out, or the thrown value of a read or a wrapping that
/// throws.
///
/// With a getter of a host the member is read for the truth test of `:298`,
/// once more for the type test of the same line when the first read answers a
/// value that is true, and for its value at `:348` on the fast path, three
/// reads that an object of a host answers one getter call each. Without one
/// the first read is the value of the member, which the two tests and the fast
/// path take, since an object of a Rust type answers every read with the same
/// value ([`HOST_GETTERS`]).
fn project_member(
    agent: &Agent,
    state: &CallState,
    resolver: &Value,
    context: &Rc<ExecutionContext>,
    selected: &SelectedField,
) -> Result<Option<Value>, Thrown> {
    // Check if ANY of the requested nodes for this field use an alias
    let has_aliases = selected.aliased;
    let first = read(resolver, &selected.name, FIRST_READ)?;
    // If the field is an app resolver, we just wrap it and return it
    let is_app_resolver = truthy(&first)
        && if HOST_GETTERS {
            matches!(
                read(resolver, &selected.name, Read::Test)?,
                Value::Function(_)
            )
        } else {
            matches!(first, Value::Function(_))
        };

    // `__typename` never takes the alias route, because the type resolution
    // of the executor reads the member before any field resolver runs and
    // needs the name itself there, not a function that answers it.
    if has_aliases && !is_app_resolver && selected.name != TYPENAME {
        return Ok(Some(Value::Function(alias_router(
            state, resolver, context, selected,
        ))));
    }

    // Fast path: No aliases involved. Safely resolve statically.
    let raw_value = if HOST_GETTERS {
        read(resolver, &selected.name, Read::Value)?
    } else {
        first
    };
    if matches!(raw_value, Value::Undefined) {
        return Ok(None);
    }
    wrap(
        agent,
        state,
        bind_if_method(raw_value, resolver),
        context,
        Some(&selected.field_nodes),
        selected.return_type.as_ref(),
        &selected.below,
    )
    .map(Some)
}

/// Returns whether `value` is true, ECMA-262 `ToBoolean`.
///
/// `undefined`, `null`, `false`, `+0`, `-0`, `NaN` and the empty string are
/// false, and every other value is true.
fn truthy(value: &Value) -> bool {
    match value {
        Value::Undefined | Value::Null => false,
        Value::Bool(flag) => *flag,
        Value::Number(number) => *number != 0.0 && !number.is_nan(),
        Value::String(text) => !text.is_empty(),
        _ => true,
    }
}

/// Returns the function that picks the member of an aliased field per call.
///
/// Gateway data carries a member under the alias the document wrote, and local
/// data carries it under the field name, so the function looks under the alias
/// of the node that runs first and under the field name second. With a getter
/// of a host it reads the alias once for the test of `undefined`, once more
/// for the value when it is defined, or else the field name once, and a read
/// that throws is the throw of the call. Without one the read of the alias is
/// its value as well ([`HOST_GETTERS`]).
fn alias_router(
    state: &CallState,
    resolver: &Value,
    context: &Rc<ExecutionContext>,
    selected: &SelectedField,
) -> Function {
    let state = state.clone();
    let resolver = resolver.clone();
    let context = Rc::clone(context);
    let return_type = selected.return_type.clone();
    // An assignment to a member names no function, so this one is anonymous as
    // well.
    Function::new(ANONYMOUS, move |agent, arguments| {
        let Some(Value::Host(HostValue::Info(info))) = arguments.get(2) else {
            return Completion::Throw(not_a_resolver_call());
        };
        let wrap = |value: Value| {
            wrap_resolver(
                agent,
                &state,
                bind_if_method(value, &resolver),
                &context,
                Some(&info.field_nodes),
                return_type.as_ref(),
            )
        };
        let route = || {
            // Priority 1: Check if this specific alias exists on the resolved object (Gateway data)
            let alias_key = info
                .field_nodes
                .first()
                .and_then(|node| node.alias.as_ref());
            if let Some(alias) = alias_key {
                let first = read(&resolver, alias.value(), FIRST_READ)?;
                if !matches!(first, Value::Undefined) {
                    return wrap(if HOST_GETTERS {
                        read(&resolver, alias.value(), Read::Value)?
                    } else {
                        first
                    });
                }
            }

            // Priority 2: Fall back to the schema key (Local execution or unaliased Gateway data)
            match read(&resolver, &info.field_name, Read::Value)? {
                Value::Undefined => Ok(Value::Undefined),
                schema_value => wrap(schema_value),
            }
        };
        Completion::from(route())
    })
}

/// Returns `resolver[key]` for a value the object branch projects, read for
/// `kind`, or the thrown value of a read that throws.
///
/// An object answers its member through [`read_member`]. An error answers
/// `message` and `name`, which every error of the reference carries, and no
/// other member: the `stack` of the engine and the own members of a
/// `GraphQLError` have no reading here. A value of the executor itself
/// answers nothing.
fn read(resolver: &Value, key: &str, kind: Read) -> Result<Value, Thrown> {
    match resolver {
        Value::Object(object) => read_member(object, key, kind),
        Value::Error(error) => Ok(match key {
            "message" => Value::String(error.message()),
            "name" => Value::String(error.name()),
            _ => Value::Undefined,
        }),
        _ => Ok(Value::Undefined),
    }
}

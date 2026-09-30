// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-core resolvers resolvers_to_graphql_resolvers: the root resolvers
 * of a service.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/define-pylon.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * Derived from @sentry/core 8.55.0, build/cjs/asyncContext/stackStrategy.js, MIT.
 */

//! The root resolvers.
//!
//! An application states its roots as objects, and the executor calls a
//! field resolver per root field. [`resolvers_to_graphql_resolvers`] builds
//! that resolver for every member of every root.
//!
//! A root resolver is three async functions inside each other with one
//! Sentry scope between the first and the second (source lines 372-420),
//! so what it returns is always a promise, also for a user function that
//! returns a plain value, and the promise settles a fixed number of jobs
//! after the user function returned. The order of the `errors` array and the
//! key order of `data` depend on that number, so the port keeps every layer.
//!
//! The subscribe resolver of a `Subscription` member is the root resolver
//! behind one synchronous check of the carrier policy of the request, the
//! allowlist of the kernel's publish module. A field the carrier does not serve
//! throws the refusal of [`crate::carrier`] before the root resolver runs, and
//! any other field calls it and answers its completion untouched, so the policy
//! of Network adds no job and no byte. The check sits in the binding, so the
//! HTTP path and the path below HTTP both reach it.

use core::{
    any::Any,
    cell::RefCell,
    mem, //
};
use std::{
    rc::Rc,
    sync::{
        Arc,
        Weak, //
    }, //
};

use aosabi_execute::{
    execute::{
        FieldResolve,
        FieldResolverFn,
        ResolveInfo, //
    },
    kernel_entry::RootFactory,
    promise::{
        Agent,
        async_function, //
    },
    value::{
        Completion,
        ErrorObject,
        HostValue,
        PlainObject,
        Value, //
    }, //
};
use aosabi_type::{
    ObjMap,
    ResolverHandle,
    get_named_type,
    schema_bindings::FieldBinding, //
};

use aosabi_http::Key;
use aosabi_json::{
    map::IndexMap,
    value::KeyHasherBuilder, //
};

use crate::{
    carrier::{
        CarrierPolicy,
        carrier_policy,
        refusal, //
    },
    context::{
        Context,
        RequestState,
        ambient_context,
        request_state, //
    },
    create_schema::ResolverMap,
    resolve_info::{
        ExecutionContext,
        run_with_execution_context, //
    },
    resolvers::{
        get_selected_fields::{
            CallState,
            get_selected_fields, //
        },
        wrap_resolver::wrap_resolver, //
    },
    service::CoreState, //
};

/// The message of a service without a `Query` member, source lines 465-475.
const QUERY_RESOLVER_REQUIRED: &str = concat!(
    "At least one 'Query' resolver must be provided.\n",
    "\n",
    "Example:\n",
    "\n",
    "export const graphql = {\n",
    "  Query: {\n",
    "    // Define at least one query resolver here\n",
    "    hello: () => 'world'\n",
    "  }\n",
    "}\n",
);

/// The context variable a root resolver stores its `info` under, source line 389.
pub const GRAPHQL_RESOLVE_INFO: Key<Rc<ResolveInfo>> = Key::new("graphqlResolveInfo");

/// The roots of a service, one factory per root object.
///
/// The reference holds the three objects themselves. A service of Aosabi is
/// shared between threads and a [`Value`] belongs to the thread that made it,
/// so each root is a factory that answers a fresh object per call, and a thread
/// reads the members of a root from one call of its factory, as
/// [`root_graphql_resolver`] states.
#[derive(Clone, Default)]
pub struct Resolvers {
    /// The factory of the `Query` object.
    pub query: Option<RootFactory>,
    /// The factory of the `Mutation` object.
    pub mutation: Option<RootFactory>,
    /// The factory of the `Subscription` object.
    pub subscription: Option<RootFactory>,
}

/// What one call of a root resolver works with.
struct RootCall {
    /// The service state.
    core: Arc<CoreState>,
    /// The member of the root object, which is the `resolver` of the reference.
    resolver: Value,
    /// The coerced arguments of the field.
    args: Rc<PlainObject>,
    /// The `info` of the field.
    info: Rc<ResolveInfo>,
}

/// One member of a root as a thread reads it, with the factory it comes from.
///
/// The member is the `value` the reference reads from the root at
/// source lines 426, 440 and 453.
struct RootMember {
    /// The factory the member comes from.
    factory: Weak<dyn Fn() -> Value + Send + Sync>,
    /// The member.
    value: Value,
}

impl RootMember {
    /// Returns whether the factory of the member lives, which is whether its weak handle upgrades.
    fn lives(&self) -> bool {
        self.factory.strong_count() > 0
    }
}

/// The members of the roots one thread reads, by [`member_key`].
type RootMembers = IndexMap<Box<[u8]>, RootMember, KeyHasherBuilder>;

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The members of the roots the thread reads, each read once.
    static ROOT_MEMBERS: RefCell<RootMembers> = RefCell::default();
}

/// The entries of the table of root members of one thread, by whether their factory lives.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RootMemberCounts {
    /// The entries whose factory lives.
    pub live: usize,
    /// The entries whose factory is gone, which leave the table at the next
    /// fill of the thread.
    pub gone: usize,
}

/// Returns the counts of the entries of the table of root members of the calling thread.
///
/// The devtool answers them, so section 8 of `tests/notebooks/20_execute.ipynb`
/// reads that a fill leaves no entry of a factory that is gone, over a root the
/// devtool holds outside every schema. A service built from SDL keeps its
/// factories alive after it is dropped, since its schema holds its types in a
/// reference cycle with the scope of its build, which holds the bindings, so
/// the entries of its roots stay live entries.
#[must_use]
pub fn root_member_counts() -> RootMemberCounts {
    ROOT_MEMBERS.with_borrow(|members| {
        let live = members.values().filter(|entry| entry.lives()).count();
        RootMemberCounts {
            live,
            gone: members.len() - live,
        }
    })
}

/// Returns the key of the member `key` of the root of `root` in the table of a thread.
///
/// The key is the address of the factory in the byte order of the machine,
/// followed by the bytes of `key`. The address has a fixed width, so two
/// members share a key only where they share the factory and the name.
fn member_key(root: &RootFactory, key: &str) -> Box<[u8]> {
    let address = Arc::as_ptr(root).cast::<()>().addr();
    let mut bytes = Vec::with_capacity(size_of::<usize>() + key.len());
    bytes.extend_from_slice(&address.to_ne_bytes());
    bytes.extend_from_slice(key.as_bytes());
    bytes.into_boxed_slice()
}

/// Returns the member `key` of the root of `root` as a root field of the calling thread reads it.
///
/// The read is the one of [`root_graphql_resolver`], with the key of the
/// table made per call. The devtool reads a root through it outside every
/// schema, so `tests/notebooks/20_execute.ipynb` drops a factory the table
/// holds entries of, which no service built from SDL drops.
#[must_use]
pub fn read_root_member(root: &RootFactory, key: &str) -> Value {
    root_member(root, &member_key(root, key), key)
}

/// Returns the member of the root of `root` that `member` keys, read once per thread.
///
/// The closure of a root field holds its factory, so the address in a key it
/// finds belongs to that factory, and the read needs no check of the weak
/// handle. A miss reads the root, which [`read_root`] states.
fn root_member(root: &RootFactory, member: &[u8], key: &str) -> Value {
    let found =
        ROOT_MEMBERS.with_borrow(|members| members.get(member).map(|entry| entry.value.clone()));
    match found {
        Some(value) => value,
        None => read_root(root, member, key),
    }
}

/// Reads the members of the root of `root` into the table and returns the one `member` keys.
///
/// The reference reads the members of a root once, when the service starts
/// (source lines 426, 440 and 453), and this is the one call of
/// `root` a thread makes for them while the factory answers the same own keys
/// on every call, which every factory of the workspace does. A root whose
/// object answers `false` to
/// [`aosabi_execute::value::ObjectLike::values_outlive_request`], a root
/// of the host of `aosabi-js`, keeps no entry, and each read calls its
/// factory and reads the member in the running request. The key that
/// missed gets an entry also where the object lacks it, read as
/// `object[key]`, and a member the table holds keeps its value. The fill
/// first drops every entry whose factory is gone, with its value. The weak
/// handle of an entry keeps a root built again in the thread from finding a
/// member of the root it replaces, since no new factory takes the address of
/// a factory an entry still names.
#[cold]
#[inline(never)]
fn read_root(root: &RootFactory, member: &[u8], key: &str) -> Value {
    // The members of gone roots drop after the borrow, so their destructors
    // find the table free.
    drop(ROOT_MEMBERS.with_borrow_mut(take_gone));
    let object = match root() {
        Value::Object(object) => Some(object),
        _ => None,
    };
    // A root of a host answers its members as handles of the running
    // request, which the host drops with it, so the table keeps none of them
    // and each request reads its member, as a read per call did.
    if let Some(object) = &object
        && !object.values_outlive_request()
    {
        return Rc::clone(object).get(key);
    }
    let mut read: Vec<(Box<[u8]>, Value)> = Vec::new();
    if let Some(object) = &object {
        for own in object.own_keys() {
            let value = Rc::clone(object).get(&own);
            read.push((member_key(root, &own), value));
        }
    }
    if !read.iter().any(|(own, _)| **own == *member) {
        let value = object.map_or(Value::Undefined, |object| object.get(key));
        read.push((Box::from(member), value));
    }
    ROOT_MEMBERS.with_borrow_mut(|members| {
        for (own, value) in read {
            members.entry(own).or_insert_with(|| RootMember {
                factory: Arc::downgrade(root),
                value,
            });
        }
        members
            .get(member)
            .map_or(Value::Undefined, |entry| entry.value.clone())
    })
}

/// Takes every entry whose factory is gone out of `members` and returns their members.
fn take_gone(members: &mut RootMembers) -> Vec<RootMember> {
    if members.values().all(RootMember::lives) {
        return Vec::new();
    }
    let mut gone = Vec::new();
    for (member, entry) in mem::take(members) {
        if entry.lives() {
            members.insert(member, entry);
        } else {
            gone.push(entry);
        }
    }
    gone
}

/// Converts the roots of a service into the resolver map of its schema.
///
/// Each factory is called once to read the member names of its root, and a root
/// without a member is left out. A member of `Subscription` is bound as
/// `subscribe` beside a `resolve` that answers the payload. The loop over extra
/// keys at `:479-483` has no reading, because [`Resolvers`] has the three roots
/// and no other member.
///
/// # Errors
///
/// Returns the plain error `At least one 'Query' resolver must be provided.`
/// with its example text when `query` is absent or has no member.
pub fn resolvers_to_graphql_resolvers(
    core: &Arc<CoreState>,
    resolvers: &Resolvers,
) -> Result<ResolverMap, Value> {
    // Convert the Query and Mutation resolvers to GraphQL resolvers.
    let mut graphql_resolvers = ResolverMap::default();
    let roots = [
        ("Query", &resolvers.query, false),
        ("Mutation", &resolvers.mutation, false),
        ("Subscription", &resolvers.subscription, true),
    ];
    for (type_name, root, subscribes) in roots {
        let Some(root) = root else {
            continue;
        };
        let fields = root_fields(core, root, subscribes);
        if !fields.is_empty() {
            graphql_resolvers
                .fields
                .insert(type_name.to_owned(), fields);
        }
    }

    // Query root type must be provided.
    if !graphql_resolvers.fields.contains_key("Query") {
        return Err(Value::from(ErrorObject::error(QUERY_RESOLVER_REQUIRED)));
    }
    Ok(graphql_resolvers)
}

/// Returns the bindings of one root, one per member of its object.
fn root_fields(
    core: &Arc<CoreState>,
    root: &RootFactory,
    subscribes: bool,
) -> ObjMap<FieldBinding> {
    let Value::Object(object) = root() else {
        return ObjMap::default();
    };
    object
        .own_keys()
        .into_iter()
        .map(|key| {
            let resolver = root_graphql_resolver(core, root, &key);
            (key, root_binding(resolver, subscribes))
        })
        .collect()
}

/// Returns the binding of one root field.
///
/// A `Subscription` member subscribes through the check of the carrier policy.
fn root_binding(resolver: FieldResolverFn, subscribes: bool) -> FieldBinding {
    if !subscribes {
        return FieldBinding {
            resolve: Some(ResolverHandle::new(FieldResolve(resolver))),
            subscribe: None,
        };
    }
    let payload: FieldResolverFn =
        Arc::new(|_agent, payload, _args, _context, _info| Completion::Normal(payload.clone()));
    FieldBinding {
        resolve: Some(ResolverHandle::new(FieldResolve(payload))),
        subscribe: Some(ResolverHandle::new(FieldResolve(allowed(resolver)))),
    }
}

/// Returns the subscribe resolver that runs `resolver` when the carrier of the request allows its field.
///
/// The policy is the one on the context in place, [`CarrierPolicy::NETWORK`]
/// when no context is in place or none was set. A refused field throws the
/// refusal synchronously, an original `GraphQLError` the executor locates at
/// the field and the masking passes, so it reaches the caller as an error
/// before the first yield. The rule is rule 9 of the plan's section KERNEL.
fn allowed(resolver: FieldResolverFn) -> FieldResolverFn {
    Arc::new(move |agent, source, args, context, info| {
        let policy = ambient_context().map_or(CarrierPolicy::NETWORK, |ctx| carrier_policy(&ctx));
        if !policy.allow.allows(&info.field_name) {
            return Completion::Throw(refusal(&info.field_name, policy.carrier).0);
        }
        resolver(agent, source, args, context, info)
    })
}

/// Returns the field resolver of the member `key` of a root.
///
/// The reference reads the member once, when the service starts. The port reads
/// it once per thread, from a table of the thread keyed by the address of the
/// factory and the key, which the first call of a field of the root in the
/// thread fills with one call of `root`. While the factory answers the same own
/// keys on every call, which every factory of the workspace does, no later call
/// of the thread calls the factory. The two async functions and the scope
/// between them stay as the reference has them.
// An `unsafe` `Send` member in the closure would skip the table.
#[must_use]
pub fn root_graphql_resolver(
    core: &Arc<CoreState>,
    root: &RootFactory,
    key: &str,
) -> FieldResolverFn {
    let core = Arc::clone(core);
    let root = Arc::clone(root);
    let member = member_key(&root, key);
    let key = key.to_owned();
    Arc::new(move |agent, _source, args, _context, info| {
        let call = RootCall {
            core: Arc::clone(&core),
            resolver: root_member(&root, &member, &key),
            args: Rc::clone(args),
            info: Rc::clone(info),
        };
        // The outer `async` arrow of `:374-420`. It returns the promise of the
        // scope, so it settles two jobs after that promise does.
        let outer = async_function(agent, |agent| {
            with_scope(agent, |agent| {
                let scope = async_function(agent, |agent| scope_body(agent, call));
                Completion::Normal(Value::Promise(scope))
            })
        });
        Completion::Normal(Value::Promise(outer))
    })
}

/// Runs the body of the async function `Sentry.withScope` is called with.
///
/// The context is the store of `asyncContext`, read with `getStore()` and not
/// through `getContext`, so its environment stays as it is. The warning of
/// `:383-387` for a missing context is a line of `consola`, which is no part of
/// the runtime logger and is not written, since every request of a service and
/// of the handler runs inside a context. Without a context the call still runs,
/// on a request state of its own.
fn scope_body(agent: &Agent, call: RootCall) -> Completion {
    let ctx = ambient_context();
    if let Some(ctx) = &ctx {
        ctx.set(GRAPHQL_RESOLVE_INFO, Rc::clone(&call.info));
    }

    // get query or mutation field
    let state = CallState {
        core: Arc::clone(&call.core),
        request: ctx
            .as_ref()
            .map_or_else(|| Rc::new(RequestState::default()), request_state),
    };
    let root_parent_type = get_named_type(&call.info.return_type);
    let selected_fields = get_selected_fields(
        &state,
        &call.info,
        &call.info.field_nodes,
        Some(&root_parent_type),
    );
    let execution_context = Rc::new(ExecutionContext {
        info: Rc::clone(&call.info),
        selected_fields,
    });

    run_with_execution_context(Rc::clone(&execution_context), || {
        let inner = async_function(agent, |agent| {
            inner_body(agent, &state, call, ctx, &execution_context)
        });
        Completion::Normal(Value::Promise(inner))
    })
}

/// Runs the body of the innermost async function, which wraps the member and
/// calls it when it is a function.
///
/// A wrapping that throws throws the body, which rejects the async function.
fn inner_body(
    agent: &Agent,
    state: &CallState,
    call: RootCall,
    ctx: Option<Context>,
    execution_context: &Rc<ExecutionContext>,
) -> Completion {
    let root_parent_type = get_named_type(&call.info.return_type);
    let wrapped = wrap_resolver(
        agent,
        state,
        call.resolver,
        execution_context,
        Some(&call.info.field_nodes),
        Some(&root_parent_type),
    );

    let Ok(Value::Function(wrapped)) = wrapped else {
        return Completion::from(wrapped);
    };
    let ctx = ctx.map_or(Value::Undefined, |ctx| {
        let ctx: Rc<dyn Any> = Rc::new(ctx);
        Value::Host(HostValue::Context(ctx))
    });
    let info = Value::Host(HostValue::Info(call.info));
    wrapped.call(agent, &[Value::from(call.args), ctx, info])
}

/// Runs `callback` inside a forked scope and pops the scope when its result settles.
///
/// The strategy `@sentry/core` runs without a client. The scope stack has no
/// reader below the Sentry plugin, so the push and the pop have no reading, and
/// what is left is the one `then` with a passing and a rethrowing handler that
/// the strategy adds to a thenable result. That `then` costs one job, which is
/// why the function exists.
pub fn with_scope(agent: &Agent, callback: impl FnOnce(&Agent) -> Completion) -> Completion {
    let maybe_promise_result = match callback(agent) {
        Completion::Normal(value) => value,
        Completion::Throw(error) => return Completion::Throw(error),
    };

    let Value::Promise(promise) = maybe_promise_result else {
        return Completion::Normal(maybe_promise_result);
    };
    let derived = promise.then(
        agent,
        Some(Box::new(|_agent, res| Completion::Normal(res))),
        Some(Box::new(|_agent, e| Completion::Throw(e))),
    );
    Completion::Normal(Value::Promise(derived))
}

// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-federation: the configured federation and the delegation of a typed
 * field.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/core/gateway.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 *
 * The cache of remote schemas, and, the class of a configured federation with
 * createGateway.
 */

//! The configured federation, the twin of `createGateway` and `configure`.
//!
//! `create_gateway::<R>()` answers the factory of a federation over the pulled
//! registry `R`, and its `configure` answers a [`Gateway`], a cheap handle over
//! the options, the table of the patches and the policies collected from them
//! once. `delegate` takes the arguments struct of a remote root field, which
//! fixes the path, the registry and the record the field answers, and the
//! literal [󰀅](crate::needs!), whose closure is typed by that record. It
//! answers a [`DelegateCall`], a future that owns a handle of its federation,
//! so a `Lazy` member that moves it in answers a `'static` future. The value it
//! answers is read through `FromDelegated<D::Return>`, so a resolver states
//! which Rust type stands for the remote one, and a type that keeps no remote
//! object is refused with the note that names `Unchecked`.
//!
//! `delegate` reads what the reference reads before its first await: the
//! resolve info, the context, the arguments and the root of the key, which the
//! operation map turns into the operation. The future then reads the remote
//! schema through `get_remote_schema`, the cache of the thread keyed by the URL
//! and the headers function, and runs [`delegate_to_schema`] over the wrapping
//! schema with the four transforms in the array order of the reference. A
//! result that is an error is thrown, what is left goes to `T::from_delegated`,
//! and the guard then answers `None` for the typed value it rejects, so the
//! guard reads the record `T` reads and not the raw object the reference hands
//! its guard.
//!
//! The cache holds one slot per URL and headers function, the introspection in
//! flight with the tasks that wait on it and then the wrapping schema beside
//! the subschema configuration its resolvers reach, whose executor sends with
//! that headers function. The map of the reference is keyed by the URL alone,
//! so every later federation of a URL sends through the executor and the
//! headers of the first, the credentials of the first among them, which the
//! owner answered with "A2 Fix it", so a federation with another headers
//! function introspects the URL once more and sends through an executor of its
//! own, the entry of the cache in `NOT_PORTED`. The patches, the policies and
//! `strict` are read from the delegating federation at each delegation and
//! never from the slot, so the headers function is the one part of a
//! configuration the slot keeps.

use std::{
    cell::RefCell,
    fmt,
    future::{
        Future,
        IntoFuture, //
    },
    marker::PhantomData,
    pin::Pin,
    rc::Rc,
    sync::Arc,
    task::{
        Context as TaskContext,
        Poll,
        Waker, //
    }, //
};

use aosabi_core::{
    Context,
    Thrown,
    get_context,
    remote::{
        Delegate,
        FromDelegated,
        Registry, //
    },
    resolve_info::get_resolve_info, //
};
use aosabi_execute::{
    errors::thrown_definition_error,
    execute::ResolveInfo,
    promise::{
        Agent,
        Promise,
        async_function, //
    },
    value::{
        Completion,
        ContextValue,
        ErrorObject,
        HostValue,
        Value,
        to_string::to_js_string, //
    }, //
};
use aosabi_json::{
    JsObject,
    map::IndexMap,
    value::KeyHasherBuilder, //
};
use aosabi_lang::{
    ast::OperationTypeNode,
    js_string::to_string_lossy, //
};
use aosabi_type::Schema;

#[cfg(feature = "host-headers")]
use crate::headers::HostHeaders;
use crate::{
    delegate::{
        delegate_to_schema::{
            DelegateToSchemaOptions,
            delegate_to_schema, //
        },
        subschema::{
            Executor,
            SchemaOrSubschemaConfig,
            SubschemaConfig, //
        }, //
    },
    executor_http::{
        HttpExecutorOptions,
        build_http_executor, //
    },
    headers::{
        Headers,
        HeadersFn, //
    },
    needs::{
        Needs,
        NeedsBuilder,
        NeedsTarget, //
    },
    patches::{
        Passthrough,
        Patches, //
    },
    policy::Policies,
    transforms::{
        ExecutionRequest,
        Transform,
        force_args::ForceArgsTransform,
        inject_needs::InjectNeedsTransform,
        inline_args::InlineArgsTransform,
        patch::{
            PatchApi,
            PatchTable,
            PatchTransform, //
        }, //
    },
    utils::same_bytes,
    wrap::{
        schema_from_executor,
        wrap_schema, //
    }, //
};

/// The message of a key that is no `<Operation>.<field>` with a root the operation map names.
///
/// The reference throws `Invalid delegate key format` and `Unsupported
/// operation type` for such a key, which the register `NOT_PORTED` holds,
/// because the key of a pulled registry is a generated constant. The inline
/// constant of `delegate` refuses such a key with the same text when the
/// call is compiled, so no typed delegation meets it at run time, and the
/// untyped seam of the devtool throws it for a key a request line names.
const MALFORMED_KEY: &str = "the key of a delegation is \"<Operation>.<field>\" with the root Query, Mutation or Subscription";

/// The message of a delegation whose future is polled outside of an agent.
///
/// No analogue in the reference, where a job queue always exists. A
/// delegation reads the resolve info first, and a resolver always runs on
/// an agent, so no delegation meets the text.
const NO_AGENT: &str = "a delegation runs outside of an agent";

/// The message of an introspection that answered something other than a schema.
///
/// No analogue in the reference. `schema_from_executor` answers a schema
/// or throws, so no delegation meets the text.
const NO_SCHEMA: &str = "the introspection of the remote answered no schema";

/// The message of an introspection whose request ended before its answer arrived.
///
/// No analogue in the reference, whose promise settles whatever request
/// awaits it. An agent of Aosabi ends with its request, so an introspection
/// it started can end unanswered, and the text stands in the place of the
/// message of a rejection.
const ENDED: &str = "the request that introspected it ended before the answer";

/// The factory `createGateway<TRegistry>()` returns.
pub struct GatewayFactory<R: Registry>(PhantomData<fn() -> R>);

/// Returns the factory of a federation over the pulled registry `R`, `createGateway<TRegistry>()`.
#[must_use]
pub const fn create_gateway<R: Registry>() -> GatewayFactory<R> {
    GatewayFactory(PhantomData)
}

/// The options of `configure`, `{url, headers?, patches?, strict?}`.
pub struct GatewayOptions<R: Registry> {
    /// The endpoint of the remote service.
    pub url: String,
    /// The headers sent with every request to the remote, a function of the context.
    pub headers: Option<HeadersFn>,
    /// The patches of the remote types, keyed by type name.
    pub patches: Option<Patches<R>>,
    /// Fail on a remote type that has no patch, instead of publishing it whole.
    pub strict: Option<bool>,
}

impl<R: Registry> Default for GatewayOptions<R> {
    fn default() -> Self {
        Self {
            url: String::new(),
            headers: None,
            patches: None,
            strict: None,
        }
    }
}

impl<R: Registry> GatewayFactory<R> {
    /// Returns the federation over `config`, the twin of `configure(config)`.
    ///
    /// The policies are collected once, as the constructor of the reference
    /// collects them.
    #[must_use]
    pub fn configure(self, config: GatewayOptions<R>) -> Gateway<R> {
        let patches = config.patches.map(|patches| Rc::new(patches.into_table()));
        self.__configure_table(
            config.url,
            config.headers,
            patches,
            config.strict.unwrap_or(false),
        )
    }

    /// Returns the federation over the untyped table `patches`, the seam of the devtool.
    ///
    /// The patches of a request line are ids of a behaviour table, which no
    /// typed [`Patches`] can name, so `aosabi-devtool federation` hands the
    /// table it builds here. `configure` ends in this function, so both
    /// collect the policies the same way.
    #[doc(hidden)]
    #[must_use]
    pub fn __configure_table(
        self,
        url: String,
        headers: Option<HeadersFn>,
        patches: Option<Rc<PatchTable>>,
        strict: bool,
    ) -> Gateway<R> {
        let policies = patches
            .as_deref()
            .map_or_else(Policies::new, PatchTable::policies);
        Gateway {
            config: Rc::new(Config {
                url,
                headers,
                #[cfg(feature = "host-headers")]
                host: None,
                patches,
                policies: Rc::new(policies),
                strict,
            }),
            registry: PhantomData,
        }
    }

    /// Returns the federation over the untyped table `patches` with the
    /// headers function of the host `headers`, the seam of the TypeScript
    /// option under the feature `host-headers`.
    ///
    /// An app of the TypeScript option configures its federation with
    /// functions of the host, which no [`HeadersFn`] and no typed
    /// [`Patches`] hold, so the engine of that option hands the table it
    /// builds of the patches of the app and the call of its headers
    /// function here. The policies are collected the way
    /// [`GatewayFactory::__configure_table`] collects them, and a slot of
    /// the cache is keyed by the identity of `headers` where a function of
    /// the configuration keys it by its address.
    #[cfg(feature = "host-headers")]
    #[doc(hidden)]
    #[must_use]
    pub fn __configure_host(
        self,
        url: String,
        headers: Option<HostHeaders>,
        patches: Option<Rc<PatchTable>>,
        strict: bool,
    ) -> Gateway<R> {
        let policies = patches
            .as_deref()
            .map_or_else(Policies::new, PatchTable::policies);
        Gateway {
            config: Rc::new(Config {
                url,
                headers: None,
                host: headers,
                patches,
                policies: Rc::new(policies),
                strict,
            }),
            registry: PhantomData,
        }
    }
}

/// The configuration a [`Gateway`] holds, read once by `configure`.
struct Config {
    /// The endpoint of the remote service.
    url: String,
    /// The headers function of the configuration.
    headers: Option<HeadersFn>,
    /// The headers function of the host, none for a federation of the Rust
    /// option.
    #[cfg(feature = "host-headers")]
    host: Option<HostHeaders>,
    /// The table of the patches, none without the option `patches`.
    patches: Option<Rc<PatchTable>>,
    /// The policies of the patches keyed `<Type>.<field>`, `collectPolicies(patches)`.
    policies: Rc<Policies>,
    /// Whether a remote type with no patch is refused.
    strict: bool,
}

impl Config {
    /// Returns the HTTP executor of the remote, the executor `delegate` hands `getRemoteSchema`.
    ///
    /// The headers function of the executor is always set, and it answers a
    /// copy of what the headers function of the configuration answers for the
    /// context of the request, or no header without one. The introspection
    /// carries no context, so the function of the configuration answers for
    /// none (quirk Q77).
    fn executor(&self) -> Executor {
        let headers = self.headers;
        #[cfg(feature = "host-headers")]
        let host = self.host.clone();
        build_http_executor(HttpExecutorOptions {
            endpoint: self.url.clone(),
            headers: Some(Rc::new(move |request: &ExecutionRequest| {
                let context = request
                    .context
                    .as_ref()
                    .and_then(|context| context.downcast_ref::<Context>());
                #[cfg(feature = "host-headers")]
                if let Some(host) = &host {
                    return (host.call)(context);
                }
                headers.map_or_else(Headers::new, |headers| headers(context))
            })),
        })
    }

    /// Returns what a slot of the cache of this federation is keyed by beside the URL.
    fn key(&self) -> SlotKey {
        SlotKey {
            headers: self.headers,
            #[cfg(feature = "host-headers")]
            host: self.host.as_ref().map(|host| host.identity),
        }
    }
}

/// A configured federation, a cheap handle a patch and a resolver clone freely.
pub struct Gateway<R: Registry> {
    /// The configuration, shared by every clone.
    config: Rc<Config>,
    /// The registry the federation delegates to.
    registry: PhantomData<fn() -> R>,
}

impl<R: Registry> Clone for Gateway<R> {
    fn clone(&self) -> Self {
        Self {
            config: Rc::clone(&self.config),
            registry: PhantomData,
        }
    }
}

impl<R: Registry> fmt::Debug for Gateway<R> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Gateway")
            .field("url", &self.config.url)
            .field("headers", &self.config.headers.is_some())
            .field("patches", &self.config.patches)
            .field("policies", &self.config.policies.len())
            .field("strict", &self.config.strict)
            .finish()
    }
}

impl<R: Registry> Gateway<R> {
    /// Returns the delegation of the remote root field `args` names, with the `needs` selection.
    ///
    /// The resolve info is read first and throws
    /// `Resolve info is not available` outside a resolver, which a route
    /// handler and a job meet, and the context is read next and throws
    /// `Context not defined` outside a request. The arguments are the object of
    /// `D::arguments()`, and the root of `D::PATH` goes through the operation
    /// map. A refusal is kept and thrown by the future, as an async function
    /// rejects its promise.
    pub fn delegate<D, T, N>(&self, args: D, needs: N) -> DelegateCall<R, D, T>
    where
        D: Delegate<Registry = R>,
        D::Return: NeedsTarget,
        T: FromDelegated<D::Return>,
        N: FnOnce(
            NeedsBuilder<<D::Return as NeedsTarget>::Record>,
        ) -> NeedsBuilder<<D::Return as NeedsTarget>::Record>,
    {
        const {
            assert!(
                is_delegate_key(D::PATH),
                "the key of a delegation is \"<Operation>.<field>\" with the root Query, Mutation or Subscription"
            );
        }
        let needs = needs(NeedsBuilder::new()).build();
        DelegateCall {
            gateway: self.clone(),
            prepared: prepare(D::PATH, move || args.arguments(), needs),
            guard: None,
            call: PhantomData,
        }
    }

    /// Returns the untyped delegation of the remote root field `key` names.
    ///
    /// The form is the reference's `delegate(key, {args, needs})`. It is the
    /// seam of the devtool, whose request lines name the key, the arguments and
    /// the needs as data. It reads what [`Gateway::delegate`] reads before its
    /// first await, and its future answers the value `delegateToSchema` settles
    /// with, a result that is an error thrown. A key that is no
    /// `<Operation>.<field>` with a root of the operation map throws when the
    /// future is awaited.
    #[doc(hidden)]
    #[must_use]
    pub fn __delegate_key(
        &self,
        key: &str,
        args: JsObject,
        needs: Needs,
    ) -> Pin<Box<dyn Future<Output = Result<Value, Thrown>>>> {
        let gateway = self.clone();
        let prepared = prepare(key, move || args, needs);
        Box::pin(async move {
            let result = run(gateway, prepared?).await?;
            if result.is_error() {
                return Err(Thrown(result));
            }
            Ok(result)
        })
    }

    /// Returns the endpoint of the remote service.
    #[must_use]
    pub fn url(&self) -> &str {
        &self.config.url
    }
}

/// What a delegation reads before its first await.
struct Prepared {
    /// The resolve info of the delegating field, `info`.
    info: Rc<ResolveInfo>,
    /// The context of the request, `ctx`.
    ctx: Context,
    /// The arguments of the remote root field, `args`.
    args: JsObject,
    /// The selection fetched beside what the client selects, `needs`.
    needs: Needs,
    /// The operation the root of the key maps to, `operation`.
    operation: OperationTypeNode,
    /// The name of the remote root field, `fieldName`.
    field_name: String,
}

/// Reads the resolve info, the context, the arguments and the key of a delegation.
///
/// `info` and `ctx` are values here, so a check that either is missing has
/// no case left once the two reads answer.
/// The key passes [`is_delegate_key`] before it is split, the rule the inline
/// constant of [`Gateway::delegate`] applies, so a key the reference refuses
/// with `Invalid delegate key format`, an empty field among them, throws
/// [`MALFORMED_KEY`] here before any request.
fn prepare(
    path: &str,
    arguments: impl FnOnce() -> JsObject,
    needs: Needs,
) -> Result<Prepared, Thrown> {
    let execution = get_resolve_info()?;
    let ctx = get_context()?;
    let args = arguments();
    if !is_delegate_key(path) {
        return Err(Thrown::error(MALFORMED_KEY));
    }
    let mut halves = path.split('.');
    let (Some(root_type), Some(field_name)) = (halves.next(), halves.next()) else {
        return Err(Thrown::error(MALFORMED_KEY));
    };
    let operation = match root_type {
        "Query" => OperationTypeNode::Query,
        "Mutation" => OperationTypeNode::Mutation,
        "Subscription" => OperationTypeNode::Subscription,
        _ => return Err(Thrown::error(MALFORMED_KEY)),
    };
    Ok(Prepared {
        info: Rc::clone(&execution.info),
        ctx,
        args,
        needs,
        operation,
        field_name: String::from(field_name),
    })
}

/// Returns whether `path` splits at its first dot into a root of the operation map and a field.
///
/// The halves are what `String(key).split('.')` answers first and second, so a
/// further dot ends the field as the destructuring of the reference ends it,
/// and a field that is empty, `Query.` and `Query..x`, is refused as the falsy
/// `fieldName` of the reference is. The inline constant of
/// [`Gateway::delegate`] and [`prepare`] both ask it.
const fn is_delegate_key(path: &str) -> bool {
    let bytes = path.as_bytes();
    let mut dot = 0;
    while dot < bytes.len() && bytes[dot] != b'.' {
        dot += 1;
    }
    if dot == bytes.len() || dot + 1 == bytes.len() || bytes[dot + 1] == b'.' {
        return false;
    }
    let (root, _) = bytes.split_at(dot);
    same_bytes(root, b"Query") || same_bytes(root, b"Mutation") || same_bytes(root, b"Subscription")
}

/// The check of the option `guard`, which answers whether the delegated value stands.
type Guard<T> = Box<dyn Fn(T) -> T>;

/// One delegation, a future that owns a handle of its federation.
pub struct DelegateCall<R: Registry, D, T> {
    /// The federation that delegates.
    gateway: Gateway<R>,
    /// What the delegation read before its first await, or what that read threw.
    prepared: Result<Prepared, Thrown>,
    /// The option `guard`, applied to the value read back.
    guard: Option<Guard<T>>,
    /// The arguments struct of the remote root field.
    call: PhantomData<fn() -> D>,
}

impl<R: Registry, D, U: 'static> DelegateCall<R, D, Option<U>> {
    /// Answers `None` when `check` rejects the delegated value, the `guard` option of `delegate`.
    #[must_use]
    pub fn guard<F: Fn(&U) -> bool + 'static>(mut self, check: F) -> Self {
        self.guard = Some(Box::new(move |value: Option<U>| {
            value.filter(|value| check(value))
        }));
        self
    }
}

impl<R: Registry, D, T> fmt::Debug for DelegateCall<R, D, T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DelegateCall")
            .field("gateway", &self.gateway)
            .field(
                "needs",
                &self.prepared.as_ref().ok().map(|prepared| &prepared.needs),
            )
            .field("guard", &self.guard.is_some())
            .finish_non_exhaustive()
    }
}

impl<R, D, T> IntoFuture for DelegateCall<R, D, T>
where
    R: Registry,
    D: Delegate<Registry = R> + 'static,
    T: FromDelegated<D::Return> + 'static,
{
    type Output = Result<T, Thrown>;
    type IntoFuture = Pin<Box<dyn Future<Output = Result<T, Thrown>>>>;

    fn into_future(self) -> Self::IntoFuture {
        let Self {
            gateway,
            prepared,
            guard,
            call: _,
        } = self;
        Box::pin(async move {
            let result = run(gateway, prepared?).await?;
            // A result that is an error is thrown, so the executor reads it as
            // the failure of the field.
            if result.is_error() {
                return Err(Thrown(result));
            }
            let value = T::from_delegated(result)?;
            Ok(match guard {
                Some(guard) => guard(value),
                None => value,
            })
        })
    }
}

/// Runs one delegation and answers the value `delegateToSchema` settles with.
///
/// The remote schema is awaited through the cache, and `delegate_to_schema`
/// gets the four transforms in the array order of the reference, which
/// `Transformer::new` reverses, so the request passes the patch, the force, the
/// inline and the needs transforms in that order. That order is quirk
/// `Q79_FEDERATION_FORCE_BEFORE_NEEDS`, where the comment of the reference says
/// the force transform runs last. A synchronous throw of the delegation is the
/// rejection of the call, and any other answer is awaited, as
/// `await delegateToSchema(...)` awaits it. The slot of the remote schema is
/// kept until the answer settles, so the subschema configuration the proxying
/// resolvers reach outlives a reset of the cache in between.
///
/// The registry is read only where the federation becomes the handle every
/// patch receives, so the body is [`run_untyped`], one function for every
/// registry, as the one `delegate` of the reference serves every
/// federation.
fn run<R: Registry>(
    gateway: Gateway<R>,
    prepared: Prepared,
) -> impl Future<Output = Result<Value, Thrown>> {
    let config = Rc::clone(&gateway.config);
    let api: PatchApi = Rc::new(gateway);
    run_untyped(config, api, prepared)
}

/// The body of [`run`], with the federation as the handle of its patches.
async fn run_untyped(
    config: Rc<Config>,
    api: PatchApi,
    prepared: Prepared,
) -> Result<Value, Thrown> {
    let Prepared {
        info,
        ctx,
        args,
        needs,
        operation,
        field_name,
    } = prepared;
    let (schema, slot) = {
        let agent = Agent::current().ok_or_else(|| Thrown::type_error(NO_AGENT))?;
        get_remote_schema(&agent, &config.url, config.key(), || config.executor())
    };
    let Value::Host(HostValue::Schema(schema)) = schema.await.map_err(Thrown)? else {
        return Err(Thrown::type_error(NO_SCHEMA));
    };
    let answer = {
        let agent = Agent::current().ok_or_else(|| Thrown::type_error(NO_AGENT))?;
        let transforms: Vec<Rc<dyn Transform>> = vec![
            Rc::new(InjectNeedsTransform::new(Some(needs))),
            Rc::new(InlineArgsTransform::new(Some(args.clone()))),
            Rc::new(ForceArgsTransform::new(
                Some(Rc::clone(&config.policies)),
                ctx.clone(),
            )),
            Rc::new(PatchTransform::new(
                config.patches.clone(),
                api,
                config.strict,
            )),
        ];
        let context: ContextValue = Rc::new(ctx);
        let mut options =
            DelegateToSchemaOptions::new(SchemaOrSubschemaConfig::Schema(schema), info);
        options.operation = Some(operation);
        options.field_name = Some(field_name);
        options.args = Some(args);
        options.context = Some(context);
        options.transforms = transforms;
        match delegate_to_schema(&agent, options) {
            Completion::Normal(value) => Promise::resolve(&agent, value),
            Completion::Throw(error) => return Err(Thrown(error)),
        }
    };
    let result = answer.await.map_err(Thrown);
    drop(slot);
    result
}

/// The state of one slot of the cache of remote schemas.
enum SlotState {
    /// The introspection and the wrapping run.
    Pending,
    /// The wrapping schema, with the configuration its proxying resolvers reach.
    Ready {
        /// The wrapping schema of the remote.
        schema: Arc<Schema>,
        /// The subschema configuration `{schema, executor}` the schema was wrapped from.
        #[expect(
            dead_code,
            reason = "the slot keeps the configuration alive, which the proxying resolvers reach through a weak handle"
        )]
        config: Rc<SubschemaConfig>,
    },
    /// The error the attempt threw, `Gateway could not introspect the remote schema at <url>`.
    Failed(Value),
}

/// What a slot of the cache is keyed by beside the URL: the headers function
/// of the federation whose executor the slot keeps.
#[derive(Clone, Copy)]
struct SlotKey {
    /// The headers function of the configuration.
    headers: Option<HeadersFn>,
    /// The identity of the headers function of the host, none for a
    /// federation of the Rust option.
    #[cfg(feature = "host-headers")]
    host: Option<u64>,
}

/// One entry of the cache of remote schemas, the promise the map of the reference holds.
struct Slot {
    /// The headers function of the federation whose executor the slot keeps.
    key: SlotKey,
    /// The introspection in flight or what it settled with.
    state: RefCell<SlotState>,
    /// The tasks that wait on the introspection in flight.
    waiters: RefCell<Vec<Waker>>,
}

impl Slot {
    /// Returns a slot whose introspection runs, for the federations with the headers function
    /// `key` names.
    fn pending(key: SlotKey) -> Rc<Self> {
        Rc::new(Self {
            key,
            state: RefCell::new(SlotState::Pending),
            waiters: RefCell::new(Vec::new()),
        })
    }

    /// Returns whether the slot serves a federation with the headers function `headers`.
    ///
    /// Two federations without one share a slot, and two with one share it when
    /// both are the same function by address. Rust promises no single address
    /// per function, so one function at two addresses costs one more
    /// introspection and never sends through the other's executor, while two
    /// functions merged to one address have one body and send the same headers.
    /// A headers function of the host serves the federations whose function the
    /// host names by the same identity.
    fn serves(&self, key: SlotKey) -> bool {
        #[cfg(feature = "host-headers")]
        if self.key.host != key.host {
            return false;
        }
        match (self.key.headers, key.headers) {
            (None, None) => true,
            (Some(kept), Some(asked)) => core::ptr::fn_addr_eq(kept, asked),
            _ => false,
        }
    }

    /// Stores what the introspection settled with and wakes every task that waits on it.
    fn settle(&self, state: SlotState) {
        self.state.replace(state);
        let waiters = core::mem::take(&mut *self.waiters.borrow_mut());
        for waiter in waiters {
            waiter.wake();
        }
    }

    /// Returns the completion of a slot that settled, or `None` while it runs.
    fn settled(&self) -> Option<Completion> {
        match &*self.state.borrow() {
            SlotState::Pending => None,
            SlotState::Ready { schema, .. } => Some(Completion::Normal(Value::Host(
                HostValue::Schema(Arc::clone(schema)),
            ))),
            SlotState::Failed(error) => Some(Completion::Throw(error.clone())),
        }
    }

    /// Returns a promise of `agent` that settles as the slot does.
    ///
    /// A slot that settled answers a settled promise, as the cached promise
    /// of the reference is settled. A slot that runs answers the promise of
    /// a task that waits on it, since a promise of the agent that runs the
    /// introspection belongs to the request of that agent.
    fn promise(self: &Rc<Self>, agent: &Agent) -> Promise {
        match self.settled() {
            Some(Completion::Normal(value)) => Promise::resolve(agent, value),
            Some(Completion::Throw(error)) => Promise::reject(agent, error),
            None => agent.spawn(Box::pin(SlotWait(Rc::clone(self)))),
        }
    }
}

/// The wait of one task on a slot whose introspection runs.
struct SlotWait(Rc<Slot>);

impl Future for SlotWait {
    type Output = Completion;

    fn poll(self: Pin<&mut Self>, cx: &mut TaskContext<'_>) -> Poll<Completion> {
        if let Some(completion) = self.0.settled() {
            return Poll::Ready(completion);
        }
        let mut waiters = self.0.waiters.borrow_mut();
        if !waiters.iter().any(|known| known.will_wake(cx.waker())) {
            waiters.push(cx.waker().clone());
        }
        Poll::Pending
    }
}

thread_local! {
    /// The remote schemas by URL, each URL with a slot per headers function, the map `schemaCache`.
    ///
    /// The map is per runtime thread, which is per process for a served
    /// card with its one runtime thread, so two federations over one URL
    /// with one headers function share a slot as the module wide map of the
    /// reference does (the plan, section FEDERATION, rule 6), and a
    /// federation with another headers function takes a slot of its own,
    /// where the reference shares the first (the entry of `NOT_PORTED`).
    /// A URL holds one slot for each headers function its federations
    /// name, so the slots of one URL are few and found by a scan.
    static SCHEMA_CACHE: RefCell<IndexMap<String, Vec<Rc<Slot>>, KeyHasherBuilder>> =
        RefCell::new(IndexMap::default());
}

/// Returns the slot of the cache for the URL `url` and the headers function `key` names.
fn cached_slot(url: &str, key: SlotKey) -> Option<Rc<Slot>> {
    SCHEMA_CACHE.with(|cache| {
        cache
            .borrow()
            .get(url)
            .and_then(|slots| slots.iter().find(|slot| slot.serves(key)).cloned())
    })
}

/// The attempt of one introspection, which fails its slot when it ends without an answer.
///
/// The rejection handler of the cached promise owns it. A handler that
/// runs fails the slot with the message of the rejection. A handler that is
/// dropped unrun while the slot runs belongs to an agent that ended before
/// the answer arrived, the request that started the introspection, and the
/// slot fails with [`ENDED`] in place of the message, so a delegation that
/// waits on it settles and the next one introspects again.
struct Attempt {
    /// The slot the attempt installed.
    slot: Rc<Slot>,
    /// The URL the slot is keyed by.
    url: String,
}

impl Attempt {
    /// Removes the slot when the cache still holds it, fails it and answers the error it failed with.
    fn fail(&self, message: &str) -> Value {
        SCHEMA_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            let emptied = cache.get_mut(&self.url).is_some_and(|slots| {
                slots.retain(|known| !Rc::ptr_eq(known, &self.slot));
                slots.is_empty()
            });
            if emptied {
                cache.shift_remove(&self.url);
            }
        });
        let error = Value::from(ErrorObject::error(format!(
            "Gateway could not introspect the remote schema at {}: {message}",
            self.url
        )));
        self.slot.settle(SlotState::Failed(error.clone()));
        error
    }
}

impl Drop for Attempt {
    fn drop(&mut self) {
        if self.slot.settled().is_none() {
            let _ = self.fail(ENDED);
        }
    }
}

/// Answers a promise of the wrapping schema of the remote at `url`, introspected once per URL and
/// headers function.
///
/// An async function, so the promise it answers resolves through the promise of
/// the slot. The reference keys its map by the URL alone, and the slot here is
/// keyed by the URL and `key`, the headers function of the federation that
/// delegates, so the executor of a slot sends with the headers function of
/// every federation that reads it (the entry of `NOT_PORTED`). A URL with no
/// slot for `key` builds the executor with `make_executor`, sends the
/// introspection with no context, wraps the client schema with
/// `wrap_schema({schema, executor})` and installs the slot before the answer
/// arrives. A throw of the introspection before any request installs nothing,
/// as the throw inside `Promise.resolve(schemaFromExecutor(..))` rejects the
/// async function first. A rejection removes the slot only when the map still
/// holds it and throws the plain `Error`
/// `Gateway could not introspect the remote schema at <url>: <message>`, whose
/// cause has no reader in the executor and is not kept. The slot comes back
/// beside the promise for the caller to keep, and none when nothing was
/// installed.
fn get_remote_schema(
    agent: &Agent,
    url: &str,
    key: SlotKey,
    make_executor: impl FnOnce() -> Executor,
) -> (Promise, Option<Rc<Slot>>) {
    let cached = cached_slot(url, key);
    let mut kept = cached.clone();
    let promise = async_function(agent, |agent| {
        if let Some(cached) = cached {
            return Completion::Normal(Value::Promise(cached.promise(agent)));
        }
        let executor = make_executor();
        let introspected = match schema_from_executor(agent, &executor, None) {
            Completion::Normal(value) => Promise::resolve(agent, value),
            Completion::Throw(error) => return Completion::Throw(error),
        };
        let slot = Slot::pending(key);
        let ready = Rc::clone(&slot);
        let wrapped = introspected.then(
            agent,
            Some(Box::new(move |_agent, schema| {
                let Value::Host(HostValue::Schema(schema)) = schema else {
                    return Completion::Throw(Value::from(ErrorObject::type_error(NO_SCHEMA)));
                };
                let config = Rc::new(SubschemaConfig::new(schema, Some(executor)));
                match wrap_schema(&config) {
                    Ok(schema) => {
                        let value = Value::Host(HostValue::Schema(Arc::clone(&schema)));
                        ready.settle(SlotState::Ready { schema, config });
                        Completion::Normal(value)
                    }
                    Err(error) => Completion::Throw(thrown_definition_error(error)),
                }
            })),
            None,
        );
        let attempt = Attempt {
            slot: Rc::clone(&slot),
            url: String::from(url),
        };
        let entry = wrapped.then(
            agent,
            None,
            Some(Box::new(move |_agent, err| {
                let message = match &err {
                    Value::Error(error) => error.message(),
                    value => to_js_string(value),
                };
                Completion::Throw(attempt.fail(&to_string_lossy(&message)))
            })),
        );
        SCHEMA_CACHE.with(|cache| {
            let mut cache = cache.borrow_mut();
            match cache.get_mut(url) {
                Some(slots) => slots.push(Rc::clone(&slot)),
                None => {
                    cache.insert(String::from(url), vec![Rc::clone(&slot)]);
                }
            }
        });
        kept = Some(slot);
        Completion::Normal(Value::Promise(entry))
    });
    (promise, kept)
}

/// Returns the identity patch, which says the type is exposed in full on purpose.
#[must_use]
pub const fn passthrough() -> Passthrough {
    Passthrough
}

/// Drops every cached remote schema, the test seam `__resetSchemaCache`.
///
/// A delegation that awaits an introspection keeps its slot, and the next
/// delegation introspects again.
pub fn reset_schema_cache() {
    SCHEMA_CACHE.with(|cache| *cache.borrow_mut() = IndexMap::default());
}

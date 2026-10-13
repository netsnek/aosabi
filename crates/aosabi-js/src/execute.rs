// SPDX-License-Identifier: MIT
/*
 * aosabi-js execute: the executor of the engine over the app of the boot,
 * below HTTP, in a test build of the module.
 *
 * No analogue in the reference.
 */

//! The executor of the engine below HTTP, the feature `execute`.
//!
//! The feature makes a test build of the module, as the feature `trap` does,
//! which the npm package `aosabi` never carries and which adds no export
//! line, no import and no crate. The boot of such a build also builds the
//! [`Service`] of the app, the schema and the roots the handler of Yoga runs
//! over, and a request of the method `POST` to the path [`PATH`] answers the
//! payload `aosabi-devtool execute` answers for one request of its protocol:
//! the result text of the executor, `{"result": "<text>"}`, or
//! `{"thrown": {"name", "message"}}` for what the entry throws. The body of
//! the request is the JSON object of that protocol, its `document`, its
//! `variables`, its `operationName` and its `validate`.
//!
//! The request runs through `prepare_request` and `finish_request` of the
//! executor crate, the pipeline [`Service::execute`] runs, inside the context
//! of a synthetic `POST /graphql` that is also the context value of the
//! executor, with `@inContext` applied, and the agent of the request is
//! driven as the request loop of the module drives every agent, so a promise
//! of the host settles through the export `settle` and every job runs in the
//! microtask the host gives it in the faithful mode. The JavaScript
//! resolvers of a behaviour table or of a fixture run on the host, and the
//! wrapping of `aosabi-core` calls them as it calls the resolvers of every
//! app of the TypeScript option.

#![forbid(unsafe_code)]

use core::{
    any::Any,
    cell::RefCell, //
};
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    Service,
    context::{
        ambient_hooks,
        run_with_context,
        service_context, //
    },
    in_context::apply_in_context, //
};
use aosabi_execute::{
    kernel_entry::{
        Executed,
        GeneralExecutor,
        Prepared,
        Request,
        RunFailure,
        finish_request,
        prepare_request, //
    },
    promise::{
        Agent,
        AgentOptions, //
    },
    value::Value,
};
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::js_string::to_string_lossy;

/// The path of a request of the executor.
pub const PATH: &str = "/.ts1/execute";

/// The message of a request whose body is no request of the protocol.
const NOT_A_REQUEST: &str = "the body is no request of the protocol of tests/sidecar/execute.mjs";

/// The message of a request that ends with its result pending, the text of
/// `aosabi-devtool execute`.
const STALLED: &str =
    "the request stalled: no job, no timer and no waker is left and the result is pending";

thread_local! {
    /// The service of the app of the boot.
    static SERVICE: RefCell<Option<Rc<Service>>> = const { RefCell::new(None) };
}

/// Keeps `service`, the service of the app of the boot.
pub fn set_service(service: Service) {
    SERVICE.with_borrow_mut(|slot| *slot = Some(Rc::new(service)));
}

/// Answers the body `body` of a request of the executor with the bytes of
/// its payload, or with a payload of the one message of a request the
/// protocol does not hold.
pub async fn answer(body: Rc<[u8]>) -> Vec<u8> {
    let Some(service) = SERVICE.with_borrow(Clone::clone) else {
        return payload_of_message("the boot built no service of the app");
    };
    let Some(call) = Call::read(&body) else {
        return payload_of_message(NOT_A_REQUEST);
    };
    let request = Request {
        document: &call.document,
        variables: call.variables.clone(),
        operation_name: call.operation_name.as_deref(),
        validate: call.validate,
    };
    payload(execute(&service, &request).await)
}

/// Runs `request` on `service` to its end, as [`Service::execute`] runs it,
/// with the agent driven by the request loop of the module.
async fn execute(service: &Service, request: &Request<'_>) -> Executed {
    let context = service_context();
    apply_in_context(&context, request);
    let context_value: Rc<dyn Any> = Rc::new(context.clone());
    let executor = GeneralExecutor {
        schema: Arc::clone(service.schema()),
        root_value: Arc::new(|| Value::Undefined),
        leaf_serializers: Some(Arc::clone(service.core().leaf_serializers())),
    };
    let prepared = run_with_context(context, || {
        prepare_request(
            &executor,
            request,
            context_value,
            AgentOptions::default(),
            &setup_agent,
        )
    });
    match prepared {
        Prepared::Answered(executed) => executed,
        Prepared::Settled { agent, state } => finish_request(&agent, Ok(state)),
        Prepared::Driving { agent, result } => {
            let driven = agent.drive(&result).await;
            finish_request(&agent, driven)
        }
    }
}

/// Gives the agent of a request the ambient hooks of the context, so the
/// context follows a resolver across its awaits, as the service gives them.
fn setup_agent(agent: &Agent) {
    let (capture, restore) = ambient_hooks();
    agent.set_ambient_hooks(capture, restore);
}

/// Returns the bytes of the payload of `executed`: `result`, the result text,
/// or `thrown`, the name and the message of what the entry threw.
fn payload(executed: Executed) -> Vec<u8> {
    let mut payload = JsObject::new();
    match executed.outcome {
        Ok(mut result) => {
            crate::value::finish_unmasked(&mut result.errors);
            let text = aosabi_json::to_string(&result.to_js_value()).unwrap_or_default();
            payload.insert("result", text);
        }
        Err(RunFailure::Thrown(thrown)) => {
            let mut record = JsObject::new();
            if let Value::Error(error) = &thrown {
                record.insert("name", error.name());
                record.insert("message", error.message());
            }
            payload.insert("thrown", JsValue::Object(record));
        }
        Err(RunFailure::Stalled) => return payload_of_message(STALLED),
    }
    aosabi_json::to_bytes(&JsValue::Object(payload)).unwrap_or_default()
}

/// Returns the bytes of the payload `{"fatal": message}`, which marks a
/// request the side refuses and which the notebook never compares.
fn payload_of_message(message: &str) -> Vec<u8> {
    let mut payload = JsObject::new();
    payload.insert("fatal", message);
    aosabi_json::to_bytes(&JsValue::Object(payload)).unwrap_or_default()
}

/// One request of the protocol.
struct Call {
    /// The text of the document.
    document: String,
    /// The variables, as the request spells them.
    variables: Option<JsValue>,
    /// The name of the operation.
    operation_name: Option<String>,
    /// Whether the specified rules run before the executor.
    validate: bool,
}

impl Call {
    /// Reads the request of the JSON object `body`, or `None` for a body that
    /// is no such object or names no document.
    fn read(body: &[u8]) -> Option<Self> {
        let JsValue::Object(request) = aosabi_json::parse(body).ok()? else {
            return None;
        };
        let JsValue::String(document) = request.get("document")? else {
            return None;
        };
        let operation_name = match request.get("operationName") {
            None | Some(JsValue::Null | JsValue::Undefined) => None,
            Some(JsValue::String(name)) => Some(to_string_lossy(name).into_owned()),
            Some(_) => return None,
        };
        Some(Self {
            document: to_string_lossy(document).into_owned(),
            variables: request.get("variables").cloned(),
            operation_name,
            validate: !matches!(request.get("validate"), Some(JsValue::Bool(false))),
        })
    }
}

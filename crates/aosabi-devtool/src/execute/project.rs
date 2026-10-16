// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool execute project: the projection of one root member of a
 * loaded behaviour table, read through the operations of an object.
 *
 * No analogue in the reference.
 */

//! The operation `project` of the batch form.
//!
//! A request carries `table`, the name of a loaded service table,
//! `document`, a query, and optionally `writes`, a list of `[key, value]`
//! pairs whose value is JSON, and `reads`, a list of names. The first field
//! of the query names a member of the root `Query` of the table, and the
//! member is wrapped as the root resolver of the reference wraps it, against
//! the node of that field and the named type of its definition, into the
//! view of the selection, or under `--projection object` into the object the
//! reference builds. The writes are assigned to the projection in their
//! order, and the reads read it in theirs, so a read of a name after the
//! next field of the selection moves the slot a view looks at first.
//!
//! The answer is `{"keys": [..], "reads": [[name, has, type, value], ..]}`:
//! the own keys of the projection, `Object.keys` of the reference, and per
//! read name whether it is an own property, the `typeof` of what a read of it
//! answers and that value where it is a string, `null` otherwise. No answer
//! of the reference reads the keys of a projection, since its type resolvers
//! ask `in` and its default type resolver reads `__typename`, so a notebook
//! holds the keys of the view to those of the object through this operation.
//!
//! A table the run never loaded, one of `plain` mode, a document that does
//! not parse, holds no query or opens with no field of `Query`, a member that
//! is a function and a projection that is no object are mistakes of the case
//! and answer `fatal`.

use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::{
    context::RequestState,
    resolve_info::ExecutionContext,
    resolvers::{
        get_selected_fields::{
            CallState,
            get_selected_fields, //
        },
        wrap_resolver::wrap_resolver,
    },
    service::CoreState,
};
use aosabi_execute::{
    execute::{
        FieldNodes,
        ResolveInfo,
        SharedSchema, //
    },
    normalized_executor::get_operation_ast,
    path::add_path,
    promise::{
        Agent,
        AgentOptions, //
    },
    value::{
        Completion,
        ObjectLike,
        Value, //
    }, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    ParseOptions,
    PathSegment,
    ast::{
        DefinitionNode,
        OperationTypeNode,
        SelectionNode, //
    },
    js_string::to_string_lossy, //
};
use aosabi_type::{
    NamedType,
    ObjMap,
    get_named_type, //
};

use crate::{
    batch,
    execute::{
        SESSION,
        no_table, //
    }, //
};

/// The name of the operation, which opens every refusal it writes.
const NAME: &str = "project";

/// Answers the operation `project`, the keys and the reads of the projection of one root member.
pub(crate) fn op_project(request: &JsObject) -> Result<JsObject, JsString> {
    let name = batch::require_string(request, "table")?;
    let text = batch::require_string(request, "document")?;
    let writes = pairs(request)?;
    let reads = names(request)?;
    let projection = SESSION.with_borrow(|session| {
        let (schema, factory) = session
            .query_root(name)
            .ok_or_else(|| format!("{NAME}: {} of the service mode", no_table(name)))?;
        project(schema, factory(), text)
    })?;
    for (key, value) in writes {
        projection.set(&key, Value::from_js(value));
    }
    let keys = projection.own_keys().into_iter().map(JsValue::from);
    let reads = reads.into_iter().map(|key| {
        let has = projection.has_own(&key);
        let value = Rc::clone(&projection).get(&key);
        let text = match &value {
            Value::String(text) => JsValue::from(text.clone()),
            _ => JsValue::Null,
        };
        let read = vec![
            JsValue::from(key),
            JsValue::from(has),
            JsValue::from(type_of(&value)),
            text,
        ];
        JsValue::Array(read)
    });
    let mut payload = JsObject::new();
    payload.insert("keys", JsValue::Array(keys.collect()));
    payload.insert("reads", JsValue::Array(reads.collect()));
    Ok(payload)
}

/// Returns the projection of the member of `root` the first field of the query `text` names.
///
/// The member is wrapped as the root resolver of the reference wraps it, by
/// `wrapResolver(resolver, executionContext, info.fieldNodes, rootParentType)`
/// over the info of that field, with a store of the request and of the
/// service of its own.
fn project(schema: &SharedSchema, root: Value, text: &str) -> Result<Rc<dyn ObjectLike>, String> {
    let document = aosabi_lang::parse(text, ParseOptions::default()).map_err(|error| {
        let message = to_string_lossy(&error.message);
        format!("{NAME}: the document does not parse: {message}")
    })?;
    let operation = get_operation_ast(&document, None)
        .filter(|operation| operation.operation == OperationTypeNode::Query)
        .ok_or_else(|| format!("{NAME}: the document holds no query"))?;
    let Some(SelectionNode::Field(field)) = operation.selection_set.selections.first() else {
        return Err(format!("{NAME}: the query opens with no field"));
    };
    let Some(NamedType::Object(query)) = schema.get_root_type(OperationTypeNode::Query) else {
        return Err(format!("{NAME}: the schema holds no Query"));
    };
    let field_name = field.name.value();
    let return_type = query
        .get_fields()
        .ok()
        .and_then(|fields| fields.get(field_name))
        .map(|definition| definition.r#type.clone())
        .ok_or_else(|| format!("{NAME}: Query holds no field {field_name}"))?;
    let member = match root {
        Value::Object(root) => root.get(field_name),
        _ => Value::Undefined,
    };
    if matches!(member, Value::Function(_)) {
        return Err(format!("{NAME}: the member {field_name} is a function"));
    }

    let field_nodes: FieldNodes = Rc::from(vec![Rc::new(field.clone())]);
    let mut fragments = ObjMap::default();
    for definition in &document.definitions {
        if let DefinitionNode::FragmentDefinition(fragment) = definition {
            fragments.insert(
                String::from(fragment.name.value()),
                Rc::new(fragment.clone()),
            );
        }
    }
    let info = Rc::new(ResolveInfo {
        field_name: String::from(field_name),
        field_nodes: Rc::clone(&field_nodes),
        return_type,
        parent_type: Arc::clone(query),
        path: add_path(
            None,
            PathSegment::Key(String::from(field_name)),
            Some(query.name.as_str()),
        ),
        schema: Arc::clone(schema),
        fragments: Rc::new(fragments),
        root_value: Value::Undefined,
        operation: Rc::new(operation.clone()),
        variable_values: Rc::default(),
    });
    let state = CallState {
        core: CoreState::new(ObjMap::default()),
        request: Rc::new(RequestState::default()),
    };
    let named = get_named_type(&info.return_type);
    let selected_fields = get_selected_fields(&state, &info, &field_nodes, Some(&named));
    let context = Rc::new(ExecutionContext {
        info,
        selected_fields,
    });
    let agent = Agent::new(AgentOptions::default());
    // A read of the projection throws only where an object of a host may
    // throw, which the devtool holds none of, and the completion answers
    // either form of the result.
    match Completion::from(wrap_resolver(
        &agent,
        &state,
        member,
        &context,
        Some(&field_nodes),
        Some(&named),
    )) {
        Completion::Normal(Value::Object(projection)) => Ok(projection),
        Completion::Normal(_) => Err(format!(
            "{NAME}: the projection of {field_name} is no object"
        )),
        Completion::Throw(_) => Err(format!("{NAME}: the projection of {field_name} threw")),
    }
}

/// Returns the list `writes` of `request`, each a key and a JSON value, empty when absent.
fn pairs(request: &JsObject) -> Result<Vec<(String, &JsValue)>, String> {
    let Some(writes) = request.get("writes") else {
        return Ok(Vec::new());
    };
    let JsValue::Array(writes) = writes else {
        return Err(format!("{NAME}: writes is no array"));
    };
    writes
        .iter()
        .map(|write| match write {
            JsValue::Array(pair) => match pair.as_slice() {
                [JsValue::String(key), value] => Ok((to_string_lossy(key).into_owned(), value)),
                _ => Err(format!("{NAME}: a write is no pair of a key and a value")),
            },
            _ => Err(format!("{NAME}: a write is no pair of a key and a value")),
        })
        .collect()
}

/// Returns the list `reads` of `request`, empty when absent.
fn names(request: &JsObject) -> Result<Vec<String>, String> {
    let Some(reads) = request.get("reads") else {
        return Ok(Vec::new());
    };
    let JsValue::Array(reads) = reads else {
        return Err(format!("{NAME}: reads is no array"));
    };
    reads
        .iter()
        .map(|read| match read {
            JsValue::String(name) => Ok(to_string_lossy(name).into_owned()),
            _ => Err(format!("{NAME}: a read is no string")),
        })
        .collect()
}

/// Returns `typeof value`.
///
/// Translation of the table of ECMA-262 section 13.5.3, `typeof`, over the
/// values of the executor, where every value that is no primitive and no
/// function is an object.
const fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Undefined => "undefined",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Function(_) => "function",
        Value::Null
        | Value::Date(_)
        | Value::List(_)
        | Value::Object(_)
        | Value::Promise(_)
        | Value::Error(_)
        | Value::Host(_) => "object",
    }
}

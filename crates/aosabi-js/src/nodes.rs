// SPDX-License-Identifier: MIT
/*
 * aosabi-js nodes: the views of the resolve info, of the nodes of the
 * document and of the arguments of an execution, which the host reads
 * through the export ctx.
 *
 * No analogue in the reference.
 */

//! The views the host makes of what the engine holds of a request.
//!
//! The host answers three values of graphql-js the engine holds: the resolve
//! info the resolver wrapping stores under `graphqlResolveInfo` at each call of a root
//! resolver and hands `getResolveInfo()`, the nodes of the document that info
//! names, and the arguments of the execution Envelop hands the hooks
//! `onExecute` and `onExecuteDone`. None of them crosses whole. This module
//! numbers each per request, writes the record of what a view answers at once,
//! and answers the rest when the host asks for it, so a view reads the
//! document lazily, and `aosabi-ts/lib/nodes.js` makes each view once per
//! request and number.
//!
//! A node of the document is numbered by where it lies, the root it hangs
//! from, a field node of an info, a fragment, the operation or the document
//! of the arguments, and the steps from that root, each the index of a
//! definition or of a selection. Two ways to one node of the document take
//! one number, since a node with a location is named by its kind, its start,
//! its end and its source, so a view of the host is one object wherever the
//! app reaches the node, as graphql-js hands every place one node. The record
//! of a node holds its kind and the members a walk of a selection reads at
//! once: the alias, the name and the arguments of a field, the type condition
//! of a fragment, the name of a spread, the operation and the name of an
//! operation, and whether a selection set follows. The selections of a node
//! and the definitions of the document come when the host asks for them, each
//! as the record of its node, as do the variables of an info and of the
//! arguments, whose files may be large. The values of the arguments of a field
//! cross with its record, nested at most [`DEPTH`] levels, and a value nested
//! deeper crosses as a mark the host answers with a refusal.

#![forbid(unsafe_code)]

use core::cell::RefCell;
use std::{
    rc::Rc,
    sync::Arc, //
};

use aosabi_core::resolve_info::ExecutionContext;
use aosabi_execute::{
    execute::{
        ExecutionArgs,
        ResolveInfo, //
    },
    value::Value,
};
use aosabi_hooks::context::HookContext;
use aosabi_json::{
    JsObject,
    JsValue, //
};
use aosabi_lang::{
    PathSegment,
    ast::{
        ArgumentNode,
        DefinitionNode,
        DocumentNode,
        FieldNode,
        FragmentDefinitionNode,
        FragmentSpreadNode,
        InlineFragmentNode,
        Location,
        NameNode,
        OperationDefinitionNode,
        SelectionNode,
        StringValueNode,
        ValueNode, //
    },
};
use graphql_over_http::GraphQLParams;

use crate::{
    host::{
        self,
        Reader,
        Tag, //
    },
    request::BOOT,
    value::push_variables, //
};

/// The ops of the export `ctx` this module answers, beside `RESOLVE_INFO` and
/// the variables of `c.var`, which `crate::context` answers.
pub mod op {
    /// The children of a node, the selections of its selection set or the
    /// definitions of the document, after the number of the node.
    pub const NODE: u8 = 28;
    /// The name of the operation and the document of the arguments of an
    /// execution, after the number of the arguments.
    pub const ARGS: u8 = 29;
    /// The variables of an info, `info.variableValues`, after its number.
    pub const INFO_VARIABLES: u8 = 30;
    /// The variables of the arguments of an execution, `args.variableValues`,
    /// after the number of the arguments.
    pub const ARGS_VARIABLES: u8 = 31;
}

/// The byte of an answer of the export `ctx` that holds a view record, beside
/// 0 for a value of the engine and 1 for a value record the host wrote.
pub const VIEW: u8 = 2;

/// The kinds of a view record.
mod view {
    /// An info, `c.get('graphqlResolveInfo')`.
    pub const INFO: u8 = 0;
    /// The store of a call, `getResolveInfo()`, its info and its selected
    /// fields.
    pub const CALL: u8 = 1;
    /// The arguments of an execution, their operation name and their document.
    pub const ARGS: u8 = 2;
    /// The children of a node, each the record of its node.
    pub const CHILDREN: u8 = 3;
    /// A copy of JSON data at any depth, the variables of the arguments.
    pub const JSON: u8 = 4;
    /// The variables of an execution, the number of their object and the
    /// record of its values.
    pub const VARIABLES: u8 = 5;
}

/// The kinds of the record of a node.
mod kind {
    /// An `OperationDefinition`.
    pub const OPERATION: u8 = 0;
    /// A `FragmentDefinition`.
    pub const FRAGMENT: u8 = 1;
    /// A `Field`.
    pub const FIELD: u8 = 2;
    /// A `FragmentSpread`.
    pub const SPREAD: u8 = 3;
    /// An `InlineFragment`.
    pub const INLINE: u8 = 4;
    /// The `Document` of the arguments of an execution.
    pub const DOCUMENT: u8 = 5;
    /// A definition no view of the host answers the members of, after its
    /// kind.
    pub const OTHER: u8 = 6;
}

/// The kinds of the record of a value node of an argument.
mod value {
    /// A `Variable`, after its name.
    pub const VARIABLE: u8 = 0;
    /// An `IntValue`, after its digits.
    pub const INT: u8 = 1;
    /// A `FloatValue`, after its digits.
    pub const FLOAT: u8 = 2;
    /// A `StringValue`, after its value and the byte of `block`.
    pub const STRING: u8 = 3;
    /// A `BooleanValue`, after its byte.
    pub const BOOLEAN: u8 = 4;
    /// A `NullValue`.
    pub const NULL: u8 = 5;
    /// An `EnumValue`, after its name.
    pub const ENUM: u8 = 6;
    /// A `ListValue`, after its count and its values.
    pub const LIST: u8 = 7;
    /// An `ObjectValue`, after its count and each field, its name and its
    /// value.
    pub const OBJECT: u8 = 8;
    /// A value nested deeper than [`super::DEPTH`] levels, which the host
    /// answers with a refusal.
    pub const DEEP: u8 = 9;
}

/// The depth past which a value node of an argument crosses as the mark
/// [`value::DEEP`].
const DEPTH: usize = 64;

/// The root a node of the document hangs from.
#[derive(Clone)]
enum Root {
    /// A field node of an info or of a selected field.
    Field(Rc<FieldNode>),
    /// A fragment definition of an info.
    Fragment(Rc<FragmentDefinitionNode>),
    /// The operation of an info.
    Operation(Rc<OperationDefinitionNode>),
    /// The document of the arguments of an execution.
    Document(Arc<DocumentNode>),
}

impl Root {
    /// Returns the address of the root, which names it while it lives.
    fn addr(&self) -> usize {
        match self {
            Self::Field(node) => Rc::as_ptr(node).addr(),
            Self::Fragment(node) => Rc::as_ptr(node).addr(),
            Self::Operation(node) => Rc::as_ptr(node).addr(),
            Self::Document(node) => Arc::as_ptr(node).addr(),
        }
    }
}

/// What names a node of the document: its kind and its location, or, for a
/// node with no location, its root and its steps.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Key {
    /// The kind, the start, the end and the address of the source.
    Located(u8, usize, usize, usize),
    /// The address of the root and the steps from it.
    Steps(usize, Vec<u32>),
}

/// A node of the document the host holds a view of: its root and the steps
/// from it.
#[derive(Clone)]
struct Noted {
    /// The root the node hangs from.
    root: Root,
    /// The index of each definition or selection from the root to the node.
    steps: Vec<u32>,
}

/// The arguments of an execution the host holds a view of.
struct Args {
    /// The document.
    document: Arc<DocumentNode>,
    /// The parameters of the request, which hold the operation name and the
    /// variables as the request parser read them.
    params: Option<Rc<GraphQLParams>>,
    /// The operation name of the arguments, for an execution with no
    /// parameters.
    operation_name: Option<String>,
    /// The variables of the arguments, for an execution with no parameters.
    variables: Option<JsValue>,
}

/// The views of one request.
struct Views {
    /// The request.
    request: u32,
    /// The infos, by their number less one.
    infos: Vec<Rc<ResolveInfo>>,
    /// The arguments, by their number less one.
    args: Vec<Args>,
    /// The variables of the executions, by their number less one.
    variables: Vec<Rc<JsObject>>,
    /// The nodes, by their number less one.
    nodes: Vec<Noted>,
    /// The number of each node by what names it, sorted by what names it.
    keys: Vec<(Key, u32)>,
}

// An `unsafe` cell would skip each borrow flag.
thread_local! {
    /// The views of the requests in flight.
    static VIEWS: RefCell<Vec<Views>> = const { RefCell::new(Vec::new()) };
}

/// Runs `body` on the views of `request`, made at the first use.
fn with_views<R>(request: u32, body: impl FnOnce(&mut Views) -> R) -> R {
    VIEWS.with_borrow_mut(|views| {
        let at = views
            .iter()
            .position(|held| held.request == request)
            .unwrap_or_else(|| {
                views.push(Views {
                    request,
                    infos: Vec::new(),
                    args: Vec::new(),
                    variables: Vec::new(),
                    nodes: Vec::new(),
                    keys: Vec::new(),
                });
                views.len() - 1
            });
        body(&mut views[at])
    })
}

/// Drops the views of `request`.
pub fn release(request: u32) {
    let gone = VIEWS.with_borrow_mut(|views| {
        views
            .iter()
            .position(|held| held.request == request)
            .map(|at| views.swap_remove(at))
    });
    drop(gone);
}

/// Returns the index of the number `number`, counted from 1.
fn index_of(number: u32) -> Option<usize> {
    usize::try_from(number).ok()?.checked_sub(1)
}

/// Notes the info `info` of `request` and returns its number, counted from 1,
/// the number of the same info where it was noted before.
fn note_info(request: u32, info: &Rc<ResolveInfo>) -> u32 {
    with_views(request, |views| {
        let at = views
            .infos
            .iter()
            .position(|held| Rc::ptr_eq(held, info))
            .unwrap_or_else(|| {
                views.infos.push(Rc::clone(info));
                views.infos.len() - 1
            });
        host::length(at + 1)
    })
}

/// Notes the arguments `args` of an execution of `request`, whose context
/// `context` holds the parameters of the request, and returns their number,
/// counted from 1, the number of the arguments of the same document where
/// they were noted before, as Envelop hands every hook of an execution one
/// object of its arguments.
pub fn note_args(request: u32, args: &ExecutionArgs, context: &HookContext) -> u32 {
    if request == BOOT {
        return 0;
    }
    with_views(request, |views| {
        if let Some(at) = views
            .args
            .iter()
            .position(|held| Arc::ptr_eq(&held.document, &args.document))
        {
            return host::length(at + 1);
        }
        views.args.push(Args {
            document: Arc::clone(&args.document),
            params: context.get_as::<GraphQLParams>("params"),
            operation_name: args.operation_name.clone(),
            variables: args.variables().cloned(),
        });
        host::length(views.args.len())
    })
}

/// Returns the view record of the info `info` of `request`, which
/// `c.get('graphqlResolveInfo')` answers.
pub fn info_view(request: u32, info: &Rc<ResolveInfo>) -> Vec<u8> {
    let mut out = vec![view::INFO];
    write_info(&mut out, request, info);
    out
}

/// Returns the view record of the store of the call `context` of `request`
/// numbered `number`, which `getResolveInfo()` answers: the number, the record
/// of its info and its selected fields, each its name, the name of its type or
/// none and its field nodes.
pub fn call_view(request: u32, number: u32, context: &ExecutionContext) -> Vec<u8> {
    let mut out = vec![view::CALL];
    host::push_word(&mut out, number);
    write_info(&mut out, request, &context.info);
    host::push_count(&mut out, context.selected_fields.len());
    for field in context.selected_fields.iter() {
        push_text(&mut out, &field.name);
        match &field.return_type {
            Some(named) => {
                out.push(1);
                push_text(&mut out, named.name());
            }
            None => out.push(0),
        }
        host::push_count(&mut out, field.field_nodes.len());
        for node in field.field_nodes.iter() {
            let root = Root::Field(Rc::clone(node));
            write_node(&mut out, request, &root, &[], At::Field(node));
        }
    }
    out
}

/// Returns whether the op `code` of the export `ctx` is one this module
/// answers.
pub const fn answers(code: u8) -> bool {
    matches!(
        code,
        op::NODE | op::ARGS | op::INFO_VARIABLES | op::ARGS_VARIABLES
    )
}

/// Answers the op of the record `record` of `request`, one [`answers`] names,
/// with the view record of its answer, or `None` for a broken record and for a
/// number no view of the request holds.
// Out of line, so a request compiles the views of the nodes where it reads one.
#[inline(never)]
pub fn answer(request: u32, record: &[u8]) -> Option<Vec<u8>> {
    let (&code, rest) = record.split_first()?;
    let number = Reader::new(rest).word()?;
    match code {
        op::NODE => children(request, number),
        op::ARGS => args_view(request, number),
        op::INFO_VARIABLES => info_variables(request, number),
        _ => args_variables(request, number),
    }
}

/// Appends the record of the info `info` of `request`: its number, the name
/// of its field, its return type as the schema prints it, the name of its
/// parent type, its path from the root, each key a string or a number with
/// the name of the type it stands in or none, the record of each of its field
/// nodes, the name and the record of each fragment of the document, the record
/// of the operation and whether its root value is `undefined`.
fn write_info(out: &mut Vec<u8>, request: u32, info: &Rc<ResolveInfo>) {
    host::push_word(out, note_info(request, info));
    push_text(out, &info.field_name);
    push_text(out, &info.return_type.to_string());
    push_text(out, &info.parent_type.name);
    let mut path = Vec::new();
    let mut at = Some(&info.path);
    while let Some(segment) = at {
        path.push(segment.node());
        at = segment.node().prev.as_ref();
    }
    host::push_count(out, path.len());
    for node in path.iter().rev() {
        match &node.key {
            PathSegment::Key(key) => {
                out.push(0);
                push_text(out, key);
            }
            PathSegment::Index(index) => {
                out.push(1);
                host::push_number(out, f64::from(host::length(*index)));
            }
        }
        match node.typename.as_deref() {
            Some(typename) => {
                out.push(1);
                push_text(out, typename);
            }
            None => out.push(0),
        }
    }
    host::push_count(out, info.field_nodes.len());
    for node in info.field_nodes.iter() {
        let root = Root::Field(Rc::clone(node));
        write_node(out, request, &root, &[], At::Field(node));
    }
    host::push_count(out, info.fragments.len());
    for (name, fragment) in info.fragments.iter() {
        push_text(out, name);
        let root = Root::Fragment(Rc::clone(fragment));
        write_node(out, request, &root, &[], At::Fragment(fragment));
    }
    let root = Root::Operation(Rc::clone(&info.operation));
    write_node(out, request, &root, &[], At::Operation(&info.operation));
    out.push(u8::from(!matches!(info.root_value, Value::Undefined)));
}

/// Answers the view record of the children of the node `number` of
/// `request`, the selections of its selection set or the definitions of the
/// document, or `None` for a node no view of the request holds.
fn children(request: u32, number: u32) -> Option<Vec<u8>> {
    let noted = with_views(request, |views| views.nodes.get(index_of(number)?).cloned())?;
    let at = locate(&noted.root, &noted.steps)?;
    let mut out = vec![view::CHILDREN];
    let mut steps = noted.steps.clone();
    steps.push(0);
    let last = steps.len() - 1;
    if let At::Document(document) = at {
        host::push_count(&mut out, document.definitions.len());
        for (index, definition) in document.definitions.iter().enumerate() {
            steps[last] = host::length(index);
            write_node(
                &mut out,
                request,
                &noted.root,
                &steps,
                At::of_definition(definition),
            );
        }
        return Some(out);
    }
    let selections = at.selections().unwrap_or_default();
    host::push_count(&mut out, selections.len());
    for (index, selection) in selections.iter().enumerate() {
        steps[last] = host::length(index);
        write_node(
            &mut out,
            request,
            &noted.root,
            &steps,
            At::of_selection(selection),
        );
    }
    Some(out)
}

/// Answers the view record of the arguments `number` of `request`: the
/// operation name the parameters of the request hold, `undefined` where they
/// hold none, as a copy of JSON data, and the record of the document.
// Out of line, so a request compiles the view of the arguments where it reads one.
#[inline(never)]
fn args_view(request: u32, number: u32) -> Option<Vec<u8>> {
    let (document, name) = with_views(request, |views| {
        let args = views.args.get(index_of(number)?)?;
        let name = match &args.params {
            Some(params) => params.get("operationName").cloned(),
            None => args.operation_name.as_deref().map(JsValue::from),
        };
        Some((Arc::clone(&args.document), name))
    })?;
    let mut out = vec![view::ARGS];
    match &name {
        Some(name) => host::push_json(&mut out, name),
        None => host::push_tag(&mut out, Tag::Undefined),
    }
    let root = Root::Document(Arc::clone(&document));
    write_node(&mut out, request, &root, &[], At::Document(&document));
    Some(out)
}

/// Answers the view record of the variables of the info `number` of
/// `request`: the number of the object of the variables of its execution, one
/// for every info of the execution, as graphql-js hands every info the one
/// object it coerced, and the record of the values the engine coerced, which
/// [`push_variables`] writes with the form of each value on the host.
// Out of line, so a request compiles the view of the variables of the info where it reads one.
#[inline(never)]
fn info_variables(request: u32, number: u32) -> Option<Vec<u8>> {
    let (info, variables) = with_views(request, |views| {
        let info = Rc::clone(views.infos.get(index_of(number)?)?);
        let at = views
            .variables
            .iter()
            .position(|held| Rc::ptr_eq(held, &info.variable_values))
            .unwrap_or_else(|| {
                views.variables.push(Rc::clone(&info.variable_values));
                views.variables.len() - 1
            });
        Some((info, host::length(at + 1)))
    })?;
    let mut out = vec![view::VARIABLES];
    host::push_word(&mut out, variables);
    push_variables(&mut out, &info, request);
    Some(out)
}

/// Answers the view record of the variables of the arguments `number` of
/// `request`, those the parameters of the request hold as the request parser
/// read them, `undefined` where they hold none, as a copy of JSON data.
// Out of line, so a request compiles the view of the variables of the arguments where it reads one.
#[inline(never)]
fn args_variables(request: u32, number: u32) -> Option<Vec<u8>> {
    let variables = with_views(request, |views| {
        let args = views.args.get(index_of(number)?)?;
        Some(match &args.params {
            Some(params) => params.get("variables").cloned(),
            None => args.variables.clone(),
        })
    })?;
    let mut out = vec![view::JSON];
    match &variables {
        Some(variables) => host::push_json(&mut out, variables),
        None => host::push_tag(&mut out, Tag::Undefined),
    }
    Some(out)
}

/// A node of the document as a walk meets it.
#[derive(Clone, Copy)]
enum At<'a> {
    /// The document of the arguments of an execution.
    Document(&'a DocumentNode),
    /// An operation.
    Operation(&'a OperationDefinitionNode),
    /// A fragment definition.
    Fragment(&'a FragmentDefinitionNode),
    /// A field.
    Field(&'a FieldNode),
    /// A fragment spread.
    Spread(&'a FragmentSpreadNode),
    /// An inline fragment.
    Inline(&'a InlineFragmentNode),
    /// A definition no view answers the members of, its kind.
    Other(&'a DefinitionNode),
}

impl<'a> At<'a> {
    /// Returns the node of the definition `definition`.
    fn of_definition(definition: &'a DefinitionNode) -> Self {
        match definition {
            DefinitionNode::OperationDefinition(operation) => Self::Operation(operation),
            DefinitionNode::FragmentDefinition(fragment) => Self::Fragment(fragment),
            other => Self::Other(other),
        }
    }

    /// Returns the node of the selection `selection`.
    fn of_selection(selection: &'a SelectionNode) -> Self {
        match selection {
            SelectionNode::Field(field) => Self::Field(field),
            SelectionNode::FragmentSpread(spread) => Self::Spread(spread),
            SelectionNode::InlineFragment(inline) => Self::Inline(inline),
        }
    }

    /// Returns the selections of the selection set of the node, or `None`
    /// for a node with none.
    fn selections(self) -> Option<&'a [SelectionNode]> {
        Some(match self {
            Self::Operation(operation) => &operation.selection_set.selections,
            Self::Fragment(fragment) => &fragment.selection_set.selections,
            Self::Field(field) => &field.selection_set.as_ref()?.selections,
            Self::Inline(inline) => &inline.selection_set.selections,
            Self::Document(_) | Self::Spread(_) | Self::Other(_) => return None,
        })
    }

    /// Returns the kind of the record of the node.
    const fn kind(self) -> u8 {
        match self {
            Self::Document(_) => kind::DOCUMENT,
            Self::Operation(_) => kind::OPERATION,
            Self::Fragment(_) => kind::FRAGMENT,
            Self::Field(_) => kind::FIELD,
            Self::Spread(_) => kind::SPREAD,
            Self::Inline(_) => kind::INLINE,
            Self::Other(_) => kind::OTHER,
        }
    }

    /// Returns the location of the node, when it carries one.
    fn loc(self) -> Option<&'a Location> {
        match self {
            Self::Document(document) => document.loc.as_ref(),
            Self::Operation(operation) => operation.loc.as_ref(),
            Self::Fragment(fragment) => fragment.loc.as_ref(),
            Self::Field(field) => field.loc.as_ref(),
            Self::Spread(spread) => spread.loc.as_ref(),
            Self::Inline(inline) => inline.loc.as_ref(),
            Self::Other(other) => other.loc(),
        }
    }
}

/// Returns the node the steps `steps` from the root `root` lead to, or
/// `None` where a step leads nowhere.
fn locate<'a>(root: &'a Root, steps: &[u32]) -> Option<At<'a>> {
    let mut at = match root {
        Root::Field(field) => At::Field(field),
        Root::Fragment(fragment) => At::Fragment(fragment),
        Root::Operation(operation) => At::Operation(operation),
        Root::Document(document) => At::Document(document),
    };
    for &step in steps {
        let index = usize::try_from(step).ok()?;
        at = match at {
            At::Document(document) => At::of_definition(document.definitions.get(index)?),
            node => At::of_selection(node.selections()?.get(index)?),
        };
    }
    Some(at)
}

/// Notes the node `at` of `request`, which the steps `steps` from the root
/// `root` lead to, and returns its number, counted from 1, the number of the
/// same node where another way led to it before.
fn note_node(request: u32, root: &Root, steps: &[u32], at: At<'_>) -> u32 {
    let key = match at.loc() {
        Some(loc) => Key::Located(
            at.kind(),
            loc.start,
            loc.end,
            Arc::as_ptr(&loc.source).addr(),
        ),
        None => Key::Steps(root.addr(), steps.to_vec()),
    };
    with_views(request, |views| {
        let at = match views.keys.binary_search_by(|(held, _)| held.cmp(&key)) {
            Ok(at) => return views.keys[at].1,
            Err(at) => at,
        };
        views.nodes.push(Noted {
            root: root.clone(),
            steps: steps.to_vec(),
        });
        let number = host::length(views.nodes.len());
        views.keys.insert(at, (key, number));
        number
    })
}

/// Appends the record of the node `at` of `request`, which the steps `steps`
/// from the root `root` lead to: its number, its kind and what a view answers
/// of it at once.
///
/// An operation gives its operation, its description and its name, a fragment
/// definition its description, its name and the name of its type condition, a
/// field its alias, its name, its arguments and whether it has a selection
/// set, a spread its name and an inline fragment the name of its type
/// condition. The selections of a node and the definitions of the document
/// come with the op [`op::NODE`].
fn write_node(out: &mut Vec<u8>, request: u32, root: &Root, steps: &[u32], at: At<'_>) {
    host::push_word(out, note_node(request, root, steps, at));
    out.push(at.kind());
    match at {
        At::Document(_) => {}
        At::Operation(operation) => {
            push_text(out, operation.operation.as_str());
            push_description(out, operation.description.as_ref());
            push_name(out, operation.name.as_ref().map(NameNode::value));
        }
        At::Fragment(fragment) => {
            push_description(out, fragment.description.as_ref());
            push_text(out, fragment.name.value());
            push_text(out, fragment.type_condition.name.value());
        }
        At::Field(field) => {
            push_name(out, field.alias.as_ref().map(NameNode::value));
            push_text(out, field.name.value());
            let arguments = field.arguments.as_deref().unwrap_or_default();
            host::push_count(out, arguments.len());
            for argument in arguments {
                push_argument(out, argument);
            }
            out.push(u8::from(field.selection_set.is_some()));
        }
        At::Spread(spread) => push_text(out, spread.name.value()),
        At::Inline(inline) => {
            push_name(
                out,
                inline
                    .type_condition
                    .as_ref()
                    .map(|named| named.name.value()),
            );
        }
        At::Other(other) => push_text(out, other.kind().as_str()),
    }
}

/// Appends the description `description` of a definition: the byte 0 for
/// none, or the byte 1 and its value record.
fn push_description(out: &mut Vec<u8>, description: Option<&StringValueNode>) {
    match description {
        Some(description) => {
            out.push(1);
            push_string_value(out, description);
        }
        None => out.push(0),
    }
}

/// Appends the record of the argument `argument`, its name and its value.
fn push_argument(out: &mut Vec<u8>, argument: &ArgumentNode) {
    push_text(out, argument.name.value());
    push_value(out, &argument.value, 0);
}

/// Appends the record of the value node `node`, nested `depth` levels deep,
/// or the mark [`value::DEEP`] past [`DEPTH`] levels.
fn push_value(out: &mut Vec<u8>, node: &ValueNode, depth: usize) {
    if depth > DEPTH {
        out.push(value::DEEP);
        return;
    }
    match node {
        ValueNode::Variable(variable) => {
            out.push(value::VARIABLE);
            push_text(out, variable.name.value());
        }
        ValueNode::IntValue(int) => {
            out.push(value::INT);
            push_text(out, &int.value);
        }
        ValueNode::FloatValue(float) => {
            out.push(value::FLOAT);
            push_text(out, &float.value);
        }
        ValueNode::StringValue(string) => push_string_value(out, string),
        ValueNode::BooleanValue(boolean) => {
            out.push(value::BOOLEAN);
            out.push(u8::from(boolean.value));
        }
        ValueNode::NullValue(_) => out.push(value::NULL),
        ValueNode::EnumValue(enumerated) => {
            out.push(value::ENUM);
            push_text(out, &enumerated.value);
        }
        ValueNode::ListValue(list) => {
            out.push(value::LIST);
            host::push_count(out, list.values.len());
            for item in &list.values {
                push_value(out, item, depth + 1);
            }
        }
        ValueNode::ObjectValue(object) => {
            out.push(value::OBJECT);
            host::push_count(out, object.fields.len());
            for field in &object.fields {
                push_text(out, field.name.value());
                push_value(out, &field.value, depth + 1);
            }
        }
    }
}

/// Appends the record of the string value `string`, its value and whether it
/// is a block string.
fn push_string_value(out: &mut Vec<u8>, string: &StringValueNode) {
    out.push(value::STRING);
    push_text(out, &string.value);
    out.push(u8::from(string.block.unwrap_or(false)));
}

/// Appends a name that may be absent: the byte 0, or the byte 1 and the name.
fn push_name(out: &mut Vec<u8>, name: Option<&str>) {
    match name {
        Some(name) => {
            out.push(1);
            push_text(out, name);
        }
        None => out.push(0),
    }
}

/// Appends a length and the UTF-8 bytes of `text`.
fn push_text(out: &mut Vec<u8>, text: &str) {
    host::push_bytes(out, text.as_bytes());
}

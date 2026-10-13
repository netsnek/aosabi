// SPDX-License-Identifier: MIT
/*
 * aosabi-js value: the objects, functions and promises of the host as values
 * of the engine.
 *
 * No analogue in the reference.
 */

//! The values of the host as values of the engine.
//!
//! A JavaScript object stays on the host and stands in the engine as a
//! [`JsObject`], an [`ObjectLike`] whose reads are imports. A function stands
//! as a [`JsFunction`], a [`Function`] of the engine whose call hands `js_call`
//! the arguments the engine has already put in the order of the schema. A
//! promise stands as a [`JsPromise`], a promise of the engine whose resolving
//! functions it keeps, waited for once and settled through the export `settle`
//! in the microtask of the host the settlement runs in. A primitive, a `Date`
//! and an array cross inline, an array that holds a thenable as the promise of
//! all its items. A bigint and a symbol, which the engine holds no value of and
//! which step 1 of `wrapResolver` passes on as they are, cross as one
//! tag with a byte of their kind and the text `String` answers for them, which
//! the module reads by the [`Position`] the value stands at in the schema. The
//! answer of a call stands at the return type of the field the call resolves,
//! which the resolve info of the call names, an item of a list at the item
//! type, a member of an object of an object type at the type of its field, and
//! the value a promise fulfils with at the place of the promise. At a leaf of
//! `ID` or `GID`, whose `serialize` answers `String(value)`, the module reads
//! the text, and an array there as the array `String` joins, a `TypeError` when
//! a symbol stands among its items, as the join throws on a symbol. At any
//! other place it reads a [`foreign`] stand in, a function whose every call
//! answers another stand in. A function is the one value of the engine that the
//! executor takes for no null and that the writer of JSON leaves out of an
//! object and writes as `null` in a list, as `JSON.stringify` treats a symbol,
//! and every scalar the engine specifies or binds but `ID`, `GID` and
//! `JSON` answers it at a leaf as graphql-js answers a bigint and a symbol, so
//! a bigint and a symbol answer so at a leaf, an item of a list among
//! them, and as the whole value of `JSONObject`, and a symbol answers its
//! JSON inside a value of the scalars `JSONObject`, `JSON` and `Any` and as the
//! whole value of `Any`. A value of the engine crosses to the host as a record,
//! the arguments of a call through [`push_arguments`] and every other value
//! through [`host::encode`], so the host sees plain objects, arrays and dates.
//!
//! The host meets a thenable while it walks a value it answers, as step 3 of
//! `wrapResolver` meets it, and calls its `then` there, before it reads
//! the next item. A promise of the realm of the host that the walk answers as
//! the value takes the two callbacks that call the export `settle`, the form of
//! the plan. Any other thenable takes the call of `wrapResolver` with one callback, which
//! answers what `wrapResolver` answers for the value it receives. For a
//! thenable it calls that `then` at once, inside the callback, with one
//! callback of the same form and answers what that `then` answers. A promise of
//! the realm of the host that the walk meets as an item of a list, or that a
//! callback receives, takes the `then` of `Promise` itself with the
//! callback of `wrapResolver` and a second callback, since no code of the app sees the count of
//! the arguments of that `then`, so a field that waits on the promise it
//! derives settles in the job that promise rejects in. The host maps an array
//! as step 4 maps it, the array the walk meets and the one the callback
//! receives, each item and each nested array the same way, so the `then` of a
//! thenable item runs while the walk runs, and it answers the mapped array, or
//! a promise of the host of all its items when one of them is a thenable, which
//! the host makes with the steps of `Promise.all` and owns, so a field that
//! waits on it settles in its job. A primitive and a `Date` come back as they
//! are, as steps 1 and 2 answer them, and so does any other object, since the
//! projection of the engine reads its members, as step 6 reads them there,
//! through [`ObjectLike::read`] in the count and the order of the
//! projection of `wrapResolver`. What the outer `then` answers is the value that crosses, and a
//! promise of the host when that value is a thenable, which the host settles in
//! the job the field of `wrapResolver` settles in wherever it can tell that job: a late call
//! of the callback settles it with what the callback answers, at once or as the
//! promise of the app adopts it, and a thenable of the app that the host adopts
//! settles it through resolving functions of the host. A late call that code of
//! the app makes inside an import, a resolver the engine calls among them,
//! rides on the answer of that import, and the settlement takes effect as the
//! import returns, in the job of that call. Its reactions, the step 3 the
//! engine applies to the promise among them, run at once there, and the jobs
//! they enqueue wait for the microtask the host reserves at the call, where V8
//! queues the consumer of `wrapResolver`, and which calls the export `resume`. A settlement
//! through the resolving functions the host handed a thenable it adopts runs
//! the jobs those reactions enqueue at once as well, inside an import or in the
//! microtask of the export `settle`, since the async function of a root
//! hands that thenable its own resolving functions and reacts on the settlement
//! itself. So the record holds a promise of the host whose `then` has run, and
//! the module notes a [`JsPromise`] for it and calls no import.
//!
//! The host answers every read and every call with a completion: the byte
//! [`NORMAL`] and the value, or the byte [`THROW`] and the thrown value,
//! followed by the settlements callbacks of the app made inside the import. A
//! getter, a `Proxy` trap, a `then` or the walk of a value that throws on the
//! host is such a throw, so no exception of the host leaves an import. A `then`
//! that throws throws the walk of the value that holds its thenable, as it
//! throws the walk of `wrapResolver`. A call answers a throw as its throw
//! completion, and so does a read of the projection of `wrap_resolver` through
//! [`ObjectLike::read`], so the projection throws and the call that returned
//! the object throws with it, at the path and the column of that call, as the
//! projection throws out of `wrapResolver`. A read of the executor
//! through [`ObjectLike::get`] answers a throw as the error the executor makes
//! of the thrown value, since such a read answers a value.
//!
//! The projection reads a member as often as the projection of `wrapResolver` reads it,
//! once for the test of its truth, once more for the test of its type when
//! the first answer is true and once more for the value, and the host runs a
//! getter at each read. A read for a test, [`Read::Test`], hands over no value:
//! the host answers whether the member is `undefined`, false, true or a
//! function, so the walk of a value runs at the read of the value alone.
//!
//! The arguments of a call cross as a resolver of the app receives them. The type
//! layer of the engine carries a date argument as its `toISOString` text,
//! which the glue of a Rust resolver reads back into a date, and the walk of
//! [`push_arguments`] reads it back the same way, by the types the schema
//! gives the arguments of the field the call resolves, so a date argument is
//! a `Date` on the host, at any depth of a list and of an input object, as
//! `parseValue` of the scalar `Date` answers it. The walk reads each
//! argument beside the node of the document it comes from, so an object the
//! document writes as a literal crosses with no prototype and the value of
//! a variable crosses once per request under its name, as graphql-js makes
//! and hands them.
//!
//! Every import that runs code of the app carries the context of its call,
//! a value record that holds the locale `@inContext` names for the operation
//! or `undefined`, and the host runs that code inside the store of its
//! `AsyncLocalStorage`.
//!
//! The boot record picks how the engine reads an object. By default each read
//! of a member is one `js_get`, and the host binds a function member of an
//! object to it at the read of its value, as `bindIfMethod` of
//! `wrapResolver` binds a member of a projected object. A member of a root
//! stays unbound, since `wrapResolver` reads its roots with `Object.entries` and calls a
//! member of a root with no `this`. In the projection mode a projection names
//! its members through [`ObjectLike::select`], and its first read takes the
//! reads of the members from the first one through one `js_project` of the form
//! [`SELECTED`]: the host makes the reads of each member in the order and
//! the count of `wrapResolver`, stops after a member whose value the projection may project before it
//! reads the next member, and after a read that throws, and the reads of the
//! projection answer from that copy in their order. A read the copy does not
//! hold, a later call of the function of an aliased field among them, goes
//! through `js_get`.
//!
//! A record of the host nested deeper than [`host::DEPTH`] is a broken
//! record, the bound the host keeps as well, so neither side follows a deep
//! or cyclic value without end.
//!
//! The classes of the wrapper stamp a brand the host reports. The host reads an
//! error in one of three forms, by the place the module met it. A thrown error,
//! one a call threw, a promise rejected with or a field answered, is read while
//! the execution runs as the executor reads it there, in
//! `locatedError` of graphql-js and of its utilities and in the handler of each
//! place the error reaches, whose levels the module hands the host from the
//! place it met the error up to the field of the root: its `path`, its
//! `coordinate` for a path that is an array, and else its `message`, `nodes`,
//! `source` and `positions`, the locations of its blame, its `extensions` once
//! more when they are an object and its `stack` once more when it is true, then
//! the member `CRITICAL_ERROR` of its extensions at each place that may hold
//! null, and for a path that is an array its `path` and `coordinate` at each
//! handler above the first and its `errors` at the root. The handler of a field
//! takes an `AggregateError` apart, so it crosses with the errors of it that
//! handler handles, each read in turn, up to the first the handler throws on,
//! and an item of it that is no error crosses as the error `coerceError` makes
//! of it. The host keeps a record of each thrown error, and the error of the
//! engine holds a [`Deferred`] in place of its original error or of the Rust
//! value behind it, so [`finish`] makes the reads Yoga makes at the end of the
//! request once the execution has ended: the original error as
//! `isOriginalGraphQLError` reads it, which tells whether the mask keeps the
//! error, and then the reads of the mask or of the writer of the result, which
//! makes every error of the result anew from its message, its path, its
//! extensions and its blame. An error inside a value keeps the reads of its
//! class, its name, its message and the members its class needs, and the
//! original error of a `GraphQLError` the reads of `isOriginalGraphQLError` of
//! Yoga, its own original error, before its name and its message. The
//! extensions, the members of an error inside a value and the path of a located
//! error cross as the copy `JSON.stringify` reads, as Yoga writes them into the
//! result.
//!
//! A `GraphQLError`, made by its class, by `createGraphQLError` or by a
//! subclass, `ServiceError` among them, is an [`ErrorObject::GraphQL`] with its
//! message, its extensions, its original error, its path and the source and
//! positions its blame names, so an original error that is no `GraphQLError`
//! makes it a masked one, as `isOriginalGraphQLError` of Yoga finds, and the
//! located error of the executor takes the locations of graphql-js. A
//! `ForbiddenError` is the [`ErrorObject::Custom`] with the
//! `aosabi_auth_contract::ForbiddenError` of its message behind it, and no
//! read of its feature, which no plugin the module serves reads. Any other
//! error is an [`ErrorObject::Plain`], or an [`ErrorObject::Custom`] with its
//! extensions when it is thrown and its own enumerable members when it stands
//! in a value, and a thrown value that is no error crosses as itself, which
//! the executor wraps as graphql-js wraps it. No stack crosses, so no answer
//! and no log line of the engine holds a path of the host.

#![forbid(unsafe_code)]

use core::{
    any::Any,
    cell::{
        Cell,
        RefCell, //
    },
};
use std::{
    collections::VecDeque,
    rc::Rc,
    sync::Arc, //
};

use aosabi::auth::ForbiddenError;
use aosabi_core::{
    GraphQLError,
    GraphQLErrorOptions,
    resolve_info::get_resolve_info,
    scalars::date, //
};
use aosabi_execute::{
    coerce_error::coerce_error,
    collect_fields::{
        FieldNodeCache,
        collect_fields,
        collect_sub_fields, //
    },
    errors::{
        ThrownError,
        copy_graphql_error, //
    },
    execute::{
        FieldNodes,
        ResolveInfo, //
    },
    promise::{
        Agent,
        Promise,
        ResolvingFunctions, //
    },
    value::{
        Completion,
        ErrorName,
        ErrorObject,
        Function,
        HostValue,
        ObjectLike,
        PlainObject,
        Read,
        Selected,
        Value,
        to_string::to_js_string, //
    },
};
use aosabi_json::{
    JsObject as JsonObject,
    JsString,
    JsValue,
    Opaque, //
};
use aosabi_lang::{
    HostPlaces,
    OriginalError,
    PathSegment,
    Source,
    SourceLocation,
    get_location,
    js_string::to_string_lossy, //
};
use aosabi_scalars::{
    JsDate,
    ValueNode, //
};

use crate::{
    abi::import,
    host::{
        self,
        Brand,
        Reader,
        Tag, //
    },
    request,
};

/// The byte of a normal completion in an answer of the host.
pub const NORMAL: u8 = 0;

/// The byte of a throw completion in an answer of the host.
pub const THROW: u8 = 1;

/// The bit of the byte of a settlement of the host that marks a fulfilment.
const FULFILLED: u32 = 1;

/// The bit of the byte of a settlement of the host that marks one through
/// the resolving functions the host handed a thenable it adopts.
const ADOPTED: u32 = 2;

/// The form of a record of `js_project` that asks for the own enumerable keys.
const OWN_KEYS: u8 = 0;

/// The form of a record of `js_project` that asks for the reads of the
/// members a projection selects, in the order and the count of `wrapResolver`.
const SELECTED: u8 = 1;

/// The form of a record of `js_project` that asks for the names the
/// projection of a build of the canary holds of the object, its own
/// property names and those of its chain of prototypes but `constructor`.
const NAMES: u8 = 2;

/// The form of a record of `js_get`, and the kind of a read of an answer of
/// the form [`SELECTED`], that asks for the value of the member.
const VALUE: u8 = 0;

/// The form of a record of `js_get`, and the kind of a read of an answer of
/// the form [`SELECTED`], that asks for a test of the member alone.
const TEST: u8 = 1;

/// The bit of the flags of a selected member that marks one the projection
/// reads no value of when it is no function.
const ROUTED: u8 = 1;

/// The bit of the flags of a selected member that marks one whose value the
/// projection may project before it reads the next member.
const NESTED: u8 = 2;

/// The answer of a test of the host for a member that is `undefined`.
const TESTED_UNDEFINED: u8 = 0;

/// The answer of a test of the host for any other member that is false.
const TESTED_FALSE: u8 = 1;

/// The answer of a test of the host for a member that is true and no
/// function.
const TESTED_TRUE: u8 = 2;

/// The answer of a test of the host for a member that is a function.
const TESTED_FUNCTION: u8 = 3;

/// The form of a record of `iter_next` that pulls the next chunk of a reader
/// of the body of a request of the host.
const READ: u8 = 0;

/// The form of a record of `iter_next` that tests the member
/// `[Symbol.asyncIterator]` of an object.
const ITERABLE: u8 = 1;

/// The form of a record of `iter_next` that calls the member
/// `[Symbol.asyncIterator]` of an object.
const ITERATOR: u8 = 2;

/// The form of a read of `js_error` for a thrown error: one a call threw, a
/// promise rejected with or a field answered, which the executor throws.
const THROWN: u8 = 0;

/// The form of a read of `js_error` for an error inside a value.
const HELD: u8 = 1;

/// The form of a read of `js_error` for the original error of a
/// `GraphQLError` of the host.
const ORIGINAL: u8 = 2;

/// The form of a read of `js_error` for the first read made of a
/// thrown `GraphQLError` at the end of the execution, of its original error,
/// by the handle of the record the host kept of the error.
const END: u8 = 3;

/// The form of a read of `js_error` for the reads made of a thrown
/// error at the end of the execution once the mask has decided, by the
/// handle of the record the host kept of the error and a byte, 1 where the
/// mask masks it.
const FINAL: u8 = 4;

/// The byte of a read of the form [`FINAL`] of a `ForbiddenError` the map of
/// the identity plugin replaces before the mask reads it, beside 0 for an
/// error the mask keeps and 1 for one it masks.
const MAPPED: u8 = 2;

/// The form of the record of `js_error` that reads the value a read of the host
/// rejected with, a pull of the body of a request or the `fetch` of the host:
/// its brand, its name, its message and its member `statusCode`. No form reads
/// an error of a route of the app, whose routes run in the hono of the host,
/// and the forms of the host and of the engine share the 5.
const PULLED: u8 = 5;

/// The form of the record of `js_error` that reads a value a hook of a
/// plugin of the app threw or the promise of a done hook rejected with, as
/// `handleError` of Yoga, its mask, `getResponseInitByRespectingErrors` and
/// the writer of the result read a value that escapes the execution, and a
/// byte, 1 where the mask reads it.
const HOOK: u8 = 6;

/// The kinds of the record of the form [`HOOK`].
mod hook {
    /// A value that holds a member `errors`, which `handleError` takes apart,
    /// after the count and the record of each of its errors.
    pub const AGGREGATE: u8 = 0;
    /// The `DOMException` of an abort or of a timeout, of which `handleError`
    /// makes no error.
    pub const ABORT: u8 = 1;
    /// A `GraphQLError` the mask keeps, or any `GraphQLError` where no mask
    /// reads it, after a byte, 1 where its original errors down its chain
    /// are `GraphQLError`s to the last, the copy of its extensions, its
    /// message, the copy of its path and its blame.
    pub const KEPT: u8 = 2;
    /// A `GraphQLError` the mask masks, after the copy of its path, the copy
    /// of the member `http` of its extensions or `undefined`, and its blame.
    pub const MASKED: u8 = 3;
    /// Any other value the mask masks, after the name and the message of an
    /// error or the name `Error` and an empty message.
    pub const BARE: u8 = 4;
    /// An error of another class where no mask reads it, after its message
    /// and the copy of its extensions where they are an object, or
    /// `undefined`.
    pub const ERROR: u8 = 5;
    /// A value with a method `toString` where no mask reads it, after the
    /// text the method answers.
    pub const TEXT: u8 = 6;
    /// A value with no method `toString` where no mask reads it.
    pub const NONE: u8 = 7;
}

/// The bit of a level of a thrown error that marks the handler of an item of
/// a list, which takes no `AggregateError` apart.
const LEVEL_ITEM: u8 = 1;

/// The bit of a level of a thrown error that marks a place that may hold
/// null, where the error stops unless its extensions mark it critical.
const LEVEL_NULLABLE: u8 = 2;

/// The answer of the record of a thrown error whose member `CRITICAL_ERROR`
/// the first handler that read it found true, where 0 is no read and 1 a
/// value that is false.
const CRITICAL_TRUE: u8 = 2;

/// The kind of an error of an `AggregateError` that is an error of the app.
const ITEM_ERROR: u8 = 0;

/// The kind of an error of an `AggregateError` that the executor coerced
/// from an item that is no error.
const ITEM_COERCED: u8 = 1;

/// The byte of the record of a thrown error whose path is an array, which
/// the executor takes as it is.
const LOCATED: u8 = 1;

/// The blame of a thrown error the executor locates at the nodes of the
/// field it resolves.
const BLAME_FIELD: u8 = 0;

/// The blame of a thrown error whose nodes give no location.
const BLAME_NONE: u8 = 1;

/// The blame of a thrown error located at positions of a source.
const BLAME_SOURCE: u8 = 2;

/// What a value of a broken record of the host becomes.
const BROKEN: &str = "the host answered a broken record";

/// What a promise of the host becomes when no agent of its request runs.
const OUTSIDE: &str = "a promise of the host crossed outside a request";

/// The byte of the record of tag 15 that marks a symbol, where a bigint has 0.
const SYMBOL: u8 = 1;

/// The message of the `TypeError` that `String` of an array throws on a
/// symbol among its items, as V8 writes it.
const SYMBOL_TO_STRING: &str = "Cannot convert a Symbol value to a string";

/// The handle of an error record of a thrown value the host could not read.
const NO_HANDLE: u32 = u32::MAX;

/// What an error the host could not read becomes, one whose record or whose
/// reads through `js_error` threw on the host.
const UNREADABLE: &str = "the host could not read a thrown value";

/// The string value of the host at which a build with the feature `trap`
/// traps on purpose.
#[cfg(feature = "trap")]
const TRAP: &str = "aosabi-js: trap on purpose";

thread_local! {
    /// Whether the boot picked the projection mode over one `js_get` per read.
    static PROJECT: Cell<bool> = const { Cell::new(false) };
    /// Whether a plugin of the app maps a `ForbiddenError`, the identity
    /// plugin of `aosabi/auth/plugin`.
    static IDENTITY: Cell<bool> = const { Cell::new(false) };
}

/// Sets the read mode the boot record names, the projection when `project`
/// is true.
pub fn set_project(project: bool) {
    PROJECT.set(project);
}

/// Notes that a plugin of the app maps a `ForbiddenError` to the error
/// `FORBIDDEN`, the identity plugin, whose map reads the message and the
/// feature of the error at the end of the execution in place of the mask.
pub fn set_identity_map(on: bool) {
    IDENTITY.set(on);
}

/// The place a value of the host stands at in the schema, which tells the
/// decoder what a bigint or a symbol of the host answers there, and which
/// handlers of the executor meet an error thrown there.
#[derive(Clone)]
enum Position {
    /// A place the module knows no type of: a thrown value, a value read
    /// outside a resolver, a root of the boot and a member of an object of no
    /// known object type.
    Unknown,
    /// The value a read of the host answers, a pull of the body of a request
    /// or the `fetch` of the host, whose rejection the reader of the body or
    /// the network capability of the engine meets.
    Pulled,
    /// The value a hook of a plugin of the app answers, whose rejection the
    /// handler of Yoga meets that folds a value escaping the execution into
    /// the errors of the result.
    Hook,
    /// A place of the type the schema prints as `printed`.
    Typed {
        /// The info of the field the place belongs to, whose schema names the
        /// types.
        info: Rc<ResolveInfo>,
        /// The type of the place as the schema prints it.
        printed: Rc<str>,
        /// The place of the list or the object the place is an item or a
        /// member of, `None` for the value of the field of `info` itself.
        holder: Option<Rc<Position>>,
        /// Whether the place is an item of a list.
        item: bool,
    },
}

impl Position {
    /// Returns the place of the value the resolver that runs answers, the
    /// return type of the field its resolve info names, or
    /// [`Position::Unknown`] outside a resolver.
    fn field() -> Self {
        get_resolve_info()
            .ok()
            .map_or(Self::Unknown, |context| Self::Typed {
                printed: Rc::from(context.info.return_type.to_string()),
                info: Rc::clone(&context.info),
                holder: None,
                item: false,
            })
    }

    /// Returns the levels of the handlers of the executor that meet an error
    /// thrown at the place, the nearest first and the field of the root last,
    /// each the bits [`LEVEL_ITEM`] and [`LEVEL_NULLABLE`] of its place, or
    /// none where the module knows no place or the walk of the path finds no
    /// field.
    ///
    /// The handler of the place comes first, then the place each holder stands
    /// at, and then the places of the path of the field of the info above that
    /// field, as the executor throws an error on from a place that holds no
    /// null to the place that holds the object or the list of it.
    fn levels(&self) -> Vec<u8> {
        let mut levels = Vec::new();
        let mut at = self;
        loop {
            let Self::Typed {
                info,
                printed,
                holder,
                item,
            } = at
            else {
                return Vec::new();
            };
            levels.push(level(*item, printed));
            if let Some(holder) = holder {
                at = holder;
                continue;
            }
            let Some(above) = ancestors(info) else {
                return Vec::new();
            };
            levels.extend(above);
            return levels;
        }
    }

    /// Returns the printed type of the place without its non null mark.
    fn nullable(&self) -> Option<&str> {
        match self {
            Self::Unknown | Self::Pulled | Self::Hook => None,
            Self::Typed { printed, .. } => Some(printed.strip_suffix('!').unwrap_or(printed)),
        }
    }

    /// Returns whether the place is a leaf of the scalar `ID` or `GID`, whose
    /// `serialize` answers `String(value)`.
    fn prints_text(&self) -> bool {
        matches!(self.nullable(), Some("ID" | "GID"))
    }

    /// Returns the place of an item of a list at this place.
    fn item(&self) -> Self {
        let (Self::Typed { info, .. }, Some(inner)) = (
            self,
            self.nullable()
                .and_then(|list| list.strip_prefix('[')?.strip_suffix(']')),
        ) else {
            return Self::Unknown;
        };
        Self::Typed {
            info: Rc::clone(info),
            printed: Rc::from(inner),
            holder: Some(Rc::new(self.clone())),
            item: true,
        }
    }

    /// Returns the place of the member `key` of an object at this place, the
    /// type of the field `key` of an object type.
    fn member(&self, key: &str) -> Self {
        let (Self::Typed { info, .. }, Some(name)) = (self, self.nullable()) else {
            return Self::Unknown;
        };
        info.schema
            .get_type(name)
            .and_then(|named| named.as_object())
            .and_then(|object| object.get_fields().ok())
            .and_then(|fields| fields.get(key))
            .map_or(Self::Unknown, |field| Self::Typed {
                info: Rc::clone(info),
                printed: Rc::from(field.r#type.to_string()),
                holder: Some(Rc::new(self.clone())),
                item: false,
            })
    }
}

/// Returns the level of a place of the type the schema prints as `printed`,
/// an item of a list when `item` holds.
fn level(item: bool, printed: &str) -> u8 {
    let mut level = if item { LEVEL_ITEM } else { 0 };
    if !printed.ends_with('!') {
        level |= LEVEL_NULLABLE;
    }
    level
}

/// Returns the levels of the places above the field of `info`, the nearest
/// first: each field of its path and each item of a list on it, or `None`
/// where the walk finds no field.
///
/// The walk collects the fields the operation selects from its root along the
/// path, as the executor collects them, so an alias, a fragment and the
/// directives `@skip` and `@include` lead to the field a response key names.
fn ancestors(info: &ResolveInfo) -> Option<Vec<u8>> {
    let mut nodes = Vec::new();
    let mut at = info.path.node().prev.as_ref();
    while let Some(path) = at {
        nodes.push(path.node());
        at = path.node().prev.as_ref();
    }
    nodes.reverse();
    let schema = &info.schema;
    let cache = FieldNodeCache::new();
    let mut levels = Vec::with_capacity(nodes.len());
    let mut selected: Option<FieldNodes> = None;
    let mut printed = String::new();
    for node in nodes {
        match &node.key {
            PathSegment::Key(key) => {
                let object = schema.get_type(node.typename.as_deref()?)?.as_object()?;
                let collected = match &selected {
                    None => collect_fields(
                        schema,
                        &info.fragments,
                        &info.variable_values,
                        object,
                        &info.operation.selection_set,
                        &cache,
                    ),
                    Some(field_nodes) => collect_sub_fields(
                        schema,
                        &info.fragments,
                        &info.variable_values,
                        object,
                        field_nodes,
                        &cache,
                    ),
                }
                .ok()?;
                let field_nodes = collected.get(key.as_str())?;
                let field = object
                    .get_fields()
                    .ok()?
                    .get(field_nodes.first()?.name.value())?;
                printed = field.r#type.to_string();
                levels.push(level(false, &printed));
                selected = Some(Rc::clone(field_nodes));
            }
            PathSegment::Index(_) => {
                let inner = printed
                    .strip_suffix('!')
                    .unwrap_or(&printed)
                    .strip_prefix('[')?
                    .strip_suffix(']')?;
                levels.push(level(true, inner));
                printed = String::from(inner);
            }
        }
    }
    levels.reverse();
    Some(levels)
}

/// Where the decoder finds the place of a value record, which it works out
/// only for a record whose tag needs it.
#[derive(Clone, Copy)]
enum Place<'p> {
    /// A place the caller has worked out already.
    At(&'p Position),
    /// The place of the value the resolver that runs answers.
    Field,
    /// A member, by its key, of an object at the place the first field holds.
    Member(&'p Position, &'p str),
    /// A thrown value, which a call or a read of a projection threw, which
    /// the handler of the field of the resolver that runs meets.
    Thrown,
    /// A thrown value, which a promise rejected with, which the handler of
    /// the place of the promise meets.
    Rejected(&'p Position),
    /// A thrown value no handler of the executor meets, which a read of the
    /// executor takes as the error its value is.
    Caught,
    /// The original error of a `GraphQLError` of the host.
    Original,
    /// A thrown value a read of the host threw, which the reader of the body
    /// or the network capability of the engine meets.
    Pulled,
    /// A thrown value a hook of a plugin of the app threw, which the handler
    /// of Yoga meets that folds a value escaping the execution into the
    /// errors of the result.
    Hook,
}

impl Place<'_> {
    /// Returns the position of the place.
    fn position(self) -> Position {
        match self {
            Self::At(position) => position.clone(),
            Self::Field => Position::field(),
            Self::Member(of, key) => of.member(key),
            Self::Thrown | Self::Rejected(_) | Self::Caught | Self::Original => Position::Unknown,
            Self::Pulled => Position::Pulled,
            Self::Hook => Position::Hook,
        }
    }

    /// Returns whether the place is that of a value a hook of a plugin of the
    /// app threw or the promise of a done hook rejected with, which the
    /// module reads in the form [`HOOK`] whatever its tag.
    fn is_hook(self) -> bool {
        matches!(self, Self::Hook | Self::Rejected(Position::Hook))
    }

    /// Returns the record of the read of `js_error` for an error at the
    /// place: the byte of its form, and for a thrown error the count and the
    /// bytes of the levels of [`Position::levels`] of the handlers that meet
    /// it.
    ///
    /// An error at a place of the schema is the value of a field, which the
    /// executor throws as it completes the field, so it is read as a thrown
    /// error that the handler of that place meets, and an error at no place
    /// stands inside a value.
    fn error_record(self) -> Vec<u8> {
        let levels = match self {
            Self::Original => return vec![ORIGINAL],
            Self::Pulled | Self::Rejected(Position::Pulled) => return vec![PULLED],
            Self::Hook | Self::Rejected(Position::Hook) => {
                return vec![HOOK, u8::from(masks_errors())];
            }
            Self::Thrown => Position::field().levels(),
            Self::Rejected(position) => position.levels(),
            Self::Caught => Vec::new(),
            place => match place.position() {
                position @ Position::Typed { .. } => position.levels(),
                Position::Unknown => return vec![HELD],
                Position::Pulled => return vec![PULLED],
                Position::Hook => return vec![HOOK, u8::from(masks_errors())],
            },
        };
        let mut record = Vec::with_capacity(5 + levels.len());
        record.push(THROWN);
        host::push_count(&mut record, levels.len());
        record.extend(levels);
        record
    }
}

/// A member a projection of the projection mode reads.
struct Member {
    /// The name of the member.
    key: String,
    /// How the projection reads it, the bits [`ROUTED`] and [`NESTED`].
    flags: u8,
}

/// One read of a member the host made ahead for a projection.
struct Answered {
    /// The name of the member.
    key: String,
    /// What the read takes of the member.
    read: Read,
    /// The completion of the read.
    completion: Completion,
}

/// The reads the projection mode makes ahead for the projection in progress
/// of an object.
struct Ahead {
    /// The members the projection reads, in its order.
    members: Vec<Member>,
    /// The index of the first member the host has not read yet.
    next: usize,
    /// The reads the host made and the projection has not taken yet, in their
    /// order.
    answers: VecDeque<Answered>,
}

/// An object of the host, the handle `handle` of the table `table`.
pub struct JsObject {
    /// The request whose table holds the handle, 0 for the boot.
    table: u32,
    /// The index in that table.
    handle: u32,
    /// The place the object stands at, which gives each member its place.
    position: Position,
    /// The reads the projection mode makes ahead for a projection of the
    /// object, while one is in progress.
    // An `unsafe` cell would skip each borrow flag.
    ahead: RefCell<Option<Ahead>>,
}

impl JsObject {
    /// Returns the object of the handle `handle` of the table `table`, a
    /// root of the boot, which stands at no place of the schema.
    pub fn new(table: u32, handle: u32) -> Self {
        Self::placed(table, handle, Position::Unknown)
    }

    /// Returns the object of the handle `handle` of the table `table` that
    /// stands at `position`.
    fn placed(table: u32, handle: u32, position: Position) -> Self {
        Self {
            table,
            handle,
            position,
            ahead: RefCell::new(None),
        }
    }

    /// Returns the completion of the read of `key` for `read` through one
    /// `js_get`, whose value stands at the place of the member and whose
    /// thrown value at `thrown`: the handler of the field of the resolver
    /// that runs for a read of a projection, which throws the call that
    /// returned the object, and no handler for a read of the executor.
    fn read_now(&self, key: &str, read: Read, request: u32, thrown: Place<'_>) -> Completion {
        self.read_key(key.as_bytes(), key, read, request, thrown)
    }

    /// Returns the completion of the read of the key of the WTF-8 bytes
    /// `bytes` for `read` through one `js_get`, as [`JsObject::read_now`]
    /// reads it, with `key` the name its value is placed under.
    fn read_key(
        &self,
        bytes: &[u8],
        key: &str,
        read: Read,
        request: u32,
        thrown: Place<'_>,
    ) -> Completion {
        let mut record = Vec::new();
        push_context(&mut record, request);
        host::push_bytes(&mut record, bytes);
        record.push(match read {
            Read::Test => TEST,
            Read::Value => VALUE,
        });
        let len = import::js_get(
            self.table,
            self.handle,
            host::address(&mut record),
            host::length(record.len()),
            request,
        );
        let bytes = host::input(len);
        match read {
            Read::Test => tested(&bytes, request, thrown),
            Read::Value => answer(&bytes, request, Place::Member(&self.position, key), thrown),
        }
    }

    /// Returns the completion of the read of `key` for `read` that the
    /// projection in progress made ahead, and reads the members from the
    /// next one through `js_project` when it holds no read yet, or answers
    /// `None` for a read the projection in progress does not make, which ends
    /// it.
    fn read_ahead(&self, key: &str, read: Read, request: u32) -> Option<Completion> {
        let fetch = {
            let ahead = self.ahead.borrow();
            let state = ahead.as_ref()?;
            state.answers.is_empty()
                && state
                    .members
                    .get(state.next)
                    .is_some_and(|member| member.key == key)
        };
        if fetch {
            self.fetch_ahead(request);
        }
        let mut ahead = self.ahead.borrow_mut();
        let state = ahead.as_mut()?;
        let holds = state
            .answers
            .front()
            .is_some_and(|answered| answered.key == key && answered.read == read);
        if !holds {
            *ahead = None;
            return None;
        }
        let answered = state.answers.pop_front()?;
        if state.answers.is_empty() && state.next >= state.members.len() {
            *ahead = None;
        }
        Some(answered.completion)
    }

    /// Makes the reads of the members of the projection in progress from the
    /// next one through one `js_project` of the form [`SELECTED`].
    ///
    /// The host makes the reads of each member in the order and the count of `wrapResolver`,
    /// and stops after a member whose value the projection may project before
    /// it reads the next member, or after a read that throws. A broken answer
    /// ends the projection in progress, so each later read goes through
    /// `js_get`.
    fn fetch_ahead(&self, request: u32) {
        let mut record = Vec::new();
        push_context(&mut record, request);
        record.push(SELECTED);
        let (first, keys) = {
            let ahead = self.ahead.borrow();
            let Some(state) = ahead.as_ref() else {
                return;
            };
            let rest = state.members.get(state.next..).unwrap_or_default();
            host::push_count(&mut record, rest.len());
            for member in rest {
                host::push_bytes(&mut record, member.key.as_bytes());
                record.push(member.flags);
            }
            let keys: Vec<String> = rest.iter().map(|member| member.key.clone()).collect();
            (state.next, keys)
        };
        let len = import::js_project(
            self.table,
            self.handle,
            host::address(&mut record),
            host::length(record.len()),
            request,
        );
        let bytes = host::input(len);
        let mut reader = Reader::new(&bytes);
        let read = match reader.byte() {
            Some(NORMAL) => self.selected_reads(&mut reader, request, &keys),
            _ => None,
        };
        let mut ahead = self.ahead.borrow_mut();
        let Some((handled, answers)) = read else {
            *ahead = None;
            return;
        };
        settlements(&mut reader);
        if let Some(state) = ahead.as_mut() {
            state.next = first + handled;
            state.answers.extend(answers);
        }
    }

    /// Reads the answer of a `js_project` of the form [`SELECTED`] after its
    /// completion byte, the reads of the members `keys` from the first one,
    /// and returns the count of the members the host read with the reads, or
    /// `None` for a broken answer.
    ///
    /// Each read is the index of its member among `keys`, its kind, [`TEST`]
    /// or [`VALUE`], and its completion: a test answers as [`tested`] reads
    /// it, a value stands at the place of the member and a thrown value at no
    /// place.
    fn selected_reads(
        &self,
        reader: &mut Reader<'_>,
        request: u32,
        keys: &[String],
    ) -> Option<(usize, Vec<Answered>)> {
        let handled = reader.count()?;
        let count = reader.count()?;
        let mut answers = Vec::with_capacity(count.min(64));
        for _ in 0..count {
            let key = keys.get(reader.count()?)?;
            let read = match reader.byte()? {
                TEST => Read::Test,
                VALUE => Read::Value,
                _ => return None,
            };
            let completion = match (reader.byte()?, read) {
                (NORMAL, Read::Test) => Completion::Normal(test_value(reader.byte()?)?),
                (NORMAL, Read::Value) => Completion::Normal(decode_at(
                    reader,
                    request,
                    0,
                    Place::Member(&self.position, key),
                )?),
                (THROW, _) => Completion::Throw(decode_at(reader, request, 0, Place::Thrown)?),
                _ => return None,
            };
            answers.push(Answered {
                key: key.clone(),
                read,
                completion,
            });
        }
        Some((handled.min(keys.len()), answers))
    }
}

impl ObjectLike for JsObject {
    fn get(self: Rc<Self>, key: &str) -> Value {
        let request = request::current();
        request::adopt_agent(request);
        match self.read_now(key, Read::Value, request, Place::Caught) {
            Completion::Normal(value) => value,
            Completion::Throw(thrown) => coerce_error(thrown),
        }
    }

    /// Returns the completion of a read of `key` for `read`: one `js_get`, or
    /// in the projection mode the read the projection in progress made ahead.
    fn read(self: Rc<Self>, key: &str, read: Read) -> Completion {
        let request = request::current();
        request::adopt_agent(request);
        if PROJECT.get()
            && let Some(completion) = self.read_ahead(key, read, request)
        {
            return completion;
        }
        self.read_now(key, read, request, Place::Thrown)
    }

    /// Notes the members a projection reads, in the projection mode alone.
    fn select(&self, members: &mut dyn Iterator<Item = Selected<'_>>) {
        if !PROJECT.get() {
            return;
        }
        let members = members
            .map(|selected| Member {
                key: String::from(selected.key),
                flags: if selected.routed { ROUTED } else { 0 }
                    | if selected.nested { NESTED } else { 0 },
            })
            .collect();
        *self.ahead.borrow_mut() = Some(Ahead {
            members,
            next: 0,
            answers: VecDeque::new(),
        });
    }

    fn own_keys(&self) -> Vec<String> {
        let mut keys = Vec::new();
        self.keys(OWN_KEYS, |reader| {
            keys.push(String::from(reader.text()?));
            Some(())
        });
        keys
    }

    /// Returns the names the projection of a build of the canary holds of
    /// the object, through one `js_project` of the form [`NAMES`].
    fn property_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        self.keys(NAMES, |reader| {
            names.push(String::from(reader.text()?));
            Some(())
        });
        names
    }

    /// Returns the own enumerable keys as JavaScript strings, read as WTF-8, so
    /// a key that holds a lone surrogate stands among them as the JSON of a
    /// value writes it.
    fn own_keys_js(&self) -> Vec<JsString> {
        let mut keys = Vec::new();
        self.keys(OWN_KEYS, |reader| {
            keys.push(reader.string()?);
            Some(())
        });
        keys
    }

    /// Returns the member of a key [`JsObject::own_keys_js`] answered, one
    /// `js_get` of its WTF-8 bytes.
    fn get_js(self: Rc<Self>, key: &JsString) -> Value {
        let Some(name) = key.as_str() else {
            let request = request::current();
            request::adopt_agent(request);
            let name = to_string_lossy(key);
            return match self.read_key(key.as_wtf8(), &name, Read::Value, request, Place::Caught) {
                Completion::Normal(value) => value,
                Completion::Throw(thrown) => coerce_error(thrown),
            };
        };
        self.get(name)
    }

    /// Returns the method `[Symbol.asyncIterator]` of the object where that
    /// member is a function, a `Repeater` or an async generator of the app
    /// among them, read through one `iter_next` of the form [`ITERABLE`] at
    /// each call, as each `isAsyncIterable` of the reference reads it once.
    ///
    /// The call of the method is one `iter_next` of the form [`ITERATOR`],
    /// whose answer is the iterator, an object of the host whose members the
    /// engine reads and calls through `js_get` and `js_call`, so a promise
    /// `next` answers is one the export `settle` settles. For the source of a
    /// subscription the engine reads the member four times as a test and once
    /// at the call, where `wrapResolver` reads it three times and tests it once with
    /// `in`, and a read that throws answers no method where `wrapResolver` locates the
    /// error.
    fn async_iterator(self: Rc<Self>) -> Option<Function> {
        let request = request::current();
        request::adopt_agent(request);
        let tested = self.iterate(ITERABLE, request);
        if !matches!(tested, Completion::Normal(Value::Function(_))) {
            return None;
        }
        Some(Function::new(
            "[Symbol.asyncIterator]",
            move |_agent: &Agent, _args: &[Value]| {
                let request = request::current();
                request::adopt_agent(request);
                self.iterate(ITERATOR, request)
            },
        ))
    }

    /// A value of the host crosses as a handle of the table of the request
    /// that reads it, which the export `release` drops with the request, so
    /// no value of an object of the host outlives its request.
    fn values_outlive_request(&self) -> bool {
        false
    }
}

impl JsObject {
    /// Returns the completion of one `iter_next` of the form `form` over the
    /// object: the test of its member `[Symbol.asyncIterator]` for
    /// [`ITERABLE`], read as [`tested`] reads a test, and for [`ITERATOR`]
    /// the iterator the call of that member answers, which stands at no
    /// place of the schema, while a throw of the call stands where the
    /// handler of the field of the resolver that runs meets it.
    fn iterate(&self, form: u8, request: u32) -> Completion {
        let mut record = Vec::new();
        push_context(&mut record, request);
        record.push(form);
        let len = import::iter_next(
            self.table,
            self.handle,
            host::address(&mut record),
            host::length(record.len()),
            request,
        );
        let bytes = host::input(len);
        if form == ITERABLE {
            return tested(&bytes, request, Place::Caught);
        }
        answer(
            &bytes,
            request,
            Place::At(&Position::Unknown),
            Place::Thrown,
        )
    }

    /// Reads the own enumerable keys of the object through one `js_project`
    /// of the form `form`, [`OWN_KEYS`], or the names of the form [`NAMES`],
    /// each through `key`, which answers `None` for a broken key, after which
    /// no key is read.
    fn keys(&self, form: u8, mut key: impl FnMut(&mut Reader<'_>) -> Option<()>) {
        let request = request::current();
        request::adopt_agent(request);
        let mut record = Vec::new();
        push_context(&mut record, request);
        record.push(form);
        let len = import::js_project(
            self.table,
            self.handle,
            host::address(&mut record),
            host::length(record.len()),
            request,
        );
        let bytes = host::input(len);
        let mut reader = Reader::new(&bytes);
        let whole = match reader.byte() {
            Some(NORMAL) => reader
                .count()
                .and_then(|count| (0..count).try_for_each(|_| key(&mut reader)))
                .is_some(),
            Some(THROW) => skip(&mut reader, 0).is_some(),
            _ => false,
        };
        if whole {
            settlements(&mut reader);
        }
    }
}

/// Returns the completion of the answer of the host to a test, whose thrown
/// value stands at `thrown`, and a throw of a plain error for a broken
/// answer.
fn tested(bytes: &[u8], request: u32, thrown: Place<'_>) -> Completion {
    let mut reader = Reader::new(bytes);
    let completion = match reader.byte() {
        Some(NORMAL) => reader.byte().and_then(test_value).map(Completion::Normal),
        Some(THROW) => decode_at(&mut reader, request, 0, thrown).map(Completion::Throw),
        _ => None,
    };
    let Some(completion) = completion else {
        return Completion::Throw(error(BROKEN));
    };
    settlements(&mut reader);
    completion
}

/// Returns the value that stands in for a member a test of the host found
/// `undefined`, false, true or a function, by the byte of the answer, or
/// `None` for a byte no answer takes.
///
/// The value is what [`ObjectLike::read`] answers for a [`Read::Test`]: the
/// projection of `wrap_resolver` asks it nothing but whether it is
/// `undefined`, whether it is true and whether it is a function.
fn test_value(byte: u8) -> Option<Value> {
    Some(match byte {
        TESTED_UNDEFINED => Value::Undefined,
        TESTED_FALSE => Value::Bool(false),
        TESTED_TRUE => Value::Bool(true),
        TESTED_FUNCTION => Value::Function(Function::new("", |_agent: &Agent, _args: &[Value]| {
            Completion::Normal(Value::Undefined)
        })),
        _ => return None,
    })
}

/// A promise of the host, waited for once: the resolving functions of the
/// promise of the engine that stands for it, and the agent that promise
/// belongs to, which the export `settle` resolves or rejects.
///
/// The agent counts the promise as work the host owes it from the decode of
/// its record until the settlement or the drop of the stand in, so the loop
/// of the agent parks on it and reports no stall.
pub struct JsPromise {
    /// The resolving functions of the promise of the engine.
    resolving: ResolvingFunctions,
    /// The agent the promise belongs to, which waits on the host for it.
    agent: Agent,
    /// The place the promise stands at, where the value it fulfils with stands.
    position: Position,
    /// Whether the host has settled the promise.
    settled: Cell<bool>,
}

impl JsPromise {
    /// Returns the promise of the handle `handle` of the table of `request`.
    ///
    /// The host called its `then` while it wrote the record, and the export
    /// `settle` resolves or rejects the promise of the engine through the
    /// resolving functions the stand in keeps, in the microtask of the host
    /// that the settlement runs in, with no future and no job of its own
    /// between, or the answer of an import carries the settlement a callback
    /// of the app made inside that import, which the module applies as the
    /// import returns. The agent that runs now, the one whose code read the
    /// record, counts the promise as work the host owes it until then, so its
    /// loop parks and reports no stall. Every record is read inside an agent
    /// of its request, the record of a settlement inside the agent of its
    /// promise, so a promise of the host in it, the promise of all items of
    /// an array among them, finds its agent. A settlement the host carries
    /// follows the record of the import it rides on, so the promise of a
    /// handle of that record stands in the table when the settlement is read.
    /// The promise keeps `position`, the place it stands at, for the value it
    /// fulfils with.
    fn wait(request: u32, handle: u32, position: Position) -> Value {
        let Some(agent) = Agent::current() else {
            return error(OUTSIDE);
        };
        let (promise, resolving) = Promise::with_resolvers(&agent);
        agent.hold_host_work();
        let waited = Self {
            resolving,
            agent,
            position,
            settled: Cell::new(false),
        };
        request::wait_on(request, handle, Rc::new(waited));
        Value::Promise(promise)
    }

    /// Returns the agent the promise belongs to.
    pub fn agent(&self) -> Agent {
        self.agent.clone()
    }

    /// Reads the value record of a settlement of the promise at the cursor of
    /// `reader`, whose handles stand in the table of `request`, or `None` for
    /// a broken record: the value it fulfils with at the place of the
    /// promise when `ok` holds, and otherwise a thrown value, which the
    /// handler of the place of the promise meets.
    pub fn decode(&self, reader: &mut Reader<'_>, request: u32, ok: bool) -> Option<Value> {
        let place = if ok {
            Place::At(&self.position)
        } else {
            Place::Rejected(&self.position)
        };
        decode_at(reader, request, 0, place)
    }

    /// Settles the promise of the engine with `completion`, which enqueues
    /// its reactions on its agent, and releases the work the agent counted
    /// for it.
    pub fn settle(&self, completion: Completion) {
        if self.settled.replace(true) {
            return;
        }
        self.agent.release_host_work();
        match completion {
            Completion::Normal(value) => self.resolving.resolve(&self.agent, value),
            Completion::Throw(reason) => self.resolving.reject(&self.agent, reason),
        }
    }
}

impl Drop for JsPromise {
    fn drop(&mut self) {
        if !self.settled.get() {
            self.agent.release_host_work();
        }
    }
}

/// A function of the host, the handle `handle` of the table `table`.
///
/// Its call hands `js_call` the context of the call and the arguments as the
/// engine passes them, the positional arguments `wrap_resolver` put in the
/// order of the schema, each written by [`push_arguments`], and answers the
/// completion of the host.
#[derive(Clone, Copy)]
pub struct JsFunction {
    /// The request whose table holds the handle, 0 for the boot.
    table: u32,
    /// The index in that table.
    handle: u32,
}

impl JsFunction {
    /// Returns the function of the handle `handle` of the table `table`, named
    /// `name`, as a value of the engine.
    fn value(table: u32, handle: u32, name: &str) -> Value {
        let function = Self { table, handle };
        Value::Function(Function::new(
            name,
            move |_agent: &Agent, args: &[Value]| function.call(args),
        ))
    }

    /// Calls the function with `args` and answers the completion of the host,
    /// whose value stands at the place of the value of the field the call
    /// resolves.
    fn call(self, args: &[Value]) -> Completion {
        let request = request::current();
        request::adopt_agent(request);
        let mut record = Vec::new();
        push_context(&mut record, request);
        push_arguments(&mut record, args, request);
        let len = import::js_call(
            self.table,
            self.handle,
            host::address(&mut record),
            host::length(record.len()),
            request,
        );
        answer(&host::input(len), request, Place::Field, Place::Thrown)
    }
}

/// Where a value of an argument comes from, which gives each of its objects
/// the prototype graphql-js gives it.
#[derive(Clone, Copy)]
enum Origin<'a> {
    /// The node of the document the value stands at, a literal or a variable.
    Node(&'a ValueNode),
    /// A value the client sent in its variables, whose objects are plain
    /// objects, as `JSON.parse` and `coerceInputValue` make them.
    Sent,
    /// A default of the operation or of the schema, whose objects have no
    /// prototype, as `valueFromAST` makes them.
    Default,
}

/// Appends the count and the records of the arguments `args` of a call of a
/// function of the host.
///
/// The walk reads each argument beside the node of the document it comes from
/// and the type the schema gives it, so the host sees each value as a resolver
/// of the app does. The type layer of the engine carries a date as its
/// `toISOString` text, which the walk reads back into a date. An object the
/// document writes as a literal crosses with no prototype, as `valueFromAST` of
/// graphql-js and the `parseLiteral` of the JSON scalars make it with
/// `Object.create(null)`, an object of a variable the client sent crosses as a
/// plain object, and the value of a variable the client left out, which takes
/// the default of the operation, crosses with no prototype. The value of a
/// variable that is an object or a list crosses under the name of the variable,
/// so the host makes one value of it for the whole request and hands that one
/// value to every place the variable stands, as graphql-js hands each place the
/// value it coerced once. Outside a resolver, and for a field the schema does
/// not declare, the arguments cross as they are.
fn push_arguments(out: &mut Vec<u8>, args: &[Value], request: u32) {
    host::push_count(out, args.len());
    let context = get_resolve_info().ok();
    let info = context.as_ref().map(|context| &context.info);
    let field = info.and_then(|info| {
        info.parent_type
            .get_fields()
            .ok()
            .and_then(|fields| fields.get(&info.field_name))
    });
    let (Some(info), Some(field)) = (info, field) else {
        for arg in args {
            host::encode(out, arg);
        }
        return;
    };
    let nodes = info
        .field_nodes
        .first()
        .and_then(|node| node.arguments.as_ref());
    let walk = Walk { info, request };
    for (index, arg) in args.iter().enumerate() {
        let Some(definition) = field.args.get(index) else {
            host::encode(out, arg);
            continue;
        };
        let node = nodes.and_then(|nodes| {
            nodes
                .iter()
                .rev()
                .find(|node| node.name.value() == definition.name)
        });
        let origin = node.map_or(Origin::Default, |node| Origin::Node(&node.value));
        walk.push(out, arg, &definition.r#type.to_string(), origin, 0);
    }
}

/// The walk of the arguments of one call.
struct Walk<'a> {
    /// The info of the field the call resolves.
    info: &'a ResolveInfo,
    /// The request the call runs for.
    request: u32,
}

impl Walk<'_> {
    /// Appends the record of `value`, of the type the schema prints as
    /// `printed`, from `origin`, nested `depth` levels deep.
    ///
    /// The named type is the printed type without its list and non null
    /// marks. A list takes its items through the same named type, and an
    /// input object its members through the types of its fields. A list
    /// stands at a list of the document item by item, and a value that is no
    /// list stands at a list of one item. A value nested deeper than
    /// [`host::DEPTH`] crosses as `undefined`, as [`host::encode`] writes it.
    fn push(
        &self,
        out: &mut Vec<u8>,
        value: &Value,
        printed: &str,
        origin: Origin<'_>,
        depth: usize,
    ) {
        if depth > host::DEPTH {
            host::push_tag(out, Tag::Undefined);
            return;
        }
        if let Origin::Node(ValueNode::Variable(variable)) = origin
            && matches!(value, Value::Object(_) | Value::List(_))
        {
            let name = variable.name.value();
            host::push_tag(out, Tag::Variable);
            host::push_bytes(out, name.as_bytes());
            self.push(out, value, printed, self.variable(name), depth);
            return;
        }
        let named = printed.trim_matches(|mark| matches!(mark, '[' | ']' | '!'));
        match value {
            Value::String(text) if named == date::NAME => {
                let revived = text.as_str().map_or(Value::String(text.clone()), |text| {
                    Value::Date(JsDate::parse(text))
                });
                host::encode(out, &revived);
            }
            Value::List(items) => {
                host::push_tag(out, Tag::List);
                host::push_count(out, items.len());
                for (index, item) in items.iter().enumerate() {
                    let origin = match origin {
                        Origin::Node(ValueNode::ListValue(list)) => {
                            list.values.get(index).map_or(Origin::Sent, Origin::Node)
                        }
                        other => other,
                    };
                    self.push(out, item, named, origin, depth + 1);
                }
            }
            Value::Object(object) => {
                let bare = matches!(
                    origin,
                    Origin::Default | Origin::Node(ValueNode::ObjectValue(_))
                );
                host::push_tag(out, if bare { Tag::Bare } else { Tag::Plain });
                let fields = self
                    .info
                    .schema
                    .get_type(named)
                    .and_then(|named| named.as_input_object())
                    .and_then(|input| input.get_fields().ok());
                // The keys cross as WTF-8, so a key that holds a lone
                // surrogate, which a value of the JSON scalars may carry in
                // from a variable, reaches the app as itself. A field of an
                // input object has a name of GraphQL, which holds none.
                let keys = object.own_keys_js();
                host::push_count(out, keys.len());
                for key in keys {
                    host::push_bytes(out, key.as_wtf8());
                    let member = Rc::clone(object).get_js(&key);
                    let name = to_string_lossy(&key);
                    let printed = fields
                        .as_ref()
                        .and_then(|fields| fields.get(name.as_ref()))
                        .map_or_else(String::new, |field| field.r#type.to_string());
                    let origin = match origin {
                        Origin::Node(ValueNode::ObjectValue(node)) => node
                            .fields
                            .iter()
                            .rev()
                            .find(|field| field.name.value() == name)
                            .map_or(Origin::Default, |field| Origin::Node(&field.value)),
                        Origin::Node(_) => Origin::Sent,
                        other => other,
                    };
                    self.push(out, &member, &printed, origin, depth + 1);
                }
            }
            other => host::encode(out, other),
        }
    }

    /// Returns where the value of the variable `name` comes from: the default
    /// of the operation when the operation gives the variable one and the
    /// client left it out, and the client otherwise.
    fn variable(&self, name: &str) -> Origin<'static> {
        let defaulted = self
            .info
            .operation
            .variable_definitions
            .as_ref()
            .and_then(|definitions| {
                definitions
                    .iter()
                    .find(|definition| definition.variable.name.value() == name)
            })
            .is_some_and(|definition| definition.default_value.is_some());
        if defaulted && !request::sent_variable(self.request, name) {
            Origin::Default
        } else {
            Origin::Sent
        }
    }
}

/// Appends the record of the variables of `info` of `request`, the member
/// `variableValues` of the view of the info of `aosabi-ts/lib/nodes.js`: a
/// plain object of the values the engine coerced, in their order, each read
/// beside the type its definition in the operation declares, as the walk of
/// [`push_arguments`] reads an argument.
///
/// So a date crosses as a `Date`, as the `parseValue` and the `parseLiteral` of
/// the scalar `Date` make it, an object of a default of the operation with no
/// prototype, as `valueFromAST` makes it, and the value of a variable that is
/// an object or a list under the name of the variable, so the host hands it the
/// one value the arguments of the request read, as graphql-js hands the info
/// and every argument the one object it coerced.
///
/// The walk counts the levels of each value from the value, and the host
/// reads each member of the plain object from the depth 0 as well, so a
/// variable nests as deep as an argument of the same value before
/// [`host::DEPTH`] stops both.
pub fn push_variables(out: &mut Vec<u8>, info: &ResolveInfo, request: u32) {
    let walk = Walk { info, request };
    let definitions = info
        .operation
        .variable_definitions
        .as_deref()
        .unwrap_or_default();
    host::push_tag(out, Tag::Plain);
    host::push_count(out, info.variable_values.len());
    for (name, value) in info.variable_values.own_keys() {
        host::push_string(out, name);
        let value = Value::from_js(value);
        let declared = definitions
            .iter()
            .find(|definition| name.as_str() == Some(definition.variable.name.value()));
        let Some(definition) = declared else {
            host::encode(out, &value);
            continue;
        };
        let variable = ValueNode::Variable(definition.variable.clone());
        let printed = aosabi_lang::printer::print(&definition.r#type);
        walk.push(out, &value, &printed, Origin::Node(&variable), 0);
    }
}

/// Returns a plain error with `message`.
fn error(message: &str) -> Value {
    Value::from(ErrorObject::error(message))
}

/// Returns the name of an error the host names `name`.
fn error_name(name: JsString) -> ErrorName {
    match name.as_str() {
        Some("Error") => ErrorName::Error,
        Some("TypeError") => ErrorName::TypeError,
        Some("RangeError") => ErrorName::RangeError,
        _ => ErrorName::Other(name),
    }
}

/// Returns the error of the engine for the error `handle` of the table of
/// `request` at `place`, nested `depth` levels deep in a record, which it
/// reads through the import `js_error` in the form of the place, a thrown
/// error with the levels of the handlers that meet it, an error inside a value
/// or the original error of a `GraphQLError`, as [`Place::error_record`]
/// writes it.
///
/// The handle the host gives a thrown value it could not read, and an answer
/// of `js_error` that throws, a getter of the error among its reads, make a
/// plain error that says so, and a broken answer the plain error of a broken
/// record. An original error is read the same way one level deeper, so an
/// error that is its own original error ends past [`host::DEPTH`] levels in
/// the error of a broken record. The settlements the reads made take effect
/// once the record is read.
// Out of line, so a request compiles the read of an error of the host where one crosses.
#[inline(never)]
fn error_of(request: u32, handle: u32, depth: usize, place: Place<'_>) -> ErrorObject {
    if handle == NO_HANDLE {
        return ErrorObject::error(UNREADABLE);
    }
    let mut record = place.error_record();
    let form = record.first().copied().unwrap_or(HELD);
    let len = import::js_error(
        request,
        handle,
        host::address(&mut record),
        host::length(record.len()),
        request,
    );
    let bytes = host::input(len);
    let mut reader = Reader::new(&bytes);
    let error = match reader.byte() {
        Some(NORMAL) => match form {
            THROWN => read_thrown(&mut reader, request),
            ORIGINAL => read_original(&mut reader, request, depth),
            PULLED => read_pulled(&mut reader),
            _ => read_held(&mut reader, request, depth),
        },
        Some(THROW) => skip(&mut reader, depth).map(|()| ErrorObject::error(UNREADABLE)),
        _ => None,
    };
    let Some(error) = error else {
        return ErrorObject::error(BROKEN);
    };
    settlements(&mut reader);
    error
}

/// Reads the record `js_error` answers in the form [`THROWN`] after its
/// completion byte and returns the error the executor locates from
/// the error of the host, or `None` for a broken record.
///
/// The record is the [`Brand`]. An `AggregateError` follows with a count and
/// the errors of it the executor handles, in their order, each the byte
/// [`ITEM_ERROR`] and the record of an error of the app, or the byte
/// [`ITEM_COERCED`] and the name and the message of the error the executor
/// coerced from an item that is no error, which a plain error of the engine
/// stands for. Any other error follows as [`read_handled`] reads it.
fn read_thrown(reader: &mut Reader<'_>, request: u32) -> Option<ErrorObject> {
    let brand = Brand::from_byte(reader.byte()?)?;
    if brand != Brand::Aggregate {
        return read_handled(reader, request, brand);
    }
    let count = reader.count()?;
    let mut errors = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let error = match reader.byte()? {
            ITEM_ERROR => {
                let brand = Brand::from_byte(reader.byte()?)?;
                if brand == Brand::Aggregate {
                    return None;
                }
                read_handled(reader, request, brand)?
            }
            ITEM_COERCED => {
                let name = reader.string()?;
                let message = reader.string()?;
                ErrorObject::Plain {
                    name: error_name(name),
                    message,
                }
            }
            _ => return None,
        };
        errors.push(Value::from(error));
    }
    Some(ErrorObject::Aggregate {
        message: JsString::from(""),
        errors,
    })
}

/// Reads the record of a thrown error of the app of the brand `brand` after
/// its brand and returns the error of the engine that stands for it while the
/// execution runs, or `None` for a broken record.
///
/// The record is the byte [`LOCATED`] for an error whose path is an array,
/// which the executor takes as it is, or 0 and the message, the blame and the
/// byte 1 where the extensions of the located error are an object of the
/// app, and then the answer of the first read of the member `CRITICAL_ERROR`
/// of those extensions and the handle of the record the host keeps of the
/// error for the reads at the end of the execution.
///
/// The error of the engine holds what the executor reads while it runs: that
/// a located error carries a path, the message and the blame of any other,
/// and extensions that hold `CRITICAL_ERROR` where its first read found it
/// true, so each handler throws the error on. What
/// the mask and the writer of the result read of it waits for [`finish`] at
/// the end of the execution, through a [`Deferred`] in the place of its
/// original error or of the Rust value behind it. A `ForbiddenError` with no
/// such extensions waits for nothing and takes its message alone, since no
/// plugin the module serves reads its feature.
fn read_handled(reader: &mut Reader<'_>, request: u32, brand: Brand) -> Option<ErrorObject> {
    let located = reader.byte()? == LOCATED;
    let blamed = if located {
        None
    } else {
        let message = reader.string()?;
        let blame = read_blame(reader)?;
        let extended = match reader.byte()? {
            0 => false,
            1 => true,
            _ => return None,
        };
        Some((message, blame, extended))
    };
    let critical = reader.byte()?;
    if critical > CRITICAL_TRUE {
        return None;
    }
    let handle = reader.word()?;
    let mut extensions = JsonObject::new();
    if critical == CRITICAL_TRUE {
        extensions.insert(JsString::from(CRITICAL_ERROR), JsValue::Bool(true));
    }
    let deferred = Deferred {
        request,
        handle,
        brand,
        located,
    };
    let Some((message, blame, extended)) = blamed else {
        return Some(ErrorObject::GraphQL(GraphQLError::new(
            JsString::from(""),
            GraphQLErrorOptions {
                path: Some(Vec::new()),
                original_error: Some(deferred.original()),
                extensions: Some(extensions),
                ..GraphQLErrorOptions::default()
            },
        )));
    };
    Some(match brand {
        Brand::GraphQL => {
            let mut error = GraphQLError::new(
                message,
                GraphQLErrorOptions {
                    original_error: Some(deferred.original()),
                    extensions: Some(extensions),
                    ..GraphQLErrorOptions::default()
                },
            );
            blame_into(&mut error, blame);
            ErrorObject::GraphQL(error)
        }
        _ if extended => {
            let name = if brand == Brand::Forbidden {
                ErrorName::Other(JsString::from(FORBIDDEN_NAME))
            } else {
                ErrorName::Error
            };
            let mut props = JsonObject::with_capacity(1);
            props.insert(JsString::from("extensions"), JsValue::Object(extensions));
            let host: Rc<dyn Any> = Rc::new(deferred.host());
            ErrorObject::custom(name, message, props, Some(host))
        }
        // The map of the identity plugin reads the message and the feature of
        // the error at the end of the execution, the reads of the record.
        Brand::Forbidden if IDENTITY.get() => {
            let host: Rc<dyn Any> = Rc::new(deferred.host());
            let name = ErrorName::Other(JsString::from(FORBIDDEN_NAME));
            ErrorObject::custom(name, message, JsonObject::new(), Some(host))
        }
        Brand::Forbidden => forbidden_of(message, &Value::Undefined, Vec::new()),
        Brand::None | Brand::Aggregate => ErrorObject::Plain {
            name: ErrorName::Error,
            message,
        },
    })
}

/// Reads the record `js_error` answers in the form [`PULLED`] for an error
/// after its completion byte, and returns the error a read of the engine
/// meets for it, or `None` for a broken record.
///
/// The record is the [`Brand`], the name, the message and the value record of
/// the member `statusCode`, and the error of the engine holds the name and
/// the message and a `statusCode` that is a number as its one member, of
/// which the reader of the body and the network capability read the
/// message.
fn read_pulled(reader: &mut Reader<'_>) -> Option<ErrorObject> {
    let _brand = Brand::from_byte(reader.byte()?)?;
    let name = reader.string()?;
    let message = reader.string()?;
    let status_code = match reader.tag()? {
        Tag::Number => Some(reader.number()?),
        Tag::Undefined => None,
        _ => return None,
    };
    let mut props = JsonObject::with_capacity(1);
    if let Some(status) = status_code {
        props.insert(JsString::from("statusCode"), JsValue::Number(status));
    }
    Some(ErrorObject::custom(error_name(name), message, props, None))
}

/// Returns the value Yoga folds into the errors of a result for the value
/// `handle` of the table of `request` that a hook of a plugin of the app
/// threw or the promise of a done hook rejected with, nested `depth` levels
/// deep in a record, read through the import `js_error` in the form [`HOOK`].
///
/// The handle the host gives a thrown value it could not read, and an answer
/// of `js_error` that throws, make a plain error that says so, and a broken
/// answer the plain error of a broken record, as [`error_of`] makes them. The
/// settlements the reads made take effect once the record is read.
// Out of line, so a request compiles the value of a hook where a plugin of the app hands one.
#[inline(never)]
fn hook_value(request: u32, handle: u32, depth: usize) -> Value {
    if handle == NO_HANDLE {
        return error(UNREADABLE);
    }
    let mut record = Place::Hook.error_record();
    let len = import::js_error(
        request,
        handle,
        host::address(&mut record),
        host::length(record.len()),
        request,
    );
    let bytes = host::input(len);
    let mut reader = Reader::new(&bytes);
    let value = match reader.byte() {
        Some(NORMAL) => read_hook(&mut reader, depth),
        Some(THROW) => skip(&mut reader, depth).map(|()| error(UNREADABLE)),
        _ => None,
    };
    let Some(value) = value else {
        return error(BROKEN);
    };
    settlements(&mut reader);
    value
}

/// Reads the record `js_error` answers in the form [`HOOK`] after its
/// completion byte, nested `depth` levels deep, and returns the value that
/// stands for the value of the host as the handler of Yoga reads it, or
/// `None` for a broken record.
///
/// The host read the value as `handleError` of Yoga reads it, then as its mask
/// reads it where the mask reads it, and as `getResponseInitByRespectingErrors`
/// and the writer of the result read the error it keeps. The value of the
/// engine is the one the handler of `graphql-over-http` folds into the same
/// errors: an `AggregateError` with the values of the errors of a value that
/// holds a member `errors`, and with none for an abort, a `GraphQLError` with
/// the message, the path, the extensions and the locations of its blame where
/// the mask keeps it, with no original error where its original errors are
/// `GraphQLError`s to the last, or with the path, the member `http` and the
/// source and the positions of its blame where the mask masks it, and for any
/// other value an error, the text of its method `toString` or `undefined`, each
/// of which the handler masks where the mask reads it. The copies are read by
/// [`host::read_json`], so they cross at any depth.
fn read_hook(reader: &mut Reader<'_>, depth: usize) -> Option<Value> {
    if depth > host::DEPTH {
        return None;
    }
    let masked_original = || {
        OriginalError::Other(Box::new(ThrownError {
            name: JsString::from("Error"),
            message: JsString::from(""),
            host: None,
        }))
    };
    Some(match reader.byte()? {
        hook::AGGREGATE => {
            let count = reader.count()?;
            let mut errors = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                errors.push(read_hook(reader, depth + 1)?);
            }
            Value::from(ErrorObject::Aggregate {
                message: JsString::from(""),
                errors,
            })
        }
        hook::ABORT => Value::from(ErrorObject::Aggregate {
            message: JsString::from(""),
            errors: Vec::new(),
        }),
        hook::KEPT => {
            let original = match reader.byte()? {
                0 => false,
                1 => true,
                _ => return None,
            };
            let extensions = match host::read_json(reader)? {
                JsValue::Object(extensions) => extensions,
                _ => JsonObject::new(),
            };
            let message = reader.string()?;
            let (path, host_path) = paths_of_json(&host::read_json(reader)?);
            let blame = read_blame(reader)?;
            let mut error = GraphQLError::new(
                message,
                GraphQLErrorOptions {
                    path,
                    extensions: Some(extensions),
                    original_error: (!original).then(masked_original),
                    ..GraphQLErrorOptions::default()
                },
            );
            error.locations = locations_of_blame(&blame);
            error.host_places = host_places(host_path, blame.odd_locations());
            Value::from(ErrorObject::GraphQL(error))
        }
        hook::MASKED => {
            let (path, host_path) = paths_of_json(&host::read_json(reader)?);
            let http = Some(host::read_json(reader)?).filter(truthy_json);
            let blame = read_blame(reader)?;
            let mut extensions = JsonObject::new();
            if let Some(http) = http {
                extensions.insert(JsString::from("http"), http);
            }
            let mut error = GraphQLError::new(
                JsString::from(""),
                GraphQLErrorOptions {
                    path,
                    extensions: Some(extensions),
                    original_error: Some(masked_original()),
                    ..GraphQLErrorOptions::default()
                },
            );
            error.host_places = host_places(host_path, blame.odd_locations());
            (error.source, error.positions) = blame.positions().unzip();
            Value::from(ErrorObject::GraphQL(error))
        }
        hook::BARE => {
            let name = reader.string()?;
            let message = reader.string()?;
            Value::from(ErrorObject::Plain {
                name: error_name(name),
                message,
            })
        }
        hook::ERROR => {
            let message = reader.string()?;
            let mut props = JsonObject::new();
            if let extensions @ JsValue::Object(_) = host::read_json(reader)? {
                props.insert(JsString::from("extensions"), extensions);
            }
            Value::from(ErrorObject::custom(ErrorName::Error, message, props, None))
        }
        hook::TEXT => Value::String(reader.string()?),
        hook::NONE => Value::Undefined,
        _ => return None,
    })
}

/// Returns whether the mask of Yoga reads a value that escapes the execution,
/// which the handler of `aosabi-core` turns off where `NODE_ENV` is
/// `development`.
fn masks_errors() -> bool {
    aosabi_core::app::env_alias::var("NODE_ENV").as_deref() != Some("development")
}

/// The name of the class `ForbiddenError`, which the executor reads
/// no name of and the error of the engine takes from its brand.
const FORBIDDEN_NAME: &str = "ForbiddenError";

/// The member of the extensions of an error that `handleFieldError` of the
/// executor reads at a place that may hold null.
const CRITICAL_ERROR: &str = "CRITICAL_ERROR";

/// A thrown error of the host whose reads at the end of the execution wait:
/// the handle of the record the host kept of it in the table of its request,
/// its brand and whether the executor took it as it is.
///
/// It stands in the host of a [`ThrownError`], the original error of the
/// error of the engine that stands for the error of the host or the Rust
/// value behind it, which [`finish`] finds among the errors of the result.
struct Deferred {
    /// The request whose table holds the record.
    request: u32,
    /// The handle of the record.
    handle: u32,
    /// The brand of the error.
    brand: Brand,
    /// Whether the path of the error is an array, so the executor took it as
    /// it is.
    located: bool,
}

impl Deferred {
    /// Returns the host of a thrown error of the engine that names the
    /// record.
    // An `unsafe` `Rc` would skip the atomic count each copy takes.
    fn host(self) -> Opaque {
        Opaque(Arc::new(self))
    }

    /// Returns the original error that names the record.
    fn original(self) -> OriginalError {
        OriginalError::Other(Box::new(ThrownError {
            name: JsString::from("Error"),
            message: JsString::from(""),
            host: Some(self.host()),
        }))
    }
}

/// Makes the reads at the end of the execution of every thrown
/// error of the host among `errors`, the errors of a result, and gives each
/// error what they answer, before any plugin of the app and the mask read the
/// result.
///
/// A `ForbiddenError` where a plugin of the app maps it, the identity plugin,
/// takes the reads of the map in place of those of the mask, its message and
/// its feature, since the map replaces it before the mask reads the result, and
/// the Rust value behind it keeps both as the host holds them.
///
/// The reads are those of [`Deferred`], through `js_error`, in the order of
/// the errors of the result. A `GraphQLError` first takes its original error
/// in the form [`END`], read as `isOriginalGraphQLError` of Yoga reads it,
/// which tells whether the mask keeps the error, and an error of any other
/// class is masked. Then the form [`FINAL`] makes the reads of the mask or of
/// the writer of the result, as [`Final::apply`] gives them to the error.
pub fn finish(errors: &mut [GraphQLError]) {
    if errors.is_empty() {
        return;
    }
    finish_all(errors);
}

/// Makes the reads of [`finish`] of the errors `errors`, one or more.
// Out of line, so a result with no error compiles none of the reads.
#[inline(never)]
fn finish_all(errors: &mut [GraphQLError]) {
    for error in errors {
        let Some(deferred) = deferred_of(error) else {
            continue;
        };
        if deferred.brand == Brand::GraphQL {
            let Some(original) = read_end(&deferred, &[END], |reader| {
                Some(original_of(decode_at(
                    reader,
                    deferred.request,
                    1,
                    Place::Original,
                )?))
            }) else {
                continue;
            };
            match error.original_error.as_mut() {
                Some(OriginalError::GraphQL(inner)) if !deferred.located => {
                    inner.original_error = original;
                }
                _ => error.original_error = original,
            }
        }
        if deferred.brand == Brand::Forbidden && IDENTITY.get() {
            if let Some(read) = read_end(&deferred, &[FINAL, MAPPED], read_mapped) {
                read.apply(error, &deferred);
            }
            continue;
        }
        let masked = deferred.brand != Brand::GraphQL || !is_original(error);
        let read = read_end(&deferred, &[FINAL, u8::from(masked)], |reader| {
            read_final(reader, &deferred, masked)
        });
        if let Some(read) = read {
            read.apply(error, &deferred);
        }
    }
}

/// Makes the reads of a thrown error of the host that fails a
/// stream of results, `error`, the rejection of the iterator of a
/// subscription of the app, which the executor wraps as the `GraphQLError`
/// that blames the operation, before the masks of Envelop and of Yoga read
/// it, and replaces it with the error the reads answer.
///
/// The hook `onSubscribeError` of the plugin of the reads at the end calls it,
/// which stands before the mask of Envelop, whose hook of the same name masks
/// the error, and before the handler of the errors of a stream of Yoga, which
/// masks it again and folds it into the result of a frame, so the reads are
/// those [`finish`] makes of an error of a single result.
pub fn finish_stream_error(error: &mut Value) {
    let Value::Error(object) = error else {
        return;
    };
    let ErrorObject::GraphQL(graphql) = object.as_ref() else {
        return;
    };
    if deferred_of(graphql).is_none() {
        return;
    }
    let mut errors = [copy_graphql_error(graphql)];
    finish(&mut errors);
    let [finished] = errors;
    *error = Value::from(ErrorObject::GraphQL(finished));
}

/// Makes the reads the writer of the result makes at the end of an execution
/// that no mask follows of every thrown error of the host among `errors`, the
/// errors of a result, and gives each error what they answer.
///
/// It is [`finish`] for the executor of the feature `execute`, which answers
/// below HTTP as `normalizedExecutor` answers before Yoga reads the result,
/// so no read of the form [`END`] is made and the form [`FINAL`] reads every
/// error as an error the mask keeps.
#[cfg(feature = "execute")]
pub fn finish_unmasked(errors: &mut [GraphQLError]) {
    for error in errors {
        let Some(deferred) = deferred_of(error) else {
            continue;
        };
        let read = read_end(&deferred, &[FINAL, 0], |reader| {
            read_final(reader, &deferred, false)
        });
        if let Some(read) = read {
            read.apply(error, &deferred);
        }
    }
}

/// Answers whether `error` is a `GraphQLError` whose original errors down
/// its chain are `GraphQLError`s to the last, `isOriginalGraphQLError` of
/// Yoga, so the mask keeps it.
fn is_original(error: &GraphQLError) -> bool {
    match &error.original_error {
        None => true,
        Some(OriginalError::GraphQL(inner)) => is_original(inner),
        Some(OriginalError::Other(_)) => false,
    }
}

/// Reads the error `deferred` names through `js_error` with the record
/// `record` and answers what `read` takes of the answer, whose settlements
/// take effect once it is read, or `None` for an answer that throws or is
/// broken, which leaves the error as it stands.
fn read_end<T>(
    deferred: &Deferred,
    record: &[u8],
    read: impl FnOnce(&mut Reader<'_>) -> Option<T>,
) -> Option<T> {
    let mut record = record.to_vec();
    let len = import::js_error(
        deferred.request,
        deferred.handle,
        host::address(&mut record),
        host::length(record.len()),
        deferred.request,
    );
    let bytes = host::input(len);
    let mut reader = Reader::new(&bytes);
    let answer = match reader.byte() {
        Some(NORMAL) => read(&mut reader),
        _ => None,
    };
    if answer.is_some() {
        settlements(&mut reader);
    }
    answer
}

/// Returns the record of the reads at the end of the execution `error` holds,
/// as [`read_handled`] put it in place of its original error, of the
/// original error of the error that stands for the error of the host, or of
/// the Rust value behind it, or `None` for any other error.
fn deferred_of(error: &GraphQLError) -> Option<Deferred> {
    let thrown = match error.original_error.as_ref()? {
        OriginalError::Other(other) => other.downcast_ref::<ThrownError>()?,
        OriginalError::GraphQL(inner) => match inner.original_error.as_ref()? {
            OriginalError::Other(other) => other.downcast_ref::<ThrownError>()?,
            OriginalError::GraphQL(_) => return None,
        },
    };
    let deferred = thrown.host.as_ref()?.downcast_ref::<Deferred>()?;
    Some(Deferred {
        request: deferred.request,
        handle: deferred.handle,
        brand: deferred.brand,
        located: deferred.located,
    })
}

/// What the reads of the form [`FINAL`] answer for a thrown error.
enum Final {
    /// The mask keeps a `GraphQLError` the executor took as it is, which the
    /// writer of the result makes anew from its message, its path, its
    /// extensions and its blame.
    Kept {
        /// The message.
        message: JsString,
        /// The locations the blame gives.
        locations: Option<Vec<SourceLocation>>,
        /// The path, `None` where the copy holds no list of strings and
        /// whole numbers.
        path: Option<Vec<PathSegment>>,
        /// The path and the locations the model holds no form for, as the
        /// JSON graphql-js writes.
        host: Option<Box<HostPlaces>>,
        /// The extensions.
        extensions: JsonObject,
    },
    /// The mask keeps the located error the executor made, whose extensions
    /// are those of the error of the host.
    Extended(JsonObject),
    /// The mask masks a `GraphQLError` the executor took as it is, with its
    /// path, the member http of its extensions and the source and positions
    /// of its blame.
    Masked {
        /// The path.
        path: Option<Vec<PathSegment>>,
        /// The path and the locations the model holds no form for, as the
        /// JSON graphql-js writes.
        host: Option<Box<HostPlaces>>,
        /// The member http of the extensions, where it is true.
        http: Option<JsValue>,
        /// The source and the positions of the blame, `None` where it gives
        /// no location.
        blame: Option<(Arc<Source>, Vec<usize>)>,
    },
    /// The mask masks the located error the executor made, the member http
    /// of whose extensions it reads.
    Http(Option<JsValue>),
    /// The mask masks an error of a class that is no `GraphQLError` the
    /// executor took as it is, and reads nothing of it.
    Bare,
    /// The map of the identity plugin replaces a `ForbiddenError` before the
    /// mask, with its message and the copy of its feature where the host
    /// found it true.
    Mapped {
        /// The message.
        message: JsString,
        /// The copy of the feature, `None` where it is false or where its
        /// copy is `undefined`.
        feature: Option<JsValue>,
    },
}

/// Reads the record `js_error` answers in the form [`FINAL`] with the byte
/// [`MAPPED`] after its completion byte: the message of the error and a byte
/// 0 for a feature that is false or 1 and the copy JSON reads of it.
///
/// The host tested the truth of the feature on the value it holds, as the map
/// tests it, so a copy whose own truth differs, the `null` of an
/// invalid `Date` among them, stands as the feature the map writes, and only a
/// copy that is `undefined`, of a function or of a symbol, leaves the member
/// out, as `JSON.stringify` leaves it out of the error.
fn read_mapped(reader: &mut Reader<'_>) -> Option<Final> {
    let message = reader.string()?;
    let feature = match reader.byte()? {
        0 => None,
        _ => Some(decode(reader, request::current())?.to_js()).filter(|copy| !copy.is_undefined()),
    };
    Some(Final::Mapped { message, feature })
}

/// Reads the record `js_error` answers in the form [`FINAL`] after its
/// completion byte for the error `deferred` names, masked when `masked`
/// holds, or answers `None` for a broken record.
///
/// For an error the mask keeps the record is the copy of the extensions, the
/// message, the copy of the path and the blame of a `GraphQLError` the
/// executor took as it is, or the copy of the extensions the located error of
/// the executor holds. For a masked `GraphQLError` the executor took as it is
/// the record is the copy of its path, the copy of the member http of its
/// extensions or `undefined`, and its blame, for the located error of the
/// executor that copy alone, and for any other error nothing.
fn read_final(reader: &mut Reader<'_>, deferred: &Deferred, masked: bool) -> Option<Final> {
    let request = deferred.request;
    let unknown = Position::Unknown;
    let held = Place::At(&unknown);
    let object = |value: Value| match value.to_js() {
        JsValue::Object(object) => object,
        _ => JsonObject::new(),
    };
    let http = |value: Value| Some(value.to_js()).filter(truthy_json);
    Some(match (masked, deferred.located) {
        (false, true) => {
            let extensions = object(decode_at(reader, request, 1, held)?);
            let message = reader.string()?;
            let (path, host_path) = paths_of(&decode_at(reader, request, 1, held)?);
            let blame = read_blame(reader)?;
            Final::Kept {
                message,
                locations: locations_of_blame(&blame),
                path,
                host: host_places(host_path, blame.odd_locations()),
                extensions,
            }
        }
        (false, false) => Final::Extended(object(decode_at(reader, request, 1, held)?)),
        (true, false) => Final::Http(http(decode_at(reader, request, 1, held)?)),
        (true, true) if deferred.brand != Brand::GraphQL => Final::Bare,
        (true, true) => {
            let (path, host_path) = paths_of(&decode_at(reader, request, 1, held)?);
            let http = http(decode_at(reader, request, 1, held)?);
            let blame = read_blame(reader)?;
            let host = host_places(host_path, blame.odd_locations());
            let blame = blame.positions();
            Final::Masked {
                path,
                host,
                http,
                blame,
            }
        }
    })
}

/// Returns whether the JSON value `value` is true, `ToBoolean` of ECMA-262.
fn truthy_json(value: &JsValue) -> bool {
    match value {
        JsValue::Undefined | JsValue::Null => false,
        JsValue::Bool(value) => *value,
        JsValue::Number(number) => *number != 0.0 && !number.is_nan(),
        JsValue::String(text) => !text.is_empty(),
        JsValue::Array(_) | JsValue::Object(_) | JsValue::Opaque(_) => true,
    }
}

impl Final {
    /// Gives `error`, which holds the record `deferred` names, what the reads
    /// of the form [`FINAL`] answered, so the mask of the engine and the
    /// writer of the result answer it.
    ///
    /// A masked error keeps of its extensions the member http alone, which
    /// the mask carries over, and a masked `GraphQLError` the executor took as
    /// it is takes its path and the source and the positions of its blame in
    /// place of its nodes, from which the mask computes its locations. A
    /// masked error of a class that is no `GraphQLError` the executor took as
    /// it is loses its path, so the mask answers no path and no locations, as
    /// the mask of Yoga answers an error that is no `GraphQLError`.
    fn apply(self, error: &mut GraphQLError, deferred: &Deferred) {
        let http_only = |http: Option<JsValue>| {
            let mut extensions = JsonObject::new();
            if let Some(http) = http {
                extensions.insert(JsString::from("http"), http);
            }
            extensions
        };
        match self {
            Self::Kept {
                message,
                locations,
                path,
                host,
                extensions,
            } => {
                error.message = message;
                error.locations = locations;
                error.path = path;
                error.host_places = host;
                error.extensions = extensions;
            }
            Self::Extended(extensions) => {
                if let Some(OriginalError::GraphQL(inner)) = error.original_error.as_mut() {
                    inner.extensions = extensions.clone();
                }
                error.extensions = extensions;
            }
            Self::Masked {
                path,
                host,
                http,
                blame,
            } => {
                error.path = path;
                error.host_places = host;
                error.nodes = None;
                error.locations = None;
                (error.source, error.positions) = blame.unzip();
                error.extensions = http_only(http);
            }
            Self::Http(http) => {
                error.extensions = http_only(http);
                if let Some(OriginalError::Other(other)) = error.original_error.as_mut()
                    && let Some(thrown) = other.downcast_mut::<ThrownError>()
                {
                    thrown.host = (deferred.brand == Brand::Forbidden).then(|| {
                        // An `unsafe` `Rc` would skip the atomic count each
                        // copy takes.
                        Opaque(Arc::new(ForbiddenError::with_message(
                            thrown.message.clone(),
                        )))
                    });
                }
            }
            Self::Mapped { message, feature } => {
                error.extensions = JsonObject::new();
                if let Some(OriginalError::Other(other)) = error.original_error.as_mut()
                    && let Some(thrown) = other.downcast_mut::<ThrownError>()
                {
                    let mut forbidden = ForbiddenError::with_message(message);
                    forbidden.feature = feature;
                    forbidden.feature_true = true;
                    // An `unsafe` `Rc` would skip the atomic count each copy
                    // takes.
                    thrown.host = Some(Opaque(Arc::new(forbidden)));
                }
            }
            Self::Bare => {
                error.path = None;
                error.extensions = JsonObject::new();
                error.original_error = Some(OriginalError::Other(Box::new(ThrownError {
                    name: JsString::from("Error"),
                    message: JsString::from(""),
                    host: None,
                })));
            }
        }
    }
}

/// The blame of a thrown error of the host, its nodes, its source and its
/// positions as the constructor of a `GraphQLError` reads them, which give an
/// error its locations.
enum Blame {
    /// No nodes, where the executor locates the error at the nodes of the
    /// field it resolves, and the mask and the writer of the result give the
    /// error they make anew no location.
    Field,
    /// Nodes of the error that give no location.
    None,
    /// Positions in the bodies of sources, each a body and a position in it.
    Source(Vec<(JsString, usize)>),
    /// Positions of which one is no whole number of the range of a `u32`, each
    /// a body and a position in it, whose locations the model holds no column
    /// for, so the error writes them as graphql-js computes them.
    Odd(Vec<(JsString, f64)>),
}

impl Blame {
    /// Returns the locations of a blame of positions of which one is no
    /// whole number, as the JSON graphql-js writes of them, and `None` for
    /// any other blame.
    fn odd_locations(&self) -> Option<JsValue> {
        let Self::Odd(pairs) = self else {
            return None;
        };
        Some(JsValue::Array(
            pairs
                .iter()
                .map(|(body, position)| {
                    let (line, column) = location_of(body, *position);
                    let mut location = JsonObject::with_capacity(2);
                    location.insert(JsString::from("line"), JsValue::Number(line));
                    location.insert(JsString::from("column"), JsValue::Number(column));
                    JsValue::Object(location)
                })
                .collect(),
        ))
    }

    /// Returns the source and the positions of a blame of positions, the
    /// source of the first standing for every position, and `None` for a
    /// blame that gives no position.
    // An `unsafe` `Rc` would skip the atomic count each copy takes.
    fn positions(self) -> Option<(Arc<Source>, Vec<usize>)> {
        let Self::Source(pairs) = self else {
            return None;
        };
        let body = pairs.first()?.0.clone();
        let positions = pairs.iter().map(|(_, position)| *position).collect();
        Some((Arc::new(Source::new(body)), positions))
    }
}

/// Reads the blame of a record of the form [`THROWN`] or [`FINAL`], or
/// answers `None` for a broken record.
fn read_blame(reader: &mut Reader<'_>) -> Option<Blame> {
    Some(match reader.byte()? {
        BLAME_FIELD => Blame::Field,
        BLAME_NONE => Blame::None,
        BLAME_SOURCE => {
            let count = reader.count()?;
            let mut pairs = Vec::with_capacity(count.min(64));
            for _ in 0..count {
                let body = reader.string()?;
                pairs.push((body, reader.number()?));
            }
            let whole: Option<Vec<(JsString, usize)>> = pairs
                .iter()
                .map(|(body, position)| Some((body.clone(), index_of(*position)?)))
                .collect();
            match whole {
                Some(pairs) => Blame::Source(pairs),
                None => Blame::Odd(pairs),
            }
        }
        _ => return None,
    })
}

/// Gives the error `error` of the engine that stands for a thrown
/// `GraphQLError` of the host its blame `blame`, from which the located error
/// of the executor takes its locations: nodes with no location where the
/// blame gives none, and otherwise the source of the first position with
/// every position, as the located error of graphql-js takes the source and
/// the positions of the error of the host, which its constructor took from
/// the first of its nodes where it was given none.
fn blame_into(error: &mut GraphQLError, blame: Blame) {
    match blame {
        Blame::Field => {}
        Blame::None => error.nodes = Some(Vec::new()),
        blame @ Blame::Odd(_) => {
            error.nodes = Some(Vec::new());
            error.host_places = host_places(None, blame.odd_locations());
        }
        blame @ Blame::Source(_) => {
            (error.source, error.positions) = blame.positions().unzip();
        }
    }
}

/// Returns the locations of a blame of positions, each in the body it lies
/// in, as the constructor of a `GraphQLError` computes them from the nodes it
/// is given where it is given no source, and from the one source of every
/// position otherwise, or `None` for a blame that gives none.
fn locations_of_blame(blame: &Blame) -> Option<Vec<SourceLocation>> {
    let Blame::Source(pairs) = blame else {
        return None;
    };
    Some(
        pairs
            .iter()
            .map(|(body, position)| get_location(&Source::new(body.clone()), *position))
            .collect(),
    )
}

/// Reads the record `js_error` answers in the form [`ORIGINAL`] after its
/// completion byte, nested `depth` levels deep, and returns the error of the
/// engine for the original error of a `GraphQLError` of the host, or `None`
/// for a broken record.
///
/// The record is the [`Brand`], then for a `GraphQLError` its own original
/// error, which `isOriginalGraphQLError` of Yoga reads, a value record read
/// the same way, and then the name and the message any error crosses with.
/// A `ForbiddenError` stands as any other error here, since the map of the
/// plugin `useIdentity` reads the original error of the error the executor
/// located and not the one below it.
fn read_original(reader: &mut Reader<'_>, request: u32, depth: usize) -> Option<ErrorObject> {
    let brand = Brand::from_byte(reader.byte()?)?;
    let original = if brand == Brand::GraphQL {
        decode_at(reader, request, depth + 1, Place::Original)?
    } else {
        Value::Undefined
    };
    let name = reader.string()?;
    let message = reader.string()?;
    Some(if brand == Brand::GraphQL {
        ErrorObject::GraphQL(GraphQLError::new(
            message,
            GraphQLErrorOptions {
                original_error: original_of(original),
                ..GraphQLErrorOptions::default()
            },
        ))
    } else {
        ErrorObject::Plain {
            name: error_name(name),
            message,
        }
    })
}

/// Reads the record `js_error` answers in the form [`HELD`] after its
/// completion byte, nested `depth` levels deep, and returns the error a
/// throw of a Rust app of its class makes, or `None` for a broken record.
///
/// The record is the [`Brand`], the name and the message, then the
/// extensions, the original error and the path of a `GraphQLError`, each a
/// value record, and the own enumerable members of any other error, the
/// extensions and each member as the copy `JSON.stringify` reads. The
/// extensions an error holds are its whole extensions, those its original
/// error lends it among them, so they reach the error of the engine as given
/// and not through its original error.
fn read_held(reader: &mut Reader<'_>, request: u32, depth: usize) -> Option<ErrorObject> {
    let brand = Brand::from_byte(reader.byte()?)?;
    let name = reader.string()?;
    let message = reader.string()?;
    let unknown = Position::Unknown;
    let extensions = decode_at(reader, request, depth + 1, Place::At(&unknown))?;
    let original = decode_at(reader, request, depth + 1, Place::Original)?;
    let path = decode_at(reader, request, depth + 1, Place::At(&unknown))?;
    let count = reader.count()?;
    let mut members = Vec::with_capacity(count.min(64));
    for _ in 0..count {
        let key = reader.string()?;
        members.push((
            key,
            decode_at(reader, request, depth + 1, Place::At(&unknown))?.to_js(),
        ));
    }
    Some(match brand {
        Brand::GraphQL => ErrorObject::GraphQL(GraphQLError::new(
            message,
            GraphQLErrorOptions {
                path: path_of(&path),
                original_error: original_of(original),
                extensions: Some(match extensions.to_js() {
                    JsValue::Object(extensions) => extensions,
                    _ => JsonObject::new(),
                }),
                ..GraphQLErrorOptions::default()
            },
        )),
        Brand::Forbidden => {
            let feature = members
                .iter()
                .find(|(key, _)| key.as_str() == Some("feature"))
                .map_or(Value::Undefined, |(_, value)| Value::from_js(value));
            forbidden(name, message, &feature, members)
        }
        Brand::None | Brand::Aggregate if members.is_empty() => ErrorObject::Plain {
            name: error_name(name),
            message,
        },
        Brand::None | Brand::Aggregate => {
            ErrorObject::custom(error_name(name), message, props(members), None)
        }
    })
}

/// Returns the members `members` of an error of the host as the own
/// enumerable properties of an error of the engine, a member that holds
/// `undefined` kept, as the inspect of graphql-js shows it.
fn props(members: Vec<(JsString, JsValue)>) -> JsonObject {
    let mut props = JsonObject::with_capacity(members.len());
    for (key, value) in members {
        props.insert(key, value);
    }
    props
}

/// Returns the `ForbiddenError` of the engine for one of the host with `name`,
/// `message`, `feature` and `members`: its own members in the order the host
/// holds them, which the class of the wrapper gives
/// them, and the Rust value behind it, which takes the message and the copy of
/// the feature as the host holds them.
fn forbidden(
    name: JsString,
    message: JsString,
    feature: &Value,
    members: Vec<(JsString, JsValue)>,
) -> ErrorObject {
    let mut error = ForbiddenError::with_message(message.clone());
    if !matches!(feature, Value::Undefined) {
        error = error.with_feature(feature.to_js());
    }
    // An `unsafe` `Rc` would skip the atomic count each copy takes.
    let host: Rc<dyn Any> = Rc::new(Opaque(Arc::new(error)));
    ErrorObject::custom(error_name(name), message, props(members), Some(host))
}

/// Returns the `ForbiddenError` of the engine for a thrown one of the host
/// with `message`, `feature` and `members`, named as the class names it,
/// since the executor reads no name of a thrown error.
fn forbidden_of(
    message: JsString,
    feature: &Value,
    members: Vec<(JsString, JsValue)>,
) -> ErrorObject {
    forbidden(JsString::from("ForbiddenError"), message, feature, members)
}

/// Returns the original error of a `GraphQLError` of the host, `None` for
/// `undefined` and `null`.
///
/// A `GraphQLError` stays one, any other error keeps its name, its message
/// and the Rust value behind it, as the located error of the executor keeps
/// them, and a value that is no error takes the string `String` answers for
/// it, so a mask finds an original error that is no `GraphQLError` in both.
fn original_of(original: Value) -> Option<OriginalError> {
    let error = match original {
        Value::Undefined | Value::Null => return None,
        Value::Error(error) => error,
        other => {
            return Some(OriginalError::Other(Box::new(ThrownError {
                name: JsString::from("Error"),
                message: to_js_string(&other),
                host: None,
            })));
        }
    };
    Some(match &*error {
        ErrorObject::GraphQL(error) => OriginalError::from(copy_graphql_error(error)),
        error => OriginalError::Other(Box::new(ThrownError {
            name: error.name(),
            message: error.message(),
            host: error
                .host()
                .and_then(|host| host.downcast_ref::<Opaque>())
                .cloned(),
        })),
    })
}

/// Returns the path of a `GraphQLError` of the host, the list of its keys and
/// indexes, or `None` for a value that is no list of strings and whole
/// numbers.
fn path_of(path: &Value) -> Option<Vec<PathSegment>> {
    let Value::List(items) = path else {
        return None;
    };
    items
        .iter()
        .map(|item| match item {
            Value::String(key) => Some(PathSegment::Key(to_string_lossy(key).into_owned())),
            Value::Number(index) => index_of(*index).map(PathSegment::Index),
            _ => None,
        })
        .collect()
}

/// Returns the path of a `GraphQLError` of the host as the model holds it, and
/// as the JSON graphql-js writes of it where the model holds no form for an
/// item of it, a number that is no whole number among them.
pub(crate) fn paths_of(path: &Value) -> (Option<Vec<PathSegment>>, Option<JsValue>) {
    match path_of(path) {
        Some(path) => (Some(path), None),
        None if matches!(path, Value::List(_)) => (None, Some(path.to_js())),
        None => (None, None),
    }
}

/// Returns the path of a `GraphQLError` of the host from the copy `path` JSON
/// reads of it, as [`paths_of`] returns it from a value.
pub(crate) fn paths_of_json(path: &JsValue) -> (Option<Vec<PathSegment>>, Option<JsValue>) {
    let JsValue::Array(items) = path else {
        return (None, None);
    };
    let segments: Option<Vec<PathSegment>> = items
        .iter()
        .map(|item| match item {
            JsValue::String(key) => Some(PathSegment::Key(to_string_lossy(key).into_owned())),
            JsValue::Number(index) => index_of(*index).map(PathSegment::Index),
            _ => None,
        })
        .collect();
    match segments {
        Some(path) => (Some(path), None),
        None => (None, Some(path.clone())),
    }
}

/// Returns the places of an error the model holds no form for, or `None`
/// where neither the path nor the locations need one.
pub(crate) fn host_places(
    path: Option<JsValue>,
    locations: Option<JsValue>,
) -> Option<Box<HostPlaces>> {
    (path.is_some() || locations.is_some()).then(|| Box::new(HostPlaces { path, locations }))
}

/// Returns the line and the column of the position `position` of the body
/// `body`, which may be no whole number, as `getLocation` of graphql-js
/// computes them over the code units of the body with JavaScript numbers.
///
/// A terminator `\r\n`, `\n` or `\r` that starts before the position opens
/// the next line, and the column is the position plus 1 less the start of its
/// line, so the position 1.5 of the body `a\nbc` lies at the column 0.5 of
/// line 2.
fn location_of(body: &JsString, position: f64) -> (f64, f64) {
    let units: Vec<u16> = to_string_lossy(body).encode_utf16().collect();
    let mut line = 1.0;
    let mut line_start = 0.0;
    let mut at = 0;
    while at < units.len() {
        let length: u8 = match units[at] {
            0x0d if units.get(at + 1) == Some(&0x0a) => 2,
            0x0a | 0x0d => 1,
            _ => {
                at += 1;
                continue;
            }
        };
        let index = f64::from(u32::try_from(at).unwrap_or(u32::MAX));
        if index >= position {
            break;
        }
        line += 1.0;
        line_start = index + f64::from(length);
        at += usize::from(length);
    }
    (line, position + 1.0 - line_start)
}

/// Returns `number` as the index of a list, or `None` for a number that is no
/// whole number of the range of a `u32`.
#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the number is a whole number of the range of a u32, checked first"
)]
pub(crate) fn index_of(number: f64) -> Option<usize> {
    if number.fract() != 0.0 || !(0.0..=f64::from(u32::MAX)).contains(&number) {
        return None;
    }
    usize::try_from(number as u32).ok()
}

/// Steps over one value record of the host, nested `depth` levels deep, with
/// no read of the host, and answers `None` for a broken record.
///
/// A thrown value the module drops, the throw of a projection or of the keys
/// of an object, is skipped this way, so neither its error nor any value in
/// it is read through an import.
fn skip(reader: &mut Reader<'_>, depth: usize) -> Option<()> {
    if depth > host::DEPTH {
        return None;
    }
    match reader.tag()? {
        Tag::Undefined | Tag::Null | Tag::False | Tag::True | Tag::Context => {}
        Tag::Number | Tag::Date => {
            reader.number()?;
        }
        Tag::String | Tag::Bytes => {
            reader.bytes()?;
        }
        Tag::Object | Tag::Promise | Tag::Error | Tag::Delegation => {
            reader.word()?;
        }
        Tag::List => {
            for _ in 0..reader.count()? {
                skip(reader, depth + 1)?;
            }
        }
        Tag::Plain => {
            for _ in 0..reader.count()? {
                reader.bytes()?;
                skip(reader, depth + 1)?;
            }
        }
        Tag::Function => {
            reader.word()?;
            reader.bytes()?;
        }
        Tag::Foreign => {
            reader.byte()?;
            reader.bytes()?;
        }
        Tag::Topic => {
            reader.word()?;
            reader.bytes()?;
            host::read_json(reader)?;
        }
        Tag::Fetched => crate::outbound::skip_fetched(reader)?,
        Tag::Bare | Tag::Variable | Tag::Held | Tag::File => return None,
    }
    Some(())
}

/// Traps on purpose when `text` is the marker [`TRAP`], in a build with the
/// feature `trap` alone, the test build of `13_ts1_boundary`, section 9,
/// which judges that the host never calls a trapped instance again.
#[cfg(feature = "trap")]
fn trap_at(text: &JsString) {
    if text.as_str() == Some(TRAP) {
        core::arch::wasm32::unreachable();
    }
}

/// Returns the completion of an answer of the host, whose handles stand in
/// the table of `request`, whose value stands at `place` and whose thrown
/// value at `thrown`, and a throw of a plain error for a broken answer.
///
/// The settlements the host carries after the completion take effect before
/// the completion is answered, so before the code that called the import goes
/// on.
fn answer(bytes: &[u8], request: u32, place: Place<'_>, thrown: Place<'_>) -> Completion {
    let mut reader = Reader::new(bytes);
    let completion = match reader.byte() {
        Some(NORMAL) => decode_at(&mut reader, request, 0, place).map(Completion::Normal),
        Some(THROW) => decode_at(&mut reader, request, 0, thrown).map(Completion::Throw),
        _ => None,
    };
    let Some(completion) = completion else {
        return Completion::Throw(error(BROKEN));
    };
    settlements(&mut reader);
    completion
}

/// Returns the handle of the object the member `key` of the object `handle`
/// of the table of the boot holds, read through one `js_get` for its value,
/// or `None` where the member holds no object of the host.
///
/// The boot reads each root of the copy of the graphql object of the app the
/// host made, whose members are data properties, so the read runs no code of
/// the app and carries no settlement.
pub fn boot_member(handle: u32, key: &str) -> Option<u32> {
    let mut record = Vec::new();
    push_context(&mut record, request::BOOT);
    host::push_bytes(&mut record, key.as_bytes());
    record.push(VALUE);
    let len = import::js_get(
        request::BOOT,
        handle,
        host::address(&mut record),
        host::length(record.len()),
        request::BOOT,
    );
    let bytes = host::input(len);
    let mut reader = Reader::new(&bytes);
    match (reader.byte()?, reader.tag()?) {
        (NORMAL, Tag::Object) => reader.word(),
        _ => None,
    }
}

/// Applies the settlements the host carries after the record of an import,
/// in the order the callbacks of the app made them inside it.
///
/// Each is the number of a request, the handle of a promise of its table,
/// the byte of the settlement, which [`settlement_of`] reads, and the value
/// record, whose handles stand in the table of that request. The record of
/// the import is read first, so a promise a settlement names that the same
/// record holds has its slot already. A broken settlement ends the reading.
fn settlements(reader: &mut Reader<'_>) {
    let Some(count) = reader.count() else {
        return;
    };
    for _ in 0..count {
        let (Some(request), Some(handle), Some(byte)) =
            (reader.word(), reader.word(), reader.byte())
        else {
            return;
        };
        let (fulfilled, adopted) = settlement_of(u32::from(byte));
        if !request::settle_carried(request, handle, fulfilled, adopted, reader) {
            return;
        }
    }
}

/// Returns what the byte `byte` of a settlement of the host says: whether
/// it fulfils, bit 0, and whether it comes through the resolving functions
/// the host handed a thenable it adopts, bit 1.
#[must_use]
pub fn settlement_of(byte: u32) -> (bool, bool) {
    (byte & FULFILLED != 0, byte & ADOPTED != 0)
}

/// Reads one value record of the host, whose handles stand in the table of
/// `request` and which stands at no place of the schema, the value of a
/// settlement of a promise the request waits on no more, or `None` for a
/// broken record.
pub fn decode(reader: &mut Reader<'_>, request: u32) -> Option<Value> {
    decode_at(reader, request, 0, Place::At(&Position::Unknown))
}

/// Reads one value record at `place`, nested `depth` levels deep, or `None`
/// for a broken record and one nested deeper than [`host::DEPTH`].
fn decode_at(
    reader: &mut Reader<'_>,
    request: u32,
    depth: usize,
    place: Place<'_>,
) -> Option<Value> {
    if depth > host::DEPTH {
        return None;
    }
    let tag = reader.tag()?;
    decode_tagged(tag, reader, request, depth, place)
}

/// Reads the rest of a value record whose tag `tag` the cursor of `reader`
/// has passed, at `place` and nested `depth` levels deep.
///
/// A bigint or a symbol, tag 15, is the text `String` answers for it at a leaf
/// of `ID` or `GID` and a [`foreign`] stand in at any other place. An array at
/// such a leaf is the array `String` joins there, whose items keep that text,
/// and a `TypeError` when a symbol stands among them, since `String` of an
/// array throws on a symbol. An object and a promise keep their place, so a
/// member of the object and the value the promise fulfils with take theirs. An
/// error is read through `js_error` by [`error_of`], and an error or an object
/// a hook of a plugin of the app threw or the promise of a done hook rejected
/// with by [`hook_value`].
fn decode_tagged(
    tag: Tag,
    reader: &mut Reader<'_>,
    request: u32,
    depth: usize,
    place: Place<'_>,
) -> Option<Value> {
    let value = match tag {
        Tag::Undefined => Value::Undefined,
        Tag::Null => Value::Null,
        Tag::False => Value::Bool(false),
        Tag::True => Value::Bool(true),
        Tag::Number => Value::Number(reader.number()?),
        Tag::String => {
            let text = reader.string()?;
            #[cfg(feature = "trap")]
            trap_at(&text);
            Value::String(text)
        }
        Tag::Date => Value::Date(JsDate::from_time_value(reader.number()?)),
        Tag::Object | Tag::Error if place.is_hook() => hook_value(request, reader.word()?, depth),
        Tag::Object => {
            let handle = reader.word()?;
            Value::Object(Rc::new(JsObject::placed(request, handle, place.position())))
        }
        Tag::List => {
            let position = place.position();
            if position.prints_text() {
                let (items, symbol) = joined(reader, request, depth)?;
                if symbol {
                    return Some(Value::from(ErrorObject::type_error(SYMBOL_TO_STRING)));
                }
                return Some(Value::List(Rc::new(items)));
            }
            let item = position.item();
            let count = reader.count()?;
            let mut items = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                items.push(decode_at(reader, request, depth + 1, Place::At(&item))?);
            }
            Value::List(Rc::new(items))
        }
        Tag::Function => {
            let handle = reader.word()?;
            let name = reader.text()?;
            JsFunction::value(request, handle, name)
        }
        Tag::Promise => {
            let handle = reader.word()?;
            JsPromise::wait(request, handle, place.position())
        }
        Tag::Error => {
            let handle = reader.word()?;
            Value::from(error_of(request, handle, depth, place))
        }
        Tag::Plain => {
            let count = reader.count()?;
            let mut entries = Vec::with_capacity(count.min(1024));
            for _ in 0..count {
                let key = String::from(reader.text()?);
                let member = decode_at(reader, request, depth + 1, Place::At(&Position::Unknown))?;
                entries.push((key, member));
            }
            Value::Object(PlainObject::from_entries(entries))
        }
        Tag::Foreign => {
            let _kind = reader.byte()?;
            let text = reader.string()?;
            if place.position().prints_text() {
                Value::String(text)
            } else {
                foreign()
            }
        }
        // An `unsafe` `Rc` would skip the atomic count each copy of the two
        // marks of a chunk and of the context takes.
        Tag::Bytes => Value::Host(HostValue::Opaque(Arc::new(crate::context::Chunk(
            reader.bytes()?.to_vec(),
        )))),
        Tag::Context => Value::Host(HostValue::Opaque(Arc::new(crate::context::ContextMark))),
        Tag::Topic => crate::pubsub::topic(reader)?,
        Tag::Fetched => crate::outbound::read_fetched(reader)?,
        Tag::Delegation => crate::federation::delegation(request, reader.word()?),
        Tag::Bare | Tag::Variable | Tag::Held | Tag::File => return None,
    };
    Some(value)
}

/// Reads the count and the items of an array a leaf of `ID` or `GID` prints
/// whole, nested `depth` levels deep, and answers whether a symbol stands
/// among them, in a nested array as well, or `None` for a broken record.
///
/// A bigint and a symbol keep the text `String` answers for each, which is
/// what the join writes for a bigint, and every other item stands at no
/// place, since a member of an object the join meets is read by no field.
// Out of line, so a request compiles the join of a list where a value prints as text.
#[inline(never)]
fn joined(reader: &mut Reader<'_>, request: u32, depth: usize) -> Option<(Vec<Value>, bool)> {
    let count = reader.count()?;
    let mut items = Vec::with_capacity(count.min(1024));
    let mut symbol = false;
    for _ in 0..count {
        if depth + 1 > host::DEPTH {
            return None;
        }
        let item = match reader.tag()? {
            Tag::Foreign => {
                symbol |= reader.byte()? == SYMBOL;
                Value::String(reader.string()?)
            }
            Tag::List => {
                let (nested, held) = joined(reader, request, depth + 1)?;
                symbol |= held;
                Value::List(Rc::new(nested))
            }
            other => decode_tagged(
                other,
                reader,
                request,
                depth + 1,
                Place::At(&Position::Unknown),
            )?,
        };
        items.push(item);
    }
    Some((items, symbol))
}

/// Returns the stand in of a bigint or a symbol of the host at a place other
/// than a leaf of `ID` or `GID`, an anonymous function whose every call
/// answers another stand in.
///
/// The executor calls a function a member of an object answers, and the step 5
/// of `wrap_resolver` wraps a function in a resolver that calls it, where `wrapResolver`
/// reads a bigint or a symbol as a value and calls nothing, so a call of the
/// stand in answers another stand in and the value of the field stays a stand
/// in, which a leaf completes as it completes a function.
fn foreign() -> Value {
    Value::Function(Function::new("", |_agent: &Agent, _args: &[Value]| {
        Completion::Normal(foreign())
    }))
}

/// Appends the context of a call of `request`: the locale `@inContext` names
/// for the running operation as a string record, or `undefined`, then one
/// byte, 1 where the request context of the engine is in place and 0 where it
/// is not, the two numbers of the logger `getLogger()` answers there, which
/// [`aosabi_core::logger::Logger::to_raw`] gives, and the number of the info
/// of the resolver the call runs in, 0 outside a resolver, which
/// `getResolveInfo()` of the host reads through the export `ctx`.
///
/// The host runs the code of the app inside the store of its
/// `AsyncLocalStorage`, whose context `getContext()` answers where the byte is
/// 1 and which throws `Context not defined` where it is 0, as the store
/// holds the context only behind the async context bind of the base pipeline,
/// and whose logger `getLogger()` answers after any await, as the store of
/// `loggerContext` holds it. The context in place is noted for the
/// request, so a call of `c` from the app finds the context of its request
/// whenever it runs.
pub fn push_context(out: &mut Vec<u8>, request: u32) {
    match aosabi_core::in_context::get_locale() {
        Some(locale) => {
            host::push_tag(out, Tag::String);
            host::push_string(out, &JsString::from(locale));
        }
        None => host::push_tag(out, Tag::Undefined),
    }
    let context = aosabi_core::context::ambient_context();
    out.push(u8::from(context.is_some()));
    if let Some(context) = context {
        crate::context::note_context(request, &context);
    }
    let (index, generation) = aosabi_core::logger::get_logger().to_raw();
    host::push_word(out, index);
    host::push_word(out, generation);
    let info = get_resolve_info().map_or(0, |info| crate::context::note_info(request, &info));
    host::push_word(out, info);
}

/// Pulls the next chunk of the reader `handle` of the table of `request`, the
/// body of the request the host holds, through one `iter_next` of the form
/// [`READ`], and answers its completion, the promise of the chunk.
pub fn pull(request: u32, handle: u32) -> Completion {
    request::adopt_agent(request);
    let mut record = Vec::new();
    push_context(&mut record, request);
    record.push(READ);
    let len = import::iter_next(
        request,
        handle,
        host::address(&mut record),
        host::length(record.len()),
        request,
    );
    answer(
        &host::input(len),
        request,
        Place::At(&Position::Pulled),
        Place::Pulled,
    )
}

/// Hands the host the outbound request `record` of `request` through the
/// import `host_fetch`, and answers its completion, the promise of the
/// answer of the `fetch` of the host.
pub fn fetch(request: u32, record: &mut [u8]) -> Completion {
    request::adopt_agent(request);
    let len = import::host_fetch(request, host::address(record), host::length(record.len()));
    answer(
        &host::input(len),
        request,
        Place::At(&Position::Pulled),
        Place::Pulled,
    )
}

/// Calls the function `handle` of the table `table` of the host through
/// which the wrapper calls a hook of a plugin of the app, with the arguments
/// `count` written by `write`, and answers the completion of the host.
///
/// The value the call answers stands at the place [`Position::Hook`], so a
/// promise it answers rejects into the reads of [`HOOK`], and an error or an
/// object it throws is read the same way, as the handler of Yoga reads a
/// value that escapes the execution.
pub fn call_hook(
    table: u32,
    handle: u32,
    request: u32,
    count: usize,
    write: impl FnOnce(&mut Vec<u8>),
) -> Completion {
    call_at(
        table,
        handle,
        request,
        count,
        write,
        &Position::Hook,
        Place::Hook,
    )
}

/// Calls the function `handle` of the table `table` of the host with the one
/// argument `arg`, a value of the engine written as [`host::encode`] writes
/// it, and answers the completion of the host: a patch, a guard and a
/// headers function of a federation of the app, `crate::federation`.
///
/// The answer stands at no place of the schema, since the transform of the
/// patches reads its members as data, and a thrown value is read as the throw
/// of the field whose resolver runs, the delegating field, since the reference
/// throws it inside the async function of `delegate`, whose rejection the
/// executor meets at that field.
pub fn call_value(table: u32, handle: u32, request: u32, arg: &Value) -> Completion {
    call_at(
        table,
        handle,
        request,
        1,
        |out| host::encode(out, arg),
        &Position::Unknown,
        Place::Thrown,
    )
}

/// Calls the function `handle` of the table `table` of the host with the
/// arguments `count` written by `write`, its answer standing at `position`
/// and its thrown value at `thrown`, and answers the completion of the host.
fn call_at(
    table: u32,
    handle: u32,
    request: u32,
    count: usize,
    write: impl FnOnce(&mut Vec<u8>),
    position: &Position,
    thrown: Place<'_>,
) -> Completion {
    request::adopt_agent(request);
    let mut record = Vec::new();
    push_context(&mut record, request);
    host::push_count(&mut record, count);
    write(&mut record);
    let len = import::js_call(
        table,
        handle,
        host::address(&mut record),
        host::length(record.len()),
        request,
    );
    answer(&host::input(len), request, Place::At(position), thrown)
}

/// The forms of the original error of an error of a result in the payload of
/// a done hook of a plugin of the app.
mod original {
    /// No original error.
    pub const NONE: u8 = 0;
    /// The error of the host the record kept, after the handle of the record
    /// in the table of the request.
    pub const KEPT: u8 = 1;
    /// An error of the engine, after its value record.
    pub const ENGINE: u8 = 2;
}

/// Appends the original error of `error`, an error of a result of `request`,
/// to the payload of a done hook of a plugin of the app.
///
/// A thrown error of the host whose reads at the end of the execution the
/// host kept a record of crosses as that error, by the handle of its record,
/// so the done hook reads the value the app threw. A `GraphQLError` of the
/// engine crosses as the class `GraphQLError` of the wrapper, a
/// `ForbiddenError` as its class with its message and its feature, and any
/// other error with its name and its message, as an error the engine raises
/// toward the app crosses.
pub fn push_original(out: &mut Vec<u8>, error: &GraphQLError, request: u32) {
    if let Some(deferred) = deferred_of(error).filter(|deferred| deferred.request == request) {
        out.push(original::KEPT);
        host::push_word(out, deferred.handle);
        return;
    }
    let thrown = match error.original_error.as_ref() {
        Some(OriginalError::GraphQL(inner)) => {
            out.push(original::ENGINE);
            host::push_tag(out, Tag::Error);
            host::encode_error(out, &ErrorObject::GraphQL(copy_graphql_error(inner)), 1);
            return;
        }
        Some(OriginalError::Other(other)) => other.downcast_ref::<ThrownError>(),
        None => None,
    };
    let Some(thrown) = thrown else {
        out.push(original::NONE);
        return;
    };
    let host = thrown.host.as_ref();
    out.push(original::ENGINE);
    host::push_tag(out, Tag::Error);
    if let Some(forbidden) = host.and_then(|host| host.downcast_ref::<ForbiddenError>()) {
        out.push(Brand::Forbidden.byte());
        host::push_string(out, &JsString::from(ForbiddenError::NAME));
        host::push_string(out, &forbidden.message);
        host::push_tag(out, Tag::Undefined);
        host::push_tag(out, Tag::Undefined);
        let mut members = JsonObject::with_capacity(1);
        members.insert(
            JsString::from("feature"),
            forbidden.feature.clone().unwrap_or_default(),
        );
        host::encode(out, &Value::from_js(&JsValue::Object(members)));
        return;
    }
    host::encode_error(
        out,
        &ErrorObject::Plain {
            name: error_name(thrown.name.clone()),
            message: thrown.message.clone(),
        },
        1,
    );
}

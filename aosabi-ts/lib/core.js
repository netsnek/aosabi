// SPDX-License-Identifier: MIT
/*
 * core.js: the host half of the bridge between the engine and the app, the
 * module core.wasm or the native library of the engine for Bun on Linux, a
 * plain ES module of the package aosabi with no npm dependency.
 *
 * No analogue in the reference. It runs in Node 24.13.0, in workerd 1.20250718.0
 * under the flag nodejs_compat_v2, which gives it AsyncLocalStorage, and in Bun
 * 1.3.14.
 */

// The module is instantiated from its compiled form, booted with the SDL and
// the IR of the build of the app and its graphql object, and then answers
// each web standard Request. An engine opens the instance: wasmEngine opens
// one of the module over the import object of the functions below, and the
// engine of aosabi/bun opens the native library over the same functions as
// callbacks of bun:ffi, whose boot takes them as typed function pointers. The
// two have the same exports, and each gives this module the reads and the one
// write of its memory, so the bridge is the same over both. A value of the app
// stays here behind a handle, an index into the table of its request, and
// table 0 holds the copy of the graphql object and its roots. The table of a
// request is dropped when its answer is read, so no value of the app outlives
// its request. The records are those of crates/aosabi-js/src/host.rs, and
// every import answers a completion, the byte 0 and a value, the record of an
// error for js_error, or the byte 1 and a thrown value, as
// crates/aosabi-js/src/value.rs reads it.
//
// Three conditions keep the safe fn imports of the module sound. Every import
// runs the JavaScript it reaches inside a try, the getters, the traps of a
// Proxy, a then, the walk of a value and the queueing of a job among
// it, and answers a throw as a throw completion, or answers its request with
// a failure of status 500 once the export that called it has returned, so no
// exception crosses a frame of the module. An instance whose export threw has
// trapped and is never called again: every request that waits on it answers
// with a failure of status 500, its imports do nothing more, and the next
// request runs on a fresh instance of the same compiled module, while a panic
// of the native library ends the process under the panic strategy abort, so
// the container restarts where a wasm instance is replaced. And the wrapper
// writes the memory of the engine in send alone, into the range the export
// alloc has just answered, keeps no address of that memory across calls, and
// reads a record of the module in place before its next call into the
// module, through views of the memory it makes anew once the memory grew and
// its buffer was replaced, so no view outlives the buffer it was made over.
//
// The walk of a value calls the then of each thenable where step 3 of the
// resolver wrapper calls it, with the one callback of that call, so no import
// hands the host a promise to wait on, and it maps an array as step 4 maps
// it, so an array that holds a thenable crosses as the promise of all its
// items, which the wrapper makes and owns. The field of such a then takes
// what the then answered, and the export settle settles it in the job
// the field settles in wherever the wrapper can tell that job, as
// callThen, mapList and writeAnswered say. A settlement that a callback of
// the app makes inside an import rides on the answer of that import, which
// the module applies before the code that called the import goes on, so it
// takes effect in the job of the call, and the wrapper reserves a microtask
// at the call, where V8 queues the reactions of the promise of the app, in
// which the consumer of the engine runs, so no export but alloc runs inside
// an import, as settleField says. A settlement through the resolving
// functions the wrapper hands a thenable of the app it adopts carries the
// bit ADOPTED, so the module runs a level of reactions more at once, since
// the async function of a root hands that thenable its own resolving
// functions and reacts on such a settlement itself.
//
// A projection of the module reads a member as often as the projection of
// the resolver wrapper reads it, once for the test of its truth, once more for
// the test of its type when the first answer is true and once more for its
// value, and a getter of the app runs at each read. A read for a test hands
// over no value, so the walk of a value runs at the read of the value alone.
// In the read mode project the module names the members a projection reads,
// and one import makes their reads in that order and count, until a member
// whose value the projection may project before it reads the next one.
//
// An error of the app crosses as a handle alone, and the module reads it
// through the import js_error, in the form of the place it met it: a thrown
// error as the executor reads it while the execution runs, in
// locatedError of graphql-js and in the handler of each place it reaches, an
// error inside a value with the reads of its class, and the original error of
// a GraphQLError with the reads of isOriginalGraphQLError of Yoga, so the
// masking of the engine sees the classes a masking tells apart with
// instanceof. The wrapper keeps a record of each thrown error, and once the
// execution has ended the module makes through it the reads Yoga makes at the
// end of the request, of its original error, of the member http of a masked
// error and of what the writer of the result reads of an error it makes anew,
// so no field of the request sees a read made at the end. The
// extensions and the other members of an error cross as the copy
// JSON.stringify reads, as Yoga writes them into the result. An error the
// engine raises toward the app crosses with its brand, and the decoder makes
// it an instance of the class of lib/errors.js that brand names, or of the
// intrinsic AggregateError.
//
// The graphql object of the app is read once when createCore makes the core,
// as the service reads it when it starts, each root as often as the service
// reads it and each member of each root once in the order of Object.entries, and
// the module reads the roots from that copy when it boots and each member at
// every call, with no code of the app run.
import { AsyncLocalStorage } from "node:async_hooks";
import { AGGREGATE, brandOf, fromEngine, FORBIDDEN, GRAPHQL, GraphQLError } from "./errors.js";
import { argsOf, readView, VIEW } from "./nodes.js";
import { PUSH, PushedBody } from "./stream.js";
import { initOptions, Toucan } from "./toucan.js";

// The store of every call into the app: the context of its request, which
// getContext() answers, and the locale of @inContext, which getLocale()
// answers.
export const context = new AsyncLocalStorage();

const encoder = new TextEncoder();
const decoder = new TextDecoder();
const strict = new TextDecoder("utf-8", { fatal: true });

// The functions of the host the wrapper calls, taken when this module loads,
// before any code of the app runs, so an app that replaces a global, as fake
// timers replace queueMicrotask, does not reach the wrapper: queueMicrotask,
// setTimeout, Promise with its then and its resolve, Reflect.apply, a promise
// fulfilled already, whose reaction is one microtask, the map and the some of
// Array that the walk of an array calls, and Object.defineProperty and
// Object.create, with which the decoder makes an object and each of its
// members an own data property, and clearTimeout and the monotonic clock of
// the host, performance.now, or Date.now where a host holds none, which the
// timers of the engine read.
const enqueueMicrotask = globalThis.queueMicrotask.bind(globalThis);
const later = globalThis.setTimeout.bind(globalThis);
const unlater = globalThis.clearTimeout.bind(globalThis);
const clock = typeof globalThis.performance?.now === "function"
  ? globalThis.performance.now.bind(globalThis.performance)
  : Date.now;
const HostPromise = Promise;
const promiseThen = Promise.prototype.then;
const promiseResolve = Promise.resolve;
const apply = Reflect.apply;
const fulfilled = Promise.resolve();
const arrayMap = Array.prototype.map;
const arraySome = Array.prototype.some;
const defineProperty = Object.defineProperty;
const createObject = Object.create;
// The fetch of the host, which the outbound requests of the engine take, its
// crypto, the node:crypto and the Buffer of node:buffer of the host itself, and
// its read of a file, the readFileSync and the join of node:fs and node:path,
// each where the host gives it through process.getBuiltinModule, and nothing on
// a host that gives no file system, the global scope of a Worker among them. The
// module of the host is taken at the run and never imported, since Wrangler
// bundles an import of node:crypto as the polyfill of unenv, whose sign is not
// implemented, where the node:crypto of workerd signs.
const hostFetch = globalThis.fetch.bind(globalThis);
const builtin = (name) => {
  try {
    return globalThis.process?.getBuiltinModule?.(name);
  } catch {
    return undefined;
  }
};
const hostCrypto = builtin("node:crypto");
const HostBuffer = builtin("node:buffer")?.Buffer;
const hostFs = builtin("node:fs");
const hostPath = builtin("node:path");

// A rejection handler that does nothing, which handles the rejection of a
// promise of the wrapper whose consumer the module stands for, as the
// consumer of that promise handles it.
const ignore = () => {};

// The tags of a value record, crates/aosabi-js/src/host.rs.
const TAG = {
  UNDEFINED: 0, NULL: 1, FALSE: 2, TRUE: 3, NUMBER: 4, STRING: 5, DATE: 6,
  OBJECT: 7, LIST: 8, FUNCTION: 9, PROMISE: 10, ERROR: 11, PLAIN: 12, BARE: 13,
  VARIABLE: 14, FOREIGN: 15, CONTEXT: 16,
  HELD: 20, BYTES: 21, FILE: 22, EXECUTE: 23, DONE: 24, TOPIC: 25, FETCHED: 26, DELEGATION: 27,
};

// The member of the marker of a delegation of a federation of the app that
// holds its instance, its request and the number of its promise in the list
// of that request, crates/aosabi-js/src/federation.rs.
const DELEGATION = Symbol("aosabi.delegation");

// The marker of a delegation lib/federation.js answers for
// gateway.delegate(key, options): the engine starts the delegation of the
// crate aosabi-federation as a task of the agent of the request and keeps
// the promise it settles with, so a resolver or a member of a patch that
// answers the marker answers that promise of the engine itself, which the
// walk writes as the tag DELEGATION, and the value of the delegation never
// crosses to the host and back. It is a thenable whose member then throws a
// TypeError that names the waiting, since its value lives in the engine
// alone, so an app that awaits it itself, that answers a list of them, which
// the walk hands Promise.all, or that answers it from an async resolver
// or an async member of a patch, whose promise resolves its answer through
// that member, meets that TypeError.
export class Delegation {
  constructor(state, request, number) {
    defineProperty(this, DELEGATION, { __proto__: null, value: { state, request, number } });
    Object.freeze(this);
  }

  then() {
    throw new TypeError("a delegation of aosabi settles in the engine where a resolver or a member of a patch " +
      "answers it, and an app that awaits it itself waits for a fixture that needs it");
  }
}

// The bytes of each file the module handed the app, by the web File the
// reader made of them, so a file a resolver hands a federation among the
// arguments of a delegation crosses back as the upload it was.
const FILES = new WeakMap();

// The number of each federation of the app, the member lib/federation.js gives
// it, shared through the registry of symbols, the policy pass attaches to a
// patch, the symbol of the reference, and the member of the patch of
// passthrough(), which the engine answers with no call of the host.
const GATEWAY = Symbol.for("aosabi.gateway");
const POLICY = Symbol.for("aosabi.gateway.policy");
const PASSTHROUGH = Symbol.for("aosabi.passthrough");

// The identity of each headers function of a federation of the app, which
// keys the slot of the cache of remote schemas of the engine as the address
// of a function keys it on the Rust option, so two federations of one URL
// share a slot where they share the function.
const HEADERS = new WeakMap();
let headersCount = 0;

// Answers the identity of the headers function `headers`.
function identityOf(headers) {
  let identity = HEADERS.get(headers);
  if (identity === undefined) {
    headersCount += 1;
    identity = headersCount;
    HEADERS.set(headers, identity);
  }
  return identity;
}

// The depth past which the data of a delegation is refused, the bound of the
// reader of an answer of a federation, where the builder of the syntax tree
// of the reference recurses until the stack of the host ends.
const DATA_DEPTH = 1000;

// The marker subscribe of a pub sub of lib/pubsub.js answers, whose topic
// lives in the pub sub of the engine: the number of its pub sub, its routing
// key and its id. A resolver of a subscription answers it, and the walk
// writes it as the tag TOPIC, which the module reads into the repeater of the
// topic, crates/aosabi-js/src/pubsub.rs. It is no async iterable of the host,
// since its events reach the engine alone, so an app that iterates it itself
// meets a TypeError that names the waiting.
export class Topic {
  constructor(pubSub, routingKey, id) {
    defineProperty(this, TOPIC, { __proto__: null, value: { pubSub, routingKey, id } });
    Object.freeze(this);
  }

  [Symbol.asyncIterator]() {
    throw new TypeError("a topic of a pub sub of aosabi streams through the engine where a subscription answers " +
      "it, and an iteration of the app over it waits for a fixture that needs it");
  }
}

// Publishes to the topic `routingKey` of the pub sub `pubSub` with the
// arguments `args`, pubSub.publish(routingKey, ...args), through the export
// publish of the instance of the engine made last, in a microtask of the
// host, so no export runs inside an import of the code of the app that
// publishes. Each argument crosses as the copy of JSON data JSON.stringify
// reads of it at the call, the payload the engine hands each subscriber of
// the topic. A publish before an app booted, where no subscriber can wait,
// or on an instance that trapped, reaches nothing.
export function publishTopic(pubSub, routingKey, args) {
  const out = new Out();
  out.word(pubSub);
  out.text(String(routingKey));
  out.word(args.length);
  for (const arg of args) writeJsonDeep(out, jsonCopy(arg));
  enqueueMicrotask(() => {
    const state = latest;
    if (state === null || !state.booted || state.dead) return;
    try {
      state.call("publish", state.send(out));
    } catch {
      // The instance trapped, and condemn answered the requests on it.
    }
  });
}

// The ops of the export ctx, the call of the request it names,
// crates/aosabi-js/src/context.rs.
const CTX = {
  GET: 17, RESOLVE_INFO: 25, SET_RESULT: 26, VAR_NAMES: 27, ZITADEL_CLAIMS: 33, SENTRY: 34, FEDERATION: 35,
  OPEN: 36, END: 37,
};

// The acts of the op SENTRY that build the client of a request and report
// the error hono set on c, crates/aosabi-js/src/sentry.rs, beside those of
// lib/toucan.js, and the number of the client of the base pipeline of the
// canary.
const SENTRY_ACT = { START: 5, CAPTURE: 6 };
const CANARY_CLIENT = 0xffffffff;

// The first number of the promise of the claims of a provider of
// aosabi/auth/zitadel, far above the numbers of the continuations of a chain
// and below 2 ** 31, since a module hands a number of 32 bits to the host as
// a signed integer.
const CLAIM_NUMBERS = 0x40000000;

// The ops of the call of the crypto of the host, crates/aosabi-js/src/outbound.rs.
const CRYPTO = { SHA256: 0, SIGN: 1, READ_FILE: 2, NOW: 3, IMPORT: 4 };

// Answers the call `op` of the crypto of the host with the fields `fields`:
// the SHA-256 digest of bytes, the RSASSA-PKCS1-v1_5 signature with SHA-256
// of bytes under the RSAPrivateKey DER of a key, both of node:crypto, the
// crypto of Bun and of Cloudflare, or the bytes of a file below the working
// directory, as the key loader joins its path, or the clock of the host,
// Date.now(), as eight little endian bytes of a double, or the import of the
// RSAPrivateKey DER of a key, which answers no bytes where the host takes the
// key, or null where the host answers none.
function cryptoCall(op, fields) {
  if (op === CRYPTO.SHA256 && hostCrypto !== undefined) return hostCrypto.createHash("sha256").update(fields[0]).digest();
  if (op === CRYPTO.SIGN && hostCrypto !== undefined && HostBuffer !== undefined) {
    return hostCrypto.sign("sha256", fields[1], { key: HostBuffer.from(fields[0]), format: "der", type: "pkcs1" });
  }
  if (op === CRYPTO.IMPORT && hostCrypto !== undefined && HostBuffer !== undefined) {
    hostCrypto.createPrivateKey({ key: HostBuffer.from(fields[0]), format: "der", type: "pkcs1" });
    return new Uint8Array(0);
  }
  if (op === CRYPTO.NOW) {
    const bytes = new Uint8Array(8);
    new DataView(bytes.buffer).setFloat64(0, Date.now(), true);
    return bytes;
  }
  if (op === CRYPTO.READ_FILE && hostFs !== undefined && hostPath !== undefined) {
    return hostFs.readFileSync(hostPath.join(globalThis.process.cwd(), decoder.decode(fields[0])));
  }
  return null;
}

// The ops of the export logger, crates/aosabi-js/src/log.rs.
export const LOG = { EMIT: 0, CHILD: 1, WITH_TAG: 2, CREATE: 3, LEVEL: 4, CONFIGURE: 5 };

// The bytes of a completion.
const NORMAL = 0;
const THROW = 1;

// The kinds of a record of the tag FOREIGN.
const BIGINT = 0;
const SYMBOL = 1;

// The brands of an error record, crates/aosabi-js/src/host.rs: no brand, the
// class GraphQLError, ServiceError and every other subclass of it among them,
// the class ForbiddenError and the intrinsic AggregateError. KINDS names the
// kind of lib/errors.js of each.
const BRAND = { NONE: 0, GRAPHQL: 1, FORBIDDEN: 2, AGGREGATE: 3 };
const KINDS = [undefined, GRAPHQL, FORBIDDEN, AGGREGATE];

// The bit of the byte of a settlement that marks one through the resolving
// functions the wrapper hands a thenable it adopts, beside the bit 0 of a
// fulfilment, crates/aosabi-js/src/value.rs.
const ADOPTED = 2;

// The forms of the record of js_project: the own enumerable keys, the reads
// of the members a projection selects, and the names the projection of a
// build of the canary holds, crates/aosabi-js/src/value.rs.
const OWN_KEYS = 0;
const SELECTED = 1;
const NAMES = 2;

// Returns the names the projection of a build of the canary holds of
// `object`, its own property names and those of its chain of prototypes up to
// Object.prototype, but constructor, getAllPropertyNames of the canary's
// dist/index.js:47-68 with the patch whose type resolvers test a value with in
// over them.
function canaryNames(object) {
  const names = new Set(Object.getOwnPropertyNames(object));
  for (let proto = Object.getPrototypeOf(object); proto && proto !== Object.prototype;
    proto = Object.getPrototypeOf(proto)) {
    for (const name of Object.getOwnPropertyNames(proto)) names.add(name);
  }
  names.delete("constructor");
  return [...names];
}

// The forms of the record of js_get, and the kinds of a read of the form
// SELECTED: the value of a member, and a test of it alone.
const VALUE = 0;
const TEST = 1;

// The bits of the flags of a selected member: one the projection reads no
// value of when it is no function, and one whose value the projection may
// project before it reads the next member.
const ROUTED = 1;
const NESTED = 2;

// The answers of a test: the member is undefined, false, true and no
// function, or a function.
const TESTED = { UNDEFINED: 0, FALSE: 1, TRUE: 2, FUNCTION: 3 };

// The forms of a read of js_error: a thrown error, an error inside a value,
// the original error of a GraphQLError, and the two reads of a thrown error at
// the end of the execution, of its original error and of what the mask or the
// writer of the result reads once the mask has decided.
const THROWN = 0;
const ORIGINAL = 2;
const END = 3;
const FINAL = 4;

// The form of a read of js_error of the value a read of the host rejected
// with, a pull of the body of a request or the fetch of the host, its name,
// its message and its member statusCode, crates/aosabi-js/src/value.rs.
const PULLED = 5;

// The form of a read of js_error of a value a hook of a plugin of the app
// threw or the promise of a done hook rejected with, the reads of the handler
// of Yoga that folds a value escaping the execution into the errors of the
// result, and the kinds of its record, crates/aosabi-js/src/value.rs: a value
// that holds a member errors, an abort, a GraphQLError the mask keeps or that
// no mask reads, one the mask masks, any other value the mask masks, and where
// no mask reads it an error of another class, a value with a method toString
// and one with none.
const HOOK = 6;
const HOOK_KIND = { AGGREGATE: 0, ABORT: 1, KEPT: 2, MASKED: 3, BARE: 4, ERROR: 5, TEXT: 6, NONE: 7 };

// The byte of a read of the form FINAL of a ForbiddenError that the map of
// the identity plugin replaces before the mask, beside 0 for an error the
// mask keeps and 1 for one it masks, crates/aosabi-js/src/value.rs.
const MAPPED = 2;

// The forms of the outcome of an execution in the payload of a done hook of a
// plugin of the app, crates/aosabi-js/src/plugin.rs: a single result and a
// stream of results.
const OUTCOME = { SINGLE: 0, STREAM: 1 };

// The forms of the original error of an error of that outcome: none, the
// error of the app the record kept by the handle of the record, and an error
// of the engine as a value record, crates/aosabi-js/src/value.rs.
const ORIGINAL_FORM = { NONE: 0, KEPT: 1, ENGINE: 2 };

// The members of a result in the payload of a done hook and in a result the
// app leaves, crates/aosabi-js/src/plugin.rs: errors that hold an array, the
// data, extensions that hold an object, and any other member, errors and
// extensions that hold null among them.
const MEMBER = { ERRORS: 0, DATA: 1, EXTENSIONS: 2, OTHER: 3 };

// The forms of an error of a result the app sets: an error of the outcome the
// payload handed, by its index there, and a GraphQLError of the app.
const SET_ERROR = { ENGINE: 0, APP: 1 };

// The member of an error of the payload of a done hook that holds the number
// of its call, its index among the errors of the outcome, so the error
// crosses back as the error of the engine it stands for, and what the error
// held when the payload handed it, so a change in place shows. It enumerates
// not, so no spread and no JSON.stringify sees it.
const ENGINE_ERROR = Symbol("aosabi.engine.error");

// The member of the payload of a done hook through which the wrapper hands
// the engine the result the hook leaves once it returned.
const FINISH = Symbol("aosabi.finish");

// Returns an object whose members `names` are getters that throw a TypeError
// naming the waiting, the prototype of a stand in of a value of graphql-js
// whose other members the engine holds no form for.
function refusing(what, names) {
  const prototype = {};
  for (const name of names) {
    defineProperty(prototype, name, {
      __proto__: null,
      get() {
        throw new TypeError("the member " + name + " of " + what + " of aosabi waits for a fixture that needs it");
      },
      configurable: true,
    });
  }
  return prototype;
}

// The prototype of a node of the document an error of the payload of a done hook
// blames, which holds its kind and its location as a node of graphql-js holds
// them, and of a location, which holds its start, its end and its source and
// answers its JSON as a Location of graphql-js answers it.
const NODE_PROTOTYPE = refusing("a node of the document a done hook reads",
  ["alias", "name", "arguments", "directives", "selectionSet"]);
const LOCATION_PROTOTYPE = refusing("the location of a node a done hook reads", ["startToken", "endToken"]);
defineProperty(LOCATION_PROTOTYPE, "toJSON", {
  __proto__: null,
  value: function toJSON() {
    return { start: this.start, end: this.end };
  },
  writable: true,
  configurable: true,
});

// The member of the payload of onExecute through which the wrapper keeps the
// hooks the app answered in the table of the request.
const KEEP = Symbol("aosabi.keep");

// The forms of the hook onExecute of a plugin of the app in the boot record,
// crates/aosabi-js/src/plugin.rs: none, a function of the app, and the map of
// the identity plugin, an item of the engine.
const EXECUTE_FORM = { NONE: 0, HOST: 1, IDENTITY: 2 };

// The member of the hook onExecute of the identity plugin of
// aosabi/auth/plugin that names it the map of the engine, shared through the
// registry of symbols, so the entry aosabi needs no import of the entry of
// the plugin.
export const ENGINE_HOOK = Symbol.for("aosabi.engine.hook");

// The hooks of Envelop and of Yoga a plugin may hold beside onExecute, in the
// order of MEMBER_ORDER of crates/aosabi-core/src/plugin/mod.rs, each of which
// fails the boot of the engine with its name, as aosabi migrate names it.
export const OTHER_HOOKS = [
  "onPluginInit", "onEnveloped", "onSchemaChange", "onParse", "onValidate", "onContextBuilding",
  "onSubscribe", "onRequest", "onResponse", "onDispose", "instrumentation", "onYogaInit", "onRequestParse",
  "onParams", "onExecutionResult", "onResultProcess",
];

// The bits of a level of the record of the form THROWN, one handler of the
// executor that meets a thrown error: the item of a list, which takes no
// AggregateError apart, and a place that may hold null, where the error stops
// unless its extensions mark it CRITICAL_ERROR.
const LEVEL_ITEM = 1;
const LEVEL_NULLABLE = 2;

// The member of the extensions handleFieldError of the executor reads at a
// place that may hold null, and the answers the record gives of its first
// read: none, a value that is false and one that is true.
const CRITICAL_ERROR = "CRITICAL_ERROR";
const CRITICAL = { UNREAD: 0, FALSE: 1, TRUE: 2 };

// The kinds of an error of an AggregateError the executor handles: an error of
// the app, and the error coerceError made of an item that is no error.
const ITEM = { ERROR: 0, COERCED: 1 };

// The byte of the record of a thrown error whose path is an array, which the
// executor takes as it is, and the blames of one it locates: the nodes of the
// field, nodes that give no location, and positions of a source.
const LOCATED = 1;
const BLAME = { FIELD: 0, NONE: 1, SOURCE: 2 };

// The depth past which neither side follows a value, the bound of DEPTH in
// crates/aosabi-js/src/host.rs.
const DEPTH = 64;

// The handle of an error the wrapper cannot read, which names no entry of a
// table, crates/aosabi-js/src/host.rs.
const ABSENT = 0xffffffff;

// The roots of the graphql object of an app, in the order the service reads them.
const ROOTS = ["Query", "Mutation", "Subscription"];

// The bits of the mark of an answer, crates/aosabi-js/src/request.rs: one for
// the answer of a result processor of Yoga that answers with a stream, events
// or parts, which the host gets as a stream, so it frames it as it frames
// the stream of an answer, and one for a body that is a string, which the host hands
// its platform as a string, so a platform that writes the type of a string
// body on the wire alone writes it. The compress() of the base pipeline
// codes an answer in the hono of the host, lib/pylon.js.
const MARK_STREAM = 4;
const MARK_TEXT = 8;

// The bit of the mark of the head of an answer whose body the module pushes
// after it through the import host_push, which the host answers with a
// Response over the stream of stream.js, crates/aosabi-js/src/request.rs.
const MARK_PUSHED = 64;

// The byte after the record of a request the export fetch takes, which names
// what answers it, crates/aosabi-js/src/request.rs: the server of the
// GraphQL endpoint with no router, for the handler of the glue,
// crates/aosabi-js/src/endpoint.rs, or the app of the engine, the routes of
// the plugins of the engine and its not found handler.
export const TO = { ENDPOINT: 0, APP: 1 };

// The forms of the record of iter_next, crates/aosabi-js/src/value.rs: the
// read of a chunk of a reader, the test of the member Symbol.asyncIterator of
// an object and the call of that member.
const ITERATE = { READ: 0, ITERABLE: 1, ITERATOR: 2 };

// The member of a marker of a topic that holds its topic, the number of its
// pub sub, its routing key and its id.
const TOPIC = Symbol("aosabi.topic");

// The job modes and the read modes of the boot record,
// crates/aosabi-js/src/boot.rs.
const MODES = { faithful: 0, fast: 1 };
const READS = { get: 0, project: 1 };

// What answerOf answers for a value of the walk, the value a callback of
// the call of the wrapper receives or an item of an array: a value that holds no
// thenable, a promise of the host of all items of an array, or what a then
// answered, which the walk has not read.
const PLAIN = 0;
const THENABLE = 1;
const ANSWERED = 2;

// Returns the WTF-8 bytes of a string that holds a lone surrogate, which
// TextEncoder would replace by U+FFFD: a pair as its code point and a lone
// surrogate as the three bytes of its code unit, as aosabi_json writes them.
function wtf8(value) {
  const bytes = [];
  for (let i = 0; i < value.length; i++) {
    let unit = value.charCodeAt(i);
    if (unit >= 0xd800 && unit <= 0xdbff && i + 1 < value.length) {
      const low = value.charCodeAt(i + 1);
      if (low >= 0xdc00 && low <= 0xdfff) {
        unit = 0x10000 + ((unit - 0xd800) << 10) + (low - 0xdc00);
        i++;
      }
    }
    if (unit < 0x80) bytes.push(unit);
    else if (unit < 0x800) bytes.push(0xc0 | (unit >> 6), 0x80 | (unit & 0x3f));
    else if (unit < 0x10000) bytes.push(0xe0 | (unit >> 12), 0x80 | ((unit >> 6) & 0x3f), 0x80 | (unit & 0x3f));
    else {
      bytes.push(0xf0 | (unit >> 18), 0x80 | ((unit >> 12) & 0x3f), 0x80 | ((unit >> 6) & 0x3f),
        0x80 | (unit & 0x3f));
    }
  }
  return Uint8Array.from(bytes);
}

// Returns the string of WTF-8 bytes, a lone surrogate kept as its code unit.
function fromWtf8(bytes) {
  try {
    return strict.decode(bytes);
  } catch {
    const units = [];
    for (let i = 0; i < bytes.length;) {
      const lead = bytes[i];
      let point;
      if (lead < 0x80) {
        point = lead;
        i += 1;
      } else if (lead < 0xe0) {
        point = ((lead & 0x1f) << 6) | (bytes[i + 1] & 0x3f);
        i += 2;
      } else if (lead < 0xf0) {
        point = ((lead & 0x0f) << 12) | ((bytes[i + 1] & 0x3f) << 6) | (bytes[i + 2] & 0x3f);
        i += 3;
      } else {
        point = ((lead & 0x07) << 18) | ((bytes[i + 1] & 0x3f) << 12) | ((bytes[i + 2] & 0x3f) << 6) |
          (bytes[i + 3] & 0x3f);
        i += 4;
      }
      if (point >= 0x10000) {
        point -= 0x10000;
        units.push(0xd800 + (point >> 10), 0xdc00 + (point & 0x3ff));
      } else {
        units.push(point);
      }
    }
    let text = "";
    for (let at = 0; at < units.length; at += 4096) text += String.fromCharCode(...units.slice(at, at + 4096));
    return text;
  }
}

// Whether a record defers its short strings to the send, where the module
// strings.wasm writes each into the memory of the engine through the JS String
// Builtins, which the first engine with that module sets (round JSSTRING).
let deferStrings = false;

// The longest string, in code units or bytes, that crosses through strings.wasm.
// A longer one is written into its record through TextEncoder.encodeInto, as
// with no strings.wasm, so the input buffer of a record asks three bytes a code
// unit of no string longer than this, and is read through the strict TextDecoder
// over a view of the memory of the engine, whose paths for ASCII are faster than
// the loop of the module for a long text.
const SHORT = 64;

// The longest input buffer alloc answers, whose length crosses as an i32 of
// the module, so a longer one would reach it wrapped.
const MAX_INPUT = 0xffffffff;

// The buffers of records the host has sent, which the next records take again,
// at most SPARE_BUFFERS of them and each of at most SPARE_BYTES, so the buffer a
// large record grew goes to the collector with its record and holds no memory of
// the isolate after it. 27 of the 28 records of a warm { hello } are 64 bytes or
// shorter.
const spareBuffers = [];
const SPARE_BUFFERS = 32;
const SPARE_BYTES = 4096;

// A growable buffer of the bytes of one record. Where strings are deferred, a
// string of at most SHORT code units takes four bytes of the record, the place
// of its length, and waits in `strings` beside its place until send writes the
// record into the memory of the engine, so its bytes never pass through this
// buffer.
class Out {
  constructor() {
    const spare = spareBuffers.pop();
    if (spare === undefined) {
      this.bytes = new Uint8Array(256);
      this.view = new DataView(this.bytes.buffer);
    } else {
      this.bytes = spare.bytes;
      this.view = spare.view;
    }
    this.length = 0;
    this.strings = undefined;
  }

  // Hands the buffer of the record to the next record, once the host has
  // sent it, and leaves the record with no buffer. A record the host sends
  // again, the boot record a fresh instance boots from, keeps its buffer, and
  // a buffer longer than SPARE_BYTES stays with its record.
  spare() {
    if (this.kept || this.bytes.length > SPARE_BYTES || spareBuffers.length >= SPARE_BUFFERS) return;
    spareBuffers.push({ bytes: this.bytes, view: this.view });
    this.bytes = null;
    this.view = null;
    this.length = 0;
  }

  ensure(more) {
    if (this.length + more <= this.bytes.length) return;
    let size = this.bytes.length * 2;
    while (size < this.length + more) size *= 2;
    const bytes = new Uint8Array(size);
    bytes.set(this.bytes.subarray(0, this.length));
    this.bytes = bytes;
    this.view = new DataView(bytes.buffer);
  }

  byte(value) {
    this.ensure(1);
    this.bytes[this.length++] = value;
  }

  word(value) {
    this.ensure(4);
    this.view.setUint32(this.length, value, true);
    this.length += 4;
  }

  number(value) {
    this.ensure(8);
    this.view.setFloat64(this.length, value, true);
    this.length += 8;
  }

  // A string as its length and its bytes: UTF-8 by TextEncoder.encodeInto
  // when it is well formed, and WTF-8 otherwise, so a lone surrogate crosses
  // as itself. A deferred string, a string of at most SHORT code units, waits
  // for send, and any other value takes the path it took before, which throws
  // for a value that is no string, so a writer meets the throw where it met it
  // with no deferral.
  text(value) {
    if (deferStrings && typeof value === "string" && value.length <= SHORT) {
      this.ensure(4);
      (this.strings ??= []).push(this.length, value);
      this.length += 4;
      return;
    }
    if (!value.isWellFormed()) {
      this.raw(wtf8(value));
      return;
    }
    this.ensure(4 + value.length * 3);
    const { written } = encoder.encodeInto(value, this.bytes.subarray(this.length + 4));
    this.view.setUint32(this.length, written, true);
    this.length += 4 + written;
  }

  raw(bytes) {
    this.ensure(4 + bytes.length);
    this.view.setUint32(this.length, bytes.length, true);
    this.bytes.set(bytes, this.length + 4);
    this.length += 4 + bytes.length;
  }

  // The bytes of the record `other`, with no length before them.
  append(other) {
    this.ensure(other.length);
    if (other.strings !== undefined) {
      const strings = (this.strings ??= []);
      for (let i = 0; i < other.strings.length; i += 2) {
        strings.push(other.strings[i] + this.length, other.strings[i + 1]);
      }
    }
    this.bytes.set(other.bytes.subarray(0, other.length), this.length);
    this.length += other.length;
  }

  // Cuts the record back to its first `at` bytes, with the strings deferred
  // after them.
  truncate(at) {
    this.length = at;
    const strings = this.strings;
    if (strings === undefined) return;
    let keep = strings.length;
    while (keep > 0 && strings[keep - 2] >= at) keep -= 2;
    strings.length = keep;
  }

  // The bytes of the record with every deferred string in its place, as the
  // record reads with no deferral, for an engine with no module strings.wasm.
  flat() {
    if (this.strings === undefined || this.strings.length === 0) return this.bytes.subarray(0, this.length);
    const out = new Out();
    const strings = this.strings;
    let from = 0;
    for (let i = 0; i < strings.length; i += 2) {
      const at = strings[i];
      out.ensure(at - from);
      out.bytes.set(this.bytes.subarray(from, at), out.length);
      out.length += at - from;
      const value = strings[i + 1];
      if (!value.isWellFormed()) {
        out.raw(wtf8(value));
      } else {
        out.ensure(4 + value.length * 3);
        const { written } = encoder.encodeInto(value, out.bytes.subarray(out.length + 4));
        out.view.setUint32(out.length, written, true);
        out.length += 4 + written;
      }
      from = at + 4;
    }
    out.ensure(this.length - from);
    out.bytes.set(this.bytes.subarray(from, this.length), out.length);
    out.length += this.length - from;
    return out.bytes.subarray(0, out.length);
  }
}

// A reader of the fields of a record the module wrote, over a copy of it or
// over the memory of the module from the position `at` of the record, and
// over the memory `memory` of the instance that wrote it, where the record
// names a range the host reads in place, the bytes of a file. Where
// the record stays at the address `base` of that memory while it is read, the
// record of an import, which a vector of the frame that called the import
// holds, a text is read there through the module strings.wasm of the memory,
// and through the strict TextDecoder where that module answers none (round
// JSSTRING).
class In {
  constructor(bytes, memory = undefined, base = -1, at = 0, view = undefined) {
    this.bytes = bytes;
    this.view = view ?? new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
    this.at = at;
    this.memory = memory;
    this.base = memory?.codec ? base : -1;
  }

  byte() {
    return this.bytes[this.at++];
  }

  word() {
    const value = this.view.getUint32(this.at, true);
    this.at += 4;
    return value;
  }

  number() {
    const value = this.view.getFloat64(this.at, true);
    this.at += 8;
    return value;
  }

  // An address of eight little endian bytes, which hold the address of the
  // module and of the native library alike.
  address() {
    const low = this.view.getUint32(this.at, true);
    const high = this.view.getUint32(this.at + 4, true);
    this.at += 8;
    return high * 2 ** 32 + low;
  }

  raw() {
    const length = this.word();
    const bytes = this.bytes.subarray(this.at, this.at + length);
    this.at += length;
    return bytes;
  }

  text() {
    if (this.base >= 0) {
      const length = this.view.getUint32(this.at, true);
      const value = length <= SHORT ? this.memory.codec.decode(this.base + this.at + 4, length) : null;
      if (value !== null) {
        this.at += 4 + length;
        return value;
      }
    }
    return fromWtf8(this.raw());
  }

  // One value record of the module as a plain JavaScript value: a plain
  // object, an object with no prototype, an array, a Date or an error,
  // nested at most DEPTH levels. Each member of an object is an own data
  // property, made as JSON.parse makes one and with no setter of
  // Object.prototype run, so a key __proto__ stays an own key and never sets
  // the prototype of the object. The value of a variable, which the module
  // writes under its name, is made once in `variables`, the values of the
  // variables of the request, and every later record of that variable
  // answers the value made first, as graphql-js hands every place of a
  // variable the one value it coerced. An error is its brand, its name, its
  // message, its path, its locations and its members, the extensions of a
  // GraphQLError and the own members of any other, and becomes an instance of
  // the class its brand names through fromEngine of lib/errors.js. A file of
  // a multipart request becomes a File of the platform with its name, its
  // type and its bytes, which the host reads at their place in the memory of
  // the module, their part of the one copy of the body there, before the
  // call that named them returns, stamped by the clock of the host where the
  // engine read none, and its type is the one the parser of the engine gave
  // it on every host, crates/aosabi-js/src/host.rs. The tags of the
  // request a call runs for read through `scope`: the context c of hono the
  // request runs in and a value of the host the engine holds by its handle,
  // crates/aosabi-js/src/context.rs.
  value(depth = 0, variables = undefined, scope = undefined) {
    if (depth > DEPTH) throw new RangeError("the module wrote a value nested deeper than " + DEPTH + " levels");
    const tag = this.byte();
    switch (tag) {
      case TAG.UNDEFINED: return undefined;
      case TAG.NULL: return null;
      case TAG.FALSE: return false;
      case TAG.TRUE: return true;
      case TAG.NUMBER: return this.number();
      case TAG.STRING: return this.text();
      case TAG.DATE: return new Date(this.number());
      case TAG.LIST: {
        const items = new Array(this.word());
        for (let i = 0; i < items.length; i++) items[i] = this.value(depth + 1, variables, scope);
        return items;
      }
      case TAG.ERROR: {
        const brand = this.byte();
        const name = this.text();
        const message = this.text();
        const path = this.value(depth + 1, variables, scope);
        const locations = this.value(depth + 1, variables, scope);
        const members = this.value(depth + 1, variables, scope);
        return fromEngine(KINDS[brand], name, message, path, locations, members);
      }
      case TAG.PLAIN:
      case TAG.BARE: {
        const object = tag === TAG.BARE ? createObject(null) : {};
        for (let i = this.word(); i > 0; i--) {
          const key = this.text();
          const member = this.value(depth + 1, variables, scope);
          defineProperty(object, key, { __proto__: null, value: member, writable: true, enumerable: true,
            configurable: true });
        }
        return object;
      }
      case TAG.VARIABLE: {
        const name = this.text();
        const value = this.value(depth, variables, scope);
        if (variables === undefined) return value;
        if (variables.has(name)) return variables.get(name);
        variables.set(name, value);
        return value;
      }
      case TAG.CONTEXT:
        if (scope === undefined) break;
        return scope.context();
      case TAG.HELD: {
        const handle = this.word();
        if (scope === undefined) break;
        return scope.held(handle);
      }
      case TAG.EXECUTE: {
        const args = this.word();
        if (scope === undefined) break;
        return scope.execute(args);
      }
      case TAG.DONE: {
        const number = this.word();
        const args = this.word();
        const result = this.outcome(depth, variables, scope, number);
        if (scope === undefined) break;
        return scope.done(number, result, args);
      }
      case TAG.FILE: {
        const name = this.text();
        const type = this.text();
        const lastModified = this.number();
        const address = this.address();
        const length = this.word();
        if (this.memory === undefined) break;
        const bytes = length === 0 ? new Uint8Array(0) : this.memory.copy(address, length);
        const file = new File([bytes], name, lastModified === 0 ? { type } : { type, lastModified });
        FILES.set(file, bytes);
        // Bun writes a charset into the type of a text file it makes, which the
        // parser of undici leaves out, so the file keeps the type the parser of
        // the engine gave it.
        if (file.type !== type) {
          defineProperty(file, "type", { __proto__: null, value: type, writable: false, enumerable: false,
            configurable: true });
        }
        return file;
      }
      default:
        break;
    }
    throw new Error("the module wrote a broken record");
  }

  // One record of JSON data the module wrote, a copy of a member of a result
  // of an execution, at any depth: a primitive, an array and a plain object
  // whose members are own data properties made as JSON.parse makes them, as
  // value reads them, and any other leaf as value reads it. The walk keeps a
  // stack of its own in place of recursion, so a result nested deeper than
  // DEPTH levels reads whole, as a done hook takes the result its executor
  // answered, crates/aosabi-js/src/host.rs, push_json.
  json() {
    const open = [];
    for (;;) {
      const tag = this.byte();
      let value;
      if (tag === TAG.LIST) {
        const length = this.word();
        value = new Array(length);
        if (length > 0) {
          open.push({ object: value, list: true, at: 0, left: length, key: undefined });
          continue;
        }
      } else if (tag === TAG.PLAIN) {
        const count = this.word();
        value = {};
        if (count > 0) {
          open.push({ object: value, list: false, at: 0, left: count, key: this.text() });
          continue;
        }
      } else {
        this.at -= 1;
        value = this.value();
      }
      for (;;) {
        const top = open[open.length - 1];
        if (top === undefined) return value;
        if (top.list) {
          top.object[top.at++] = value;
        } else {
          defineProperty(top.object, top.key, { __proto__: null, value, writable: true, enumerable: true,
            configurable: true });
        }
        top.left -= 1;
        if (top.left > 0) {
          if (!top.list) top.key = this.text();
          break;
        }
        open.pop();
        value = top.object;
      }
    }
  }

  // The outcome of an execution in the payload of a done hook, as the result
  // the hook reads: an object that holds Symbol.asyncIterator for a stream of
  // results, which no execution answers, since a subscription runs
  // through the hooks onSubscribe, which Envelop calls in place of onExecute,
  // so its iteration is a refusal, and otherwise the result with its members
  // in the order the engine holds them, crates/aosabi-js/src/plugin.rs: the
  // errors where the result holds the member, an empty array among them, the
  // data, the extensions and any other member a done hook set before, each
  // an own member as JSON.parse makes one, read at any depth through json.
  outcome(depth, variables, scope, number) {
    if (this.byte() === OUTCOME.STREAM) {
      return {
        [Symbol.asyncIterator]() {
          throw new TypeError("a stream of results of aosabi reaches no hook of a plugin of the app, and an " +
            "iteration over it waits for a fixture that needs it");
        },
      };
    }
    const sources = new Array(this.word());
    for (let i = 0; i < sources.length; i++) {
      const body = this.text();
      const name = this.text();
      const line = this.number();
      const column = this.number();
      sources[i] = { body, name, locationOffset: { line, column } };
    }
    const result = {};
    for (let count = this.word(); count > 0; count--) {
      const kind = this.byte();
      let key;
      let member;
      switch (kind) {
        case MEMBER.ERRORS: {
          key = "errors";
          member = new Array(this.word());
          for (let i = 0; i < member.length; i++) member[i] = this.handedError(depth, variables, scope, sources, number);
          break;
        }
        case MEMBER.DATA:
          key = "data";
          member = this.json();
          break;
        case MEMBER.EXTENSIONS:
          key = "extensions";
          member = this.json();
          break;
        default:
          key = this.text();
          member = this.json();
          break;
      }
      defineProperty(result, key, { __proto__: null, value: member, writable: true, enumerable: true,
        configurable: true });
    }
    return result;
  }

  // An error of the outcome in the payload of the done hook `number`, whose
  // sources `sources` holds: a GraphQLError of the wrapper with the message,
  // the path, the locations and the extensions its formatted error writes,
  // its original error, the error of the app where it threw one, and the
  // nodes, the source and the positions of the error of the engine, each node
  // a stand in with its kind and its location, so a GraphQLError made of its
  // nodes computes the same locations, and in the member ENGINE_ERROR the
  // number of the call, its index among the errors of the outcome and what it
  // held as handed.
  handedError(depth, variables, scope, sources, number) {
    const index = this.word();
    const message = this.text();
    const path = this.json();
    const locations = this.json();
    const extensions = this.json();
    let originalError;
    switch (this.byte()) {
      case ORIGINAL_FORM.KEPT: {
        const handle = this.word();
        originalError = scope?.held(handle)?.error;
        break;
      }
      case ORIGINAL_FORM.ENGINE:
        originalError = this.value(depth + 1, variables, scope);
        break;
      default:
        break;
    }
    let nodes;
    if (this.byte() === 1) {
      nodes = new Array(this.word());
      for (let i = 0; i < nodes.length; i++) {
        const node = createObject(NODE_PROTOTYPE);
        node.kind = this.text();
        let loc;
        if (this.byte() === 1) {
          loc = createObject(LOCATION_PROTOTYPE);
          loc.start = this.number();
          loc.end = this.number();
          loc.source = sources[this.word()];
        }
        node.loc = loc;
        nodes[i] = node;
      }
    }
    const source = this.byte() === 1 ? sources[this.word()] : undefined;
    let positions;
    if (this.byte() === 1) {
      positions = new Array(this.word());
      for (let i = 0; i < positions.length; i++) positions[i] = this.number();
    }
    const error = new GraphQLError(message, { nodes, source, positions, path, originalError, extensions });
    if (locations !== undefined) error.locations = locations;
    defineProperty(error, ENGINE_ERROR, {
      __proto__: null,
      value: { number, index, text: handedText(error), nodes: error.nodes, source: error.source, originalError },
    });
    return error;
  }
}

// Returns the text of what an error of the payload of a done hook holds that the
// writer of Yoga reads, its message, its path, its extensions and its positions,
// the copy JSON reads of them.
function handedText(error) {
  return JSON.stringify([error.message, error.path, error.extensions, error.positions]);
}

// The member `key` of `object` of the table `table`, read for its value. A
// member of a root, an object of the table 0, stays as it is, since the
// service reads its roots with Object.entries and calls a member of a root with no
// this. A function member of any other object is bound to it, as bindIfMethod
// of the resolver wrapper binds a member of a projected object.
function member(table, object, key) {
  const value = object[key];
  return table !== 0 && typeof value === "function" ? value.bind(object) : value;
}

// The answer of a test of `value`, a member a projection read for the test of
// its truth or of its type.
function testOf(value) {
  if (value === undefined) return TESTED.UNDEFINED;
  if (!value) return TESTED.FALSE;
  return typeof value === "function" ? TESTED.FUNCTION : TESTED.TRUE;
}

// Returns whether `value` is an object and not null, isObjectLike of graphql-js.
function isObjectLike(value) {
  return typeof value == "object" && value !== null;
}

// Returns `array`, or undefined for one that is empty or absent, undefinedIfEmpty
// of graphql-js.
function undefinedIfEmpty(array) {
  return array === undefined || array.length === 0 ? undefined : array;
}

// Returns the message of the located error graphql-js makes of an error whose
// message is `message`: the Error constructor leaves an undefined message out
// and takes the string of any other value, which throws on a symbol.
function messageOf(message) {
  if (message === undefined) return "";
  if (typeof message === "symbol") throw new TypeError("Cannot convert a Symbol value to a string");
  return String(message);
}

// Returns the copy of `value` that JSON.stringify reads as the member `key` of
// an object, as Yoga writes the result: toJSON called with the key, every
// getter run once and a function, a symbol and undefined left out, so the copy
// holds plain objects, arrays and primitives alone.
function copyOf(key, value) {
  const text = JSON.stringify({ [key]: value });
  return JSON.parse(text)[key];
}

// Returns `error`, or the error the executor makes of a thrown value
// that is no error, with its reads: an object whose message is a string becomes
// an Error of that message, with its cause, its stack and its name where it
// holds them, and any other value an Error of its string.
function coerceError(error) {
  if (error instanceof Error) return error;
  if (typeof error === "object" && error != null) {
    if ("message" in error && typeof error.message === "string") {
      let errorOptions;
      if ("cause" in error) errorOptions = { cause: error.cause };
      const coercedError = new Error(error.message, errorOptions);
      if ("stack" in error && typeof error.stack === "string") coercedError.stack = error.stack;
      if ("name" in error && typeof error.name === "string") coercedError.name = error.name;
      return coercedError;
    }
  }
  return new Error(String(error));
}

// The largest piece of a chunk of the body of a request the host hands the
// engine at once, so a chunk the platform gives whole crosses through a
// buffer of the module of at most this length.
const PIECE = 65536;

// Returns the flat record of the web standard request `request`,
// crates/aosabi-js/src/request.rs: its method, its URL, its header fields and
// the form of its body, the byte 0 for a GET and a HEAD, which carry no body
// under the Fetch standard, or the byte 1 and the handle in `table`, the
// table of the request, of the reader of its body, which the engine pulls
// once a request parser of Yoga reads the body. The form is decided by the
// method alone and the getter body is not read here, since a stream taken
// from it before a middleware of the app reads c.req.raw.clone() is the one
// the clone tees away, so the reader takes the body of the request at the
// first pull of the engine. The body stays in the stream of the request
// until then, so a route of the app reads it from there and it never crosses.
function recordOf(request, table) {
  const out = new Out();
  out.text(request.method);
  out.text(request.url);
  const fields = [...request.headers];
  out.word(fields.length);
  for (const [name, value] of fields) {
    out.text(name);
    out.text(value);
  }
  if (request.method === "GET" || request.method === "HEAD") {
    out.byte(0);
  } else {
    out.byte(1);
    out.word(table.push(bodyReader(request)) - 1);
  }
  return out;
}

// The member of a reader of a body of the host the engine pulls, which
// cancels the reader once the client of the answer over it went away.
const CANCEL = Symbol("aosabi.cancel");

// Returns the reader of the body of the web standard request `request`, which
// takes the stream of the request at its first read, after any clone the app
// made of the request before it, and hands a chunk longer than PIECE in pieces
// of PIECE bytes, views of the chunk with no copy. A body whose Content-Length
// names at most PIECE bytes is read whole through arrayBuffer at the first read,
// as one piece, and the import iter_next answers the read after it with the end
// of the body at once, with no promise. A request with no body ends at the first
// read, and a request whose body the app has read already fails with the error
// the host's own read of a used body throws, the error Yoga meets where
// it reads the body of c.req.raw.
function bodyReader(request) {
  let reader = null;
  let rest = null;
  let whole = false;
  const piece = (chunk) => {
    if (chunk.length <= PIECE) return chunk;
    rest = chunk.subarray(PIECE);
    return chunk.subarray(0, PIECE);
  };
  return {
    read() {
      if (rest !== null) {
        const chunk = rest;
        rest = null;
        return HostPromise.resolve({ done: false, value: piece(chunk) });
      }
      if (whole) return HostPromise.resolve({ done: true });
      if (reader === null) {
        try {
          if (request.bodyUsed) return apply(promiseThen, request.arrayBuffer(), [() => ({ done: true })]);
          const stream = request.body;
          if (stream === null) return HostPromise.resolve({ done: true });
          const length = Number(request.headers.get("content-length"));
          if (length > 0 && length <= PIECE) {
            whole = true;
            return apply(promiseThen, request.arrayBuffer(), [
              (bytes) => ({ done: false, value: new Uint8Array(bytes) }),
            ]);
          }
          reader = stream.getReader();
        } catch (error) {
          return HostPromise.reject(error);
        }
      }
      return apply(promiseThen, reader.read(), [
        (result) => (result.done ? result : { done: false, value: piece(result.value) }),
      ]);
    },
    // Whether the body was read whole, so the read after it answers its end
    // with no promise, which the import iter_next answers at once.
    get ended() {
      return whole && rest === null;
    },
    [CANCEL]() {
      if (reader !== null) reader.cancel().catch(() => {});
    },
  };
}

// Returns the web standard response of an answer record of `request`. The
// head of an answer whose body the module pushes after it answers with the
// stream of `pushed`, the body of stream.js the pushes of the request fill,
// whose cancel calls `onCancel`, or with its bytes where the body ended
// before the host made the response, and a head that takes no body cancels
// it at once.
function responseOf(bytes, request, pushed, onCancel) {
  let mark;
  let status;
  let body;
  const headers = new Headers();
  if (bytes instanceof Uint8Array) {
    const reader = new In(bytes);
    mark = reader.byte();
    status = reader.view.getUint16(reader.at, true);
    reader.at += 2;
    for (let count = reader.word(); count > 0; count--) {
      const name = reader.text();
      headers.append(name, reader.text());
    }
    body = reader.raw();
  } else {
    // The answer readAnswer read in place, whose strings strings.wasm made.
    ({ mark, status, body } = bytes);
    const fields = bytes.fields;
    for (let i = 0; i < fields.length; i += 2) headers.append(fields[i], fields[i + 1]);
  }
  const empty = request.method === "HEAD" || status === 204 || status === 304;
  if ((mark & MARK_PUSHED) !== 0 && pushed !== undefined) {
    let stream = null;
    if (empty) {
      pushed.onCancel = onCancel;
      pushed.cancel();
    } else if ((mark & MARK_STREAM) === 0 && pushed.ended && pushed.readable === null) {
      // A body that ended within the first turn of the host is a whole body, as
      // the Node adapter writes the length of a body that ended within its first
      // reads.
      stream = pushed.whole();
    } else {
      stream = pushed.stream(onCancel);
    }
    return new Response(stream, { status, headers });
  }
  let payload = empty ? null : body;
  if (!empty && (mark & MARK_TEXT) !== 0 && (mark & MARK_STREAM) === 0) {
    payload = decoder.decode(body);
  }
  if (!empty && (mark & MARK_STREAM) !== 0) {
    payload = new ReadableStream({
      start(controller) {
        if (body.length > 0) controller.enqueue(body);
        controller.close();
      },
    });
  }
  const response = new Response(payload, { status, headers });
  if (typeof payload === "string" || payload instanceof Uint8Array) WHOLE.set(response, payload);
  return response;
}

// The whole bodies of the responses responseOf makes, a string or bytes, by
// the response.
const WHOLE = new WeakMap();

// Returns the body of the response `response` of the core in the form the engine
// wrote it, the string or the bytes of a whole body, which a host writes with
// its length and with the type of a string where the answer names none, as Bun
// writes text/plain;charset=utf-8, or the stream of any other body. A route of a
// plugin of the engine that hono runs answers with it, as a route of hono
// answers the body of a helper of c or the response of an HTTPException.
export function bodyOf(response) {
  return WHOLE.has(response) ? WHOLE.get(response) : response.body;
}

// Returns the answer record of a failure the wrapper writes itself, as the
// module writes its own failures in crates/aosabi-js/src/request.rs: no
// coding mark, the status, no header field and the text as the body. The text
// names the failure of the wrapper and carries no message of the app.
function failureRecord(status, text) {
  const body = encoder.encode(text);
  const bytes = new Uint8Array(11 + body.length);
  const view = new DataView(bytes.buffer);
  view.setUint16(1, status, true);
  view.setUint32(3, 0, true);
  view.setUint32(7, body.length, true);
  bytes.set(body, 11);
  return bytes;
}

// The answer of a request whose instance trapped.
const TRAPPED = failureRecord(500, "the module trapped");

// The answer of a request with a job the host could not queue.
const UNQUEUED = failureRecord(500, "the host could not queue a job of the request");

// The answer of a request whose answer the host could not read.
const UNREAD = failureRecord(500, "the host could not read the answer of the module");

// The answer of a request with a settled value the host could not hand over.
const UNSETTLED = failureRecord(500, "the host could not settle a promise of the request");

// Returns the byte of the boot record for an option of the configuration
// that is true, false or left out.
function flag(value) {
  if (value === undefined) return 0;
  return value ? 1 : 2;
}

// Writes `copy`, data of JSON, nested `depth` levels deep: a primitive as
// itself, an array as a list and a plain object as the tag PLAIN with its keys
// and members, which the module takes as data of no object of the app.
export function writeJson(out, copy, depth = 0) {
  if (depth > DEPTH) throw new RangeError("a value of the app nests deeper than " + DEPTH + " levels");
  switch (typeof copy) {
    case "undefined": out.byte(TAG.UNDEFINED); return;
    case "boolean": out.byte(copy ? TAG.TRUE : TAG.FALSE); return;
    case "number": out.byte(TAG.NUMBER); out.number(copy); return;
    case "string": out.byte(TAG.STRING); out.text(copy); return;
    default: break;
  }
  if (copy === null) {
    out.byte(TAG.NULL);
    return;
  }
  if (Array.isArray(copy)) {
    out.byte(TAG.LIST);
    out.word(copy.length);
    for (const item of copy) writeJson(out, item, depth + 1);
    return;
  }
  const keys = Object.keys(copy);
  out.byte(TAG.PLAIN);
  out.word(keys.length);
  for (const key of keys) {
    out.text(key);
    writeJson(out, copy[key], depth + 1);
  }
}

// Returns the copy of `value` JSON.stringify writes and JSON.parse reads, or
// undefined where JSON.stringify writes nothing, as c.json writes its body.
export function jsonCopy(value) {
  const text = JSON.stringify(value);
  return text === undefined ? undefined : JSON.parse(text);
}

// Writes `copy`, data of JSON, as writeJson writes it, at any depth: the walk
// keeps a stack of its own in place of recursion and no bound of DEPTH, so the
// copy of a result a done hook leaves nested deeper than that crosses whole,
// crates/aosabi-js/src/host.rs, read_json.
function writeJsonDeep(out, copy) {
  const open = [];
  let value = copy;
  for (;;) {
    switch (typeof value) {
      case "undefined": out.byte(TAG.UNDEFINED); break;
      case "boolean": out.byte(value ? TAG.TRUE : TAG.FALSE); break;
      case "number": out.byte(TAG.NUMBER); out.number(value); break;
      case "string": out.byte(TAG.STRING); out.text(value); break;
      default:
        if (value === null) {
          out.byte(TAG.NULL);
        } else if (Array.isArray(value)) {
          out.byte(TAG.LIST);
          out.word(value.length);
          open.push({ object: value, keys: null, at: 0 });
        } else {
          const keys = Object.keys(value);
          out.byte(TAG.PLAIN);
          out.word(keys.length);
          open.push({ object: value, keys, at: 0 });
        }
        break;
    }
    for (;;) {
      const top = open[open.length - 1];
      if (top === undefined) return;
      if (top.keys === null) {
        if (top.at < top.object.length) {
          value = top.object[top.at++];
          break;
        }
      } else if (top.at < top.keys.length) {
        const key = top.keys[top.at++];
        out.text(key);
        value = top.object[key];
        break;
      }
      open.pop();
    }
  }
}

// Answers the policy `policy` pass attached to a patch as data, each field
// with its allowed names as strings and its forced values, and throws on a
// forced value that is a function of the context, which waits for a fixture
// that needs it, since the engine calls no such function.
function policyOf(policy) {
  const out = {};
  for (const [field, fieldPolicy] of Object.entries(policy)) {
    const copy = {};
    if (fieldPolicy?.args != null) copy.args = Array.from(fieldPolicy.args, String);
    if (fieldPolicy?.force != null) {
      const force = {};
      for (const [name, value] of Object.entries(fieldPolicy.force)) {
        if (typeof value === "function") {
          throw new TypeError("a forced value of a policy of aosabi that is a function of the context waits for a " +
            "fixture that needs it");
        }
        force[name] = value;
      }
      copy.force = force;
    }
    out[field] = copy;
  }
  return out;
}

// Calls the hook onExecuteDone of the hooks `hooks` an onExecute of a plugin of
// the app answered with the payload `payload`, as the orchestrator of Envelop
// calls it, a member read once and called as a method of the hooks, and then
// hands the engine the result the hook leaves. A hook that throws throws the
// call, which rejects the execution in the engine as it rejects it in Envelop. A
// hook that answers a thenable, a value whose member then is no null, has its
// then called at once with a callback that hands the engine the result the hook
// leaves once the thenable fulfils, as handleMaybePromise of iterateAsync calls
// it, and the call answers a promise of the host that fulfils after that hand
// over and rejects with the reason of the thenable or with what the hand over
// threw, which the engine awaits before the next done hook runs. Its answer, the
// hooks onNext and onEnd of a stream, takes no effect, as Envelop applies them
// to a stream of results alone, which no execution answers.
function callDone(hooks, payload) {
  const onExecuteDone = hooks.onExecuteDone;
  if (onExecuteDone === undefined || onExecuteDone === null) return undefined;
  const answer = apply(onExecuteDone, hooks, [payload]);
  if (answer?.then == null) {
    payload[FINISH]();
    return undefined;
  }
  return new HostPromise((resolve, reject) => {
    answer.then(() => {
      try {
        payload[FINISH]();
      } catch (error) {
        reject(error);
        return;
      }
      resolve(undefined);
    }, reject);
  });
}

// Returns the function the engine calls the hook `onExecute` of a plugin of the
// app through: the hook with the payload of the engine, called as the
// orchestrator of Envelop calls it, with no this, and the answer the handle of
// the hooks it answered in the table of the request, or undefined where it
// answered none. A hook that answers a promise throws, since the hook of the
// engine runs at once and Envelop awaits it.
function executeOf(onExecute) {
  return (payload) => {
    const hooks = apply(onExecute, undefined, [payload]);
    if (!hooks) return undefined;
    if (typeof hooks.then === "function") {
      throw new TypeError("onExecute of a plugin of aosabi answers its hooks at once, and a promise of them waits " +
        "for a fixture that needs it");
    }
    return payload[KEEP](hooks);
  };
}

// Writes the option `logger` of a configuration as the engine takes it,
// crates/aosabi-js/src/boot.rs and src/log.rs: 0 where it is absent, 2 for
// false, which turns the line of each request off, 1 for any other value
// that is no object, which takes the defaults, and 3 for an object with the
// copy JSON reads of it, its sink left out, since a function of the app
// crosses into no logger of the engine, as configureLogger of the logger
// reads it.
export function writeLoggerOption(record, logger) {
  if (logger === undefined || logger === null) {
    record.byte(0);
  } else if (logger === false) {
    record.byte(2);
  } else if (typeof logger !== "object") {
    record.byte(1);
  } else {
    record.byte(3);
    const { sink: _sink, ...options } = logger;
    writeJson(record, jsonCopy(options));
  }
}

// Writes the plugins of the configuration in their order: a marker by its
// name, the JSON copy of its options and the functions of its options the
// marker hands, each its key and its handle in the table of the boot, 0 for
// a function the engine runs none of, and a plugin of the app by its name,
// its strategy, its roles, its dependencies, its hook onExecute and the names
// of the other hooks it holds, crates/aosabi-js/src/plugin.rs. Its middleware
// and the routes its setup registers run in the hono of the host,
// lib/pylon.js, and cross not.
function writePlugins(record, plugins, handle) {
  record.word(plugins.length);
  for (const plugin of plugins) {
    if (plugin.marker !== undefined) {
      record.byte(0);
      record.text(String(plugin.marker));
      writeJson(record, jsonCopy(plugin.options ?? null));
      const functions = Object.entries(plugin.functions ?? {});
      record.word(functions.length);
      for (const [key, fn] of functions) {
        record.text(key);
        record.word(typeof fn === "function" ? handle(fn) : 0);
      }
      continue;
    }
    record.byte(1);
    if (plugin.name === undefined) {
      record.byte(0);
    } else {
      record.byte(1);
      record.text(String(plugin.name));
    }
    record.byte(plugin.strategy === "first" ? 1 : plugin.strategy === "last" ? 2 : 0);
    if (plugin.roles === undefined) {
      record.byte(0);
    } else {
      record.byte(1);
      record.word(plugin.roles.length);
      for (const role of plugin.roles) record.byte(role === "web" ? 0 : 1);
    }
    if (plugin.dependsOn === undefined) {
      record.byte(0);
    } else {
      record.byte(1);
      record.word(plugin.dependsOn.length);
      for (const name of plugin.dependsOn) record.text(String(name));
    }
    if (!plugin.onExecute) {
      record.byte(EXECUTE_FORM.NONE);
    } else if (plugin.onExecute[ENGINE_HOOK] === "useIdentity") {
      record.byte(EXECUTE_FORM.IDENTITY);
    } else {
      record.byte(EXECUTE_FORM.HOST);
      record.word(handle(executeOf(plugin.onExecute)));
      record.word(handle(callDone));
    }
    const hooks = plugin.hooks ?? [];
    record.word(hooks.length);
    for (const name of hooks) record.text(name);
  }
}

// Returns the boot record of the SDL `schema`, the JSON text `ir` of the IR
// of the same build, the configuration `config`, the two modes, the
// variables `env` of the process of the host and the plan `plan` of the app,
// its plugins and its type resolvers, crates/aosabi-js/src/boot.rs. Each
// function of the plan is the handle `handle` answers for it, an index of the
// table of the boot. The text `ir` may be the schema record of that build in
// place of the IR.
function bootRecord(schema, ir, config, mode, read, env, plan, handle) {
  const record = new Out();
  record.text(schema);
  record.text(ir);
  writeTypeResolvers(record, plan.types ?? []);
  record.byte(flag(config.graphiql));
  record.byte(flag(config.landingPage));
  record.byte(MODES[mode]);
  record.byte(READS[read]);
  record.byte(typeof navigator !== "undefined" && navigator.userAgent?.startsWith("Bun") ? 1 : 0);
  const vars = Object.entries(env).filter(([, value]) => typeof value === "string");
  record.word(vars.length);
  for (const [name, value] of vars) {
    record.text(name);
    record.text(value);
  }
  writeLoggerOption(record, config.logger);
  const sentry = initOptions();
  if (sentry === undefined) {
    record.byte(0);
  } else {
    record.byte(1);
    writeJson(record, jsonCopy(sentry));
  }
  writePlugins(record, plan.plugins, handle);
  return record;
}

// Writes the type resolvers of the form of the canary that typeResolversOf
// of lib/canary.js read off the glue of a build of the canary: a count, and
// each the name of its union or interface, a count of its members and each
// the name of the member, a count and the names of the fields its check
// tests with in, crates/aosabi-js/src/boot.rs.
function writeTypeResolvers(record, types) {
  record.word(types.length);
  for (const { type, checks } of types) {
    record.text(type);
    record.word(checks.length);
    for (const { name, fields } of checks) {
      record.text(name);
      record.word(fields.length);
      for (const field of fields) record.text(field);
    }
  }
}

// Returns a fresh record a module of the package writes a call into.
export function record() {
  return new Out();
}

// The instance of the engine made last, whose logger a call of the app
// outside every request writes through.
let latest = null;

// Returns the bridge of the store of the running call of the app, or of the
// instance made last for a call outside every request, or null before the
// first instance.
export function bridge() {
  return context.getStore()?.bridge ?? latest?.bridgeOf(0) ?? null;
}

// Returns the engine of the compiled module `module`, core.wasm: each open
// instantiates it over the import object `aosabi_host` of the functions of
// the host and answers its exports and the reads and the write of its linear
// memory, each through the views of the buffer the memory holds, which are
// made anew once the memory grew and its buffer was replaced, and a reader of
// a record in place over those views.
export function wasmEngine(module) {
  const strings = module?.[STRINGS];
  return {
    kind: "wasm",
    open(host) {
      const exports = new WebAssembly.Instance(module, { aosabi_host: host }).exports;
      const memory = exports.memory;
      let buffer = null;
      let bytes = null;
      let view = null;
      const fresh = () => {
        if (memory.buffer !== buffer) {
          buffer = memory.buffer;
          bytes = new Uint8Array(buffer);
          view = new DataView(buffer);
        }
      };
      const engine = {
        exports,
        codec: codecOf(strings, memory),
        write: (at, source) => {
          fresh();
          bytes.set(source, at);
        },
        copy: (at, length) => {
          fresh();
          return bytes.slice(at, at + length);
        },
        word: (at) => {
          fresh();
          return view.getUint32(at, true);
        },
        buffer: () => memory.buffer,
        place: (at) => {
          fresh();
          return new In(bytes, engine, 0, at, view);
        },
      };
      return engine;
    },
  };
}

// The member of the compiled module core.wasm under which a host hands the
// compiled module strings.wasm beside it, round JSSTRING: aosabi/worker sets
// it where Wrangler uploads that module, and no other host sets it.
export const STRINGS = Symbol.for("aosabi.strings");

// Returns the exports encode and decode of an instance of the compiled module
// `strings`, strings.wasm, over the memory `memory` of an instance of
// core.wasm, or null where no host handed that module or the host compiled it
// without the builtins js-string of V8, whose imports then go unmet, so the
// wrapper takes TextEncoder and TextDecoder as before. Once an instance has
// the module, every record defers its strings to the send, which writes them
// into the memory through encode, tools/jsstring/strings-wasm.mjs.
function codecOf(strings, memory) {
  if (!(strings instanceof WebAssembly.Module) || !(memory instanceof WebAssembly.Memory)) return null;
  try {
    const codec = new WebAssembly.Instance(strings, { aosabi: { memory } }).exports;
    deferStrings = true;
    return codec;
  } catch {
    return null;
  }
}

// Returns the coding mark of an answer, the first byte of its record or the
// member mark of the answer readAnswer read, 0 for an empty record.
function markOf(answer) {
  if (answer instanceof Uint8Array) return answer.length > 0 ? answer[0] : 0;
  return answer.mark;
}

// Makes and boots one instance of the engine `engine` for the app whose
// graphql object the copy `graphql` stands for, with the boot record `boot`,
// and returns its state.
function instantiate(engine, graphql, boot, functions = []) {
  return bootApp(open(engine), graphql, boot, functions);
}

// Boots the app whose graphql object the copy `graphql` stands for, with the
// boot record `boot` and the functions `functions` of the table of the boot,
// over the instance `state` that open made, and returns its state.
function bootApp(state, graphql, boot, functions) {
  const table = state.tables.get(0);
  const graphqlHandle = table.push(graphql) - 1;
  for (const fn of functions) table.push(fn);
  const failed = state.call("boot", state.send(boot), graphqlHandle);
  if (failed !== 0) throw new Error(decoder.decode(state.answerAt(failed)));
  state.booted = true;
  latest = state;
  return state;
}

// The handle of no graphql object, which boots the logger of the engine alone,
// crates/aosabi-js/src/boot.rs.
const NO_APP = 0xffffffff;

// Opens one instance of the engine `engine`, the engine of aosabi/bun or of
// the compiled module core.wasm `module`, and boots its logger alone with the
// variables `env` of the process of the host, before the glue of the app
// runs, so a logger call at the load of a module of the app or in the setup
// of a plugin writes through the engine, and returns its state, over which
// createCore boots the app when its host boots it. The record is the byte of
// the platform of the host and the variables of the process.
export function primeCore({ module, engine }, env) {
  const state = open(engine ?? wasmEngine(module));
  const record = new Out();
  record.byte(typeof navigator !== "undefined" && navigator.userAgent?.startsWith("Bun") ? 1 : 0);
  const vars = Object.entries(env ?? processEnv()).filter(([, value]) => typeof value === "string");
  record.word(vars.length);
  for (const [name, value] of vars) {
    record.text(name);
    record.text(value);
  }
  const failed = state.call("boot", state.send(record), NO_APP);
  if (failed !== 0) throw new Error(decoder.decode(state.answerAt(failed)));
  latest = state;
  return state;
}

// The time of the last turn of the loop of the host a logger call saw, and
// whether a turn is pending, which a task of the host marks.
let turns = 0;
let seen = 0;
let pending = false;

// Answers whether the loop of the host turned since the last logger call, and
// asks the host for a task that marks the next turn. The lazy sink of the
// development format of the engine writes the fallback line until the loop turns
// after its first record, as the sink waits for the dynamic import of its
// pretty module, which Node resolves from the file system when its loop turns. A
// host that takes no task here, the global scope of a Worker among them, marks
// no turn, and the requests of the engine mark theirs.
export function loopTurned() {
  const turned = turns !== seen;
  seen = turns;
  if (!pending) {
    try {
      later(() => {
        pending = false;
        turns += 1;
      }, 0);
      pending = true;
    } catch {
      // No task here, so no turn is marked.
    }
  }
  return turned;
}

// Opens one instance of the engine `engine` and returns its state, with no
// app booted on it.
function open(engine) {
  const state = {
    exports: null,
    memory: null,
    dead: null,
    tables: new Map([[0, []]]),
    failures: new Map(),
    contexts: new Map(),
    nexts: new Map(),
    variables: new Map(),
    waiting: new Map(),
    done: new Map(),
    inside: 0,
    carried: [],
    awaited: new WeakMap(),
    fields: new Map(),
    alls: new WeakMap(),
    mapped: new WeakMap(),
    pushed: new Map(),
    timers: new Map(),
    clients: new Map(),
    executions: new Map(),
    gateways: new Set(),
    jobs: new Map(),
    scopes: new Map(),
    bridges: new Map(),
  };

  // Calls the export `name`. An export throws only when the instance traps,
  // so a throw condemns the instance before it travels on.
  const call = (name, ...args) => {
    try {
      return state.exports[name](...args);
    } catch (error) {
      condemn(error);
      throw error;
    }
  };

  // Returns the microtask that runs one job of `request` through the export
  // run_job, made at the first job of the request and kept until the host
  // drops it, since it reads the state of the request when it runs. The job
  // is queued as the reaction of a promise fulfilled already, which V8 queues
  // in the one queue of microtasks queueMicrotask queues in, at the same
  // place, with no call into the host.
  const jobOf = (request) => {
    let job = state.jobs.get(request);
    if (job === undefined) {
      job = () => {
        if (state.dead || !state.tables.has(request)) return;
        try {
          call("run_job", request);
        } catch {
          // The instance trapped, and condemn answered the requests on it.
        }
      };
      state.jobs.set(request, job);
    }
    return job;
  };

  // Marks the instance dead, so it is never called again, answers every
  // request that waits on it with the failure of a trap, fails every body it
  // pushes with that error and clears every timer it asked for.
  const condemn = (error) => {
    if (state.dead) return;
    state.dead = error instanceof Error ? error : new Error("the module trapped");
    for (const resolve of state.waiting.values()) resolve(TRAPPED);
    state.waiting.clear();
    state.done.clear();
    for (const body of state.pushed.values()) body.fail(state.dead);
    state.pushed.clear();
    for (const timers of state.timers.values()) {
      for (const timer of timers) unlater(timer);
    }
    state.timers.clear();
  };

  // Hands `request` the answer record `bytes`: to the fetch that waits on it,
  // or to the fetch whose export has not returned yet, which finds it when
  // that export returns 0.
  const answerWith = (request, bytes) => {
    if (!state.tables.has(request)) return;
    const resolve = state.waiting.get(request);
    if (resolve === undefined) state.done.set(request, bytes);
    else resolve(bytes);
  };

  // Returns a reader of the record at `address` of the memory of the engine,
  // `length` bytes long: in place where the engine answers views of its
  // memory, read whole before the next call into the engine, which may grow
  // the memory, and over a copy where it answers none, the engine of
  // aosabi/bun.
  const inPlace = (address, length) => {
    if (state.memory.place === undefined) return new In(copy(address, length), state.memory, address);
    return state.memory.place(address);
  };

  // Copies the record `out` into the input buffer of the engine and returns
  // its length, and hands the buffer of the record to the next record. This
  // is the one place the wrapper calls alloc and writes the memory of the
  // engine, into the range alloc has just answered for the record, through
  // the views of its memory.
  //
  // A record with deferred strings is written in its pieces: the bytes before
  // each string, then its length and its bytes, which encode of strings.wasm
  // writes into the memory, the input buffer sized for three bytes a code
  // unit of each, at most SHORT units, so no deferred string passes through a
  // buffer of the wrapper, and its length is what the record holds. An engine
  // with no codec takes the flat record. A record whose input buffer would
  // pass MAX_INPUT throws a RangeError before alloc, which would take its
  // length wrapped.
  const send = (out) => {
    const strings = out.strings;
    const pieces = strings !== undefined && strings.length !== 0;
    const codec = pieces ? state.memory.codec : null;
    const flat = pieces && !codec ? out.flat() : null;
    let length = flat === null ? out.length : flat.length;
    if (codec) for (let i = 1; i < strings.length; i += 2) length += strings[i].length * 3;
    if (length > MAX_INPUT) {
      throw new RangeError("a record of " + length + " bytes passes the " + MAX_INPUT + " bytes alloc takes");
    }
    const at = call("alloc", length);
    if (at === 0 && length !== 0) throw new RangeError("the module made no buffer of " + length + " bytes");
    if (codec) length = writeStrings(out, strings, codec, at);
    else if (length !== 0) state.memory.write(at, flat ?? out.bytes.subarray(0, length));
    out.spare();
    return length;
  };

  // Writes the record `out` with its deferred strings `strings`, each of at
  // most SHORT code units, in its pieces at `at`, the range alloc has just
  // answered for it in send, and returns the length of the record written.
  const writeStrings = (out, strings, codec, at) => {
    const buffer = state.memory.buffer();
    const memory = new Uint8Array(buffer);
    const view = new DataView(buffer);
    const bytes = out.bytes;
    let to = at;
    let from = 0;
    for (let i = 0; i < strings.length; i += 2) {
      const place = strings[i];
      if (place > from) {
        memory.set(bytes.subarray(from, place), to);
        to += place - from;
      }
      const written = codec.encode(strings[i + 1], to + 4);
      view.setUint32(to, written, true);
      to += 4 + written;
      from = place + 4;
    }
    if (out.length > from) {
      memory.set(bytes.subarray(from, out.length), to);
      to += out.length - from;
    }
    return to - at;
  };

  // Returns a copy of `length` bytes at `address` of the memory of the
  // engine, made before any code of the app runs.
  const copy = (address, length) => (length === 0 ? new Uint8Array(0) : state.memory.copy(address, length));

  // Returns a copy of the answer at `address`, a length and the record.
  const answerAt = (address) => copy(address + 4, state.memory.word(address));

  // Returns the answer record of a request at `address` of the output buffer,
  // read there before the next call: with a codec its mark, its status, its
  // header fields as strings that decode of strings.wasm made in place, and
  // a copy of its body, and with none a copy of the record, as answerAt.
  const readAnswer = (address) => {
    const codec = state.memory.codec;
    if (!codec) return answerAt(address);
    const buffer = state.memory.buffer();
    const memory = new Uint8Array(buffer);
    const view = new DataView(buffer);
    let at = address + 4;
    const mark = memory[at];
    const status = view.getUint16(at + 1, true);
    const fields = new Array(view.getUint32(at + 3, true) * 2);
    at += 7;
    for (let i = 0; i < fields.length; i++) {
      const length = view.getUint32(at, true);
      at += 4;
      fields[i] = (length <= SHORT ? codec.decode(at, length) : null) ?? fromWtf8(memory.subarray(at, at + length));
      at += length;
    }
    const length = view.getUint32(at, true);
    at += 4;
    return { mark, status, fields, body: memory.slice(at, at + length) };
  };

  // Returns the values of the variables of `request` the module has written
  // so far, by name.
  const variablesOf = (request) => {
    let variables = state.variables.get(request);
    if (variables === undefined) {
      variables = new Map();
      state.variables.set(request, variables);
    }
    return variables;
  };

  const tableOf = (request) => {
    let table = state.tables.get(request);
    if (table === undefined) {
      table = [];
      state.tables.set(request, table);
    }
    return table;
  };

  // Returns the body the module pushes for `request`, made at the first push
  // or when the host takes the head, whichever comes first.
  const pushedOf = (request) => {
    let body = state.pushed.get(request);
    if (body === undefined) {
      body = new PushedBody();
      state.pushed.set(request, body);
    }
    return body;
  };

  // Returns the context of a call the module wrote at the cursor of
  // `reader`: the locale of @inContext, whether the request context of the
  // engine is in place, the two numbers of the logger getLogger() answers
  // there and the number of the info of the resolver the call runs in,
  // crates/aosabi-js/src/value.rs, push_context.
  const readCall = (reader) => {
    const locale = reader.value();
    const bound = reader.byte() === 1;
    const logger = [reader.word(), reader.word()];
    const info = reader.word();
    return { locale, bound, logger, info };
  };

  // The context of a call that reads an error, inside the request context
  // with no locale, the scoped logger and no info.
  const ERROR_CALL = { locale: undefined, bound: true, logger: undefined, info: 0 };

  // Runs `body` inside the store of a call of `request` with the context
  // `call` of readCall. The store holds the c of hono the request runs in
  // where the request context of the engine is in place, which getContext()
  // answers, as the store holds the context behind the async context bind
  // of its base pipeline alone, the locale of @inContext, the logger and the
  // info of the call, and the bridge of the request to the engine. The boot
  // runs outside of any store, as the service reads the roots when it
  // starts.
  const within = (request, call, body) => {
    const own = state.contexts.get(request);
    if (own === undefined) return body();
    return context.run({
      context: call.bound ? own : undefined,
      request,
      bridge: bridgeOf(request),
      locale: call.locale,
      logger: call.logger,
      info: call.info,
    }, body);
  };

  // Returns the scope of the reader for `request`: the c of hono the request
  // runs in, a value of the host by its handle, the payloads of the hooks of
  // a plugin of the app with the number of the arguments of their execution,
  // and a call of an op of the views of lib/nodes.js with the values of the
  // variables of the request. The scope
  // is made at the first read of the request and kept until the host drops
  // the request, since each of its functions reads the state of the request
  // when it runs, so no read makes its functions anew. A read after the host
  // dropped the request, the member args of a payload the app kept among
  // them, takes a scope made for that read alone, which nothing keeps.
  const scopeOf = (request) => {
    let scope = state.scopes.get(request);
    if (scope === undefined) {
      scope = newScope(request);
      if (state.tables.has(request)) state.scopes.set(request, scope);
    }
    return scope;
  };
  const newScope = (request) => ({
    context: () => state.contexts.get(request),
    held: (handle) => tableOf(request)[handle],
    execute: (args) => executePayload(request, args),
    done: (number, result, args) => donePayload(request, number, result, args),
    variables: () => variablesOf(request),
    call: (op, write) => {
      const out = new Out();
      out.byte(op);
      write(out);
      return ctxCall(request, out);
    },
  });

  // Returns the payload of the hook onExecute of a plugin of the app for
  // `request`, with the members of the payload of Envelop in their order. The
  // member args is the view of the arguments `args` of the execution of
  // lib/nodes.js, the one object the payload of onExecuteDone hands as well.
  // Each other member is a refusal that names the waiting where the app calls or
  // reads it, since the engine runs the execution itself, and the member KEEP,
  // which enumerates not, keeps the hooks the app answers in the table of the
  // request.
  const executePayload = (request, args) => {
    const refusal = (member) => () => {
      throw new TypeError("the member " + member + " of the payload of onExecute of aosabi waits for a fixture " +
        "that needs it");
    };
    const payload = {
      executeFn: refusal("executeFn"),
      setExecuteFn: refusal("setExecuteFn"),
      setResultAndStopExecution: refusal("setResultAndStopExecution"),
      get context() {
        return refusal("context")();
      },
      extendContext: refusal("extendContext"),
      get args() {
        return argsOf(scopeOf(request), args);
      },
    };
    defineProperty(payload, KEEP, { __proto__: null, value: (hooks) => tableOf(request).push(hooks) - 1 });
    return payload;
  };

  // Returns the payload of the hook onExecuteDone of a plugin of the app for the
  // call `number` of `request`, {args, result, setResult} as Envelop hands it:
  // the outcome of the engine as the result, and setResult, which sets the
  // result the hook leaves while the hook runs. The member FINISH, which
  // enumerates not, hands the engine the result the hook leaves once it
  // returned, or once the thenable it answered fulfilled, the one setResult set
  // last or the result the payload handed as the hook left it, so a change the
  // hook makes in place reaches the engine as it reaches Yoga, which reads the
  // one object Envelop holds after the hooks ran. A stream the hook left as it
  // is stays the engine's. The arguments of the execution are the view `args` of
  // lib/nodes.js, the one object the payload of onExecute hands as well.
  const donePayload = (request, number, result, args) => {
    let left = result;
    let open = true;
    const payload = {
      get args() {
        return argsOf(scopeOf(request), args);
      },
      result,
      setResult: (value) => {
        if (open) left = value;
      },
    };
    defineProperty(payload, FINISH, {
      __proto__: null,
      value: () => {
        open = false;
        if (left === result && Symbol.asyncIterator in result) return;
        const out = new Out();
        out.byte(CTX.SET_RESULT);
        out.word(number);
        writeSetResult(out, left, number);
        ctxCall(request, out);
      },
    });
    return payload;
  };

  // Writes the result `value` the done hook `number` leaves as the engine takes
  // it, crates/aosabi-js/src/plugin.rs: its members in the order of its own
  // keys, as JSON.stringify writes them and Yoga writes the object: the errors
  // where the member holds an array, each an error the payload handed by its
  // index or a GraphQLError of the app by its formatted error, the data, the
  // extensions where they hold an object, and any other member, errors and
  // extensions that hold null among them, by the copy JSON reads of it at any
  // depth through writeJsonDeep, a member whose copy is undefined left out. A
  // value that is no object, an array, a stream, a result with a member
  // stringify or toJSON, through which Yoga writes it, errors that are no array
  // and no null, extensions that are no object and no null, and an error that is
  // no GraphQLError, an error the payload handed that the hook changed in place
  // or a GraphQLError of the app whose original error the mask masks throw a
  // TypeError that names the waiting, since the result of the engine holds no
  // form for them.
  const writeSetResult = (out, value, number) => {
    if (value === null || typeof value !== "object" || Array.isArray(value)) {
      throw new TypeError("setResult of aosabi takes a result of an execution, an object");
    }
    if (Symbol.asyncIterator in value) {
      throw new TypeError("setResult of aosabi takes a result of an execution, and a stream of results waits " +
        "for a fixture that needs it");
    }
    if (value.stringify || typeof value.toJSON === "function") {
      throw new TypeError("setResult of aosabi takes a result that writes itself through stringify or toJSON " +
        "with a fixture that needs it");
    }
    const members = [];
    for (const key of Object.keys(value)) {
      const member = value[key];
      if (key === "errors" && Array.isArray(member)) {
        members.push([MEMBER.ERRORS, key, member]);
        continue;
      }
      const copy = copyOf(key, member);
      if (copy === undefined) continue;
      if (key === "data") {
        members.push([MEMBER.DATA, key, copy]);
      } else if (key === "extensions" && copy !== null) {
        if (typeof copy !== "object" || Array.isArray(copy)) {
          throw new TypeError("setResult of aosabi takes the extensions of a result as an object or null");
        }
        members.push([MEMBER.EXTENSIONS, key, copy]);
      } else if (key === "errors" && copy !== null) {
        throw new TypeError("setResult of aosabi takes the errors of a result as an array or null");
      } else {
        members.push([MEMBER.OTHER, key, copy]);
      }
    }
    out.word(members.length);
    for (const [kind, key, member] of members) {
      out.byte(kind);
      if (kind === MEMBER.ERRORS) {
        writeSetErrors(out, member, number);
      } else if (kind === MEMBER.OTHER) {
        out.text(key);
        writeJsonDeep(out, member);
      } else {
        writeJsonDeep(out, member);
      }
    }
  };

  // Writes the errors `errors` of a result the done hook `number` leaves: an
  // error its payload handed as it was handed by its index, and a GraphQLError
  // of the app by its formatted error, whose original errors down its chain are
  // GraphQLErrors to the last, so the mask of Yoga keeps it, as the engine keeps
  // an error of the app it writes.
  const writeSetErrors = (out, errors, number) => {
    out.word(errors.length);
    for (const error of errors) {
      const handed = error?.[ENGINE_ERROR];
      if (handed !== undefined && handed.number === number) {
        if (handedText(error) !== handed.text || error.nodes !== handed.nodes || error.source !== handed.source ||
            error.originalError !== handed.originalError) {
          throw new TypeError("setResult of aosabi takes an error of the result as the payload handed it, and a " +
            "change of its message, its path, its extensions, its nodes, its source, its positions or its " +
            "original error in place waits for a fixture that needs it, while a new GraphQLError in its place " +
            "serves");
        }
        out.byte(SET_ERROR.ENGINE);
        out.word(handed.index);
      } else if (brandOf(error) === GRAPHQL) {
        for (let original = error.originalError; original != null; original = original.originalError) {
          if (brandOf(original) !== GRAPHQL) {
            throw new TypeError("setResult of aosabi takes a GraphQLError of the app whose original errors are " +
              "GraphQLErrors, and one the mask masks waits for a fixture that needs it");
          }
        }
        const formatted = jsonCopy(error);
        out.byte(SET_ERROR.APP);
        out.text(String(formatted?.message ?? ""));
        writeJsonDeep(out, formatted?.path);
        writeJsonDeep(out, formatted?.locations);
        writeJsonDeep(out, formatted?.extensions);
      } else {
        throw new TypeError("setResult of aosabi takes errors of the class GraphQLError, and any other error " +
          "waits for a fixture that needs it");
      }
    }
  };

  // Returns the client of Sentry of `request`, the Toucan of lib/toucan.js
  // over the client the engine keeps for the request, one object for the
  // request, which the middleware of lib/pylon.js sets under the key sentry
  // of c as @hono/sentry sets its one Toucan.
  const sentryOf = (request) => {
    let client = state.clients.get(request);
    if (client === undefined) {
      const call = (act, number, write) => {
        const out = new Out();
        out.byte(CTX.SENTRY);
        out.byte(act);
        out.word(number);
        if (write !== undefined) write(out);
        return ctxCall(request, out);
      };
      client = new Toucan(call, (out, value) => writeJson(out, jsonCopy(value)));
      state.clients.set(request, client);
    }
    return client;
  };

  // Calls the op of `request` the record `out` names through the export ctx
  // and answers its value, a view of lib/nodes.js where the engine answers a
  // view record, or throws what it threw.
  const ctxCall = (request, out) => {
    if (state.dead) throw new Error("the module trapped");
    const address = call("ctx", request, send(out));
    const reader = inPlace(address + 4, state.memory.word(address));
    if (reader.byte() === NORMAL) {
      if (reader.byte() === VIEW) return readView(reader, scopeOf(request));
      return reader.value(0, undefined, scopeOf(request));
    }
    throw reader.value(0, undefined, scopeOf(request));
  };

  // Calls the logger the record `out` names through the export logger and
  // answers its value, or throws what it threw.
  const logCall = (out) => {
    if (state.dead) throw new Error("the module trapped");
    const address = call("logger", send(out));
    const reader = inPlace(address + 4, state.memory.word(address));
    const kind = reader.byte();
    const value = reader.value();
    if (kind === NORMAL) return value;
    throw value;
  };

  // The bridge of `request` to the engine, which the entries of the package
  // reach through the store of a call: the logger, the store of
  // getResolveInfo() and a delegation of a federation. The bridge is made at
  // the first call of the request and kept until the host drops the request,
  // so no store of a call makes its functions anew.
  const bridgeOf = (request) => {
    let bridge = state.bridges.get(request);
    if (bridge === undefined) {
      bridge = newBridge(request);
      state.bridges.set(request, bridge);
    }
    return bridge;
  };
  const newBridge = (request) => ({
    request,
    log: logCall,
    info: (number) => {
      const out = new Out();
      out.byte(CTX.RESOLVE_INFO);
      out.word(number);
      return ctxCall(request, out);
    },
    delegate: (gateway, key, args, needs, guard, info) => delegateOf(request, gateway, key, args, needs, guard, info),
  });

  // Starts the delegation of the federation `gateway` of the app under the
  // key `key`, with the data `args` and `needs` and the guard `guard`, in
  // the call of `request` whose info is `info`, and answers its marker,
  // crates/aosabi-js/src/federation.rs. The first delegation of a federation
  // in an instance hands the engine its configuration, so an instance made
  // anew after a trap is handed it again.
  const delegateOf = (request, gateway, key, args, needs, guard, info) => {
    if (state.dead) throw new Error("the module trapped");
    const out = new Out();
    out.byte(CTX.FEDERATION);
    out.word(info);
    const number = gateway[GATEWAY];
    out.word(number);
    if (state.gateways.has(number)) {
      out.byte(0);
    } else {
      out.byte(1);
      writeGateway(out, gateway);
    }
    out.text(key);
    writeData(out, args);
    writeData(out, needs);
    if (guard) {
      out.byte(1);
      out.word(tableOf(request).push(guard) - 1);
    } else {
      out.byte(0);
    }
    const at = ctxCall(request, out);
    state.gateways.add(number);
    return new Delegation(state, request, at);
  };

  // Writes the configuration of the federation `gateway`: its URL, strict, its
  // headers function and its patches in the order of their keys, each function
  // of the app in the table of the boot, which lives as long as the instance, as
  // a function of the wrapper the engine calls. The headers function is called
  // as the base pipeline calls it, a method of the configuration with the context of the
  // request or none, and its object answers as the pairs of a spread with each
  // value as String writes it. A patch is called with the object and the context
  // of its federation, (data, api), and the patch of passthrough() is the
  // identity the engine answers itself. The policy pass attaches to a patch
  // crosses as data, and a forced value that is a function of the context waits
  // for a fixture that needs it.
  const writeGateway = (out, gateway) => {
    const config = gateway.config ?? {};
    const boot = state.tables.get(0);
    out.text(String(config.url));
    out.byte(config.strict ? 1 : 0);
    const headers = config.headers;
    if (headers) {
      out.byte(1);
      out.word(boot.push((c) => Object.entries({ ...apply(headers, config, [c]) })
        .map(([name, value]) => [name, String(value)])) - 1);
      out.word(identityOf(headers));
    } else {
      out.byte(0);
    }
    const patches = config.patches ? Object.entries(config.patches) : [];
    out.word(patches.length);
    for (const [typeName, patch] of patches) {
      out.text(typeName);
      if (patch?.[PASSTHROUGH] === true) {
        out.byte(1);
      } else {
        out.byte(0);
        out.word(boot.push((data) => patch(data, gateway.apiContext)) - 1);
      }
      const policy = patch?.[POLICY];
      if (policy) {
        out.byte(1);
        writeData(out, policyOf(policy));
      } else {
        out.byte(0);
      }
    }
  };

  // Writes `root`, data of the app a delegation hands the engine: a primitive as
  // itself, an array item by item, a file the module handed the app, the file of
  // an upload of its request, as the upload it was, and any other object, a Date
  // among them, by its own enumerable keys, which a Date holds none of, while a
  // bigint, a symbol and a function cross as the string String writes, as the
  // builder of the syntax tree of the reference reads each. Among the arguments
  // of a delegation the reference sends as an upload every object isBinary
  // holds, one with the members arrayBuffer and stream as functions and size as
  // a number, so a File or a Blob the app makes, a slice of an upload and a blob
  // of a fetch as well, while this walk writes a file of FILES alone as an
  // upload, since the bytes of any other are read with an await, which the
  // marker of a delegation does not cross. Such an object crosses by its own
  // enumerable keys, which a File holds none of, so the remote reads {} and not
  // the file, a difference that waits for a fixture that needs it.
  // The walk keeps a stack of its own and refuses data nested deeper than
  // DATA_DEPTH levels, where the reference ends its stack.
  const writeData = (out, root) => {
    const open = [];
    let value = root;
    for (;;) {
      if (open.length > DATA_DEPTH) throw new RangeError("Maximum call stack size exceeded");
      switch (typeof value) {
        case "undefined": out.byte(TAG.UNDEFINED); break;
        case "boolean": out.byte(value ? TAG.TRUE : TAG.FALSE); break;
        case "number": out.byte(TAG.NUMBER); out.number(value); break;
        case "string": out.byte(TAG.STRING); out.text(value); break;
        case "object": {
          if (value === null) {
            out.byte(TAG.NULL);
            break;
          }
          const bytes = FILES.get(value);
          if (bytes !== undefined) {
            out.byte(TAG.FILE);
            out.text(String(value.name));
            out.text(String(value.type));
            out.number(Number(value.lastModified));
            out.raw(bytes);
          } else if (Array.isArray(value)) {
            out.byte(TAG.LIST);
            out.word(value.length);
            open.push({ object: value, keys: null, at: 0 });
          } else {
            const keys = Object.keys(value);
            out.byte(TAG.PLAIN);
            out.word(keys.length);
            open.push({ object: value, keys, at: 0 });
          }
          break;
        }
        default: out.byte(TAG.STRING); out.text(String(value)); break;
      }
      let next = false;
      while (!next) {
        const top = open[open.length - 1];
        if (top === undefined) return;
        if (top.keys === null) {
          if (top.at < top.object.length) {
            value = top.object[top.at++];
            next = true;
          }
        } else if (top.at < top.keys.length) {
          const key = top.keys[top.at++];
          out.text(key);
          value = top.object[key];
          next = true;
        }
        if (!next) open.pop();
      }
    }
  };

  // Returns the promise of the claims of `request` a provider of
  // aosabi/auth/zitadel answers over the issuer `issuer` and the key path
  // `keyPath`, crates/aosabi-js/src/zitadel.rs. The promise is made and kept
  // under a number of its own before the call, since the import js_next may
  // settle it before the export ctx returns, and the numbers of the
  // continuations of a chain, which count from 1, never reach it.
  let claimNumbers = CLAIM_NUMBERS;
  const claimsOf = (request, issuer, keyPath) => {
    if (state.dead) return HostPromise.reject(new Error("the module trapped"));
    const number = claimNumbers;
    claimNumbers = claimNumbers >= 0x7ffffffe ? CLAIM_NUMBERS : claimNumbers + 1;
    let resolve;
    let reject;
    const promise = new HostPromise((fulfil, fail) => {
      resolve = fulfil;
      reject = fail;
    });
    let nexts = state.nexts.get(request);
    if (nexts === undefined) {
      nexts = new Map();
      state.nexts.set(request, nexts);
    }
    nexts.set(number, { resolve, reject });
    try {
      const out = new Out();
      out.byte(CTX.ZITADEL_CLAIMS);
      out.text(String(issuer));
      if (keyPath === undefined) {
        out.byte(0);
      } else {
        out.byte(1);
        out.text(String(keyPath));
      }
      out.word(number);
      ctxCall(request, out);
    } catch (error) {
      nexts.delete(number);
      reject(error);
    }
    return promise;
  };
  state.bridgeOf = bridgeOf;

  // Writes the value record of `value`, whose handles go to the table of
  // `request`. The checks run in the order of the resolver wrapper: a primitive,
  // a Date, a thenable, an array and a function, and any other object stays here
  // behind a handle. An array is mapped as step 4 maps it, through mapList, and
  // crosses as the mapped array, or as the promise of all its items that the
  // wrapper owns when one of them is a thenable, and an array mapList has mapped
  // crosses as its mapping, its then read no more. A walk that throws, by a
  // getter of then, a then, a revoked Proxy or a value nested deeper than DEPTH
  // levels, a cyclic array among them, throws to the import that called it. A
  // value the walk takes as it is, a thrown value, which wrapResolver never
  // walks, among them, is written with `raw` true, so the walk reads and calls
  // no then of it and maps no array of it, and a value whose then the walk has
  // read already and found no function with `checked` true. An error crosses as
  // its handle alone, which the module reads through the import js_error.
  const writeValue = (out, value, request, depth, raw = false, checked = false) => {
    if (depth > DEPTH) throw new RangeError("a value of the app nests deeper than " + DEPTH + " levels");
    switch (typeof value) {
      case "undefined": out.byte(TAG.UNDEFINED); return;
      case "boolean": out.byte(value ? TAG.TRUE : TAG.FALSE); return;
      case "number": out.byte(TAG.NUMBER); out.number(value); return;
      case "string": out.byte(TAG.STRING); out.text(value); return;
      case "bigint":
      case "symbol":
        out.byte(TAG.FOREIGN);
        out.byte(typeof value === "symbol" ? SYMBOL : BIGINT);
        out.text(String(value));
        return;
      default: break;
    }
    if (value === null) {
      out.byte(TAG.NULL);
      return;
    }
    if (value instanceof Date) {
      out.byte(TAG.DATE);
      out.number(value.getTime());
      return;
    }
    const kinds = state.mapped.get(value);
    if (kinds !== undefined) {
      writeMapped(out, value, kinds, request, depth);
      return;
    }
    const table = tableOf(request);
    if (!raw && value instanceof Delegation) {
      const own = value[DELEGATION];
      if (own.state === state && own.request === request) {
        out.byte(TAG.DELEGATION);
        out.word(own.number);
        return;
      }
    }
    if (!raw && !checked && typeof value.then === "function") {
      writeThen(out, value, request, depth);
      return;
    }
    if (Array.isArray(value)) {
      if (raw) {
        out.byte(TAG.LIST);
        out.word(value.length);
        for (const item of value) writeValue(out, item, request, depth + 1, true);
        return;
      }
      const [mapped, kind] = mapList(value, request, depth);
      if (kind === THENABLE) writeAnswered(out, mapped, request, depth);
      else writeMapped(out, mapped, state.mapped.get(mapped), request, depth);
      return;
    }
    if (typeof value === "function") {
      const name = value.name;
      out.byte(TAG.FUNCTION);
      out.word(table.push(value) - 1);
      out.text(typeof name === "string" ? name : "");
      return;
    }
    if (value instanceof Error) {
      out.byte(TAG.ERROR);
      out.word(table.push(value) - 1);
      return;
    }
    if (!raw) {
      if (value instanceof Topic) {
        const { pubSub, routingKey, id } = value[TOPIC];
        out.byte(TAG.TOPIC);
        out.word(pubSub);
        out.text(String(routingKey));
        writeJsonDeep(out, jsonCopy(id));
        return;
      }
    }
    out.byte(TAG.OBJECT);
    out.word(table.push(value) - 1);
  };

  // Notes `reason`, the value a reader of a body of the host of `request`
  // failed with, which the body the module pushes for the request fails with
  // where the engine pushes its failure.
  const noteFailure = (request, reason) => {
    if (state.tables.has(request)) state.failures.set(request, { reason });
  };

  // Settles the promise `handle` of `request`, the read of a chunk of a body
  // of the host, with `result`: its bytes, or null at the end of the stream.
  const settleChunk = (request, handle, result) => {
    if (state.dead || !state.tables.has(request)) return;
    try {
      const out = new Out();
      let ok = 1;
      if (result.done) {
        out.byte(TAG.NULL);
      } else if (ArrayBuffer.isView(result.value)) {
        out.byte(TAG.BYTES);
        out.raw(new Uint8Array(result.value.buffer, result.value.byteOffset, result.value.byteLength));
      } else {
        ok = 0;
        const failure = new TypeError("Received non-Uint8Array chunk");
        noteFailure(request, failure);
        writeValue(out, failure, request, 0, true);
      }
      call("settle", request, handle, ok, send(out));
    } catch {
      if (!state.dead) answerWith(request, UNSETTLED);
    }
  };

  // Settles the promise `handle` of `request`, the answer of the fetch of the
  // host to an outbound request of the engine, with the response and the
  // bytes of its body in `got`, or with the message of `reason`.
  const settleFetched = (request, handle, got, reason) => {
    if (state.dead || !state.tables.has(request)) return;
    try {
      const out = new Out();
      out.byte(TAG.FETCHED);
      if (got === undefined) {
        out.byte(0);
        out.text(String(reason?.message ?? reason));
      } else {
        out.byte(1);
        out.word(got.response.status);
        out.text(got.response.statusText);
        const fields = [...got.response.headers];
        out.word(fields.length);
        for (const [name, field] of fields) {
          out.text(name);
          out.text(field);
        }
        out.raw(new Uint8Array(got.buffer));
      }
      call("settle", request, handle, 1, send(out));
    } catch {
      if (!state.dead) answerWith(request, UNSETTLED);
    }
  };

  // Writes a thenable as step 3 of the resolver wrapper meets it, which reads
  // its then once to check it, as the walk has, and once more to call it. A
  // promise of the realm of the host, whose then is the then of Promise,
  // takes the two callbacks of the host in place of the one callback of the
  // wrapper, the form of the plan, which settle the value that callback receives
  // in the job it runs in. Any other thenable takes the call of the wrapper with one
  // callback through callThen, and what its then answers is the value of the
  // field, as the call of wrapResolver answers it there.
  const writeThen = (out, value, request, depth) => {
    const then = value.then;
    if (then === promiseThen) {
      waitOn(out, value, request);
      return;
    }
    writeAnswered(out, callThen(then, value, request), request, depth);
  };

  // Calls the then `then` of the thenable `value` with the one callback of
  // the wrapper, and answers what then answers.
  //
  // The callback answers what wrapResolver answers for the value it receives,
  // through answerOf. A call of the callback after then has returned is late,
  // and a field that waits on the promise of the host that this then answered
  // takes what the callback answers, since a then that hands the callback to a
  // promise, as a query builder does, answers the promise that the return of the
  // callback settles: a value that holds no thenable in the job the callback
  // runs in, through settleField at once, and a promise one job after that
  // promise settles, as the promise of the app adopts it, through a then of the
  // host called one microtask later. A thenable of the app that a nested then
  // answered is left to the reaction on what then answered, one job late, since
  // only a second call of its then would tell the job it calls back in. A call
  // of the callback while then runs settles no field, since what then answers
  // carries it.
  //
  // The then of Promise, the intrinsic one, takes a second callback beside
  // the one of the wrapper, since the app sees no count of the arguments of that then. It
  // settles the fields that wait on the promise this then derives as rejected,
  // in the job that promise rejects in, and throws the reason again, so that
  // promise rejects as it would with the one callback. A then of the app takes
  // the one callback alone, since it sees the count of its arguments.
  const callThen = (then, value, request) => {
    const call = { returned: false, answered: undefined };
    const callbacks = [
      (resolved) => {
        const [result, kind] = answerOf(resolved, request, 0);
        if (call.returned) answeredLate(call.answered, result, kind);
        return result;
      },
    ];
    if (then === promiseThen) {
      callbacks.push((reason) => {
        rejectedLate(call.answered, reason);
        throw reason;
      });
    }
    const answered = apply(then, value, callbacks);
    call.answered = answered;
    call.returned = true;
    return answered;
  };

  // Answers what wrapResolver answers for the value `resolved`, nested `depth`
  // levels deep in a walk, in the order of its checks, with its kind: the value
  // that a callback of the call of the wrapper receives, and each item of an array the walk
  // maps. A primitive, a Date and an object whose then is no function and that
  // is no array come back as they are, the object since the projection of the
  // engine reads its members, as step 6 of wrapResolver reads them there,
  // through ObjectLike::read in the count and the order of the projection,
  // each read a js_get or a part of a js_project. A thenable has its then read
  // once to check it and once more to call it there, at once, through callThen,
  // and what that then answers comes back. An array is mapped at once through
  // mapList.
  const answerOf = (resolved, request, depth) => {
    if (depth > DEPTH) throw new RangeError("a value of the app nests deeper than " + DEPTH + " levels");
    if (
      resolved === null ||
      (typeof resolved !== "object" && typeof resolved !== "function") ||
      resolved instanceof Date
    ) {
      return [resolved, PLAIN];
    }
    if (typeof resolved.then === "function") {
      return [callThen(resolved.then, resolved, request), ANSWERED];
    }
    if (!Array.isArray(resolved)) return [resolved, PLAIN];
    return mapList(resolved, request, depth);
  };

  // Maps the array `list`, nested `depth` levels deep, as step 4 of the
  // resolver wrapper maps it: each item through answerOf with the map of Array,
  // so the then of a thenable item runs while the walk runs, a nested array
  // mapped the same way, and then the some of Array over the results. The
  // some reads the then of what a then answered alone, since the results of
  // the wrapper hold a copy of its own of an object whose members the document
  // selects, a mapped array or a wrapped function of its own and a primitive
  // otherwise, none of which the app sees read. It answers the mapped array,
  // noted with the kind of each item so it crosses as it is, or the promise
  // of all its items that allOf makes when one of them is a thenable, which
  // settles where the Promise.all of the walk settles, and which the wrapper owns, so
  // a field that waits on it settles in its job.
  const mapList = (list, request, depth) => {
    const kinds = [];
    const results = apply(arrayMap, list, [
      (item, index) => {
        const [result, kind] = answerOf(item, request, depth + 1);
        kinds[index] = kind;
        return result;
      },
    ]);
    const thenable = apply(arraySome, results, [
      (result, index) => {
        if (kinds[index] === THENABLE) return true;
        if (kinds[index] !== ANSWERED || result === null) return false;
        if (typeof result !== "object" && typeof result !== "function") return false;
        return typeof result.then === "function";
      },
    ]);
    if (thenable) return [allOf(results), THENABLE];
    state.mapped.set(results, kinds);
    return [results, PLAIN];
  };

  // Writes the array `list` that mapList mapped, nested `depth` levels deep,
  // each item by its kind in `kinds`: what a then answered, which is no
  // thenable here, as writeAnswered writes such a value, and every other item
  // as it is, an inner mapped array by its own kinds, with no then read again.
  const writeMapped = (out, list, kinds, request, depth) => {
    if (depth > DEPTH) throw new RangeError("a value of the app nests deeper than " + DEPTH + " levels");
    out.byte(TAG.LIST);
    out.word(list.length);
    for (let index = 0; index < list.length; index++) {
      const answered = kinds[index] === ANSWERED;
      writeValue(out, list[index], request, depth + 1, !answered, answered);
    }
  };

  // Settles every field that waits on `answered`, what a then answered whose
  // callback has just answered `result` of the kind `kind` late, as callThen
  // says. What a nested then answered is a value with no thenable when it is
  // a primitive, a promise of the host when it is one, and otherwise the walk
  // reads its then once to tell.
  const answeredLate = (answered, result, kind) => {
    const fields = waitingOn(answered);
    if (fields.length === 0) return;
    let promised = kind === THENABLE;
    if (kind === ANSWERED) {
      if (result !== null && (typeof result === "object" || typeof result === "function")) {
        if (result instanceof HostPromise) {
          promised = true;
        } else {
          let then;
          try {
            then = result.then;
          } catch {
            return;
          }
          if (typeof then === "function") return;
        }
      }
    }
    for (const field of fields) {
      if (promised) adoptLater(field, result);
      else settleField(field, 1, result);
    }
  };

  // Settles every field that waits on `answered`, what the then of Promise
  // derived, as rejected with `reason`, in the job that promise rejects in,
  // as callThen says.
  const rejectedLate = (answered, reason) => {
    for (const field of waitingOn(answered)) settleField(field, 0, reason);
  };

  // Returns a copy of the fields that wait on `answered`, what a then
  // answered, none for a promise of all items that allOf made, which settles
  // its fields itself, since no callback answers it.
  const waitingOn = (answered) => {
    if (answered === null || typeof answered !== "object" || state.alls.has(answered)) return [];
    const fields = state.awaited.get(answered);
    return fields === undefined ? [] : [...fields];
  };

  // Returns a promise of the host of all of `items`, as Promise.all of the
  // host answers it: each item through the resolve of Promise and the then of
  // the promise that answers, in the order of the items, and the array of the
  // values in the job the last of them fulfils, or the first rejection. A
  // field that waits on the promise settles in the job it settles in, which
  // the reaction of a then on it would reach one job later.
  const allOf = (items) => {
    let resolveAll;
    let rejectAll;
    const promise = new HostPromise((resolve, reject) => {
      resolveAll = resolve;
      rejectAll = reject;
    });
    const fields = [];
    state.alls.set(promise, fields);
    const values = [];
    let remaining = 1;
    const finish = () => {
      resolveAll(values);
      for (const field of fields) settleField(field, 1, values);
    };
    const fail = (reason) => {
      rejectAll(reason);
      for (const field of fields) settleField(field, 0, reason);
    };
    let index = 0;
    for (const item of items) {
      const at = index;
      index += 1;
      values.push(undefined);
      const next = apply(promiseResolve, HostPromise, [item]);
      let called = false;
      remaining += 1;
      next.then((value) => {
        if (called) return;
        called = true;
        values[at] = value;
        remaining -= 1;
        if (remaining === 0) finish();
      }, fail);
    }
    remaining -= 1;
    if (remaining === 0) finish();
    return promise;
  };

  // Writes the promise `promise` of the realm of the host behind a handle of
  // the table of `request`, after its then has taken the two callbacks that
  // hand its settlement to the export settle.
  const waitOn = (out, promise, request) => {
    const handle = tableOf(request).push(promise) - 1;
    apply(promiseThen, promise, [
      (value) => settle(request, handle, 1, value),
      (reason) => settle(request, handle, 0, reason),
    ]);
    out.byte(TAG.PROMISE);
    out.word(handle);
  };

  // Writes `value`, what the then of a thenable that is no promise of the host
  // answered, the value the resolver hands its consumer, or the promise of
  // all items that mapList made of an array. Its then is read once, as a promise
  // reads the then of a value it is resolved with, and a value whose then is no
  // function crosses as it is. A thenable takes a field, a handle of the table
  // of `request` that the export settle settles. A promise of all items that
  // allOf made settles the field in its own job, its rejection handled as
  // the consumer handles it. Any other promise of the host settles the field
  // in the reaction of a then of the host, unless a late callback settles the
  // field first: the job the consumer reacts in for a promise that settled
  // before the job in which the async function of a root adopts it, and one job
  // after it otherwise, since the engine meets the field through a step 3 of its
  // own. A thenable of the app is adopted as a promise adopts it, its then
  // called one microtask later with resolving functions of the field. A field
  // that waits on a promise of the host stands in the list of that promise until
  // it settles or its request is released, so a promise of the app that a then
  // answers on every request, a promise the app keeps among them, holds the
  // fields of the requests in flight alone.
  const writeAnswered = (out, value, request, depth) => {
    if (value === null || (typeof value !== "object" && typeof value !== "function")) {
      writeValue(out, value, request, depth);
      return;
    }
    const then = value.then;
    if (typeof then !== "function") {
      writeValue(out, value, request, depth, false, true);
      return;
    }
    const field = { request, handle: tableOf(request).push(value) - 1, done: false, waiting: null };
    if (then === promiseThen) {
      const all = state.alls.get(value);
      if (all !== undefined) {
        all.push(field);
        apply(promiseThen, value, [undefined, ignore]);
      } else {
        noteWaiting(field, value);
        apply(promiseThen, value, [
          (settled) => settleField(field, 1, settled),
          (reason) => settleField(field, 0, reason),
        ]);
      }
    } else {
      callLater(then, value, field);
    }
    out.byte(TAG.PROMISE);
    out.word(field.handle);
  };

  // Notes that `field` waits on `promise`, a promise of the host a then
  // answered, in the list of that promise and in the fields of its request.
  const noteWaiting = (field, promise) => {
    let waiting = state.awaited.get(promise);
    if (waiting === undefined) {
      waiting = [];
      state.awaited.set(promise, waiting);
    }
    waiting.push(field);
    field.waiting = waiting;
    const fields = state.fields.get(field.request);
    if (fields === undefined) state.fields.set(field.request, [field]);
    else fields.push(field);
  };

  // Takes `field` out of the list of the promise it waits on.
  const forget = (field) => {
    const waiting = field.waiting;
    if (waiting === null) return;
    field.waiting = null;
    const at = waiting.indexOf(field);
    if (at !== -1) waiting.splice(at, 1);
  };

  // Calls `then` of the thenable `value` one microtask later with resolving
  // functions of `field`, as the job of a promise resolved with that thenable
  // calls it, and a throw of the call rejects the field.
  const callLater = (then, value, field) => {
    apply(promiseThen, fulfilled, [() => {
      const [resolve, reject] = resolvingOf(field);
      try {
        apply(then, value, [resolve, reject]);
      } catch (error) {
        reject(error);
      }
    }]);
  };

  // Returns the resolving functions of `field`, which settle it once, as the
  // resolving functions of a promise settle it. Each settlement through them
  // is adopted: it stands for one through the resolving functions the
  // async function of a root hands the thenable itself, whose consumer
  // reacts on the settlement at once, so the module runs a level of
  // reactions more at once, as settleField says.
  const resolvingOf = (field) => {
    let resolved = false;
    return [
      (value) => {
        if (resolved) return;
        resolved = true;
        resolveField(field, value);
      },
      (reason) => {
        if (resolved) return;
        resolved = true;
        settleField(field, 0, reason, true);
      },
    ];
  };

  // Resolves `field` with `value` as the resolve function of a promise does,
  // each settlement adopted: a value whose then is no function fulfils it at
  // once, and a thenable is adopted one microtask later, its then read once
  // here.
  const resolveField = (field, value) => {
    if (value === null || (typeof value !== "object" && typeof value !== "function")) {
      settleField(field, 1, value, true);
      return;
    }
    let then;
    try {
      then = value.then;
    } catch (error) {
      settleField(field, 0, error, true);
      return;
    }
    if (typeof then !== "function") {
      settleField(field, 1, value, true);
      return;
    }
    callLater(then, value, field);
  };

  // Settles `field` with the promise of the host `promise` one job after it
  // settles, the job a promise of the app that adopts it settles in: a then
  // of the host on it, called one microtask later, as the job of the adoption
  // calls it.
  const adoptLater = (field, promise) => {
    apply(promiseThen, fulfilled, [() => {
      try {
        apply(promiseThen, promise, [
          (settled) => settleField(field, 1, settled),
          (reason) => settleField(field, 0, reason),
        ]);
      } catch (error) {
        settleField(field, 0, error);
      }
    }]);
  };

  // Settles `field` once and takes it out of the list of the promise it
  // waits on. Outside an import it settles through the export settle at
  // once. Inside an import, where a callback of the app runs it that code of
  // the engine called, the settlement rides on the answer of that import,
  // which the module applies as the import returns, before the code that
  // called it goes on, so it takes effect in the job of that call, and its
  // reservation runs the reactions of the consumer of the engine where V8
  // runs the reactions of the promise of the app, as carry says. No export
  // but alloc runs inside an import. An adopted settlement, one through
  // resolvingOf, carries the bit ADOPTED in its byte, so the module runs the
  // jobs the reactions of the settlement enqueue at once as well, where the
  // consumer of the engine reaches the settlement through the step 3 the
  // engine applies to the field and the adoption of the promise that step
  // derives, while the consumer reacts on the settlement itself.
  const settleField = (field, ok, value, adopted = false) => {
    if (field.done) return;
    field.done = true;
    forget(field);
    const mark = adopted ? ADOPTED : 0;
    if (state.inside === 0) settle(field.request, field.handle, ok, value, mark);
    else carry(field.request, field.handle, ok, value, mark);
  };

  // Writes the value record of a settlement of `value`, fulfilled when `ok`
  // is 1, whose handles go to the table of `request`, and returns the byte of
  // the settlement it wrote. The walk of a fulfilled value calls the then of
  // each thenable in it, as the reaction of the resolver wrapper walks the
  // value, and a value whose walk throws rejects with that throw, as that
  // reaction throws there. A rejection passes untouched, since the then of
  // wrapResolver takes no second callback.
  const writeSettled = (out, request, ok, value) => {
    const at = out.length;
    try {
      writeValue(out, value, request, 0, ok === 0);
      return ok;
    } catch (error) {
      out.truncate(at);
      try {
        writeValue(out, error, request, 0, true);
      } catch {
        out.truncate(at);
        unreadable(out);
      }
      return 0;
    }
  };

  // Notes the settlement of the promise `handle` of `request` with `value`
  // for the answer of the import that runs, as its request, its handle, its
  // byte with the bits `mark` and its value record, which writeSettled
  // writes now, where the settlement happens, and reserves the microtask of
  // its consumer at this call. The settlement takes its place in the order
  // before its value is walked, so a settlement a then of that walk makes
  // follows it.
  const carry = (request, handle, ok, value, mark = 0) => {
    if (state.dead || !state.tables.has(request)) return;
    reserve(request);
    const out = new Out();
    state.carried.push(out);
    out.word(request);
    out.word(handle);
    const at = out.length;
    out.byte(ok);
    const written = writeSettled(out, request, ok, value);
    out.bytes[at] = written | mark;
  };

  // Queues the microtask a settlement inside an import reserves for
  // `request` at the call that made it, where V8 queues the reactions of the
  // promise of the app the callback settled, and so where V8 runs the
  // consumer of that promise. It calls the export resume, which runs the
  // reactions of the consumer of the engine the module kept for the
  // reservation, the reservations of a request in the order they were made.
  // A microtask the host cannot queue answers the request with a failure,
  // since those reactions would never run.
  const reserve = (request) => {
    try {
      enqueueMicrotask(() => {
        if (state.dead || !state.tables.has(request)) return;
        try {
          call("resume", request);
        } catch {
          // The instance trapped, and condemn answered the requests on it.
        }
      });
    } catch {
      answerWith(request, UNQUEUED);
    }
  };

  // Writes the record of a thrown value that the wrapper cannot read, an
  // error with no handle, which the module answers as a plain error that
  // names it.
  const unreadable = (out) => {
    out.byte(TAG.ERROR);
    out.word(ABSENT);
  };

  // Returns the byte of the brand of the error `error`.
  const brandByte = (error) => {
    const kind = brandOf(error);
    return kind === GRAPHQL ? BRAND.GRAPHQL : kind === FORBIDDEN ? BRAND.FORBIDDEN : BRAND.NONE;
  };

  // Writes `copy`, the copy of copyOf, nested `depth` levels deep, as
  // writeJson writes data of JSON.
  const writeCopy = writeJson;

  // Writes what the engine reads of the error `error` a read of the host
  // rejected with, a pull of the body of a request or the fetch of the host:
  // its brand, the string of its name and of its message and its member
  // statusCode where it is a number. A read that throws, a getter among
  // them, throws to the import.
  const writePulled = (out, error) => {
    const kind = brandOf(error) ?? (error instanceof AggregateError ? AGGREGATE : undefined);
    out.byte(kind === GRAPHQL ? BRAND.GRAPHQL : kind === FORBIDDEN ? BRAND.FORBIDDEN
      : kind === AGGREGATE ? BRAND.AGGREGATE : BRAND.NONE);
    out.text(String(error.name));
    out.text(String(error.message));
    const status = error.statusCode;
    if (typeof status === "number") {
      out.byte(TAG.NUMBER);
      out.number(status);
    } else {
      out.byte(TAG.UNDEFINED);
    }
  };

  // Writes what the handler of Yoga reads of the value `error` a hook of a
  // plugin of the app threw or the promise of a done hook rejected with, nested
  // `depth` levels deep in the errors of such a value: what handleError reads of
  // a value that escapes the execution, what its mask reads where `masked`
  // holds, and what the writer of the result reads of a GraphQLError it keeps,
  // in their order. A value that holds a member errors gives each of its errors,
  // the DOMException of an abort or a timeout nothing, a GraphQLError the mask
  // keeps, or that no mask reads, whether its original errors are GraphQLErrors
  // to the last, its extensions, its message, its path and its blame as
  // writeFinal reads a GraphQLError the executor took as it is, and one the mask
  // masks its path, the member http of its extensions and its blame. Any other
  // value the mask masks gives the name and the message of an error, and where
  // no mask reads it an error gives its message and its extensions as
  // createGraphQLError reads an original error and any other value the text of
  // its method toString, crates/aosabi-js/src/value.rs. The copies cross at any
  // depth through writeJsonDeep. A read that throws throws to the import.
  const writeHook = (out, error, masked, depth) => {
    if (depth > DEPTH) throw new RangeError("the errors a hook threw nest deeper than " + DEPTH + " levels");
    if (error != null && typeof error === "object" && "errors" in error) {
      const items = [];
      for (const item of error.errors) items.push(item);
      out.byte(HOOK_KIND.AGGREGATE);
      out.word(items.length);
      for (const item of items) writeHook(out, item, masked, depth + 1);
      return;
    }
    if (typeof error === "object" && error?.constructor?.name === "DOMException" &&
        (error.name === "AbortError" || error.name === "TimeoutError")) {
      out.byte(HOOK_KIND.ABORT);
      return;
    }
    if (brandOf(error) === GRAPHQL) {
      const original = isOriginal(error);
      if (masked && !original) {
        out.byte(HOOK_KIND.MASKED);
        const nodes = error.nodes;
        const source = error.source;
        const positions = error.positions;
        writeJsonDeep(out, copyOf("path", error.path));
        void error.coordinate;
        const held = error.extensions;
        if (held == null || !held.http) out.byte(TAG.UNDEFINED);
        else writeJsonDeep(out, copyOf("http", error.extensions.http));
        writeBlame(out, nodes, source, positions);
        return;
      }
      out.byte(HOOK_KIND.KEPT);
      out.byte(original ? 1 : 0);
      void error.message;
      if (error.locations != null) void error.locations;
      if (error.path != null) void error.path;
      writeJsonDeep(out, copyOf("extensions", error.extensions));
      out.text(messageOf(error.message));
      const nodes = error.nodes;
      const source = error.source;
      const positions = error.positions;
      writeJsonDeep(out, copyOf("path", error.path));
      void error.originalError;
      void error.coordinate;
      writeBlame(out, nodes, source, positions);
      return;
    }
    if (masked) {
      out.byte(HOOK_KIND.BARE);
      const named = error instanceof Error;
      out.text(named ? String(error.name) : "Error");
      out.text(named ? String(error.message) : "");
      return;
    }
    if (error instanceof Error) {
      out.byte(HOOK_KIND.ERROR);
      out.text(messageOf(error.message));
      const extensions = error.extensions;
      writeJsonDeep(out, isObjectLike(extensions) ? copyOf("extensions", error.extensions) : undefined);
      return;
    }
    if (error != null && typeof error.toString === "function") {
      out.byte(HOOK_KIND.TEXT);
      out.text(String(error.toString()));
      return;
    }
    out.byte(HOOK_KIND.NONE);
  };

  // Answers whether `error` is a GraphQLError whose original errors down its
  // chain are GraphQLErrors to the last, as isOriginalGraphQLError of Yoga reads
  // it, the original error once to test it and once more when it is no null.
  const isOriginal = (error) => {
    if (brandOf(error) !== GRAPHQL) return false;
    if (error.originalError != null) return isOriginal(error.originalError);
    return true;
  };

  // Writes what the module reads of the error `error` of `request` inside a
  // value: its brand, the string of its name and of its message, and what the
  // engine needs of its class. A GraphQLError gives the copy of its
  // extensions, its original error, which the module reads in the form of an
  // original error, and its path, and any other error the copy of each of its
  // own enumerable members, since JSON.stringify reads an error inside a value
  // so. No stack crosses. A read that throws, a getter or the trap of a Proxy
  // among them, throws to the import.
  const writeError = (out, error, request) => {
    const brand = brandByte(error);
    out.byte(brand);
    out.text(String(error.name));
    out.text(String(error.message));
    if (brand === BRAND.GRAPHQL) {
      writeCopy(out, copyOf("extensions", error.extensions), 0);
      writeValue(out, error.originalError, request, 0, true);
      writeValue(out, error.path, request, 0, true);
      out.word(0);
      return;
    }
    out.byte(TAG.UNDEFINED);
    out.byte(TAG.UNDEFINED);
    out.byte(TAG.UNDEFINED);
    const keys = Object.keys(error);
    out.word(keys.length);
    for (const key of keys) {
      out.text(key);
      writeCopy(out, copyOf(key, error[key]), 0);
    }
  };

  // Writes what the executor reads of the error `error` of `request`
  // that a call threw, a promise rejected with or a field answered while it
  // executes, in the order and the count it reads it there, the bytes `levels`
  // the handlers that meet the error, the nearest first and the field of the
  // root last, and none where the module knows no place. The handler of a field
  // takes an AggregateError apart: it reads its errors, coerces, locates and
  // handles each in turn, and stops at the first whose handling throws it on, so
  // no error after that one is read, while the handler of an item of a list
  // handles an AggregateError as any other error. Where the module knows no
  // place every error of an AggregateError is read, with no reads of a handler.
  const writeThrown = (out, error, levels, request) => {
    if ((levels.length === 0 || (levels[0] & LEVEL_ITEM) === 0) && error instanceof AggregateError) {
      out.byte(BRAND.AGGREGATE);
      const items = [];
      for (const item of error.errors) {
        const record = new Out();
        const throws = writeItem(record, item, levels, request);
        items.push(record);
        if (throws) break;
      }
      out.word(items.length);
      for (const record of items) out.append(record);
      return;
    }
    writeHandled(out, error, levels, request);
  };

  // Writes one error `item` of an AggregateError the handler of a field
  // handles, as coerceError makes it, and answers whether its handling throws
  // it on, which ends the loop of the handler. An item that is no error is
  // the error coerceError makes of it, whose name and message are its own
  // data, and its located error holds no extensions of the app.
  const writeItem = (out, item, levels, request) => {
    const error = coerceError(item);
    if (error === item) {
      out.byte(ITEM.ERROR);
      return writeHandled(out, error, levels, request);
    }
    out.byte(ITEM.COERCED);
    out.text(String(error.name));
    out.text(String(error.message));
    return levels.length !== 0 && (levels[0] & LEVEL_NULLABLE) === 0;
  };

  // Writes what the executor reads of the error `error` of the app as
  // locatedError of graphql-js and of the utilities locates it and as each
  // handler of `levels` handles it, and answers whether the handler of the first
  // level throws it on. An error whose path is an array is taken as it is, after
  // its coordinate is read, and any other is read as the constructor of the
  // located error reads it: its message, its nodes, its source and its
  // positions, the locations of its blame, its extensions once more when they
  // are an object and its stack once more when it is true. The wrapper keeps the
  // error with what the module reads of it at the end of the execution, as
  // writeEnd and writeFinal make it there, where the mask and the writer of the
  // result of Yoga read it, and the handle of that record closes the record. The
  // feature of a ForbiddenError is not read, since no plugin the module serves
  // maps it.
  const writeHandled = (out, error, levels, request) => {
    const brand = brandByte(error);
    out.byte(brand);
    const path = error.path;
    const located = Array.isArray(path);
    let extensions;
    if (located) {
      void error.coordinate;
      out.byte(LOCATED);
    } else {
      out.byte(0);
      const message = error.message;
      const nodes = error.nodes;
      const source = error.source;
      const positions = error.positions;
      out.text(messageOf(message));
      writeBlame(out, nodes, source, positions);
      extensions = error.extensions;
      extensions = isObjectLike(extensions) ? error.extensions : undefined;
      if (error.stack) void error.stack;
      out.byte(extensions === undefined ? 0 : 1);
    }
    const [critical, throws] = handled(error, located, extensions, levels);
    out.byte(critical);
    out.word(tableOf(request).push({ error, located, extensions, brand }) - 1);
    return throws;
  };

  // Makes the reads of handleFieldError of the executor at each level of
  // `levels` the error `error` reaches, and of each handler above the first,
  // which locates it again, and answers the first answer of the member
  // CRITICAL_ERROR of its extensions and whether the handler of the first level
  // throws it on. At a place that holds no null the error goes on, and at one
  // that may it goes on when that member is true. The handler above reads the
  // path and the coordinate of an error the executor took as it is, and the
  // field of the root its errors, while a located error of the executor holds
  // its own path and the extensions `extensions` of the app, the object
  // `isObjectLike` found or undefined.
  const handled = (error, located, extensions, levels) => {
    let critical = CRITICAL.UNREAD;
    let first = false;
    for (let index = 0; index < levels.length; index++) {
      if (index > 0 && located) {
        void error.path;
        void error.coordinate;
      }
      let throws = true;
      if ((levels[index] & LEVEL_NULLABLE) !== 0) {
        const held = located ? error.extensions : extensions;
        const value = held == null ? undefined : held[CRITICAL_ERROR];
        if (critical === CRITICAL.UNREAD) critical = value ? CRITICAL.TRUE : CRITICAL.FALSE;
        throws = Boolean(value);
      }
      if (index === 0) first = throws;
      if (!throws) return [critical, first];
    }
    if (located && levels.length !== 0 && error.errors) void [...error.errors];
    return [critical, first];
  };

  // Writes the first read made at the end of the execution of the thrown
  // GraphQLError the record `deferred` of writeHandled keeps, its original error
  // as isOriginalGraphQLError of Yoga reads it when the mask asks whether to
  // keep the error, which the module reads in the form of an original error.
  const writeEnd = (out, deferred, request) => {
    writeValue(out, originalOf(deferred.error), request, 1, true);
  };

  // Writes the reads made at the end of the execution of the thrown error
  // the record `deferred` of writeHandled keeps once the mask has decided,
  // `masked` true where it masks the error. The mask reads of a GraphQLError the
  // executor took as it is its nodes, its source, its positions, its path and
  // its coordinate, and the member http of its extensions, and the constructor
  // of the masked error the blame of those nodes, that source and those
  // positions, and of the located error the executor made of any other error the
  // member http of the extensions it holds, while it reads nothing of an error
  // that is no GraphQLError. An error the mask keeps is written as the writer of
  // the result makes it anew: a GraphQLError the executor took as it is through
  // its toJSON, its message, its locations, its path and its extensions, whose
  // members but http and unexpected the new error takes, and then its message,
  // its nodes, its source, its positions, its path, its original error and its
  // coordinate, from whose blame the new error computes its locations, and a
  // located error of the executor through the walk of the extensions of the app
  // it holds.
  const writeFinal = (out, deferred, masked, request) => {
    const { error, located, extensions, brand } = deferred;
    if (masked === MAPPED) {
      writeIdentity(out, error);
      return;
    }
    if (!masked) {
      if (!located) {
        writeCopy(out, copyOf("extensions", extensions), 0);
        return;
      }
      void error.message;
      if (error.locations != null) void error.locations;
      if (error.path != null) void error.path;
      writeCopy(out, copyOf("extensions", error.extensions), 0);
      out.text(messageOf(error.message));
      const nodes = error.nodes;
      const source = error.source;
      const positions = error.positions;
      writeCopy(out, copyOf("path", error.path), 0);
      void error.originalError;
      void error.coordinate;
      writeBlame(out, nodes, source, positions);
      return;
    }
    if (!located) {
      writeHttp(out, () => extensions);
      return;
    }
    if (brand !== BRAND.GRAPHQL) return;
    const nodes = error.nodes;
    const source = error.source;
    const positions = error.positions;
    writeCopy(out, copyOf("path", error.path), 0);
    void error.coordinate;
    writeHttp(out, () => error.extensions);
    writeBlame(out, nodes, source, positions);
  };

  // Writes the reads the map of the identity plugin makes at the end of the
  // execution of the thrown ForbiddenError `error`, which replaces it before the
  // mask reads it: its message, and its feature once for the test of its truth
  // and once more for its value when it is true, whose copy JSON reads the error
  // FORBIDDEN carries.
  const writeIdentity = (out, error) => {
    out.text(messageOf(error.message));
    if (!error.feature) {
      out.byte(0);
      return;
    }
    out.byte(1);
    writeCopy(out, copyOf("feature", error.feature), 0);
  };

  // Writes the copy of the member http of the extensions `held` answers, as
  // the mask of Yoga reads it: once to test it and, when it is true, once more
  // for the masked error, each read through `held`.
  const writeHttp = (out, held) => {
    const first = held();
    if (first == null || !first.http) {
      out.byte(TAG.UNDEFINED);
      return;
    }
    writeCopy(out, copyOf("http", held().http), 0);
  };

  // Returns the original error of the error `error`, read as
  // isOriginalGraphQLError of Yoga reads it, once to test it and once more
  // when it is no null.
  const originalOf = (error) => {
    const original = error.originalError;
    return original != null ? error.originalError : original;
  };

  // Writes the blame of a thrown error that is no located error, as the
  // constructor of the located error of graphql-js reads it from the nodes of
  // the error, or of the field where it holds none, its source and its
  // positions: the positions of the source when it holds both, the field when
  // it holds no nodes, the start of each location of its nodes in its own
  // source, and no location for nodes that hold none. Each read of the nodes,
  // their locations and the body of a source is the read of that constructor.
  const writeBlame = (out, nodes, source, positions) => {
    const list = nodes == null ? undefined : undefinedIfEmpty(Array.isArray(nodes) ? nodes : nodes ? [nodes] : undefined);
    const locations = undefinedIfEmpty(list?.map((node) => node.loc).filter((loc) => loc != null));
    if (source == null) void locations?.[0]?.source;
    if (positions == null) void locations?.map((loc) => loc.start);
    if (positions && source) {
      const at = positions.map((position) => [source.body, position]);
      writeSource(out, at);
      return;
    }
    if (nodes == null) {
      out.byte(BLAME.FIELD);
      return;
    }
    if (locations === undefined) {
      out.byte(BLAME.NONE);
      return;
    }
    writeSource(out, locations.map((loc) => {
      const at = loc.source;
      const start = loc.start;
      return [at.body, start];
    }));
  };

  // Writes the blame of positions in the bodies of sources, each pair of `at`
  // a body and a position in it.
  const writeSource = (out, at) => {
    out.byte(BLAME.SOURCE);
    out.word(at.length);
    for (const [body, position] of at) {
      out.text(String(body));
      out.number(Number(position));
    }
  };

  // Writes what the module reads of the original error `error` of a
  // GraphQLError of the app: its brand, its own original error for a
  // GraphQLError, read as isOriginalGraphQLError of Yoga reads it, and then
  // the string of its name and of its message.
  const writeOriginal = (out, error, request) => {
    const brand = brandByte(error);
    out.byte(brand);
    if (brand === BRAND.GRAPHQL) writeValue(out, originalOf(error), request, 1, true);
    out.text(String(error.name));
    out.text(String(error.message));
  };

  // Answers the reads of the members `members` of `object` of the table `table`
  // that a projection selects, each a name and its flags, in the order and the
  // count of the projection of the resolver wrapper: the member once for the
  // test of its truth, once more for the test of its type when the first answer
  // is true, and once more for its value, but where the projection routes it and
  // it is no function, and the value bound to the object and walked as
  // writeValue walks it. The reads stop after a member whose value the
  // projection may project before it reads the next member, an object, and after
  // a read or a walk that throws, whose throw is the completion of that read.
  const readSelected = (table, object, members, request) => {
    const reads = new Out();
    let count = 0;
    let handled = 0;
    const tested = (index, value) => {
      reads.word(index);
      reads.byte(TEST);
      reads.byte(NORMAL);
      reads.byte(testOf(value));
      count += 1;
    };
    const threw = (index, kind, error) => {
      reads.word(index);
      reads.byte(kind);
      reads.byte(THROW);
      const at = reads.length;
      try {
        writeValue(reads, error, request, 0, true);
      } catch {
        reads.truncate(at);
        unreadable(reads);
      }
      count += 1;
    };
    for (let index = 0; index < members.length; index++) {
      const [key, flags] = members[index];
      handled = index + 1;
      let first;
      try {
        first = object[key];
      } catch (error) {
        threw(index, TEST, error);
        break;
      }
      tested(index, first);
      let isFunction = false;
      if (first) {
        let second;
        try {
          second = object[key];
        } catch (error) {
          threw(index, TEST, error);
          break;
        }
        tested(index, second);
        isFunction = typeof second === "function";
      }
      if ((flags & ROUTED) !== 0 && !isFunction) continue;
      let value;
      try {
        value = member(table, object, key);
      } catch (error) {
        threw(index, VALUE, error);
        break;
      }
      const at = reads.length;
      try {
        reads.word(index);
        reads.byte(VALUE);
        reads.byte(NORMAL);
        writeValue(reads, value, request, 0);
        count += 1;
      } catch (error) {
        reads.truncate(at);
        threw(index, VALUE, error);
        break;
      }
      if ((flags & NESTED) !== 0 && isObjectLike(value)) break;
    }
    const out = new Out();
    out.byte(NORMAL);
    out.word(handled);
    out.word(count);
    out.append(reads);
    return out;
  };

  // Returns the completion record of a throw of `error`.
  const thrown = (request, error) => {
    const out = new Out();
    out.byte(THROW);
    try {
      writeValue(out, error, request, 0, true);
    } catch {
      out.truncate(1);
      unreadable(out);
    }
    return out;
  };

  // Returns the completion record of `value`, or of the throw of the walk of
  // `value`.
  const normal = (request, value) => {
    const out = new Out();
    out.byte(NORMAL);
    try {
      writeValue(out, value, request, 0);
    } catch (error) {
      return thrown(request, error);
    }
    return out;
  };

  // Returns the completion record of a call of the app that returned `value`
  // or threw it.
  const completion = (request, threw, value) => (threw ? thrown(request, value) : normal(request, value));

  // The body of every import that answers a record: nothing on a dead
  // instance, and a throw completion where the host throws. The count
  // `inside` tells a callback of the app that it runs inside an import, and
  // the settlements such callbacks make follow the record the body answers,
  // as a count and the settlements in the order they happened, which the
  // module applies once it has read that record.
  const guarded = (request, body) => {
    if (state.dead) return 0;
    state.inside += 1;
    state.carried = [];
    try {
      let out;
      try {
        out = body();
      } catch (error) {
        if (state.dead) return 0;
        out = thrown(request, error);
      }
      if (state.dead) return 0;
      out.word(state.carried.length);
      for (const settlement of state.carried) out.append(settlement);
      return send(out);
    } catch {
      return 0;
    } finally {
      state.carried = [];
      state.inside -= 1;
    }
  };

  // Settles the promise `handle` of `request` with `value`, fulfilled when
  // `ok` is 1, through the export settle, the value walked as writeSettled
  // walks it, and the bits `mark` beside the bit of a fulfilment. A trap of
  // the export condemns the instance, which answers the request with its
  // failure, and any other throw is the wrapper's own, a buffer the module
  // could not make among them, and answers the request with a failure, since
  // the field would never settle.
  const settle = (request, handle, ok, value, mark = 0) => {
    if (state.dead || !state.tables.has(request)) return;
    try {
      const out = new Out();
      const written = writeSettled(out, request, ok, value);
      call("settle", request, handle, written | mark, send(out));
    } catch {
      if (!state.dead) answerWith(request, UNSETTLED);
    }
  };

  const host = {
    // The own enumerable keys of an object, the reads of the members a
    // projection selects, as readSelected makes them, or the names of the
    // projection of a build of the canary, as canaryNames makes them.
    js_project(table, handle, record, length, request) {
      return guarded(request, () => {
        const reader = inPlace(record, length);
        const site = readCall(reader);
        const form = reader.byte();
        const members = [];
        if (form === SELECTED) {
          for (let count = reader.word(); count > 0; count--) members.push([reader.text(), reader.byte()]);
        }
        const object = state.tables.get(table)[handle];
        return within(request, site, () => {
          if (form === SELECTED) return readSelected(table, object, members, request);
          const out = new Out();
          out.byte(NORMAL);
          try {
            const keys = form === NAMES ? canaryNames(object) : Object.keys(object);
            out.word(keys.length);
            for (const key of keys) out.text(key);
          } catch (error) {
            return thrown(request, error);
          }
          return out;
        });
      });
    },

    // A member read for its value, or for a test, which answers the byte of
    // testOf and hands over no value.
    js_get(table, handle, record, length, request) {
      return guarded(request, () => {
        const reader = inPlace(record, length);
        const site = readCall(reader);
        const key = reader.text();
        const form = reader.byte();
        const object = state.tables.get(table)[handle];
        return within(request, site, () => {
          let value;
          let threw = false;
          try {
            value = form === TEST ? object[key] : member(table, object, key);
          } catch (error) {
            threw = true;
            value = error;
          }
          if (threw || form !== TEST) return completion(request, threw, value);
          const out = new Out();
          out.byte(NORMAL);
          out.byte(testOf(value));
          return out;
        });
      });
    },

    js_call(table, handle, args, length, request) {
      return guarded(request, () => {
        const reader = inPlace(args, length);
        const site = readCall(reader);
        const values = new Array(reader.word());
        const variables = variablesOf(request);
        const scope = scopeOf(request);
        for (let i = 0; i < values.length; i++) values[i] = reader.value(0, variables, scope);
        const fn = state.tables.get(table)[handle];
        return within(request, site, () => {
          let value;
          let threw = false;
          try {
            value = fn(...values);
          } catch (error) {
            threw = true;
            value = error;
          }
          return completion(request, threw, value);
        });
      });
    },

    // The error `handle` of the table `table` in the form the record names,
    // read inside the store of its request, as the executor reads a
    // thrown value inside the context of its request: a thrown error with the
    // levels of the handlers that meet it, an error inside a value, the
    // original error of a GraphQLError, the reads at the end of the
    // execution of a thrown error, whose kept record the handle names, the
    // value a read of the host rejected with and a value a hook of a plugin
    // threw.
    js_error(table, handle, record, length, request) {
      return guarded(request, () => {
        const reader = inPlace(record, length);
        const form = reader.byte();
        const levels = [];
        if (form === THROWN) {
          for (let count = reader.word(); count > 0; count--) levels.push(reader.byte());
        }
        const masked = form === FINAL || form === HOOK ? reader.byte() : 0;
        const error = state.tables.get(table)[handle];
        return within(request, ERROR_CALL, () => {
          const out = new Out();
          out.byte(NORMAL);
          try {
            if (form === THROWN) writeThrown(out, error, levels, request);
            else if (form === ORIGINAL) writeOriginal(out, error, request);
            else if (form === END) writeEnd(out, error, request);
            else if (form === FINAL) writeFinal(out, error, masked, request);
            else if (form === PULLED) writePulled(out, error);
            else if (form === HOOK) writeHook(out, error, masked === 1, 0);
            else writeError(out, error, request);
          } catch (thrownError) {
            return thrown(request, thrownError);
          }
          return out;
        });
      });
    },

    // The value `handle` iterated in the form the record names: the next
    // chunk of a reader, the body of a web standard Response a handler
    // answered, as the promise of its read, whose settlement hands the
    // engine the bytes of the chunk or null at its end, the test of the
    // member Symbol.asyncIterator of an object, which isAsyncIterable reads,
    // as the byte of testOf, and the iterator the call of that member
    // answers, object[Symbol.asyncIterator](), a Repeater or an async
    // generator of the app answering itself, which crosses as it is with
    // no walk, since no wrapResolver meets it. The test and the call run
    // inside the store of the call, and a throw of either is a throw
    // completion.
    iter_next(table, handle, record, length, request) {
      return guarded(request, () => {
        const reader = inPlace(record, length);
        const site = readCall(reader);
        const form = reader.byte();
        const object = state.tables.get(table)[handle];
        if (form === ITERATE.READ) {
          if (object.ended === true) {
            const out = new Out();
            out.byte(NORMAL);
            out.byte(TAG.NULL);
            return out;
          }
          const read = object.read();
          const held = tableOf(request).push(read) - 1;
          apply(promiseThen, read, [
            (result) => settleChunk(request, held, result),
            (reason) => {
              noteFailure(request, reason);
              settle(request, held, 0, reason);
            },
          ]);
          const out = new Out();
          out.byte(NORMAL);
          out.byte(TAG.PROMISE);
          out.word(held);
          return out;
        }
        return within(request, site, () => {
          let value;
          try {
            value = object[Symbol.asyncIterator];
            if (form === ITERATE.ITERATOR) value = apply(value, object, []);
          } catch (error) {
            return thrown(request, error);
          }
          const out = new Out();
          out.byte(NORMAL);
          if (form === ITERATE.ITERABLE) {
            out.byte(testOf(value));
            return out;
          }
          try {
            writeValue(out, value, request, 0, true);
          } catch (error) {
            return thrown(request, error);
          }
          return out;
        });
      });
    },

    // A record the module pushes to the body of `request` after the head of
    // its answer, stream.js: a piece of the body, its end, its failure with
    // the message, or the release of the request, after which the host drops
    // its table, since the engine holds nothing of it any more. A body fails
    // with the reason a reader of a body of the host of the request failed
    // with, the value the stream of the app failed with, or with an Error of
    // the message where no stream of the host failed.
    host_push(request, kind, at, length) {
      if (state.dead) return;
      try {
        const bytes = copy(at, length);
        if (kind === PUSH.RELEASED) {
          const body = state.pushed.get(request);
          body?.end();
          if (body?.taken) state.pushed.delete(request);
          state.forget(request, false);
          return;
        }
        if (!state.tables.has(request)) return;
        const body = pushedOf(request);
        if (kind === PUSH.PIECE) body.piece(bytes);
        else if (kind === PUSH.END) body.end();
        else if (kind === PUSH.FAILED) {
          const noted = state.failures.get(request);
          body.fail(noted !== undefined ? noted.reason : new Error(decoder.decode(bytes)));
        }
      } catch {
        // A push the host cannot take is dropped, and nothing crosses.
      }
    },

    // The milliseconds of the monotonic clock of the host, which the timers
    // of the engine read.
    host_now() {
      try {
        return clock();
      } catch {
        return 0;
      }
    },

    // A timer of the host that calls the export wake of `request` once
    // `delay` milliseconds have passed, where the least timer of its agent
    // is due, kept with the request, so the host clears it with the request.
    host_timer(request, delay) {
      if (state.dead) return;
      try {
        let timers = state.timers.get(request);
        if (timers === undefined) {
          timers = new Set();
          state.timers.set(request, timers);
        }
        const timer = later(() => {
          timers.delete(timer);
          if (state.dead || !state.tables.has(request)) return;
          try {
            call("wake", request);
          } catch {
            // The instance trapped, and condemn answered the requests on it.
          }
        }, delay);
        timers.add(timer);
      } catch {
        // A timer the host cannot set wakes nothing, and nothing crosses.
      }
    },

    // A line the logger of the engine wrote, to console.log of the host, or
    // to console.error for the stream 2, as the sinks write it.
    host_write(stream, at, length) {
      if (state.dead) return;
      try {
        const codec = length <= SHORT ? state.memory.codec : null;
        const line = codec?.decode(at, length) ?? decoder.decode(copy(at, length));
        if (stream === 2) console.error(line);
        else console.log(line);
      } catch {
        // A console that throws writes nothing, and nothing crosses.
      }
    },

    // Settles the promise of the function next of the continuation `number`
    // of `request` with the completion record at `at`, once the rest of the
    // chain has run.
    js_next(request, number, at, length) {
      if (state.dead) return;
      try {
        const nexts = state.nexts.get(request);
        const pending = nexts?.get(number);
        if (pending === undefined) return;
        nexts.delete(number);
        const reader = inPlace(at, length);
        const kind = reader.byte();
        const value = reader.value(0, undefined, scopeOf(request));
        if (kind === NORMAL) pending.resolve(value);
        else pending.reject(value);
      } catch {
        // A broken record settles nothing, and nothing crosses.
      }
    },

    // `length` random bytes of crypto.getRandomValues of the host, written
    // into the input buffer as every record, the randomness of a request id.
    host_random(length) {
      if (state.dead) return 0;
      try {
        const out = new Out();
        out.ensure(length);
        globalThis.crypto.getRandomValues(out.bytes.subarray(0, length));
        out.length = length;
        return send(out);
      } catch {
        return 0;
      }
    },

    // The faithful mode: one microtask of this host for each job of the agent,
    // the microtask jobOf keeps for the request, queued as a reaction of
    // `fulfilled`, the promise this module made fulfilled when it loaded,
    // which V8 queues in the same queue the app's own microtasks run in. The
    // export run_job runs inside a try, whose trap condemns the instance and
    // answers every request on it with the failure of a trap. A microtask the
    // host cannot queue answers its request with a failure once the export
    // that asked for it has returned, since the job would never run, and
    // nothing of the throw reaches the module.
    host_enqueue(request) {
      if (state.dead) return;
      try {
        apply(promiseThen, fulfilled, [jobOf(request)]);
      } catch {
        answerWith(request, UNQUEUED);
      }
    },

    // An outbound request of the engine, through the fetch of the host with
    // the method, the URL, the header fields and the body the engine wrote and
    // redirect manual, as the network capability of the engine follows no
    // redirect, which answers the promise of its answer at once. Its
    // settlement carries the status, the status text, the header fields and
    // the whole body of the response, or the message of the failure, the tag
    // FETCHED, crates/aosabi-js/src/outbound.rs.
    host_fetch(request, at, length) {
      return guarded(request, () => {
        const reader = inPlace(at, length);
        const method = reader.text();
        const url = reader.text();
        const headers = [];
        for (let count = reader.word(); count > 0; count--) headers.push([reader.text(), reader.text()]);
        const body = reader.raw().slice();
        const init = { method, headers, redirect: "manual" };
        if (body.length !== 0 || (method !== "GET" && method !== "HEAD")) init.body = body;
        let pending;
        try {
          pending = hostFetch(url, init);
        } catch (error) {
          pending = HostPromise.reject(error);
        }
        const read = apply(promiseThen, pending, [
          (response) => apply(promiseThen, response.arrayBuffer(), [(buffer) => ({ response, buffer })]),
        ]);
        const held = tableOf(request).push(read) - 1;
        apply(promiseThen, read, [
          (got) => settleFetched(request, held, got, undefined),
          (reason) => settleFetched(request, held, undefined, reason),
        ]);
        // The execution context of a Worker keeps the fetch alive past the
        // answer of the request, as the transport of Toucan hands its fetch to
        // waitUntil, so the envelope of a capture the engine waits on no more
        // still leaves.
        const execution = state.executions.get(request);
        if (typeof execution?.waitUntil === "function") {
          execution.waitUntil(apply(promiseThen, read, [undefined, () => {}]));
        }
        const out = new Out();
        out.byte(NORMAL);
        out.byte(TAG.PROMISE);
        out.word(held);
        return out;
      });
    },

    // A call of the crypto of the host, which answers at once: a byte 1 with
    // the bytes of its answer, or a byte 0 where the host answers none.
    host_crypto(at, length) {
      if (state.dead) return 0;
      try {
        const reader = inPlace(at, length);
        const op = reader.byte();
        const fields = [];
        for (let count = reader.word(); count > 0; count--) fields.push(reader.raw().slice());
        let answer = null;
        try {
          answer = cryptoCall(op, fields);
        } catch {
          answer = null;
        }
        const out = new Out();
        if (answer === null || answer === undefined) {
          out.byte(0);
        } else {
          out.byte(1);
          out.raw(new Uint8Array(answer.buffer, answer.byteOffset, answer.byteLength));
        }
        return send(out);
      } catch {
        return 0;
      }
    },

    host_done(request, address) {
      if (state.dead) return;
      let bytes;
      try {
        bytes = readAnswer(address);
      } catch {
        bytes = UNREAD;
      }
      answerWith(request, bytes);
    },
  };

  state.memory = engine.open(host);
  state.exports = state.memory.exports;
  state.call = call;
  state.send = send;
  state.answerAt = answerAt;
  state.readAnswer = readAnswer;
  state.booted = false;
  // Takes every field of `request` that still waits out of the list of its
  // promise once the request is answered.
  state.release = (request) => {
    const fields = state.fields.get(request);
    if (fields === undefined) return;
    state.fields.delete(request);
    for (const field of fields) forget(field);
  };
  // Drops what the host holds of `request` once its answer is read or the
  // engine released it, calling the export release first where `release`
  // holds: its table, its context, its continuations, its variables, its
  // fields that wait and its timers.
  state.forget = (request, release) => {
    state.failures.delete(request);
    if (release && !state.dead) {
      try {
        call("release", request);
      } catch {
        // The instance trapped, and condemn marked it.
      }
    }
    state.tables.delete(request);
    state.contexts.delete(request);
    state.nexts.delete(request);
    state.variables.delete(request);
    state.clients.delete(request);
    state.executions.delete(request);
    state.jobs.delete(request);
    state.scopes.delete(request);
    state.bridges.delete(request);
    state.release(request);
    for (const timer of state.timers.get(request) ?? []) unlater(timer);
    state.timers.delete(request);
  };
  // Answers the head `bytes` of an answer of `request`, the request `id`,
  // whose body the module pushes after it, with a Response over the stream of
  // the body the pushes fill, once the export wake of the request has started
  // its body. A body no result processor of Yoga writes, the answer of a
  // route of a plugin of the engine, has the first turn of the host to end, a
  // task of a timer of 0 ms, and one that ended there answers whole, as the
  // Node adapter reads a body before it writes the head. A cancel of the stream by its reader calls the export
  // release one microtask later, never inside an import, and the engine
  // pushes the release of the request once the cancel has run.
  state.pushedResponse = async (bytes, request, id) => {
    const body = pushedOf(id);
    body.taken = true;
    if (state.tables.has(id) && !state.dead) {
      try {
        call("wake", id);
      } catch {
        // The instance trapped, and condemn failed the body.
      }
    }
    if ((markOf(bytes) & MARK_STREAM) === 0) {
      let timer = null;
      await HostPromise.race([body.settled(), new HostPromise((resolve) => {
        timer = later(resolve, 0);
      })]);
      unlater(timer);
    }
    const response = responseOf(bytes, request, body, () => {
      // The readers of the bodies of the app the engine pulls for the request
      // are cancelled, as the Node adapter cancels the reader of the body of a
      // response whose client went away.
      for (const held of state.tables.get(id) ?? []) {
        if (typeof held?.[CANCEL] === "function") held[CANCEL]();
      }
      enqueueMicrotask(() => {
        if (state.dead || !state.tables.has(id)) return;
        try {
          call("release", id);
        } catch {
          // The instance trapped, and condemn answered the requests on it.
        }
      });
    });
    if (!state.tables.has(id)) state.pushed.delete(id);
    return response;
  };
  state.ctxCall = ctxCall;
  state.sentryOf = sentryOf;
  state.claimsOf = claimsOf;
  return state;
}

// Returns the copy of the root `name` of the graphql object `graphql` the module
// reads its members from, read as the service reads it when it starts: the
// root once for the test of its truth, and for a root that is true once more for
// Object.keys and, when it has a key, once more for Object.entries, which runs a
// getter of a member once, in the order of the keys. A root that is false
// answers undefined and a true root with no key an empty copy, each of which
// the service leaves out. Each entry is an own data property of an object with no
// prototype, so a read of the module runs no code of the app.
function rootOf(graphql, name) {
  if (!graphql[name]) return undefined;
  const copy = createObject(null);
  if (Object.keys(graphql[name]).length > 0) {
    for (const [key, value] of Object.entries(graphql[name])) {
      defineProperty(copy, key, { __proto__: null, value, writable: true, enumerable: true, configurable: true });
    }
  }
  return copy;
}

// Returns the copy of the graphql object `graphql` of the app whose handle the
// module boots from: an object with no prototype whose members Query, Mutation
// and Subscription are the copies rootOf makes or undefined. The service then reads
// every other key of the object and adds its value to the resolvers of the
// schema, which the engine takes from no app, so such a key fails the boot with
// its name.
function graphqlOf(graphql) {
  const copy = createObject(null);
  for (const name of ROOTS) {
    defineProperty(copy, name, {
      __proto__: null, value: rootOf(graphql, name), writable: true, enumerable: true, configurable: true,
    });
  }
  for (const key of Object.keys(graphql)) {
    if (ROOTS.includes(key)) continue;
    void graphql[key];
    throw new Error("the member " + key + " of graphql has no reading in aosabi, which takes the roots Query, " +
      "Mutation and Subscription alone");
  }
  return copy;
}

// Returns the variables of the process of the host, `process.env`, which
// workerd fills with the text bindings of the Worker under the flag
// nodejs_compat_populate_process_env, or none where the host has no process.
function processEnv() {
  return globalThis.process?.env ?? {};
}

// Returns the plan of an app that hands the core no plugins of its own, its
// plugins the markers of `config`, a plugin with no marker by its name, which
// the boot refuses.
function planOf(config) {
  return {
    plugins: (config.plugins ?? []).map((plugin) => ({ marker: plugin.marker ?? String(plugin.name), options: plugin.options,
      functions: plugin.functions })),
  };
}

// Returns the execution context of the context `c` of hono, or undefined where
// its host hands none, whose getter throws.
function executionOf(c) {
  try {
    return c.executionCtx;
  } catch {
    return undefined;
  }
}

// Returns the type and the value of the exception of the error `error` hono
// set on c, as the middleware of @hono/sentry hands it to captureException
// and the event builder of Toucan reads it: the name and the message of an
// Error and the text of any other value under the type Error, the message No
// error message where it is empty, which the engine writes, plugins/aosabi-sentry,
// middleware.rs.
function exceptionOf(error) {
  if (error instanceof Error) return [String(error.name), String(error.message)];
  return ["Error", String(error)];
}

// The core of the app that holds the request of each context c of hono, by
// c, which a call of the package reads where the store of the async context
// holds none, a provider of aosabi/auth/zitadel called from the stream of a
// body after the answer among them, as a provider reads c alone.
const CORES = new WeakMap();

// Returns the core of the app a step of its base pipeline saw the context `c`
// of hono in, or undefined.
export function coreOf(c) {
  return CORES.get(c);
}

// Instantiates the compiled module `module`, or opens the engine `engine`
// that aosabi/bun hands, the native library or core.wasm, and boots it with
// the SDL `schema`, the JSON text `ir` of the IR of the same build, the
// graphql object `graphql` of the app, which holds its roots, as the glue of
// a build hands it to handler(), its configuration `config`, the
// default export of its aosabi.config.ts, the plan `plan` of its plugins and
// of its type resolvers, which the entry aosabi/pylon makes of the app, and
// the variables `env` of the process of the host, and returns the core of the
// app. The graphql object is read once here, as graphqlOf reads it, and every
// instance boots over the same copy. The configuration crosses as its options
// graphiql, landingPage and logger and its plugins, the plan as its plugins,
// each function a handle of the table of the boot. The job mode `mode` is
// faithful, one microtask per job of the engine, or fast, and the read mode
// `read` is get, one import per read, or project, one import per projected
// object. A request whose instance traps answers with a failure of status
// 500, a request after a trap runs on a fresh instance, and `instances`
// counts the instances made. Where primeCore opened an instance of the same
// engine for the logger of the app before its boot, `primed` is that state,
// and the app boots over it, so the logger keeps the records and the sinks
// of the passes of executeConfig. The text `ir` may be the schema record of
// the build in place of the IR.
//
// The routes and the middleware of the app run in its hono, lib/pylon.js,
// and the core answers the requests the hono of the app hands it: the
// GraphQL endpoint and the routes of the plugins of the engine. A request is
// the context c of hono it runs in. The step of the request logger of
// the base pipeline opens it in the engine with the logger of the
// request, open(c, true), a middleware of a plugin of the engine opens it
// where it needs it first, and answer(c) hands it to the app of the engine.
// The step of the logger writes the access line once its chain has run,
// note(c), and the request ends once the step of the async context bind of
// that pipeline has released it and the fetch of the app that dispatches it
// has ended, end(c), so the variables the engine sets for the request, the
// client of Sentry and the loggers derived while it runs live as long as the
// request, for a middleware the app registered before the base pipeline as
// well. A request no step opened, of an app the glue booted with no base
// pipeline, answers through answer(c) alone.
export function createCore({
  module, engine, schema, ir, graphql = {}, config = {}, mode = "faithful", read = "get", plan, env, primed,
}) {
  const source = engine ?? wasmEngine(module);
  if (!(mode in MODES)) throw new Error("no job mode named " + mode);
  if (!(read in READS)) throw new Error("no read mode named " + read);
  if (typeof ir !== "string") throw new Error("the core boots from the JSON text of the IR of the build or from its schema record");
  const copy = graphqlOf(graphql);
  const functions = [];
  const handles = new Map();
  const handle = (fn) => {
    if (typeof fn !== "function") throw new TypeError("a handler of the app is no function: " + String(fn));
    if (!handles.has(fn)) {
      functions.push(fn);
      handles.set(fn, functions.length);
    }
    return handles.get(fn);
  };
  const boot = bootRecord(schema, ir, config, mode, read, env ?? processEnv(), plan ?? planOf(config), handle);
  boot.kept = true;
  let live = primed !== undefined && primed !== null && !primed.booted && !primed.dead
    ? bootApp(primed, copy, boot, functions)
    : instantiate(source, copy, boot, functions);
  let instances = 1;
  let next = 0;

  // The requests of the app the core holds, by the context c of hono each
  // runs in: the instance, the number, the bridge and the logger of the
  // request once a step opened it in the engine, and the holds of the steps
  // that end it.
  const held = new WeakMap();
  // The fetches of the app that still dispatch a web standard Request of the
  // host, by that Request, and the context c of hono a step made the request
  // of, by its Request, so the request of the engine lives until the chain
  // of the app has run whole.
  const dispatching = new WeakMap();
  const contexts = new WeakMap();

  // Returns the request the core holds for the context `c`, made at the
  // first call.
  const heldOf = (c) => {
    let request = held.get(c);
    if (request === undefined) {
      request = { state: undefined, id: 0, bridge: undefined, logger: undefined, holds: 0 };
      held.set(c, request);
      contexts.set(c.req.raw, c);
      CORES.set(c, core);
    }
    return request;
  };

  // Returns the next number of a request.
  const nextId = () => {
    next = next >= 0x7ffffffe ? 1 : next + 1;
    return next;
  };

  // Returns the live instance, made anew where the one before trapped.
  const liveState = () => {
    if (live.dead) {
      live = instantiate(source, copy, boot, functions);
      instances += 1;
    }
    return live;
  };

  // Writes the record of the request of the context `c` into a new table of
  // `state` under `id` and notes c as its context, with the execution
  // context of its host, and answers the record.
  const enter = (state, id, c) => {
    const table = [];
    const record = recordOf(c.req.raw, table);
    state.tables.set(id, table);
    state.contexts.set(id, c);
    const executionCtx = executionOf(c);
    if (executionCtx !== undefined) state.executions.set(id, executionCtx);
    return record;
  };

  // Answers the request `id` of `state` with the record `record` through the
  // export fetch, an empty record for a request the engine holds open, and
  // returns the web standard Response of the request `raw`, whose body the
  // module pushes after its head where it is a stream. A request no step
  // opened is dropped once its answer is read, and an open one where its
  // step ends it.
  const answerWith = async (state, id, record, raw, opened) => {
    let pushed = false;
    try {
      let bytes;
      try {
        const address = state.call("fetch", state.send(record), id);
        if (address !== 0) {
          bytes = state.readAnswer(address);
        } else if (state.done.has(id)) {
          bytes = state.done.get(id);
        } else {
          bytes = await new HostPromise((resolve) => {
            if (state.dead) resolve(TRAPPED);
            else state.waiting.set(id, resolve);
          });
        }
      } catch (error) {
        // A throw of an export is a trap, which condemn has marked, and any
        // other throw is the wrapper's own and rejects the fetch.
        if (!state.dead) throw error;
        bytes = TRAPPED;
      }
      if (!state.dead && (markOf(bytes) & MARK_PUSHED) !== 0) {
        pushed = true;
        return state.pushedResponse(bytes, raw, id);
      }
      return responseOf(bytes, raw);
    } finally {
      state.waiting.delete(id);
      state.done.delete(id);
      if (!pushed && !opened) state.forget(id, true);
    }
  };

  // Ends the request `id` of `state` a step opened, the op END of
  // crates/aosabi-js/src/context.rs with no access line: the engine drops
  // the request, at once or where the body it pushes ends.
  const close = (state, id) => {
    if (state.dead) return;
    const out = new Out();
    out.byte(CTX.END);
    out.byte(0);
    out.byte(1);
    try {
      state.ctxCall(id, out);
    } finally {
      if (!state.pushed.has(id)) state.forget(id, false);
    }
  };

  // Runs `call` with a request of its own for the context `c`, opened in the
  // engine over the request of c with no logger and ended once the promise
  // `call` answers has settled, for a call that comes after the end of the
  // request of c, which the core holds no longer: a provider of
  // aosabi/auth/zitadel called from the stream of a body after the answer, which
  // reads the request of c as a provider reads c after the answer.
  const alone = (c, call) => {
    const state = liveState();
    const id = nextId();
    const out = new Out();
    out.byte(CTX.OPEN);
    out.byte(0);
    out.append(enter(state, id, c));
    let answered;
    try {
      state.ctxCall(id, out);
      answered = call(state, id);
    } catch (error) {
      close(state, id);
      return HostPromise.reject(error);
    }
    return HostPromise.resolve(answered).then((value) => {
      close(state, id);
      return value;
    }, (error) => {
      close(state, id);
      throw error;
    });
  };

  const core = {
    get instances() {
      return instances;
    },

    get engine() {
      return source.kind;
    },

    // The requests of the live instance whose table the host still holds,
    // those in flight and those whose body the engine still pushes.
    get requests() {
      return live.tables.size - 1;
    },

    // The bytes of the linear memory of the live instance of the module,
    // which grows and never shrinks, so they are the peak of that instance,
    // or null for the native library, whose memory is the process's.
    get memory() {
      const memory = live.exports?.memory;
      return memory instanceof WebAssembly.Memory ? memory.buffer.byteLength : null;
    },

    // Whether the live instance hands the strings of its records through
    // strings.wasm, which a host hands beside core.wasm and the builtins
    // js-string of V8 link (round JSSTRING), or takes TextEncoder and
    // TextDecoder.
    get codec() {
      return Boolean(live.memory?.codec);
    },

    // Opens the request of the context `c` of hono in the engine, the op OPEN
    // of crates/aosabi-js/src/context.rs, at the first call for c, and with
    // `logger` true makes the logger of the request there, once, and answers
    // the request the core holds for c: its instance, its number and the two
    // numbers of the handle of its logger.
    open(c, logger = false) {
      const request = heldOf(c);
      if (request.state !== undefined && (!logger || request.logger !== undefined || request.state.dead)) {
        return request;
      }
      const out = new Out();
      out.byte(CTX.OPEN);
      out.byte(logger ? 1 : 0);
      if (request.state === undefined) {
        const state = liveState();
        const id = nextId();
        out.append(enter(state, id, c));
        request.state = state;
        request.id = id;
        request.bridge = state.bridgeOf(id);
      }
      const handle = request.state.ctxCall(request.id, out);
      if (Array.isArray(handle)) request.logger = handle;
      return request;
    },

    // Holds the request of the context `c` of hono until a matching release,
    // a step of the chain that ends it: the async context bind of the base
    // pipeline and the middleware of Sentry of the canary, which
    // stands before that pipeline. A hold opens nothing in the engine.
    hold(c) {
      heldOf(c).holds += 1;
    },

    // Releases a hold of the request of the context `c` of hono, and ends
    // the request once the last hold is released and the fetch that
    // dispatches it has ended.
    release(c) {
      const request = held.get(c);
      if (request === undefined) return;
      request.holds -= 1;
      if (request.holds <= 0 && !dispatching.has(c.req.raw)) this.end(c);
    },

    // Notes that a fetch of the app dispatches the web standard Request `raw`
    // of the host, until dispatched(raw).
    dispatch(raw) {
      dispatching.set(raw, (dispatching.get(raw) ?? 0) + 1);
    },

    // Notes that the fetch of the Request `raw` has ended, and ends the
    // request of the context a step made it of once no step holds it.
    dispatched(raw) {
      const count = (dispatching.get(raw) ?? 1) - 1;
      if (count > 0) {
        dispatching.set(raw, count);
        return;
      }
      dispatching.delete(raw);
      const c = contexts.get(raw);
      if (c !== undefined && (held.get(c)?.holds ?? 0) <= 0) this.end(c);
    },

    // Answers the request of the context `c` of hono on what `to` names, the
    // server of the GraphQL endpoint or the app of the engine, and returns
    // its web standard Response: the request the core holds open for c, or a
    // request of its own where no step opened one.
    answer(c, to = TO.APP) {
      const request = held.get(c);
      if (request === undefined || request.holds === 0) {
        const state = liveState();
        const id = nextId();
        const record = enter(state, id, c);
        record.byte(to);
        return answerWith(state, id, record, c.req.raw, false);
      }
      const { state, id } = this.open(c);
      if (state.dead) return HostPromise.resolve(responseOf(TRAPPED, c.req.raw));
      const record = new Out();
      record.byte(to);
      return answerWith(state, id, record, c.req.raw, true);
    },

    // Writes the access line of the request of the context `c`, its status
    // and the milliseconds it took, through the logger of the request where
    // the access line is on, the op END of crates/aosabi-js/src/context.rs
    // with no end, as the step of the logger of the base pipeline writes
    // it once the rest of its chain has run.
    note(c, status, durationMs) {
      const request = held.get(c);
      if (request?.state === undefined || request.state.dead) return;
      const out = new Out();
      out.byte(CTX.END);
      out.byte(1);
      out.word(status);
      out.number(durationMs);
      out.byte(0);
      request.state.ctxCall(request.id, out);
    },

    // Ends the request of the context `c` the core holds, the op END of
    // crates/aosabi-js/src/context.rs with no access line: the engine drops
    // the request, at once or where the body it pushes ends.
    end(c) {
      const request = held.get(c);
      if (request === undefined) return;
      held.delete(c);
      contexts.delete(c.req.raw);
      if (request.state !== undefined) close(request.state, request.id);
    },

    // Answers the variable `key` the engine set in the context it runs the
    // request of the context `c` in, the bag of @inContext and the info a
    // root resolver stores under graphqlResolveInfo, or undefined where the
    // core holds no request for c.
    variable(c, key) {
      const request = held.get(c);
      const state = request?.state;
      if (state === undefined || state.dead || !state.tables.has(request.id)) return undefined;
      const out = new Out();
      out.byte(CTX.GET);
      out.text(String(key));
      return request.state.ctxCall(request.id, out);
    },

    // Answers the names of the variables the engine set in that context, in
    // the order they were first set.
    variableNames(c) {
      const request = held.get(c);
      const state = request?.state;
      if (state === undefined || state.dead || !state.tables.has(request.id)) return [];
      const out = new Out();
      out.byte(CTX.VAR_NAMES);
      return request.state.ctxCall(request.id, out);
    },

    // Builds the client of Sentry of the request of the context `c`, the
    // marker useSentry of the number `marker` among those of the
    // configuration or, for undefined, the client of the base pipeline of
    // the canary, and answers the facade of lib/toucan.js over it, or
    // undefined where the request holds no client.
    sentry(c, marker) {
      const { state, id } = this.open(c);
      const out = new Out();
      out.byte(CTX.SENTRY);
      out.byte(SENTRY_ACT.START);
      out.word(marker === undefined ? CANARY_CLIENT : marker);
      return state.ctxCall(id, out) === true ? state.sentryOf(id) : undefined;
    },

    // Reports the error `error` hono set on the context `c` through the
    // client of Sentry of its request, as the middleware of @hono/sentry
    // reports it once the rest of the chain has run.
    capture(c, error) {
      const request = held.get(c);
      if (request?.state === undefined || request.state.dead) return;
      const [name, message] = exceptionOf(error);
      const out = new Out();
      out.byte(CTX.SENTRY);
      out.byte(SENTRY_ACT.CAPTURE);
      out.word(0);
      out.text(name);
      out.text(message);
      request.state.ctxCall(request.id, out);
    },

    // Answers the promise of the claims of the request of the context `c`
    // that a provider of aosabi/auth/zitadel over the issuer `issuer` and the
    // key path `keyPath` answers, crates/aosabi-js/src/zitadel.rs, on the
    // request the core holds for c, or on a request of its own where the
    // request of c has ended, `alone`.
    claims(c, issuer, keyPath) {
      if (!held.has(c)) return alone(c, (state, id) => state.claimsOf(id, issuer, keyPath));
      const { state, id } = this.open(c);
      return state.claimsOf(id, issuer, keyPath);
    },
  };
  return core;
}

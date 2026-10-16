// SPDX-License-Identifier: MIT
/*
 * nodes.js: the views of the package aosabi of the resolve info, of the nodes
 * of the document and of the arguments of an execution, which answer the
 * members of graphql-js an app reads, each when it is read, over what the
 * engine holds of a request.
 *
 * A view answers the members of the objects of graphql-js an app reads there:
 * GraphQLResolveInfo of @graphql-tools/executor 1.5.1 at
 * esm/execution/execute.js:405-421 of that package, the nodes of graphql
 * 16.12.0 its parser makes at language/parser.js of that package, and the
 * arguments of an execution of graphql-yoga 5.18.0 at
 * esm/process-request.js:44-50 of that package, which Envelop hands the hooks
 * onExecute and onExecuteDone.
 */

// The engine numbers each info, each store of a call of getResolveInfo(),
// each node of the document and the arguments of an execution of a request,
// and answers the record of what a view answers at once:
// crates/aosabi-js/src/nodes.rs. The views of a request stand in a map of the
// context c of the request, so the engine and the host name one object by one
// number, and a node an app reaches two ways is one object, as graphql-js
// hands every place one node. A member whose value is large or lies deeper,
// the selections of a selection set, the definitions of the document and the
// variables, is a getter that asks the engine for it at its first read and
// keeps the answer as a data member of the view, so every later read answers
// the same value. A member of graphql-js whose value the engine holds no form
// for is a getter that throws a TypeError naming the waiting, as the stand ins
// of the nodes of an error of a done hook in lib/core.js do, and every other
// own member of the object of graphql-js stands in its order, so the keys of a
// view are the keys of graphql-js.

// The ops of the export ctx the views call, crates/aosabi-js/src/nodes.rs.
const OP = { NODE: 28, ARGS: 29, INFO_VARIABLES: 30, ARGS_VARIABLES: 31 };

// The byte of an answer of the export ctx that holds a view record, beside 0
// for a value of the engine and 1 for a value record the host wrote,
// crates/aosabi-js/src/nodes.rs.
export const VIEW = 2;

// The kinds of a view record, of the record of a node and of the record of a
// value node, crates/aosabi-js/src/nodes.rs.
const RECORD = { INFO: 0, CALL: 1, ARGS: 2, CHILDREN: 3, JSON: 4, VARIABLES: 5 };
const NODE = { OPERATION: 0, FRAGMENT: 1, FIELD: 2, SPREAD: 3, INLINE: 4, DOCUMENT: 5, OTHER: 6 };
const VALUE = { VARIABLE: 0, INT: 1, FLOAT: 2, STRING: 3, BOOLEAN: 4, NULL: 5, ENUM: 6, LIST: 7, OBJECT: 8, DEEP: 9 };

// The depth past which the engine writes the mark of a value node,
// crates/aosabi-js/src/nodes.rs.
const DEPTH = 64;

// The tag of a plain object in a value record of the module,
// crates/aosabi-js/src/host.rs.
const PLAIN = 12;

// The objects whose members a refusal names.
const NODE_WHAT = "a node of the document";
const INFO_WHAT = "the resolve info";
const ARGS_WHAT = "the arguments of an execution";
const TYPE_WHAT = "a type of the resolve info";
const DEEP_WHAT = "a value node nested deeper than " + DEPTH + " levels";

const defineProperty = Object.defineProperty;
const createObject = Object.create;

// The views of each request by the context c of the request, which goes with
// its request, and the views with it.
const requests = new WeakMap();

// Returns the views of the request of `scope`.
function viewsOf(scope) {
  const key = scope.context();
  if (key === undefined) throw new TypeError("a view of aosabi reads a request that has ended");
  let views = requests.get(key);
  if (views === undefined) {
    views = { infos: new Map(), calls: new Map(), args: new Map(), variables: new Map(), nodes: new Map(),
      types: new Map() };
    requests.set(key, views);
  }
  return views;
}

// Returns the message of a member `key` of `what` whose value the engine
// holds no form for.
function waiting(key, what) {
  return "the member " + key + " of " + what + " of aosabi waits for a fixture that needs it";
}

// Defines the own data member `key` of `object` with `value`, as a parser of
// graphql-js makes one.
function data(object, key, value) {
  defineProperty(object, key, { __proto__: null, value, writable: true, enumerable: true, configurable: true });
}

// Defines the own member `key` of `object` whose first read answers `make()`
// and keeps it as a data member.
function lazy(object, key, make) {
  defineProperty(object, key, {
    __proto__: null,
    get() {
      const value = make();
      data(object, key, value);
      return value;
    },
    set(value) {
      data(object, key, value);
    },
    enumerable: true,
    configurable: true,
  });
}

// Defines the own member `key` of `object` of `what` whose read throws a
// TypeError naming the waiting, enumerable unless `hidden`, and which a write
// makes a data member, as a member of graphql-js takes a write.
function refused(object, key, what, hidden = false) {
  defineProperty(object, key, {
    __proto__: null,
    get() {
      throw new TypeError(waiting(key, what));
    },
    set(value) {
      data(object, key, value);
    },
    enumerable: !hidden,
    configurable: true,
  });
}

// Returns a prototype whose members `names` are getters that throw a
// TypeError naming the waiting, with `methods` beside them.
function refusing(what, names, methods) {
  const prototype = {};
  for (const name of names) {
    defineProperty(prototype, name, {
      __proto__: null,
      get() {
        throw new TypeError(waiting(name, what));
      },
      configurable: true,
    });
  }
  for (const [name, value] of Object.entries(methods)) {
    defineProperty(prototype, name, { __proto__: null, value, writable: true, configurable: true });
  }
  return prototype;
}

// The prototypes of the views of a type of the resolve info: a named type with
// its name, and the two wrappers with the type they wrap, each with the
// toString and the toJSON of graphql-js, and the members of a named type the
// engine holds no form for refused.
const NAMED_TYPE = refusing(TYPE_WHAT, ["description", "astNode", "extensions", "extensionASTNodes", "getFields",
  "getInterfaces", "getTypes", "getValues", "getValue", "isTypeOf", "resolveType", "serialize", "parseValue",
  "parseLiteral", "specifiedByURL", "toConfig"], {
  toString() {
    return this.name;
  },
  toJSON() {
    return this.toString();
  },
});
const LIST_TYPE = refusing(TYPE_WHAT, [], {
  toString() {
    return "[" + String(this.ofType) + "]";
  },
  toJSON() {
    return this.toString();
  },
});
const NON_NULL_TYPE = refusing(TYPE_WHAT, [], {
  toString() {
    return String(this.ofType) + "!";
  },
  toJSON() {
    return this.toString();
  },
});

// Returns the view of the type the schema prints as `printed`, one object per
// request and type, as a schema of graphql-js holds one object per type.
function typeOf(views, printed) {
  let type = views.types.get(printed);
  if (type !== undefined) return type;
  if (printed.endsWith("!")) {
    type = createObject(NON_NULL_TYPE);
    data(type, "ofType", typeOf(views, printed.slice(0, -1)));
  } else if (printed.startsWith("[") && printed.endsWith("]")) {
    type = createObject(LIST_TYPE);
    data(type, "ofType", typeOf(views, printed.slice(1, -1)));
  } else {
    type = createObject(NAMED_TYPE);
    data(type, "name", printed);
  }
  views.types.set(printed, type);
  return type;
}

// Returns the node Name of `value`.
function nameNode(value) {
  const node = {};
  data(node, "kind", "Name");
  data(node, "value", value);
  refused(node, "loc", NODE_WHAT);
  return node;
}

// Returns the node NamedType of the name `name`, a type condition.
function namedTypeNode(name) {
  const node = {};
  data(node, "kind", "NamedType");
  data(node, "name", nameNode(name));
  refused(node, "loc", NODE_WHAT);
  return node;
}

// Returns the node SelectionSet of the node `number`, whose selections the
// engine answers at their first read.
function selectionSetNode(scope, number) {
  const node = {};
  data(node, "kind", "SelectionSet");
  lazy(node, "selections", () => scope.call(OP.NODE, (out) => out.word(number)));
  refused(node, "loc", NODE_WHAT);
  return node;
}

// Returns a value node of the record at the cursor of `reader`, the value of
// an argument, nested `depth` levels deep, in the members graphql-js gives
// each kind.
function readValue(reader, depth = 0) {
  const node = {};
  const kind = reader.byte();
  switch (kind) {
    case VALUE.VARIABLE:
      data(node, "kind", "Variable");
      data(node, "name", nameNode(reader.text()));
      break;
    case VALUE.INT:
    case VALUE.FLOAT:
    case VALUE.ENUM:
      data(node, "kind", kind === VALUE.INT ? "IntValue" : kind === VALUE.FLOAT ? "FloatValue" : "EnumValue");
      data(node, "value", reader.text());
      break;
    case VALUE.STRING:
      data(node, "kind", "StringValue");
      data(node, "value", reader.text());
      data(node, "block", reader.byte() === 1);
      break;
    case VALUE.BOOLEAN:
      data(node, "kind", "BooleanValue");
      data(node, "value", reader.byte() === 1);
      break;
    case VALUE.NULL:
      data(node, "kind", "NullValue");
      break;
    case VALUE.LIST: {
      const values = new Array(reader.word());
      for (let i = 0; i < values.length; i++) values[i] = readValue(reader, depth + 1);
      data(node, "kind", "ListValue");
      data(node, "values", values);
      break;
    }
    case VALUE.OBJECT: {
      const fields = new Array(reader.word());
      for (let i = 0; i < fields.length; i++) {
        const field = {};
        data(field, "kind", "ObjectField");
        data(field, "name", nameNode(reader.text()));
        data(field, "value", readValue(reader, depth + 1));
        refused(field, "loc", NODE_WHAT);
        fields[i] = field;
      }
      data(node, "kind", "ObjectValue");
      data(node, "fields", fields);
      break;
    }
    case VALUE.DEEP:
      refused(node, "kind", DEEP_WHAT);
      return node;
    default:
      throw new Error("the module wrote a broken record");
  }
  refused(node, "loc", NODE_WHAT);
  return node;
}

// Returns the description of a definition at the cursor of `reader`, a node
// StringValue or undefined.
function readDescription(reader) {
  return reader.byte() === 1 ? readValue(reader) : undefined;
}

// Returns a name at the cursor of `reader` that may be absent.
function readName(reader) {
  return reader.byte() === 1 ? reader.text() : undefined;
}

// Returns the nodes Argument of the record at the cursor of `reader`.
function readArguments(reader) {
  const args = new Array(reader.word());
  for (let i = 0; i < args.length; i++) {
    const argument = {};
    data(argument, "kind", "Argument");
    data(argument, "name", nameNode(reader.text()));
    data(argument, "value", readValue(reader));
    refused(argument, "loc", NODE_WHAT);
    args[i] = argument;
  }
  return args;
}

// Returns the view of the node of the record at the cursor of `reader`, the
// one view of its number in the request of `scope`, with the members its
// parser of graphql-js gives each kind in their order: the members a walk of a
// selection reads answered, a selection set and the definitions of the
// document at their first read, and the directives, the variable definitions,
// the location and the token count refused.
function readNode(reader, scope, views) {
  const number = reader.word();
  const kind = reader.byte();
  const node = {};
  switch (kind) {
    case NODE.DOCUMENT:
      data(node, "kind", "Document");
      lazy(node, "definitions", () => scope.call(OP.NODE, (out) => out.word(number)));
      refused(node, "loc", NODE_WHAT);
      refused(node, "tokenCount", NODE_WHAT, true);
      break;
    case NODE.OPERATION: {
      data(node, "kind", "OperationDefinition");
      data(node, "operation", reader.text());
      data(node, "description", readDescription(reader));
      const name = readName(reader);
      data(node, "name", name === undefined ? undefined : nameNode(name));
      refused(node, "variableDefinitions", NODE_WHAT);
      refused(node, "directives", NODE_WHAT);
      data(node, "selectionSet", selectionSetNode(scope, number));
      refused(node, "loc", NODE_WHAT);
      break;
    }
    case NODE.FRAGMENT:
      data(node, "kind", "FragmentDefinition");
      data(node, "description", readDescription(reader));
      data(node, "name", nameNode(reader.text()));
      data(node, "typeCondition", namedTypeNode(reader.text()));
      refused(node, "directives", NODE_WHAT);
      data(node, "selectionSet", selectionSetNode(scope, number));
      refused(node, "loc", NODE_WHAT);
      break;
    case NODE.FIELD: {
      data(node, "kind", "Field");
      const alias = readName(reader);
      data(node, "alias", alias === undefined ? undefined : nameNode(alias));
      data(node, "name", nameNode(reader.text()));
      data(node, "arguments", readArguments(reader));
      refused(node, "directives", NODE_WHAT);
      data(node, "selectionSet", reader.byte() === 1 ? selectionSetNode(scope, number) : undefined);
      refused(node, "loc", NODE_WHAT);
      break;
    }
    case NODE.SPREAD:
      data(node, "kind", "FragmentSpread");
      data(node, "name", nameNode(reader.text()));
      refused(node, "directives", NODE_WHAT);
      refused(node, "loc", NODE_WHAT);
      break;
    case NODE.INLINE: {
      data(node, "kind", "InlineFragment");
      const typeCondition = readName(reader);
      data(node, "typeCondition", typeCondition === undefined ? undefined : namedTypeNode(typeCondition));
      refused(node, "directives", NODE_WHAT);
      data(node, "selectionSet", selectionSetNode(scope, number));
      refused(node, "loc", NODE_WHAT);
      break;
    }
    case NODE.OTHER:
      data(node, "kind", reader.text());
      refused(node, "loc", NODE_WHAT);
      break;
    default:
      throw new Error("the module wrote a broken record");
  }
  const known = views.nodes.get(number);
  if (known !== undefined) return known;
  views.nodes.set(number, node);
  return node;
}

// Returns the views of a count and the records of as many nodes at the
// cursor of `reader`.
function readNodes(reader, scope, views) {
  const nodes = new Array(reader.word());
  for (let i = 0; i < nodes.length; i++) nodes[i] = readNode(reader, scope, views);
  return nodes;
}

// Returns the path of the record at the cursor of `reader`, the objects of
// addPath of graphql-js from the root, each key a string or a number with the
// name of the type it stands in.
function readPath(reader) {
  let path;
  for (let count = reader.word(); count > 0; count--) {
    const key = reader.byte() === 0 ? reader.text() : reader.number();
    const typename = reader.byte() === 1 ? reader.text() : undefined;
    path = { prev: path, key, typename };
  }
  return path;
}

// Returns the view of the info of the record at the cursor of `reader`, the
// one view of its number in the request of `scope`, with the members of the
// info of @graphql-tools/executor 1.5.1 in their order: the name of the field,
// its nodes, its return type, its parent type, its path, the fragments of the
// document, its root value, which Yoga leaves undefined, the operation, its
// variables at their first read, the one object of every info of the
// execution, and its signal, which Yoga leaves undefined, and the schema
// refused.
function readInfo(reader, scope, views) {
  const number = reader.word();
  const fieldName = reader.text();
  const returnType = reader.text();
  const parentType = reader.text();
  const path = readPath(reader);
  const fieldNodes = readNodes(reader, scope, views);
  const fragments = createObject(null);
  for (let count = reader.word(); count > 0; count--) {
    const name = reader.text();
    data(fragments, name, readNode(reader, scope, views));
  }
  const operation = readNode(reader, scope, views);
  const rootValueHeld = reader.byte() === 1;
  const known = views.infos.get(number);
  if (known !== undefined) return known;
  const info = {};
  data(info, "fieldName", fieldName);
  data(info, "fieldNodes", fieldNodes);
  data(info, "returnType", typeOf(views, returnType));
  data(info, "parentType", typeOf(views, parentType));
  data(info, "path", path);
  refused(info, "schema", INFO_WHAT);
  data(info, "fragments", fragments);
  if (rootValueHeld) refused(info, "rootValue", INFO_WHAT);
  else data(info, "rootValue", undefined);
  data(info, "operation", operation);
  lazy(info, "variableValues", () => scope.call(OP.INFO_VARIABLES, (out) => out.word(number)));
  data(info, "signal", undefined);
  views.infos.set(number, info);
  return info;
}

// Returns the variables of an execution at the cursor of `reader`, a plain
// object whose members are read as lib/core.js reads the members of a value
// record, each value from the depth 0. The walk of the arguments that writes
// the value of a variable counts its levels from that value, as it does for an
// argument of a call, push_variables of crates/aosabi-js/src/value.rs, so a
// variable nests as deep as an argument of the same value before the bound of
// DEPTH levels stops both.
function readVariables(reader, scope) {
  if (reader.byte() !== PLAIN) throw new Error("the module wrote a broken record");
  const values = scope.variables();
  const variables = {};
  for (let i = reader.word(); i > 0; i--) {
    const key = reader.text();
    data(variables, key, reader.value(0, values, scope));
  }
  return variables;
}

// Returns the value of a view record of the export ctx at the cursor of
// `reader`, for the request of `scope`: an info, the store of a call of
// getResolveInfo(), the operation name and the document of the arguments of an
// execution, the views of the children of a node, a copy of JSON data or the
// variables of an execution. The variables are the one object of their number
// in the request, each value in the form the engine writes for the host, a
// date a Date and an object of a default of the operation an object with no
// prototype, and the value of a variable that is an object or a list the one
// value of `scope.variables()`, which the arguments of the request read as
// well, as graphql-js hands the info and every argument the one value it
// coerced. The store of a call is the one object of its number in the request,
// its info and its selected fields, each its name, its nodes and its type, as
// the execution context of a request holds them.
export function readView(reader, scope) {
  const views = viewsOf(scope);
  switch (reader.byte()) {
    case RECORD.INFO:
      return readInfo(reader, scope, views);
    case RECORD.CALL: {
      const number = reader.word();
      const info = readInfo(reader, scope, views);
      const selectedFields = new Array(reader.word());
      for (let i = 0; i < selectedFields.length; i++) {
        const name = reader.text();
        const returnType = reader.byte() === 1 ? typeOf(views, reader.text()) : undefined;
        const fieldNodes = readNodes(reader, scope, views);
        selectedFields[i] = { name, fieldNodes, returnType };
      }
      const known = views.calls.get(number);
      if (known !== undefined) return known;
      const store = { info, selectedFields };
      views.calls.set(number, store);
      return store;
    }
    case RECORD.ARGS: {
      const operationName = reader.json();
      const document = readNode(reader, scope, views);
      return { operationName, document };
    }
    case RECORD.CHILDREN:
      return readNodes(reader, scope, views);
    case RECORD.JSON:
      return reader.json();
    case RECORD.VARIABLES: {
      const number = reader.word();
      const known = views.variables.get(number);
      if (known !== undefined) return known;
      const variables = readVariables(reader, scope);
      views.variables.set(number, variables);
      return variables;
    }
    default:
      throw new Error("the module wrote a broken record");
  }
}

// Returns the view of the arguments `number` of an execution of the request of
// `scope`, the one object Envelop hands the hooks onExecute and onExecuteDone
// of an execution, with the members of the arguments Yoga makes in their
// order: the document and the operation name at their first read, through one
// call of the engine, the variables the request parser read at their first
// read, and the schema and the context value refused. Yoga leaves out the root
// value and the resolvers, so the view leaves them out as well.
export function argsOf(scope, number) {
  const views = viewsOf(scope);
  const known = views.args.get(number);
  if (known !== undefined) return known;
  let described;
  const describe = () => (described ??= scope.call(OP.ARGS, (out) => out.word(number)));
  const args = {};
  refused(args, "schema", ARGS_WHAT);
  lazy(args, "document", () => describe().document);
  refused(args, "contextValue", ARGS_WHAT);
  lazy(args, "variableValues", () => scope.call(OP.ARGS_VARIABLES, (out) => out.word(number)));
  lazy(args, "operationName", () => describe().operationName);
  views.args.set(number, args);
  return args;
}

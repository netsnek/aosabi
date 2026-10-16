// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * errors.js: the error classes of the package aosabi, GraphQLError, ServiceError, createGraphQLError
 * and ForbiddenError, which stamp the private brand the module reads through the import js_error.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/core/define-pylon.ts and
 * packages/pylon/src/auth/errors.ts, Apache-2.0. Changed by the Aosabi authors.
 *
 * The class HTTPException is hono's own, from the real hono the entry
 * aosabi/pylon runs the routes of an app in.
 *
 * Derived from graphql-js 16.12.0, src/error/GraphQLError.ts, MIT.
 * Derived from graphql-js 16.12.0, src/language/location.ts, MIT.
 * Derived from graphql-js 16.12.0, src/language/printLocation.ts, MIT.
 */

// A masking that tells a GraphQLError from any other error with instanceof
// over the one class of graphql-js, and an auth that tells its ForbiddenError
// apart the same way, would miss them. An app on the engine imports these classes from the
// entries of the package in place of graphql, graphql-yoga and the root and
// the auth of the compatibility entry, so each constructor stamps a brand the module
// reads through the import js_error, and the engine maps the value onto the
// error a throw of a Rust app makes. The brand is a symbol no other module
// holds, on a property no enumeration of string keys lists, so JSON.stringify,
// Object.keys and the inspect of graphql-js see each instance as they see the
// instance of the original class. A subclass, ServiceError among them, carries
// the brand of the class it extends, as instanceof holds for it.

// The brand of an error, the class family it was made by.
const BRAND = Symbol("aosabi.error.brand");

// The kinds a brand names, and the kind of the intrinsic AggregateError, which
// the engine raises toward the app with its errors.
export const GRAPHQL = "GraphQLError";
export const FORBIDDEN = "ForbiddenError";
export const AGGREGATE = "AggregateError";

// The line ends of a source, the expression of location.ts.
const LINE_ENDS = /\r\n|[\n\r]/g;

// Stamps `error` with the brand `kind`, a property that neither enumerates nor
// changes.
function stamp(error, kind) {
  Object.defineProperty(error, BRAND, { value: kind });
}

// Returns the kind the brand of `value` names, or undefined for a value no
// class of this module made. A thrown value that is no object carries no
// brand. The read goes through the prototype chain, as instanceof does, and
// it runs the trap of a Proxy, so the caller reads it inside a try.
export function brandOf(value) {
  if (value === null || (typeof value !== "object" && typeof value !== "function")) return undefined;
  const kind = value[BRAND];
  return kind === GRAPHQL || kind === FORBIDDEN ? kind : undefined;
}

// Returns whether `value` is an object and not null, isObjectLike of graphql-js.
function isObjectLike(value) {
  return typeof value == "object" && value !== null;
}

// Returns `array`, or undefined for one that is empty or absent.
function undefinedIfEmpty(array) {
  return array === undefined || array.length === 0 ? undefined : array;
}

// Returns the options of a GraphQLError from the arguments after its message:
// the one object of options, or the positional form of graphql 15 when the
// first argument is absent, a node or an array of nodes.
function toNormalizedOptions(args) {
  const firstArg = args[0];
  if (firstArg == null || "kind" in firstArg || "length" in firstArg) {
    return {
      nodes: firstArg,
      source: args[1],
      positions: args[2],
      path: args[3],
      originalError: args[4],
      extensions: args[5],
    };
  }
  return firstArg;
}

// Returns the line and the column of the offset `position` of `source`,
// getLocation of graphql-js.
function getLocation(source, position) {
  let lastLineStart = 0;
  let line = 1;
  for (const match of source.body.matchAll(LINE_ENDS)) {
    if (match.index >= position) break;
    lastLineStart = match.index + match[0].length;
    line += 1;
  }
  return { line, column: position + 1 - lastLineStart };
}

// Returns the excerpt of `source` around the location `location` of the AST,
// printLocation of graphql-js.
function printLocation(location) {
  return printSourceLocation(location.source, getLocation(location.source, location.start));
}

// Returns the excerpt of `source` around `sourceLocation`, its line, the lines
// beside it and a caret under the column, printSourceLocation of graphql-js.
function printSourceLocation(source, sourceLocation) {
  const firstLineColumnOffset = source.locationOffset.column - 1;
  const body = "".padStart(firstLineColumnOffset) + source.body;
  const lineIndex = sourceLocation.line - 1;
  const lineOffset = source.locationOffset.line - 1;
  const lineNum = sourceLocation.line + lineOffset;
  const columnOffset = sourceLocation.line === 1 ? firstLineColumnOffset : 0;
  const columnNum = sourceLocation.column + columnOffset;
  const locationStr = `${source.name}:${lineNum}:${columnNum}\n`;
  const lines = body.split(/\r\n|[\n\r]/g);
  const locationLine = lines[lineIndex];
  // A minified document has one long line, which prints in parts of 80.
  if (locationLine.length > 120) {
    const subLineIndex = Math.floor(columnNum / 80);
    const subLineColumnNum = columnNum % 80;
    const subLines = [];
    for (let i = 0; i < locationLine.length; i += 80) {
      subLines.push(locationLine.slice(i, i + 80));
    }
    return (
      locationStr +
      printPrefixedLines([
        [`${lineNum} |`, subLines[0]],
        ...subLines.slice(1, subLineIndex + 1).map((subLine) => ["|", subLine]),
        ["|", "^".padStart(subLineColumnNum)],
        ["|", subLines[subLineIndex + 1]],
      ])
    );
  }
  return (
    locationStr +
    printPrefixedLines([
      [`${lineNum - 1} |`, lines[lineIndex - 1]],
      [`${lineNum} |`, locationLine],
      ["|", "^".padStart(columnNum)],
      [`${lineNum + 1} |`, lines[lineIndex + 1]],
    ])
  );
}

// Returns the lines of an excerpt, each prefix padded to the longest one,
// the lines that are absent left out.
function printPrefixedLines(lines) {
  const existingLines = lines.filter(([_, line]) => line !== undefined);
  const padLen = Math.max(...existingLines.map(([prefix]) => prefix.length));
  return existingLines.map(([prefix, line]) => prefix.padStart(padLen) + (line ? " " + line : "")).join("\n");
}

// An error found while a GraphQL operation is parsed, validated or executed,
// with the locations of the document and the path of the result it belongs
// to, and the extensions an answer carries. Only the members the
// specification prescribes enumerate, so JSON.stringify and the inspect of
// graphql-js read message, locations, path and extensions alone, and toJSON
// answers the formatted error.
export class GraphQLError extends Error {
  constructor(message, ...rawArgs) {
    const { nodes, source, positions, path, originalError, extensions } = toNormalizedOptions(rawArgs);
    super(message);
    this.name = "GraphQLError";
    this.path = path ?? undefined;
    this.originalError = originalError ?? undefined;
    this.nodes = undefinedIfEmpty(Array.isArray(nodes) ? nodes : nodes ? [nodes] : undefined);
    const nodeLocations = undefinedIfEmpty(this.nodes?.map((node) => node.loc).filter((loc) => loc != null));
    this.source = source ?? nodeLocations?.[0]?.source;
    this.positions = positions ?? nodeLocations?.map((loc) => loc.start);
    this.locations =
      positions && source
        ? positions.map((pos) => getLocation(source, pos))
        : nodeLocations?.map((loc) => getLocation(loc.source, loc.start));
    const originalExtensions = isObjectLike(originalError?.extensions) ? originalError?.extensions : undefined;
    this.extensions = extensions ?? originalExtensions ?? Object.create(null);
    Object.defineProperties(this, {
      message: { writable: true, enumerable: true },
      name: { enumerable: false },
      nodes: { enumerable: false },
      source: { enumerable: false },
      positions: { enumerable: false },
      originalError: { enumerable: false },
    });
    if (originalError?.stack) {
      Object.defineProperty(this, "stack", { value: originalError.stack, writable: true, configurable: true });
    } else if (Error.captureStackTrace) {
      Error.captureStackTrace(this, GraphQLError);
    } else {
      Object.defineProperty(this, "stack", { value: Error().stack, writable: true, configurable: true });
    }
    stamp(this, GRAPHQL);
  }

  get [Symbol.toStringTag]() {
    return "GraphQLError";
  }

  // The message followed by the excerpt of each node of the document it
  // blames, or of each location of its source.
  toString() {
    let output = this.message;
    if (this.nodes) {
      for (const node of this.nodes) {
        if (node.loc) output += "\n\n" + printLocation(node.loc);
      }
    } else if (this.source && this.locations) {
      for (const location of this.locations) {
        output += "\n\n" + printSourceLocation(this.source, location);
      }
    }
    return output;
  }

  // The formatted error of the response: the message, the locations and the
  // path when the error has them, and the extensions when they hold a key.
  toJSON() {
    const formattedError = { message: this.message };
    if (this.locations != null) formattedError.locations = this.locations;
    if (this.path != null) formattedError.path = this.path;
    if (this.extensions != null && Object.keys(this.extensions).length > 0) {
      formattedError.extensions = this.extensions;
    }
    return formattedError;
  }
}

// The members createGraphQLError takes from an original error that is no
// Error and holds no other key, isGraphQLErrorLike of the utilities.
const ERROR_LIKE_KEYS = [
  "message", "locations", "path", "nodes", "source", "positions", "originalError", "name", "stack", "extensions",
  "coordinate",
];

// Returns whether `error` is an object whose every own enumerable key is one
// a GraphQLError has.
function isGraphQLErrorLike(error) {
  return (
    error != null && typeof error === "object" && Object.keys(error).every((key) => ERROR_LIKE_KEYS.includes(key))
  );
}

// Returns a GraphQLError of `message` and `options`, the original error made a
// GraphQLError first when it is an object that looks like one and no Error,
// and the schema coordinate kept as an enumerable member when the options name
// one.
export function createGraphQLError(message, options) {
  if (
    options?.originalError &&
    !(options.originalError instanceof Error) &&
    isGraphQLErrorLike(options.originalError)
  ) {
    options.originalError = createGraphQLError(options.originalError.message, options.originalError);
  }
  const error = new GraphQLError(message, options);
  if (options?.coordinate && error.coordinate == null) {
    Object.defineProperties(error, {
      coordinate: { value: options.coordinate, enumerable: true, configurable: true },
    });
  }
  return error;
}

// The error a resolver throws on purpose, a GraphQLError whose extensions are
// the code, the status and the details the app names, and whose original
// error and cause are the third argument.
export class ServiceError extends GraphQLError {
  constructor(message, extensions, error) {
    super(message, { originalError: error });
    this.extensions = extensions;
    this.cause = error;
  }
}

// The one authorization error, with the code FORBIDDEN, the status 403 and the
// feature whose gate denied a call. The members take the order of the
// reference, the feature first.
export class ForbiddenError extends Error {
  constructor(message = "Not permitted.", feature) {
    super(message);
    this.feature = feature;
    this.code = "FORBIDDEN";
    this.statusCode = 403;
    this.name = "ForbiddenError";
    stamp(this, FORBIDDEN);
  }
}

// Returns the error of the host for an error the engine raises toward a
// handler of the app, by the kind of its brand: a GraphQLError with its path,
// its locations and its extensions, a ForbiddenError with its message and the
// feature among its members, an AggregateError with its message and the
// errors its members hold, and otherwise an Error, a TypeError or a
// RangeError by its name, with its name and its own members, so instanceof
// holds in the app for the class the engine raised.
export function fromEngine(kind, name, message, path, locations, members) {
  if (kind === AGGREGATE) {
    const error = new AggregateError(Array.isArray(members) ? members : [], message);
    if (name !== error.name) error.name = name;
    return error;
  }
  if (kind === GRAPHQL) {
    const error = new GraphQLError(message, { path, extensions: members });
    if (locations !== undefined) error.locations = locations;
    return error;
  }
  if (kind === FORBIDDEN) {
    const error = new ForbiddenError(message, members?.feature);
    if (name !== error.name) error.name = name;
    return error;
  }
  const Intrinsic = name === "TypeError" ? TypeError : name === "RangeError" ? RangeError : Error;
  const error = new Intrinsic(message);
  if (name !== error.name) error.name = name;
  if (isObjectLike(members)) {
    for (const key of Object.keys(members)) {
      Object.defineProperty(error, key, { value: members[key], writable: true, enumerable: true, configurable: true });
    }
  }
  return error;
}

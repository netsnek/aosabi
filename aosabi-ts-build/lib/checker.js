// SPDX-License-Identifier: MIT
/*
 * aosabi-build lib/checker: the type source of the module infer.wasm over the live checker of
 * typescript, the import object aosabi_host whose every function answers one method of the
 * trait TypeSource of the crate aosabi-infer.
 *
 * A type, a symbol and a call signature cross as a handle of one table each, which hands out one
 * handle per object, so two handles are equal where the objects are, the identity of a Map key of
 * the inference.
 */

import ts from 'typescript'

/** The handle that names no type, no symbol and no signature. */
export const NONE = 0xffffffff

/**
 * The members of ts.TypeFlags the inference tests, by the bit of the crate's own numbering of
 * TypeFlags of crates/aosabi-infer/src/type_source.rs, whose bit values are no numbers of
 * typescript.
 */
const TYPE_FLAGS = [
  ts.TypeFlags.Any,
  ts.TypeFlags.String,
  ts.TypeFlags.Number,
  ts.TypeFlags.Boolean,
  ts.TypeFlags.StringLiteral,
  ts.TypeFlags.NumberLiteral,
  ts.TypeFlags.BooleanLiteral,
  ts.TypeFlags.Void,
  ts.TypeFlags.Undefined,
  ts.TypeFlags.Null,
  ts.TypeFlags.Never,
  ts.TypeFlags.Object,
  ts.TypeFlags.NonPrimitive,
  ts.TypeFlags.Union,
  ts.TypeFlags.Intersection
]

/** The members of ts.SymbolFlags the inference tests, by the bit of the crate's own numbering. */
const SYMBOL_FLAGS = [ts.SymbolFlags.Class, ts.SymbolFlags.Interface, ts.SymbolFlags.TypeAlias, ts.SymbolFlags.Enum]

/** The three roots in the order of the slot the import type_of_root names. */
export const ROOTS = ['Query', 'Mutation', 'Subscription']

/** Holds the objects of one kind of the checker by their handle, one handle per object. */
class Table {
  #handles = new Map()
  #objects = []

  /** Returns the handle of `object`, made at its first sight, or NONE for none. */
  handle(object) {
    if (object === undefined || object === null) return NONE
    let handle = this.#handles.get(object)
    if (handle === undefined) {
      handle = this.#objects.length
      this.#objects.push(object)
      this.#handles.set(object, handle)
    }
    return handle
  }

  /** Returns the object of `handle`, which throws for a handle the table never handed out. */
  object(handle) {
    const object = handle === NONE ? undefined : this.#objects[handle]
    if (object === undefined) throw new Error(`no object of the handle ${handle}`)
    return object
  }

  /** Returns the count of the objects the table holds. */
  get size() {
    return this.#objects.length
  }
}

/** Writes the four little endian bytes of `value` into `bytes` at `at` and returns the next offset. */
function putWord(bytes, at, value) {
  bytes[at] = value & 0xff
  bytes[at + 1] = (value >>> 8) & 0xff
  bytes[at + 2] = (value >>> 16) & 0xff
  bytes[at + 3] = (value >>> 24) & 0xff
  return at + 4
}

/** Collects the bytes of a record: counts and handles of four little endian bytes and texts. */
class Record {
  #parts = []
  #length = 0
  static #encoder = new TextEncoder()

  /** Appends one byte. */
  byte(value) {
    this.#parts.push(Uint8Array.of(value))
    this.#length += 1
    return this
  }

  /** Appends a count or a handle of four little endian bytes. */
  word(value) {
    const bytes = new Uint8Array(4)
    putWord(bytes, 0, value)
    this.#parts.push(bytes)
    this.#length += 4
    return this
  }

  /** Appends the UTF-8 bytes of `text` with no length. */
  raw(text) {
    const bytes = Record.#encoder.encode(text)
    this.#parts.push(bytes)
    this.#length += bytes.length
    return this
  }

  /** Appends a length and the UTF-8 bytes of `text`. */
  text(text) {
    const bytes = Record.#encoder.encode(text)
    this.word(bytes.length)
    this.#parts.push(bytes)
    this.#length += bytes.length
    return this
  }

  /** Appends a count and the handles `handles`. */
  handles(handles) {
    this.word(handles.length)
    for (const handle of handles) this.word(handle)
    return this
  }

  /** Returns the bytes of the record. */
  bytes() {
    const out = new Uint8Array(this.#length)
    let at = 0
    for (const part of this.#parts) {
      out.set(part, at)
      at += part.length
    }
    return out
  }
}

/**
 * Returns the type source of the module over `program` and `checker`, the program of the app and
 * its checker, `entry`, the source file of the entry the inference sees the types of symbols from,
 * and `roots`, the types of the three roots the inference reads before it parses, an object of
 * Query, Mutation and Subscription with undefined for a root the app declares none of.
 *
 * The answer holds `imports`, the functions of the import module aosabi_host, `bind(exports)`,
 * which hands the source the exports of the instance once it is made, `failures`, the calls of the
 * checker that threw, each with its method and its message, `trapped()`, the trap of the module
 * inside an import or null, and `counts`, the calls of each method and the objects of each table.
 * Every function runs its code inside a try, so no exception crosses a frame of the module: a call
 * that throws answers the empty answer of its method, the handle NONE, 0 or an empty record, and
 * keeps the failure, which fails the build once the export infer returns. The record of an answer
 * is written into the input buffer of the module through its export alloc, into a view of its
 * memory made for that write alone, since the memory may grow at the next call. An export throws
 * only when the module traps, so a throw of alloc condemns the instance, as lib/core.js of the
 * package aosabi condemns one: every later import answers the empty answer of its method as a
 * number, the length 0 for a record, with no call of the checker and no call, read or write of the
 * module, and the trap fails the build once infer returns into the host.
 */
export function createSource({program, checker, entry, roots}) {
  const types = new Table()
  const symbols = new Table()
  const signatures = new Table()
  const failures = []
  const calls = {}
  const decoder = new TextDecoder('utf-8', {fatal: true})
  let exports = null
  let trapped = null

  /**
   * Calls the export `name` of the module with `args` inside the import `method` and returns its
   * answer, and keeps the trap of a call that throws, which condemns the instance, before the throw
   * travels on.
   */
  function call(method, name, ...args) {
    try {
      return exports[name](...args)
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error)
      trapped = `${message}, in the export ${name} inside the import ${method}`
      throw error
    }
  }

  /** Writes `record`, the answer of the import `method`, into the input buffer and returns its length. */
  function answer(method, record) {
    const bytes = record.bytes()
    const at = call(method, 'alloc', bytes.length)
    if (at === 0 && bytes.length > 0) throw new Error(`the module gave no buffer of ${bytes.length} bytes`)
    new Uint8Array(exports.memory.buffer, at, bytes.length).set(bytes)
    return bytes.length
  }

  /** Returns the UTF-8 text the module holds at `at`, `len` bytes long. */
  function textAt(at, len) {
    return decoder.decode(new Uint8Array(exports.memory.buffer, at, len).slice())
  }

  /** Returns the `count` handles the module holds at `at`. */
  function handlesAt(at, count) {
    const view = new DataView(exports.memory.buffer, at, count * 4)
    return Array.from({length: count}, (_, i) => view.getUint32(i * 4, true))
  }

  /**
   * Returns the function of the method `name` whose body is `body`, run inside a try, with the
   * empty answer `empty` for a call that throws, and with an answer of a Record written into the
   * input buffer of the module, its length the answer of the import. Once the module trapped, the
   * function answers the empty answer as a number and runs nothing.
   */
  function method(name, empty, body) {
    calls[name] = 0
    const condemned = empty instanceof Record ? 0 : empty
    return (...args) => {
      calls[name] += 1
      if (trapped !== null) return condemned
      try {
        const value = body(...args)
        return value instanceof Record ? answer(name, value) : value
      } catch (error) {
        if (trapped !== null) return condemned
        failures.push(`${name}: ${error instanceof Error ? error.message : String(error)}`)
        try {
          return empty instanceof Record ? answer(name, empty) : empty
        } catch {
          return condemned
        }
      }
    }
  }

  const type = handle => types.object(handle)
  const symbol = handle => symbols.object(handle)
  const signature = handle => signatures.object(handle)
  const isType = value => value !== null && typeof value === 'object' && typeof value.flags === 'number'
  const bits = (flags, members) => members.reduce((set, member, bit) => (flags & member ? set | (1 << bit) : set), 0)
  const optionalText = text => (text === undefined ? new Record().byte(0) : new Record().byte(1).raw(text))
  const typeHandles = list => list.map(item => types.handle(item))
  const noText = () => new Record()
  const noList = () => new Record().word(0)
  const noOption = () => new Record().byte(0)

  /** Returns the type node of `t`, which checker.typeToTypeNode answers with no enclosing node and no flags. */
  const typeNodeOf = t => checker.typeToTypeNode(t, undefined, undefined)

  /**
   * Returns the name the declaration walk of type-definition-builder.ts:201-235 finds for
   * `found`, the alias symbol or the symbol of a type the walk names, or undefined where the walk
   * sets no name. A declaration that has a name sets the name of its escaped text, the empty text
   * where it holds none, which the reference reads as undefined. Otherwise the walk takes the first
   * named ancestor that is a variable, a property, a property signature, a parameter or a property
   * assignment. The name the walk starts from is the escaped name of the symbol, which is truthy,
   * so the reference stops after the first declaration, and a symbol of an empty name goes on
   * until a declaration names it.
   */
  function declarationName(found) {
    const declarations = found.getDeclarations()
    if (!declarations) return undefined
    const stopsAtFirst = Boolean(found.escapedName)
    for (const declaration of declarations) {
      if (declaration.name) return declaration.name.escapedText?.toString() ?? ''
      let parent = declaration.parent
      while (parent) {
        if (
          ts.isVariableDeclaration(parent) ||
          ts.isPropertyDeclaration(parent) ||
          ts.isPropertySignature(parent) ||
          ts.isParameter(parent) ||
          ts.isPropertyAssignment(parent)
        ) {
          if (ts.isIdentifier(parent.name)) return parent.name.escapedText.toString()
        }
        parent = parent.parent
      }
      if (stopsAtFirst) return undefined
    }
    return undefined
  }

  /** Returns every class and interface declaration of the program with a heritage clause, schema-parser.ts:1270-1284. */
  function heritageDeclarations() {
    const found = []
    for (const sourceFile of program.getSourceFiles()) {
      if (sourceFile.isDeclarationFile) continue
      ts.forEachChild(sourceFile, node => {
        if (!ts.isClassDeclaration(node) && !ts.isInterfaceDeclaration(node)) return
        const derived = checker.getTypeAtLocation(node)
        if (!node.heritageClauses) return
        const bases = []
        for (const clause of node.heritageClauses) {
          for (const typeRef of clause.types) bases.push(checker.getTypeAtLocation(typeRef))
        }
        found.push({derived, bases})
      })
    }
    return found
  }

  const imports = {
    type_of_root: method('type_of_root', NONE, slot => types.handle(roots[ROOTS[slot]])),
    type_to_string: method('type_to_string', noText(), t => new Record().raw(checker.typeToString(type(t)))),
    get_flags: method('get_flags', 0, t => bits(type(t).flags, TYPE_FLAGS)),
    get_object_flags: method('get_object_flags', 0, t =>
      (type(t).objectFlags ?? 0) & ts.ObjectFlags.Reference ? 1 : 0
    ),
    get_symbol: method('get_symbol', NONE, t => symbols.handle(type(t).getSymbol())),
    get_alias_symbol: method('get_alias_symbol', NONE, t => symbols.handle(type(t).aliasSymbol)),
    get_name: method('get_name', noText(), s => new Record().raw(symbol(s).getName())),
    get_symbol_flags: method('get_symbol_flags', 0, s => bits(symbol(s).flags, SYMBOL_FLAGS)),
    get_properties: method('get_properties', noList(), t => {
      const found = type(t)
      const properties = found.resolvedProperties || found.getProperties() || []
      return new Record().handles(properties.map(item => symbols.handle(item)))
    }),
    get_property: method('get_property', NONE, (t, at, len) =>
      symbols.handle(type(t).getProperty(textAt(at, len)))
    ),
    get_type_of_symbol_at_location: method('get_type_of_symbol_at_location', NONE, s =>
      types.handle(checker.getTypeOfSymbolAtLocation(symbol(s), entry))
    ),
    has_value_declaration: method('has_value_declaration', 0, s => (symbol(s).valueDeclaration ? 1 : 0)),
    get_combined_modifier_flags: method('get_combined_modifier_flags', 0, s =>
      ts.getCombinedModifierFlags(symbol(s).valueDeclaration) & ts.ModifierFlags.Private ? 1 : 0
    ),
    get_call_signatures: method('get_call_signatures', noList(), t =>
      new Record().handles(type(t).getCallSignatures().map(item => signatures.handle(item)))
    ),
    get_parameters: method('get_parameters', noList(), g =>
      new Record().handles(signature(g).getParameters().map(item => symbols.handle(item)))
    ),
    get_return_type: method('get_return_type', NONE, g => types.handle(signature(g).getReturnType())),
    is_optional_parameter: method('is_optional_parameter', 0, s => {
      const declaration = symbol(s).valueDeclaration
      return declaration.initializer !== undefined || declaration.questionToken !== undefined ? 1 : 0
    }),
    is_rest_parameter: method('is_rest_parameter', 0, s => {
      const declaration = symbol(s).valueDeclaration
      return declaration && ts.isParameter(declaration) && declaration.dotDotDotToken ? 1 : 0
    }),
    get_labeled_tuple_elements: method('get_labeled_tuple_elements', noOption(), t => {
      const found = type(t)
      if (!checker.isTupleType(found)) return new Record().byte(0)
      const elements = found.target.labeledElementDeclarations
      const record = new Record().byte(1).word(elements.length)
      elements.forEach(element => {
        const elementType = checker.getTypeAtLocation(element)
        record.text(element.name.text)
        record.word(types.handle(elementType))
        record.word(symbols.handle(checker.getSymbolAtLocation(element)))
        record.byte(element.initializer === undefined ? 0 : 1)
      })
      return record
    }),
    is_union: method('is_union', 0, t => (type(t).isUnion() ? 1 : 0)),
    is_intersection: method('is_intersection', 0, t => (type(t).isIntersection() ? 1 : 0)),
    get_types: method('get_types', noList(), t => new Record().handles(typeHandles(type(t).types))),
    get_non_nullable_type: method('get_non_nullable_type', NONE, t => types.handle(type(t).getNonNullableType())),
    get_union_type: method('get_union_type', NONE, (at, count) =>
      types.handle(checker.getUnionType(handlesAt(at, count).map(type)))
    ),
    is_type_assignable_to: method('is_type_assignable_to', 0, (source, target) =>
      checker.isTypeAssignableTo(type(source), type(target)) ? 1 : 0
    ),
    get_base_types: method('get_base_types', noOption(), t => {
      const bases = type(t).getBaseTypes?.()
      return bases === undefined ? new Record().byte(0) : new Record().byte(1).handles(typeHandles(bases))
    }),
    get_target: method('get_target', NONE, t => {
      const target = type(t).target
      return isType(target) ? types.handle(target) : NONE
    }),
    get_type_arguments: method('get_type_arguments', noList(), t =>
      new Record().handles(typeHandles(checker.getTypeArguments(type(t))))
    ),
    get_alias_type_arguments: method('get_alias_type_arguments', noOption(), t => {
      const found = type(t).aliasTypeArguments
      return found === undefined ? new Record().byte(0) : new Record().byte(1).handles(typeHandles(found))
    }),
    get_index_type_of_type: method('get_index_type_of_type', NONE, t =>
      types.handle(checker.getIndexTypeOfType(type(t), ts.IndexKind.Number))
    ),
    is_array_or_tuple_type_node: method('is_array_or_tuple_type_node', 0, t => {
      const node = typeNodeOf(type(t))
      return node && (node.kind === ts.SyntaxKind.ArrayType || node.kind === ts.SyntaxKind.TupleType) ? 1 : 0
    }),
    get_type_node_name: method('get_type_node_name', noOption(), t => {
      const name = typeNodeOf(type(t)).typeName?.symbol?.escapedName
      return optionalText(name === undefined ? undefined : String(name))
    }),
    get_literal_value: method('get_literal_value', noOption(), t => {
      const found = type(t)
      return found.isLiteral() ? optionalText(found.value?.toString() ?? '') : optionalText(undefined)
    }),
    get_declaration_name: method('get_declaration_name', noOption(), s => optionalText(declarationName(symbol(s)))),
    is_library_type: method('is_library_type', 0, t => {
      const found = type(t)
      const owner = found.aliasSymbol || found.symbol
      const declarations = owner?.getDeclarations()
      if (!declarations) return 0
      return declarations.some(d => {
        const sourceFile = d.getSourceFile()
        return program.isSourceFileDefaultLibrary(sourceFile) || sourceFile.fileName.includes('node_modules/typescript/lib')
      })
        ? 1
        : 0
    }),
    is_declared_in_file: method('is_declared_in_file', 0, (s, at, len) => {
      const needle = textAt(at, len)
      return symbol(s)
        .getDeclarations()
        ?.some(d => d.getSourceFile().fileName.includes(needle))
        ? 1
        : 0
    }),
    get_documentation_comment: method('get_documentation_comment', noText(), s =>
      new Record().raw(ts.displayPartsToString(symbol(s).getDocumentationComment(checker)))
    ),
    get_js_doc_tags: method('get_js_doc_tags', noList(), s => {
      const tags = symbol(s).getJsDocTags(checker)
      const record = new Record().word(tags.length)
      for (const tag of tags) record.text(tag.name).text(ts.displayPartsToString(tag.text))
      return record
    }),
    get_heritage_declarations: method('get_heritage_declarations', noList(), () => {
      const found = heritageDeclarations()
      const record = new Record().word(found.length)
      for (const {derived, bases} of found) record.word(types.handle(derived)).handles(typeHandles(bases))
      return record
    })
  }

  return {
    imports,
    bind(instanceExports) {
      exports = instanceExports
    },
    failures,
    trapped: () => trapped,
    counts: () => ({calls: {...calls}, types: types.size, symbols: symbols.size, signatures: signatures.size})
  }
}

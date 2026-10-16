// SPDX-License-Identifier: MIT
/*
 * strings-wasm.mjs: writes aosabi-ts/strings.wasm, the module of the hand over
 * of strings between the wrapper and core.wasm through the JS String Builtins
 * of V8, the probe of round JSSTRING.
 *
 * Rust has no stable externref and no arrays of WasmGC, so the module is
 * assembled here by hand, as the probe of the lead assembles its module of one
 * function. It imports the memory of core.wasm as aosabi.memory and the
 * functions length, intoCharCodeArray and fromCharCodeArray of wasm:js-string,
 * which workerd links to the builtins of V8 when a Worker carries the
 * compatibility flag wasm_esm_integration_builtins (cloudflare/workerd
 * PR #7685), and exports two functions:
 *
 * - encode(string, at) writes the WTF-8 bytes of the string at `at` of the
 *   memory and answers their count: UTF-8 where the string is well formed, as
 *   TextEncoder.encodeInto writes it, and a lone surrogate as the three bytes
 *   of its code unit, as wtf8 of lib/core.js writes it. The caller keeps three
 *   bytes of room per code unit.
 * - decode(at, length) answers the string of the UTF-8 bytes at `at`, as the
 *   strict TextDecoder of lib/core.js answers it, a leading byte order mark
 *   left out, or null where the bytes are no valid UTF-8, a lone surrogate in
 *   WTF-8 among them, so the caller answers those with its own decoder.
 *
 * Both go through one scratch array of i16 the module keeps and grows.
 *
 * Usage: node tools/jsstring/strings-wasm.mjs [out.wasm]
 */
import { writeFileSync } from "node:fs";
import { fileURLToPath } from "node:url";

const u32 = (value) => {
  const out = [];
  do {
    let byte = value & 0x7f;
    value >>>= 7;
    if (value !== 0) byte |= 0x80;
    out.push(byte);
  } while (value !== 0);
  return out;
};
const s32 = (value) => {
  const out = [];
  for (;;) {
    const byte = value & 0x7f;
    value >>= 7;
    if ((value === 0 && (byte & 0x40) === 0) || (value === -1 && (byte & 0x40) !== 0)) {
      out.push(byte);
      return out;
    }
    out.push(byte | 0x80);
  }
};
const name = (text) => [...u32(Buffer.byteLength(text)), ...Buffer.from(text)];
const vec = (items) => [...u32(items.length), ...items.flat()];
const section = (id, body) => [id, ...u32(body.length), ...body];

// Value types.
const I32 = 0x7f;
const EXTERNREF = 0x6f;
const A16 = 0; // the type index of (array (mut i16))
const refNull = (index) => [0x63, ...s32(index)];
const ref = (index) => [0x64, ...s32(index)];
const REF_EXTERN = [0x64, 0x6f];

// The types: 0 the array, then the functions.
const types = [
  [0x5e, 0x77, 0x01], // 0: (array (mut i16)), final, a rec group of its own
  [0x60, ...vec([[EXTERNREF]]), ...vec([[I32]])], // 1: length
  [0x60, ...vec([refNull(A16), [I32], [I32]]), ...vec([REF_EXTERN])], // 2: fromCharCodeArray
  [0x60, ...vec([[EXTERNREF], refNull(A16), [I32]]), ...vec([[I32]])], // 3: intoCharCodeArray
  [0x60, ...vec([[EXTERNREF], [I32]]), ...vec([[I32]])], // 4: encode
  [0x60, ...vec([[I32], [I32]]), ...vec([[EXTERNREF]])], // 5: decode
  [0x60, ...vec([[I32]]), ...vec([ref(A16)])], // 6: scratch
];

// The functions: 0 to 2 imported, 3 scratch, 4 encode, 5 decode.
const F = { length: 0, fromCharCodeArray: 1, intoCharCodeArray: 2, scratch: 3, encode: 4, decode: 5 };

// An assembler of one body with named locals and labels.
function body(params, locals, emit) {
  const index = new Map();
  params.forEach((p, i) => index.set(p, i));
  const declared = [];
  for (const [label, type] of locals) {
    index.set(label, index.size);
    declared.push([1, ...type]);
  }
  const code = [];
  const labels = [];
  const at = (label) => {
    const depth = labels.length - 1 - labels.lastIndexOf(label);
    if (labels.lastIndexOf(label) < 0) throw new Error("no label " + label);
    return u32(depth);
  };
  const local = (label) => {
    if (!index.has(label)) throw new Error("no local " + label);
    return u32(index.get(label));
  };
  const op = {
    get: (l) => code.push(0x20, ...local(l)),
    set: (l) => code.push(0x21, ...local(l)),
    tee: (l) => code.push(0x22, ...local(l)),
    gget: (g) => code.push(0x23, ...u32(g)),
    gset: (g) => code.push(0x24, ...u32(g)),
    const: (v) => code.push(0x41, ...s32(v)),
    add: () => code.push(0x6a),
    sub: () => code.push(0x6b),
    and: () => code.push(0x71),
    or: () => code.push(0x72),
    shl: () => code.push(0x74),
    shr: () => code.push(0x76),
    eq: () => code.push(0x46),
    ne: () => code.push(0x47),
    ltU: () => code.push(0x49),
    gtU: () => code.push(0x4b),
    geU: () => code.push(0x4f),
    load8: (offset = 0) => code.push(0x2d, 0x00, ...u32(offset)),
    store8: (offset = 0) => code.push(0x3a, 0x00, ...u32(offset)),
    select: () => code.push(0x1b),
    drop: () => code.push(0x1a),
    call: (f) => code.push(0x10, ...u32(f)),
    ret: () => code.push(0x0f),
    block: (label) => {
      code.push(0x02, 0x40);
      labels.push(label);
    },
    loop: (label) => {
      code.push(0x03, 0x40);
      labels.push(label);
    },
    if: () => {
      code.push(0x04, 0x40);
      labels.push("(if)");
    },
    end: () => {
      code.push(0x0b);
      labels.pop();
    },
    br: (label) => code.push(0x0c, ...at(label)),
    brIf: (label) => code.push(0x0d, ...at(label)),
    refNull: (heap) => code.push(0xd0, ...heap),
    isNull: () => code.push(0xd1),
    asNonNull: () => code.push(0xd4),
    arrayNew: () => code.push(0xfb, ...u32(7), ...u32(A16)),
    arrayGet: () => code.push(0xfb, ...u32(13), ...u32(A16)),
    arraySet: () => code.push(0xfb, ...u32(14), ...u32(A16)),
    arrayLen: () => code.push(0xfb, ...u32(15)),
    // i += n
    bump: (l, n) => {
      op.get(l);
      op.const(n);
      op.add();
      op.set(l);
    },
  };
  emit(op);
  if (labels.length !== 0) throw new Error("open labels " + labels.join(","));
  code.push(0x0b);
  const func = [...vec(declared), ...code];
  return [...u32(func.length), ...func];
}

// scratch(n): the scratch array, grown to hold at least n units.
const scratch = body(["n"], [["a", refNull(A16)]], (o) => {
  o.block("grow");
  o.gget(0);
  o.tee("a");
  o.isNull();
  o.brIf("grow");
  o.get("a");
  o.arrayLen();
  o.get("n");
  o.ltU();
  o.brIf("grow");
  o.get("a");
  o.asNonNull();
  o.ret();
  o.end();
  o.get("n");
  o.const(4096);
  o.get("n");
  o.const(4096);
  o.gtU();
  o.select();
  o.arrayNew();
  o.tee("a");
  o.gset(0);
  o.get("a");
  o.asNonNull();
});

// Writes the byte `value` computed by `push` at o + offset.
const byteAt = (o, offset, push) => {
  o.get("o");
  push();
  o.store8(offset);
};

// encode(s, at): the WTF-8 bytes of s at `at`, answering their count.
const encode = body(["s", "at"], [["n", [I32]], ["arr", refNull(A16)], ["i", [I32]], ["o", [I32]], ["c", [I32]],
  ["d", [I32]]], (o) => {
  o.get("s");
  o.call(F.length);
  o.tee("n");
  o.call(F.scratch);
  o.set("arr");
  o.get("s");
  o.get("arr");
  o.const(0);
  o.call(F.intoCharCodeArray);
  o.drop();
  o.get("at");
  o.set("o");
  o.block("done");
  o.loop("next");
  o.get("i");
  o.get("n");
  o.geU();
  o.brIf("done");
  o.get("arr");
  o.get("i");
  o.arrayGet();
  o.tee("c");
  o.const(0x80);
  o.ltU();
  o.if();
  byteAt(o, 0, () => o.get("c"));
  o.bump("o", 1);
  o.bump("i", 1);
  o.br("next");
  o.end();
  o.get("c");
  o.const(0x800);
  o.ltU();
  o.if();
  byteAt(o, 0, () => { o.get("c"); o.const(6); o.shr(); o.const(0xc0); o.or(); });
  byteAt(o, 1, () => { o.get("c"); o.const(0x3f); o.and(); o.const(0x80); o.or(); });
  o.bump("o", 2);
  o.bump("i", 1);
  o.br("next");
  o.end();
  // A high surrogate followed by a low one is one code point of four bytes.
  o.get("c");
  o.const(0xfc00);
  o.and();
  o.const(0xd800);
  o.eq();
  o.if();
  o.get("i");
  o.const(1);
  o.add();
  o.get("n");
  o.ltU();
  o.if();
  o.get("arr");
  o.get("i");
  o.const(1);
  o.add();
  o.arrayGet();
  o.tee("d");
  o.const(0xfc00);
  o.and();
  o.const(0xdc00);
  o.eq();
  o.if();
  o.get("c");
  o.const(0xd800);
  o.sub();
  o.const(10);
  o.shl();
  o.get("d");
  o.const(0xdc00);
  o.sub();
  o.add();
  o.const(0x10000);
  o.add();
  o.set("c");
  byteAt(o, 0, () => { o.get("c"); o.const(18); o.shr(); o.const(0xf0); o.or(); });
  byteAt(o, 1, () => { o.get("c"); o.const(12); o.shr(); o.const(0x3f); o.and(); o.const(0x80); o.or(); });
  byteAt(o, 2, () => { o.get("c"); o.const(6); o.shr(); o.const(0x3f); o.and(); o.const(0x80); o.or(); });
  byteAt(o, 3, () => { o.get("c"); o.const(0x3f); o.and(); o.const(0x80); o.or(); });
  o.bump("o", 4);
  o.bump("i", 2);
  o.br("next");
  o.end();
  o.end();
  o.end();
  // Any other unit, a lone surrogate among them, is three bytes.
  byteAt(o, 0, () => { o.get("c"); o.const(12); o.shr(); o.const(0xe0); o.or(); });
  byteAt(o, 1, () => { o.get("c"); o.const(6); o.shr(); o.const(0x3f); o.and(); o.const(0x80); o.or(); });
  byteAt(o, 2, () => { o.get("c"); o.const(0x3f); o.and(); o.const(0x80); o.or(); });
  o.bump("o", 3);
  o.bump("i", 1);
  o.br("next");
  o.end();
  o.end();
  o.get("o");
  o.get("at");
  o.sub();
});

// decode(at, len): the string of the UTF-8 bytes at `at`, or null where they are no valid UTF-8.
const decode = body(["at", "len"], [["arr", refNull(A16)], ["i", [I32]], ["u", [I32]], ["b", [I32]], ["c1", [I32]],
  ["c2", [I32]], ["c3", [I32]], ["p", [I32]]], (o) => {
  const load = (label, offset) => {
    o.get("p");
    o.load8(offset);
    o.tee(label);
    o.const(0xc0);
    o.and();
    o.const(0x80);
    o.ne();
    o.brIf("fail");
  };
  const need = (more) => {
    o.get("i");
    o.const(more);
    o.add();
    o.get("len");
    o.geU();
    o.brIf("fail");
  };
  const unit = (push) => {
    o.get("arr");
    o.get("u");
    push();
    o.arraySet();
    o.bump("u", 1);
  };
  o.get("len");
  o.call(F.scratch);
  o.set("arr");
  o.block("fail");
  o.block("done");
  o.loop("next");
  o.get("i");
  o.get("len");
  o.geU();
  o.brIf("done");
  o.get("at");
  o.get("i");
  o.add();
  o.tee("p");
  o.load8(0);
  o.tee("b");
  o.const(0x80);
  o.ltU();
  o.if();
  unit(() => o.get("b"));
  o.bump("i", 1);
  o.br("next");
  o.end();
  o.get("b");
  o.const(0xc2);
  o.ltU();
  o.brIf("fail");
  o.get("b");
  o.const(0xe0);
  o.ltU();
  o.if();
  need(1);
  load("c1", 1);
  unit(() => { o.get("b"); o.const(0x1f); o.and(); o.const(6); o.shl(); o.get("c1"); o.const(0x3f); o.and(); o.or(); });
  o.bump("i", 2);
  o.br("next");
  o.end();
  o.get("b");
  o.const(0xf0);
  o.ltU();
  o.if();
  need(2);
  load("c1", 1);
  load("c2", 2);
  // E0 needs A0 or more after it, ED less than A0, the surrogates.
  o.get("b");
  o.const(0xe0);
  o.eq();
  o.get("c1");
  o.const(0xa0);
  o.ltU();
  o.and();
  o.brIf("fail");
  o.get("b");
  o.const(0xed);
  o.eq();
  o.get("c1");
  o.const(0xa0);
  o.geU();
  o.and();
  o.brIf("fail");
  unit(() => {
    o.get("b"); o.const(0x0f); o.and(); o.const(12); o.shl();
    o.get("c1"); o.const(0x3f); o.and(); o.const(6); o.shl(); o.or();
    o.get("c2"); o.const(0x3f); o.and(); o.or();
  });
  o.bump("i", 3);
  o.br("next");
  o.end();
  o.get("b");
  o.const(0xf5);
  o.geU();
  o.brIf("fail");
  need(3);
  load("c1", 1);
  load("c2", 2);
  load("c3", 3);
  // F0 needs 90 or more after it, F4 less than 90.
  o.get("b");
  o.const(0xf0);
  o.eq();
  o.get("c1");
  o.const(0x90);
  o.ltU();
  o.and();
  o.brIf("fail");
  o.get("b");
  o.const(0xf4);
  o.eq();
  o.get("c1");
  o.const(0x90);
  o.geU();
  o.and();
  o.brIf("fail");
  o.get("b"); o.const(0x07); o.and(); o.const(18); o.shl();
  o.get("c1"); o.const(0x3f); o.and(); o.const(12); o.shl(); o.or();
  o.get("c2"); o.const(0x3f); o.and(); o.const(6); o.shl(); o.or();
  o.get("c3"); o.const(0x3f); o.and(); o.or();
  o.const(0x10000);
  o.sub();
  o.set("b");
  unit(() => { o.get("b"); o.const(10); o.shr(); o.const(0xd800); o.add(); });
  unit(() => { o.get("b"); o.const(0x3ff); o.and(); o.const(0xdc00); o.add(); });
  o.bump("i", 4);
  o.br("next");
  o.end();
  o.end();
  // A leading byte order mark is left out, as TextDecoder leaves it out, so
  // the string starts at 1 where its first unit is U+FEFF and at 0 otherwise.
  o.const(0);
  o.set("i");
  o.get("u");
  o.const(0);
  o.gtU();
  o.if();
  o.get("arr");
  o.const(0);
  o.arrayGet();
  o.const(0xfeff);
  o.eq();
  o.set("i");
  o.end();
  o.get("arr");
  o.get("i");
  o.get("u");
  o.call(F.fromCharCodeArray);
  o.ret();
  o.end();
  o.refNull([0x6f]);
});

function assemble() {
  const bytes = [0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
  bytes.push(...section(1, vec(types)));
  const imports = [
    [...name("wasm:js-string"), ...name("length"), 0x00, ...u32(1)],
    [...name("wasm:js-string"), ...name("fromCharCodeArray"), 0x00, ...u32(2)],
    [...name("wasm:js-string"), ...name("intoCharCodeArray"), 0x00, ...u32(3)],
    [...name("aosabi"), ...name("memory"), 0x02, 0x00, ...u32(0)],
  ];
  bytes.push(...section(2, vec(imports)));
  bytes.push(...section(3, vec([u32(6), u32(4), u32(5)])));
  bytes.push(...section(6, vec([[...refNull(A16), 0x01, 0xd0, ...s32(A16), 0x0b]])));
  bytes.push(...section(7, vec([[...name("encode"), 0x00, ...u32(F.encode)], [...name("decode"), 0x00,
    ...u32(F.decode)]])));
  bytes.push(...section(10, vec([scratch, encode, decode])));
  return Uint8Array.from(bytes);
}

export const STRINGS_WASM = assemble();

if (process.argv[1] === fileURLToPath(import.meta.url)) {
  const out = process.argv[2] ?? new URL("../../aosabi-ts/strings.wasm", import.meta.url);
  writeFileSync(out, STRINGS_WASM);
  console.log("wrote", String(out), STRINGS_WASM.length, "bytes");
}

// SPDX-License-Identifier: MIT
/*
 * test-strings.mjs: holds encode and decode of strings.wasm to the conversions
 * of lib/core.js they stand in for, over random and edge strings and bytes.
 *
 * encode must write the bytes Out.text writes, TextEncoder.encodeInto for a
 * well formed string and wtf8 for any other, and decode, with the decoder of
 * lib/core.js where it answers null, must answer what fromWtf8 answers, the
 * strict TextDecoder and its fallback.
 *
 * Usage: node tools/jsstring/test-strings.mjs [strings.wasm] [rounds]
 */
import { readFileSync } from "node:fs";

const path = process.argv[2] ?? new URL("../../aosabi-ts/strings.wasm", import.meta.url);
const rounds = Number(process.argv[3] ?? 200000);
const module = new WebAssembly.Module(readFileSync(path), { builtins: ["js-string"] });
const memory = new WebAssembly.Memory({ initial: 4 });
const { encode, decode } = new WebAssembly.Instance(module, { aosabi: { memory } }).exports;

// The conversions of lib/core.js, copied from it.
const encoder = new TextEncoder();
const strict = new TextDecoder("utf-8", { fatal: true });
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
const outText = (value) => {
  if (!value.isWellFormed()) return wtf8(value);
  const buffer = new Uint8Array(value.length * 3);
  const { written } = encoder.encodeInto(value, buffer);
  return buffer.subarray(0, written);
};

let seed = 12345;
const random = () => {
  seed = (seed * 1103515245 + 12345) >>> 0;
  return seed / 2 ** 32;
};
const unit = () => {
  const r = random();
  if (r < 0.5) return 0x20 + Math.floor(random() * 0x5f);
  if (r < 0.6) return Math.floor(random() * 0x80);
  if (r < 0.7) return 0x80 + Math.floor(random() * 0x780);
  if (r < 0.8) return 0x800 + Math.floor(random() * 0xf800);
  if (r < 0.9) return 0xd800 + Math.floor(random() * 0x800);
  return 0xfeff;
};
const randomString = () => {
  const length = Math.floor(random() ** 3 * 80);
  let text = "";
  for (let i = 0; i < length; i++) {
    if (random() < 0.05) text += String.fromCodePoint(0x10000 + Math.floor(random() * 0xfffff));
    else text += String.fromCharCode(unit());
  }
  return text;
};
const edges = ["", "a", "\u0000", "\u007f", "\u0080", "߿", "ࠀ", "￿", "\ud800", "\udfff",
  "𐀀", "􏿿", "a\ud800", "\ud800a", "\udc00\ud800", "﻿", "﻿abc", "a﻿",
  "😀", "x".repeat(5000), "é".repeat(3000), "€".repeat(2000), "😀".repeat(1500)];

let failures = 0;
const fail = (what, value, got, want) => {
  failures += 1;
  if (failures < 20) console.log("FAIL", what, JSON.stringify(value).slice(0, 120), got, want);
};
const same = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);
const bytesAt = (at, length) => new Uint8Array(memory.buffer, at, length);

let encoded = 0;
let decoded = 0;
for (let r = 0; r < rounds + edges.length; r++) {
  const value = r < edges.length ? edges[r] : randomString();
  const at = 64 + Math.floor(random() * 64);
  const written = encode(value, at);
  const want = outText(value);
  if (!same(bytesAt(at, written), want)) fail("encode", value, Array.from(bytesAt(at, written)), Array.from(want));
  encoded += 1;
  // The decode of what Out.text writes, and of bytes with damage.
  const cases = [want];
  if (want.length > 0) {
    const damaged = Uint8Array.from(want);
    damaged[Math.floor(random() * damaged.length)] = Math.floor(random() * 256);
    cases.push(damaged, want.subarray(0, want.length - 1), Uint8Array.from([0xef, 0xbb, 0xbf, ...want]));
  }
  for (const bytes of cases) {
    bytesAt(at, bytes.length).set(bytes);
    const got = decode(at, bytes.length);
    const expect = fromWtf8(bytes);
    const answer = got === null ? fromWtf8(bytes) : got;
    let valid = true;
    try {
      strict.decode(bytes);
    } catch {
      valid = false;
    }
    if (answer !== expect) fail("decode", Array.from(bytes), JSON.stringify(got), JSON.stringify(expect));
    if (valid !== (got !== null)) fail("decode validity", Array.from(bytes), JSON.stringify(got), valid);
    decoded += 1;
  }
}
// Every byte pair and triple lead, against the strict decoder.
for (let a = 0; a < 256; a++) {
  for (let b = 0; b < 256; b++) {
    for (const tail of [[], [0x80], [0x80, 0x80], [0xbf, 0xbf]]) {
      const bytes = Uint8Array.from([a, b, ...tail]);
      bytesAt(16, bytes.length).set(bytes);
      const got = decode(16, bytes.length);
      let valid = true;
      try {
        strict.decode(bytes);
      } catch {
        valid = false;
      }
      if (valid !== (got !== null) || (valid && got !== strict.decode(bytes))) fail("pair", Array.from(bytes), got, valid);
      decoded += 1;
    }
  }
}
console.log(JSON.stringify({ encoded, decoded, failures }));
process.exit(failures === 0 ? 0 : 1);

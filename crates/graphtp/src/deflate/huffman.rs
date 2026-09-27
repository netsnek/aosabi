// SPDX-License-Identifier: MIT
/*
 * graphtp deflate huffman: the canonical Huffman codes of DEFLATE that the
 * encoder builds and the decoder reads.
 *
 * No analogue in the reference. The codes and the tables of lengths and
 * distances follow RFC 1951.
 */

//! The canonical Huffman codes of DEFLATE, shared by the encoder and the
//! decoder.
//!
//! A code is given by the length of the code of each symbol alone, and codes of
//! one length are consecutive in the order of their symbols. The decoder builds
//! a table of the counts and the symbols from the lengths, and the encoder
//! derives the lengths from the frequencies of a block and the codes from the
//! lengths.

/// The longest code of a literal, a length or a distance, 15 bits.
pub const MAX_BITS: u8 = 15;

/// The longest code of the code length alphabet, 7 bits.
pub const MAX_CODE_LENGTH_BITS: u8 = 7;

/// The count of literal and length symbols a dynamic block may code, 286.
pub const LITERALS: usize = 286;

/// The count of literal and length symbols of the fixed code, 288, two of
/// which never occur in the data.
pub const FIXED_LITERALS: usize = 288;

/// The count of distance symbols a block may code, 30.
pub const DISTANCES: usize = 30;

/// The symbol that ends a block, 256.
pub const END_OF_BLOCK: usize = 256;

/// The shortest match, 3 bytes.
pub const MIN_MATCH: usize = 3;

/// The longest match, 258 bytes.
pub const MAX_MATCH: usize = 258;

/// The base length of each length symbol from 257 on.
pub(crate) const LENGTH_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];

/// The count of extra bits of each length symbol from 257 on.
pub(crate) const LENGTH_EXTRA: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];

/// The base distance of each distance symbol.
pub(crate) const DISTANCE_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];

/// The count of extra bits of each distance symbol.
pub(crate) const DISTANCE_EXTRA: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];

/// The order in which a dynamic block sends the lengths of the code length
/// code.
pub(crate) const ORDER: [u8; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Returns the length of the fixed code of the literal or length symbol
/// `symbol`.
pub(crate) fn fixed_literal_length(symbol: usize) -> u8 {
    match symbol {
        0..=143 | 280.. => 8,
        144..=255 => 9,
        _ => 7,
    }
}

/// The length of the fixed code of every distance symbol, 5 bits.
pub(crate) const FIXED_DISTANCE_LENGTH: u8 = 5;

/// Returns the index of the length symbol of the match length `len`, 0 for the
/// symbol 257.
///
/// The length is one of 3 to 258, and the symbol is the last whose base is at
/// most the length.
pub(crate) fn length_symbol(len: usize) -> usize {
    LENGTH_BASE
        .iter()
        .rposition(|&base| usize::from(base) <= len)
        .unwrap_or(0)
}

/// Returns the distance symbol of the distance `dist`.
///
/// The distance is one of 1 to 32,768, and the symbol is the last whose base
/// is at most the distance.
pub(crate) fn distance_symbol(dist: usize) -> usize {
    DISTANCE_BASE
        .iter()
        .rposition(|&base| usize::from(base) <= dist)
        .unwrap_or(0)
}

/// The decoding table of one canonical code: the count of codes of each
/// length and the symbols in the order of their codes.
///
/// The two arrays take 608 bytes, 16 counts and 288 symbols of 2 bytes each,
/// which is the table of puff.c.
pub(crate) struct Table {
    /// The count of codes of each length, index 0 counting the unused symbols.
    count: [u16; 16],
    /// The symbols ordered by their codes, shorter codes first.
    symbol: [u16; FIXED_LITERALS],
}

/// The count of codes a table of lengths leaves over, which tells a complete
/// code from an incomplete or an over subscribed one.
///
/// Zero is a complete code, a positive count an incomplete one, and a negative
/// count a code with more codes than its lengths leave room for.
pub(crate) type Left = i32;

impl Table {
    /// Returns the table of the code whose symbols have the lengths `lengths`,
    /// and the count of codes the lengths leave over.
    ///
    /// Each length is at most 15, and `lengths` holds at most 288 of them. A
    /// code that is over subscribed returns before its symbols are placed, and
    /// its table is never read.
    pub(crate) fn new(lengths: &[u8]) -> (Table, Left) {
        let mut table = Table {
            count: [0; 16],
            symbol: [0; FIXED_LITERALS],
        };
        for &len in lengths {
            table.count[usize::from(len & 15)] += 1;
        }
        if usize::from(table.count[0]) == lengths.len() {
            return (table, 0);
        }
        let mut left: Left = 1;
        for &count in &table.count[1..] {
            left = (left << 1) - Left::from(count);
            if left < 0 {
                return (table, left);
            }
        }
        let mut offset = [0u16; 16];
        for len in 1..15 {
            offset[len + 1] = offset[len] + table.count[len];
        }
        for (symbol, &len) in (0u16..).zip(lengths) {
            if len != 0 {
                let at = &mut offset[usize::from(len & 15)];
                if let Some(slot) = table.symbol.get_mut(usize::from(*at)) {
                    *slot = symbol;
                }
                *at += 1;
            }
        }
        (table, left)
    }

    /// Returns the count of codes of the length `len`.
    pub(crate) fn count(&self, len: usize) -> u16 {
        self.count.get(len).copied().unwrap_or(0)
    }

    /// Returns the symbol at the position `index` of the canonical order, or
    /// `None` past the symbols of the code.
    pub(crate) fn symbol(&self, index: usize) -> Option<u16> {
        self.symbol.get(index).copied()
    }

    /// Tells whether every symbol of the code of `lengths` has a length of 0
    /// or 1, the one shape in which an incomplete code is accepted.
    ///
    /// A single code of one bit is incomplete, and RFC 1951 allows it for a
    /// block that codes one distance, so puff.c accepts an incomplete code of
    /// lengths 0 and 1 alone and refuses every other incomplete one.
    pub(crate) fn is_single(&self, lengths: &[u8]) -> bool {
        usize::from(self.count[0]) + usize::from(self.count[1]) == lengths.len()
    }
}

/// Writes into `lengths` the lengths of a Huffman code of at most `limit` bits
/// for the frequencies `freq`.
///
/// A symbol of frequency 0 gets the length 0. A code needs two symbols, so
/// when fewer than two have a frequency, the symbols 0 and 1 join with the
/// frequency 1, which keeps the code complete. When the optimal code needs a
/// longer code than `limit`, every frequency is halved, a frequency above 0
/// staying at least 1, and the lengths are computed again, which ends at the
/// latest when every frequency is 1. `freq` and `lengths` have the same length
/// of at most 288 symbols.
pub(crate) fn lengths_of(freq: &[usize], limit: u8, lengths: &mut [u8]) {
    let mut order = [0u16; FIXED_LITERALS];
    let mut used = 0;
    for (symbol, &f) in (0u16..).zip(freq.iter().take(FIXED_LITERALS)) {
        if f > 0 {
            order[used] = symbol;
            used += 1;
        }
    }
    for symbol in 0..2u16 {
        if used < 2 && usize::from(symbol) < freq.len() && !order[..used].contains(&symbol) {
            order[used] = symbol;
            used += 1;
        }
    }
    let order = &mut order[..used];
    let weight = |symbol: u16| freq.get(usize::from(symbol)).copied().unwrap_or(0).max(1);
    sort_by_weight(order, weight);
    let mut depth = [0usize; FIXED_LITERALS];
    let depth = &mut depth[..used];
    let mut shift = 0;
    loop {
        for (slot, &symbol) in depth.iter_mut().zip(order.iter()) {
            *slot = (weight(symbol) >> shift).max(1);
        }
        minimum_redundancy(depth);
        if depth
            .first()
            .is_none_or(|&longest| longest <= usize::from(limit))
        {
            break;
        }
        shift += 1;
    }
    lengths.fill(0);
    for (&symbol, &len) in order.iter().zip(depth.iter()) {
        if let Some(slot) = lengths.get_mut(usize::from(symbol)) {
            *slot = u8::try_from(len).unwrap_or(limit);
        }
    }
}

/// Sorts the symbols `order` by their weight, the lower first, and a tie by the
/// symbol, so the lengths of a block never depend on the order of its tokens.
///
/// Halving every weight keeps this order, since a weight halved stays at least
/// the weight below it halved.
fn sort_by_weight(order: &mut [u16], weight: impl Fn(u16) -> usize) {
    for i in 1..order.len() {
        let symbol = order[i];
        let key = (weight(symbol), symbol);
        let mut j = i;
        while j > 0 && (weight(order[j - 1]), order[j - 1]) > key {
            order[j] = order[j - 1];
            j -= 1;
        }
        order[j] = symbol;
    }
}

/// Replaces the weights `a`, sorted from the lowest, by the lengths of an
/// optimal prefix code for them, the longest first.
///
/// The three passes are those of Moffat and Katajainen and use no memory
/// beyond `a`: the first builds the tree with parent pointers in place, the
/// second turns the pointers into the depths of the internal nodes, and the
/// third gives each leaf its depth. `a` holds at least two weights.
fn minimum_redundancy(a: &mut [usize]) {
    let n = a.len();
    if n < 2 {
        return;
    }
    a[0] += a[1];
    let mut root = 0;
    let mut leaf = 2;
    for next in 1..n - 1 {
        if leaf >= n || a[root] < a[leaf] {
            a[next] = a[root];
            a[root] = next;
            root += 1;
        } else {
            a[next] = a[leaf];
            leaf += 1;
        }
        if leaf >= n || (root < next && a[root] < a[leaf]) {
            a[next] += a[root];
            a[root] = next;
            root += 1;
        } else {
            a[next] += a[leaf];
            leaf += 1;
        }
    }
    a[n - 2] = 0;
    for next in (0..n - 2).rev() {
        a[next] = a[a[next]] + 1;
    }
    let mut avail = 1;
    let mut used = 0;
    let mut depth = 0;
    let mut internal = n - 1;
    let mut next = n;
    while avail > 0 {
        while internal > 0 && a[internal - 1] == depth {
            used += 1;
            internal -= 1;
        }
        while avail > used && next > 0 {
            next -= 1;
            a[next] = depth;
            avail -= 1;
        }
        avail = 2 * used;
        depth += 1;
        used = 0;
    }
}

/// Writes into `codes` the canonical code of each symbol of the lengths
/// `lengths`, bit reversed, since DEFLATE sends a code from its most
/// significant bit into a stream that is filled from the least significant bit
/// of each byte.
pub(crate) fn codes_of(lengths: &[u8], codes: &mut [u16]) {
    let mut count = [0u16; 16];
    for &len in lengths {
        count[usize::from(len & 15)] += 1;
    }
    count[0] = 0;
    let mut next = [0u16; 16];
    let mut code = 0u16;
    for bits in 1..16 {
        code = code.wrapping_add(count[bits - 1]) << 1;
        next[bits] = code;
    }
    for (slot, &len) in codes.iter_mut().zip(lengths) {
        if len != 0 {
            let at = &mut next[usize::from(len & 15)];
            *slot = at.reverse_bits() >> (16 - u32::from(len & 15));
            *at = at.wrapping_add(1);
        }
    }
}

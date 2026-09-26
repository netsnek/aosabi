// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass overlap: the rule for overlapping fields of the single
 * pass, which compares the fields of one response key with a record of the
 * pairs it compared.
 *
 * No analogue of the file in the reference.
 */

//! The rule for overlapping fields of the single pass.
//!
//! Every selection set compares the fields it holds directly or through
//! inline fragments with each other, with the fragments it spreads and those
//! fragments with each other, and two fields with sub selections compare
//! those sets the same way. A fragment is never compared with itself, and
//! the pairs of a selection set and a fragment and of two fragments are
//! compared once each, as the pair sets of the reference keep them, so the
//! work follows the pairs of fields with one response key and not the paths
//! through the fragments. The pairs of fragments are walked from a list in
//! the buffers, so only nested fields add to the stack, and the bound on
//! nesting bounds those.
//!
//! The record of compared pairs is an open addressing set of 64 bit entries,
//! a pair of indices the document sets beside one flag, whose slot is a
//! Fibonacci hash of the entry. It grows to keep a quarter of its slots
//! empty, so a probe ends at an empty slot within the entries it holds.
//!
//! The crossings where a comparison of names turns from pairs to sorted
//! lists, 8 arguments or input fields, 16 fields of one selection set and 64
//! pairs of two sets, are those of SP1's `pass.rs`, the source the plan names
//! for this file.

use alloc::vec::Vec;
use core::{
    cmp::Ordering,
    mem, //
};

use super::{
    FIELD,
    INLINE,
    INTERNAL,
    R,
    Reason,
    Rule,
    Sel,
    UNTYPED,
    feed::Feed,
    narrow,
    scalars::Scalars,
    sort_by_bytes,
    table::{
        Kind,
        META,
        NONE,
        TypeRef, //
    },
    v,
    walk::{
        Walker,
        defer,
        refuse, //
    },
    widen, //
};

/// An empty slot of the record of compared pairs.
const MEMO_EMPTY: u64 = u64::MAX;
/// The kind of a pair of a selection set and a fragment.
const MEMO_SET_FRAGMENT: u64 = 0;
/// The kind of a pair of two fragments.
const MEMO_FRAGMENTS: u64 = 1;

/// The stop of two fields of one response key that cannot merge.
const CONFLICT: super::Stop = refuse(Rule::OverlappingFieldsCanBeMerged);

/// The shortest run of digits in the name of an input field at which the
/// pass leaves two object values that compare alike to the rules, 16.
///
/// The rules print the fields of an object value in the order of their names,
/// sorted stably by the natural order of the reference, which reads a run of
/// digits as a double that it sums digit by digit. Every sum of a run of up
/// to 15 digits lies below 10^15 + 57, below 2^53, so the double holds it
/// exactly and two names of one object never order alike. A run of 16 digits
/// may round, as 2^53 + 1 does, so two names may order alike and keep the
/// order the document writes them in, which the comparison here does not
/// read.
const DIGIT_RUN: usize = 16;

/// Returns whether `name` holds a run of [`DIGIT_RUN`] digits or more.
fn long_digit_run(name: &[u8]) -> bool {
    name.split(|byte| !byte.is_ascii_digit())
        .any(|run| run.len() >= DIGIT_RUN)
}

/// Returns the slot of `entry` in a record of `mask + 1` slots: Fibonacci
/// hashing of the entry without its flag.
fn memo_slot(entry: u64, mask: usize) -> usize {
    let hashed = (entry >> 1).wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 32;
    usize::try_from(hashed).unwrap_or(usize::MAX) & mask
}

/// Returns whether a field reference has a definition on its parent type,
/// which the reference needs to compare return types: not below a root type
/// the schema lacks and not a meta field.
fn has_definition(field: u32) -> bool {
    field != UNTYPED && (field >> 16) != u32::from(META)
}

impl<F: Feed, S: Scalars> Walker<'_, '_, F, S> {
    /// Checks every selection set of the document, the hook of every
    /// selection set of the reference's rule.
    pub(super) fn overlapping(&mut self) -> R<()> {
        let n = self.b.frags.len();
        self.b.marks.clear();
        self.b.marks.resize(n, 0);
        self.stamp = 0;
        if self.b.memo_len > 0 {
            self.b.memo.fill(MEMO_EMPTY);
            self.b.memo_len = 0;
        }
        for index in 0..self.b.ops.len() {
            let (a, b) = self.op(index)?.sels;
            self.within(a, b)?;
        }
        for index in 0..narrow(n)? {
            let (a, b) = self.frag(index)?.sels;
            self.within(a, b)?;
        }
        for index in 0..narrow(self.b.sels.len())? {
            let sel = self.sel(index)?;
            let first = index.checked_add(1).ok_or(INTERNAL)?;
            if sel.kind == FIELD && sel.end > first {
                self.within(first, sel.end)?;
            }
        }
        Ok(())
    }

    /// Appends the fields of the selections `from` to `to` and of their
    /// inline fragments to the field list, and the fragments they spread,
    /// each once, to the fragment list.
    fn collect(&mut self, from: u32, to: u32) -> R<()> {
        self.stamp = self.stamp.checked_add(1).ok_or(INTERNAL)?;
        let stamp = self.stamp;
        self.collect_in(from, to, stamp)
    }

    /// The step of [`Walker::collect`] over one selection set.
    fn collect_in(&mut self, from: u32, to: u32, stamp: u32) -> R<()> {
        self.mark();
        let mut index = from;
        while index < to {
            let sel = self.sel(index)?;
            match sel.kind {
                FIELD => self.b.fl.push(index),
                INLINE => {
                    let first = index.checked_add(1).ok_or(INTERNAL)?;
                    self.collect_in(first, sel.end, stamp)?;
                }
                _ => {
                    let mark = self.b.marks.get_mut(widen(sel.field)?).ok_or(INTERNAL)?;
                    if *mark != stamp {
                        *mark = stamp;
                        self.b.fr.push(sel.field);
                    }
                }
            }
            if sel.end <= index {
                return Err(INTERNAL);
            }
            index = sel.end;
        }
        Ok(())
    }

    /// Returns the entry `index` of the field list.
    #[inline]
    fn fl(&self, index: usize) -> R<u32> {
        self.b.fl.get(index).copied().ok_or(INTERNAL)
    }

    /// Returns the entry `index` of the fragment list.
    #[inline]
    fn fr(&self, index: usize) -> R<u32> {
        self.b.fr.get(index).copied().ok_or(INTERNAL)
    }

    /// Returns the response key of the selection `sel`.
    #[inline]
    fn key(&self, sel: u32) -> R<&[u8]> {
        self.text(self.sel(sel)?.key)
    }

    /// Sorts the entries `from` to `to` of the field list by response key.
    fn sort_by_key(&mut self, from: usize, to: usize) -> R<()> {
        let (feed, plan) = (&*self.f, &self.b.sels);
        let key = |sel: u32| {
            let found = usize::try_from(sel).ok().and_then(|i| plan.get(i));
            found.and_then(|sel| feed.text(sel.key)).unwrap_or_default()
        };
        let list = self.b.fl.get_mut(from..to).ok_or(INTERNAL)?;
        sort_by_bytes(list, &key);
        Ok(())
    }

    /// Looks the pair of `a` and `b` of the kind `kind` up among the
    /// compared pairs and records it, the `has` and the `add` of the pair
    /// sets of the reference: true when the pair was compared before under a
    /// flag that covers `excl`. A pair compared with parents that may overlap
    /// covers both flags, and one compared with mutually exclusive parents
    /// covers only that flag.
    fn memo(&mut self, kind: u64, a: u32, b: u32, excl: bool) -> R<bool> {
        if a >= 1 << 31 || b >= 1 << 31 {
            return Err(INTERNAL);
        }
        let entry = (kind << 63) | (u64::from(a) << 32) | (u64::from(b) << 1) | u64::from(excl);
        let filled = self.b.memo_len.saturating_add(1).saturating_mul(4);
        if filled > self.b.memo.len().saturating_mul(3) {
            self.memo_grow()?;
        }
        let mask = self.b.memo.len().checked_sub(1).ok_or(INTERNAL)?;
        let mut slot = memo_slot(entry, mask);
        for _ in 0..self.b.memo.len() {
            let held = self.b.memo.get_mut(slot).ok_or(INTERNAL)?;
            if *held == MEMO_EMPTY {
                *held = entry;
                self.b.memo_len = self.b.memo_len.saturating_add(1);
                return Ok(false);
            }
            if *held >> 1 == entry >> 1 {
                if excl || *held & 1 == 0 {
                    return Ok(true);
                }
                *held = entry;
                return Ok(false);
            }
            slot = (slot + 1) & mask;
        }
        Err(INTERNAL)
    }

    /// Doubles the slots of the record of compared pairs, 64 at least, and
    /// places every entry again.
    fn memo_grow(&mut self) -> R<()> {
        let slots = self.b.memo.len().checked_mul(2).ok_or(INTERNAL)?.max(64);
        let old = mem::take(&mut self.b.memo);
        self.b.memo.resize(slots, MEMO_EMPTY);
        let mask = slots - 1;
        for entry in old {
            if entry == MEMO_EMPTY {
                continue;
            }
            let mut slot = memo_slot(entry, mask);
            let mut placed = false;
            for _ in 0..slots {
                let held = self.b.memo.get_mut(slot).ok_or(INTERNAL)?;
                if *held == MEMO_EMPTY {
                    *held = entry;
                    placed = true;
                    break;
                }
                slot = (slot + 1) & mask;
            }
            if !placed {
                return Err(INTERNAL);
            }
        }
        Ok(())
    }

    /// Checks the selection set of the selections `from` to `to`, the
    /// `findConflictsWithinSelectionSet` of the reference.
    ///
    /// A set of one field holds no pair and spreads no fragment, so it is
    /// done before its fields are collected.
    fn within(&mut self, from: u32, to: u32) -> R<()> {
        let first = self.sel(from)?;
        if first.kind == FIELD && first.end == to {
            return Ok(());
        }
        let (f0, r0) = (self.b.fl.len(), self.b.fr.len());
        self.collect(from, to)?;
        let (f1, r1) = (self.b.fl.len(), self.b.fr.len());
        self.pairs_within(f0, f1)?;
        for i in r0..r1 {
            let frag = self.fr(i)?;
            self.fields_and_fragment(from, (f0, f1), frag, false)?;
            for j in i + 1..r1 {
                let other = self.fr(j)?;
                self.fragments(frag, other, false)?;
            }
        }
        self.b.fl.truncate(f0);
        self.b.fr.truncate(r0);
        Ok(())
    }

    /// Compares the pairs of the fields `f0` to `f1` of the field list with
    /// one response key, every pair once, sorted by key above 16 fields, the
    /// `collectConflictsWithin` of the reference.
    fn pairs_within(&mut self, f0: usize, f1: usize) -> R<()> {
        if f1.checked_sub(f0).ok_or(INTERNAL)? <= 16 {
            for x in f0..f1 {
                for y in x + 1..f1 {
                    let (p, q) = (self.fl(x)?, self.fl(y)?);
                    if self.key(p)? == self.key(q)? {
                        self.find_conflict(p, q, false)?;
                    }
                }
            }
            return Ok(());
        }
        self.sort_by_key(f0, f1)?;
        let mut x = f0;
        while x < f1 {
            let mut y = x + 1;
            while y < f1 && self.key(self.fl(x)?)? == self.key(self.fl(y)?)? {
                y += 1;
            }
            for i in x..y {
                for j in i + 1..y {
                    let (p, q) = (self.fl(i)?, self.fl(j)?);
                    self.find_conflict(p, q, false)?;
                }
            }
            x = y;
        }
        Ok(())
    }

    /// Compares every pair of a field of the run `a` and a field of the run
    /// `b` of the field list with one response key, both sorted by key and
    /// merged when there are more than 64 pairs, the
    /// `collectConflictsBetween` of the reference.
    fn pairs_between(&mut self, left: (usize, usize), right: (usize, usize), excl: bool) -> R<()> {
        let (a0, a1) = left;
        let (b0, b1) = right;
        let pairs = (a1.checked_sub(a0).ok_or(INTERNAL)?)
            .saturating_mul(b1.checked_sub(b0).ok_or(INTERNAL)?);
        if pairs <= 64 {
            for x in a0..a1 {
                for y in b0..b1 {
                    let (p, q) = (self.fl(x)?, self.fl(y)?);
                    if self.key(p)? == self.key(q)? {
                        self.find_conflict(p, q, excl)?;
                    }
                }
            }
            return Ok(());
        }
        self.sort_by_key(a0, a1)?;
        self.sort_by_key(b0, b1)?;
        let (mut xa, mut yb) = (a0, b0);
        while xa < a1 && yb < b1 {
            let (kx, ky) = (self.fl(xa)?, self.fl(yb)?);
            match self.key(kx)?.cmp(self.key(ky)?) {
                Ordering::Less => xa += 1,
                Ordering::Greater => yb += 1,
                Ordering::Equal => {
                    let mut x2 = xa + 1;
                    while x2 < a1 && self.key(self.fl(x2)?)? == self.key(kx)? {
                        x2 += 1;
                    }
                    let mut y2 = yb + 1;
                    while y2 < b1 && self.key(self.fl(y2)?)? == self.key(ky)? {
                        y2 += 1;
                    }
                    for i in xa..x2 {
                        for j in yb..y2 {
                            let (p, q) = (self.fl(i)?, self.fl(j)?);
                            self.find_conflict(p, q, excl)?;
                        }
                    }
                    xa = x2;
                    yb = y2;
                }
            }
        }
        Ok(())
    }

    /// Compares the fields `fields` of the field list of the selection set
    /// that starts at `set` with the fragment `frag` and every fragment it
    /// spreads, each pair of the set and a fragment once, the
    /// `collectConflictsBetweenFieldsAndFragment` of the reference.
    fn fields_and_fragment(
        &mut self,
        set: u32,
        fields: (usize, usize),
        frag: u32,
        excl: bool,
    ) -> R<()> {
        let base = self.b.work.len();
        self.b.work.push((frag, 0));
        while self.b.work.len() > base {
            let Some((target, _)) = self.b.work.pop() else {
                break;
            };
            if self.memo(MEMO_SET_FRAGMENT, set, target, excl)? {
                continue;
            }
            let (a, b) = self.frag(target)?.sels;
            if a == set {
                // The fields of a fragment are not compared with themselves.
                continue;
            }
            let (g0, s0) = (self.b.fl.len(), self.b.fr.len());
            self.collect(a, b)?;
            let (g1, s1) = (self.b.fl.len(), self.b.fr.len());
            self.pairs_between(fields, (g0, g1), excl)?;
            for k in s0..s1 {
                let spread = self.fr(k)?;
                self.b.work.push((spread, 0));
            }
            self.b.fl.truncate(g0);
            self.b.fr.truncate(s0);
        }
        Ok(())
    }

    /// Compares the fragments `f` and `g`, then each with the fragments the
    /// other spreads, every pair once and no fragment with itself, the
    /// `collectConflictsBetweenFragments` of the reference.
    fn fragments(&mut self, f: u32, g: u32, excl: bool) -> R<()> {
        let base = self.b.work.len();
        self.b.work.push((f, g));
        while self.b.work.len() > base {
            let Some((f, g)) = self.b.work.pop() else {
                break;
            };
            if f == g {
                continue;
            }
            let (lo, hi) = if f < g { (f, g) } else { (g, f) };
            if self.memo(MEMO_FRAGMENTS, lo, hi, excl)? {
                continue;
            }
            let (fa, fb) = self.frag(f)?.sels;
            let (ga, gb) = self.frag(g)?.sels;
            let (a0, n0) = (self.b.fl.len(), self.b.fr.len());
            self.collect(fa, fb)?;
            let (a1, n1) = (self.b.fl.len(), self.b.fr.len());
            self.collect(ga, gb)?;
            let (b1, m1) = (self.b.fl.len(), self.b.fr.len());
            self.pairs_between((a0, a1), (a1, b1), excl)?;
            for k in n1..m1 {
                let spread = self.fr(k)?;
                self.b.work.push((f, spread));
            }
            for k in n0..n1 {
                let spread = self.fr(k)?;
                self.b.work.push((spread, g));
            }
            self.b.fl.truncate(a0);
            self.b.fr.truncate(n0);
        }
        Ok(())
    }

    /// Compares the selection sets `s1` and `s2`, each the selections from
    /// its first to the one past its last, the
    /// `findConflictsBetweenSubSelectionSets` of the reference.
    fn between_sub_sets(&mut self, excl: bool, s1: (u32, u32), s2: (u32, u32)) -> R<()> {
        let (a0, n0) = (self.b.fl.len(), self.b.fr.len());
        self.collect(s1.0, s1.1)?;
        let (a1, n1) = (self.b.fl.len(), self.b.fr.len());
        self.collect(s2.0, s2.1)?;
        let (b1, m1) = (self.b.fl.len(), self.b.fr.len());
        self.pairs_between((a0, a1), (a1, b1), excl)?;
        for k in n1..m1 {
            let frag = self.fr(k)?;
            self.fields_and_fragment(s1.0, (a0, a1), frag, excl)?;
        }
        for k in n0..n1 {
            let frag = self.fr(k)?;
            self.fields_and_fragment(s2.0, (a1, b1), frag, excl)?;
        }
        for k in n0..n1 {
            for l in n1..m1 {
                let (f, g) = (self.fr(k)?, self.fr(l)?);
                self.fragments(f, g, excl)?;
            }
        }
        self.b.fl.truncate(a0);
        self.b.fr.truncate(n0);
        Ok(())
    }

    /// Returns whether the type `index` is an object type.
    fn is_object(&self, index: u16) -> R<bool> {
        Ok(index != NONE && self.ty(index)?.kind == Kind::Object)
    }

    /// Compares two fields `p` and `q` of one response key, the
    /// `findConflict` of the reference.
    fn find_conflict(&mut self, p: u32, q: u32, parents_exclusive: bool) -> R<()> {
        self.mark();
        let (sa, sb) = (self.sel(p)?, self.sel(q)?);
        let exclusive = parents_exclusive
            || (sa.parent != sb.parent
                && self.is_object(sa.parent)?
                && self.is_object(sb.parent)?);
        if !exclusive && (self.text(sa.name)? != self.text(sb.name)? || !self.same_args(sa, sb)?) {
            return Err(CONFLICT);
        }
        // The reference compares return types only where it finds the field
        // among the fields of the parent type, which the meta fields are not.
        if has_definition(sa.field) && has_definition(sb.field) {
            let (ta, tb) = (self.field_of(sa.field)?.ty, self.field_of(sb.field)?.ty);
            if self.types_conflict(ta, tb)? {
                return Err(CONFLICT);
            }
        }
        let (pa, qb) = (
            p.checked_add(1).ok_or(INTERNAL)?,
            q.checked_add(1).ok_or(INTERNAL)?,
        );
        if sa.end > pa && sb.end > qb {
            self.between_sub_sets(exclusive, (pa, sa.end), (qb, sb.end))?;
        }
        Ok(())
    }

    /// Returns whether two return types conflict, the `doTypesConflict` of
    /// the reference: their wrappers differ, or one is a leaf and the named
    /// types differ.
    fn types_conflict(&self, a: TypeRef, b: TypeRef) -> R<bool> {
        let (ma, mb) = (self.t.mods(a.mods), self.t.mods(b.mods));
        if ma.ok_or(INTERNAL)? != mb.ok_or(INTERNAL)? {
            return Ok(true);
        }
        let leaf = self.ty(a.named)?.is_leaf() || self.ty(b.named)?.is_leaf();
        Ok(leaf && a.named != b.named)
    }

    /// Returns whether two fields carry the same arguments with values that
    /// print the same, in any order, the `sameArguments` of the reference.
    ///
    /// Names are unique in each list here. Up to 8 arguments the names are
    /// paired directly, above that both lists are sorted by name.
    fn same_args(&mut self, left: Sel, right: Sel) -> R<bool> {
        if left.nargs != right.nargs {
            return Ok(false);
        }
        let count = u32::from(left.nargs);
        let (ra, rb) = (left.args, right.args);
        let ea = ra.checked_add(count).ok_or(INTERNAL)?;
        let eb = rb.checked_add(count).ok_or(INTERNAL)?;
        if count <= 8 {
            for x in ra..ea {
                let ax = self.arg(x)?;
                let mut matched = None;
                for y in rb..eb {
                    let by = self.arg(y)?;
                    if self.text(by.name)? == self.text(ax.name)? {
                        matched = Some(by);
                        break;
                    }
                }
                let Some(by) = matched else {
                    return Ok(false);
                };
                if !self.val_eq(ax.val, by.val)? {
                    return Ok(false);
                }
            }
            return Ok(true);
        }
        let t0 = self.b.tmp.len();
        let mut tmp = mem::take(&mut self.b.tmp);
        tmp.extend(ra..ea);
        tmp.extend(rb..eb);
        self.sort_args(tmp.get_mut(t0..)).ok_or(INTERNAL)?;
        self.b.tmp = tmp;
        let count = widen(count)?;
        for k in 0..count {
            let first = *self.b.tmp.get(t0 + k).ok_or(INTERNAL)?;
            let second = *self.b.tmp.get(t0 + count + k).ok_or(INTERNAL)?;
            let (ax, by) = (self.arg(first)?, self.arg(second)?);
            if self.text(ax.name)? != self.text(by.name)? || !self.val_eq(ax.val, by.val)? {
                self.b.tmp.truncate(t0);
                return Ok(false);
            }
        }
        self.b.tmp.truncate(t0);
        Ok(true)
    }

    /// Sorts each half of `list`, two runs of argument indices of one length,
    /// by the names of the arguments.
    fn sort_args(&self, list: Option<&mut [u32]>) -> Option<()> {
        let list = list?;
        let half = list.len() / 2;
        let (feed, args) = (&*self.f, &self.b.args);
        let name = |index: u32| {
            let arg = usize::try_from(index).ok().and_then(|i| args.get(i));
            arg.and_then(|arg| feed.text(arg.name)).unwrap_or_default()
        };
        let (first, second) = list.split_at_mut(half);
        sort_by_bytes(first, &name);
        sort_by_bytes(second, &name);
        Some(())
    }

    /// Returns whether the values `x` and `y` print the same, the comparison
    /// of `sameArguments`, with the fields of an object in any order, or the
    /// defer of [`Reason::Digits`] for two object values that compare alike
    /// with a field name the order of the rules may not tell apart.
    fn val_eq(&mut self, x: u32, y: u32) -> R<bool> {
        self.mark();
        let (va, vb) = (self.val(x)?, self.val(y)?);
        if va.kind != vb.kind {
            return Ok(false);
        }
        match va.kind {
            v::TRUE | v::FALSE | v::NULL => Ok(true),
            v::LIST => {
                let mut i = x.checked_add(1).ok_or(INTERNAL)?;
                let mut j = y.checked_add(1).ok_or(INTERNAL)?;
                loop {
                    match (i < va.end, j < vb.end) {
                        (true, true) => {
                            if !self.val_eq(i, j)? {
                                return Ok(false);
                            }
                            let (ni, nj) = (self.val(i)?.end, self.val(j)?.end);
                            if ni <= i || nj <= j {
                                return Err(INTERNAL);
                            }
                            (i, j) = (ni, nj);
                        }
                        (false, false) => return Ok(true),
                        _ => return Ok(false),
                    }
                }
            }
            v::OBJECT => self.objects_eq(x, va.end, y, vb.end),
            _ => Ok(self.text(va.tok)? == self.text(vb.tok)?),
        }
    }

    /// Returns whether the object values `x`, whose fields end at `x_end`,
    /// and `y`, whose fields end at `y_end`, hold the same fields with values
    /// that print the same.
    ///
    /// Input field names are unique within an object here. Up to 8 fields
    /// the names are paired directly, above that both lists of fields are
    /// sorted by name. Two objects alike that way print the same in the
    /// rules when no two of their names order alike, which a name with a run
    /// of [`DIGIT_RUN`] digits may break, so such objects defer.
    fn objects_eq(&mut self, x: u32, x_end: u32, y: u32, y_end: u32) -> R<bool> {
        let t0 = self.b.tmp.len();
        let mut tmp = mem::take(&mut self.b.tmp);
        let listed = self.list_fields(&mut tmp, x, x_end);
        let t1 = tmp.len();
        let listed = listed.and_then(|()| self.list_fields(&mut tmp, y, y_end));
        let n = t1 - t0;
        if listed.is_err() || tmp.len() - t1 != n {
            tmp.truncate(t0);
            self.b.tmp = tmp;
            return listed.map(|()| false);
        }
        if n > 8 {
            let (feed, vals) = (&*self.f, &self.b.vals);
            let name = |index: u32| {
                let val = usize::try_from(index).ok().and_then(|i| vals.get(i));
                val.and_then(|val| feed.text(val.tok)).unwrap_or_default()
            };
            if let Some(first) = tmp.get_mut(t0..t1) {
                sort_by_bytes(first, &name);
            }
            if let Some(second) = tmp.get_mut(t1..) {
                sort_by_bytes(second, &name);
            }
        }
        self.b.tmp = tmp;
        let same = self.fields_eq(t0, t1, n).and_then(|same| {
            if same && self.digit_names(t0, t1)? {
                return Err(defer(Reason::Digits));
            }
            Ok(same)
        });
        self.b.tmp.truncate(t0);
        same
    }

    /// Returns whether the name of a field among the entries `t0` to `t1` of
    /// the sorting buffer holds a run of [`DIGIT_RUN`] digits or more.
    fn digit_names(&self, t0: usize, t1: usize) -> R<bool> {
        for at in t0..t1 {
            let field = *self.b.tmp.get(at).ok_or(INTERNAL)?;
            if long_digit_run(self.text(self.val(field)?.tok)?) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Appends to `list` the fields of the object value `object`, whose
    /// fields end at `end`.
    fn list_fields(&self, list: &mut Vec<u32>, object: u32, end: u32) -> R<()> {
        let mut field = object.checked_add(1).ok_or(INTERNAL)?;
        while field < end {
            list.push(field);
            let next = self.val(field)?.end;
            if next <= field {
                return Err(INTERNAL);
            }
            field = next;
        }
        Ok(())
    }

    /// Returns whether the `n` fields from `t0` and the `n` fields from `t1`
    /// of the sorting buffer pair up by name with values that print the same.
    fn fields_eq(&mut self, t0: usize, t1: usize, n: usize) -> R<bool> {
        for k in 0..n {
            let fa = *self.b.tmp.get(t0 + k).ok_or(INTERNAL)?;
            let fb = if n > 8 {
                *self.b.tmp.get(t1 + k).ok_or(INTERNAL)?
            } else {
                let name = self.val(fa)?.tok;
                let mut matched = None;
                for m in t1..t1 + n {
                    let candidate = *self.b.tmp.get(m).ok_or(INTERNAL)?;
                    if self.text(self.val(candidate)?.tok)? == self.text(name)? {
                        matched = Some(candidate);
                        break;
                    }
                }
                let Some(fb) = matched else {
                    return Ok(false);
                };
                fb
            };
            if self.text(self.val(fa)?.tok)? != self.text(self.val(fb)?.tok)? {
                return Ok(false);
            }
            let (va, vb) = (
                fa.checked_add(1).ok_or(INTERNAL)?,
                fb.checked_add(1).ok_or(INTERNAL)?,
            );
            if !self.val_eq(va, vb)? {
                return Ok(false);
            }
        }
        Ok(true)
    }
}

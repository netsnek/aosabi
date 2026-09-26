// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass after: the checks of the single pass that span the
 * document, which run after its last token.
 *
 * No analogue in the reference.
 */

//! The checks after the last token of the single pass.
//!
//! The walk records every fragment with the selections, the uses of
//! variables and the spreads of its own text, and every spread with its
//! parent type and its depths. The checks here resolve the spreads to their
//! fragments and walk the graph of fragments on stacks in the buffers, never
//! by recursion, so the stack of the pass is bounded by the nesting of one
//! definition. The rule for overlapping fields, the last check, is
//! `overlap.rs`.

use alloc::vec::Vec;
use core::{
    cmp::Ordering,
    mem, //
};

use super::{
    INTERNAL,
    NO_VALUE,
    Op,
    R,
    Reason,
    Rule,
    feed::{
        Feed,
        Tok, //
    },
    narrow,
    scalars::Scalars,
    sort_by_bytes,
    table::NONE,
    v,
    walk::{
        NOINTRO,
        Walker,
        defer,
        refuse, //
    },
    widen, //
};

/// Returns the mark of the fragment `index` in `marks`.
fn mark_of(marks: &[u32], index: u32) -> R<u32> {
    marks.get(widen(index)?).copied().ok_or(INTERNAL)
}

/// Sets the mark of the fragment `index` in `marks` to `value`.
fn set_mark(marks: &mut [u32], index: u32, value: u32) -> R<()> {
    *marks.get_mut(widen(index)?).ok_or(INTERNAL)? = value;
    Ok(())
}

/// Returns `mods` without its outermost non null wrapper, and whether it
/// had one.
fn strip_non_null(mods: &[u8]) -> (bool, &[u8]) {
    match mods.split_first() {
        Some((b'!', rest)) => (true, rest),
        _ => (false, mods),
    }
}

/// Returns whether a variable of the named type `vn` with the wrappers `vm`
/// may stand at a position of the named type `ln` with the wrappers `lm`.
///
/// This is `allowedVariableUsage` of the rule for variables in an allowed
/// position, with `isTypeSubTypeOf` over input types: a non null position
/// takes a nullable variable only where the variable or the position has a
/// default that is not `null`.
fn allowed(
    vn: u16,
    vm: &[u8],
    var_default_non_null: bool,
    ln: u16,
    lm: &[u8],
    loc_default: bool,
) -> bool {
    let (l_non_null, l_rest) = strip_non_null(lm);
    let (v_non_null, _) = strip_non_null(vm);
    if l_non_null && !v_non_null {
        if !var_default_non_null && !loc_default {
            return false;
        }
        return sub_type(vn, vm, ln, l_rest);
    }
    sub_type(vn, vm, ln, lm)
}

/// Returns whether the input type `vn` wrapped in `vm` is a sub type of the
/// input type `ln` wrapped in `lm`, the `isTypeSubTypeOf` of the reference
/// over input types, where a named type is a sub type of itself alone.
fn sub_type(vn: u16, vm: &[u8], ln: u16, lm: &[u8]) -> bool {
    match (vm.split_first(), lm.split_first()) {
        (Some((b'!', v_rest)), Some((b'!', l_rest)))
        | (Some((b'[', v_rest)), Some((b'[', l_rest))) => sub_type(vn, v_rest, ln, l_rest),
        (_, Some((b'!', _))) => false,
        (Some((b'!', v_rest)), _) => sub_type(vn, v_rest, ln, lm),
        (_, Some((b'[', _))) | (Some((b'[', _)), _) => false,
        _ => vn == ln,
    }
}

impl<F: Feed, S: Scalars> Walker<'_, '_, F, S> {
    /// Runs every check after the last token, in the order a broken rule is
    /// cheapest to find.
    pub(super) fn after(&mut self) -> R<()> {
        self.fragment_names()?;
        self.operation_names()?;
        self.cycles()?;
        self.used_fragments()?;
        self.possible_spreads()?;
        self.depths()?;
        for index in 0..self.b.ops.len() {
            self.variables(index)?;
        }
        self.overlapping()
    }

    /// Checks that the fragment names are unique and resolves every spread to
    /// its fragment, comparing pairs up to 8 fragments and sorting above.
    fn fragment_names(&mut self) -> R<()> {
        let n = self.b.frags.len();
        let sorted = n > 8;
        let mut order = mem::take(&mut self.b.scratch);
        order.clear();
        let unique = if sorted {
            order.extend(0..narrow(n)?);
            self.sort_fragments(&mut order);
            self.adjacent_names_differ(&order)
        } else {
            self.all_names_differ()
        };
        if !matches!(unique, Ok(true)) {
            self.b.scratch = order;
            return unique.and(Err(refuse(Rule::UniqueFragmentNames)));
        }
        let resolved = self.resolve_spreads(&order, sorted);
        order.clear();
        self.b.scratch = order;
        resolved
    }

    /// Sorts the fragment indices of `order` by the names of the fragments.
    fn sort_fragments(&self, order: &mut [u32]) {
        let frags = &self.b.frags;
        let name = |index: u32| {
            let frag = usize::try_from(index).ok().and_then(|i| frags.get(i));
            frag.and_then(|frag| self.f.text(frag.name))
                .unwrap_or_default()
        };
        sort_by_bytes(order, &name);
    }

    /// Returns whether no two neighbours of the sorted `order` share a name.
    fn adjacent_names_differ(&self, order: &[u32]) -> R<bool> {
        for pair in order.windows(2) {
            if let [a, b] = pair
                && self.text(self.frag(*a)?.name)? == self.text(self.frag(*b)?.name)?
            {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// Returns whether no two fragments share a name, comparing every pair.
    fn all_names_differ(&self) -> R<bool> {
        for (i, a) in self.b.frags.iter().enumerate() {
            let a = self.text(a.name)?;
            for b in self.b.frags.iter().skip(i + 1) {
                if self.text(b.name)? == a {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    /// Resolves every spread to its fragment, through the sorted `order`
    /// where `sorted` holds.
    fn resolve_spreads(&mut self, order: &[u32], sorted: bool) -> R<()> {
        for index in 0..self.b.spreads.len() {
            let spread = *self.b.spreads.get(index).ok_or(INTERNAL)?;
            let frag = self
                .find_fragment(spread.name, order, sorted)?
                .ok_or(refuse(Rule::KnownFragmentNames))?;
            self.b.spreads.get_mut(index).ok_or(INTERNAL)?.frag = frag;
            self.b
                .sels
                .get_mut(widen(spread.sel)?)
                .ok_or(INTERNAL)?
                .field = frag;
        }
        Ok(())
    }

    /// Returns the fragment named by the token `name`.
    fn find_fragment(&self, name: Tok, order: &[u32], sorted: bool) -> R<Option<u32>> {
        let text = self.text(name)?;
        if sorted {
            let found = order.binary_search_by(|&index| match self.frag(index) {
                Ok(frag) => self.f.text(frag.name).unwrap_or_default().cmp(text),
                Err(_) => Ordering::Less,
            });
            return Ok(found.ok().and_then(|at| order.get(at).copied()));
        }
        for (index, frag) in self.b.frags.iter().enumerate() {
            if self.text(frag.name)? == text {
                return narrow(index).map(Some);
            }
        }
        Ok(None)
    }

    /// Checks that a lone anonymous operation stands alone and that the
    /// operation names are unique.
    fn operation_names(&mut self) -> R<()> {
        let n = self.b.ops.len();
        if n > 1 && self.b.ops.iter().any(|op| !op.named) {
            return Err(refuse(Rule::LoneAnonymousOperation));
        }
        let base = self.b.names.len();
        for index in 0..n {
            let name = self.op(index)?.name;
            self.b.names.push(name);
        }
        if self.duplicate_names(base)? {
            return Err(refuse(Rule::UniqueOperationNames));
        }
        Ok(())
    }

    /// Checks that no fragment spreads itself, by a depth first walk with
    /// three colours on a stack of the buffers.
    fn cycles(&mut self) -> R<()> {
        let n = self.b.frags.len();
        let mut marks = mem::take(&mut self.b.marks);
        marks.clear();
        marks.resize(n, 0);
        let mut stack = mem::take(&mut self.b.work);
        stack.clear();
        let found = self.find_cycle(&mut marks, &mut stack);
        stack.clear();
        self.b.work = stack;
        self.b.marks = marks;
        found
    }

    /// Walks the graph of fragments from every fragment not yet walked, with
    /// `marks` as the colours and `stack` holding the fragment and its next
    /// spread of every step.
    fn find_cycle(&self, marks: &mut [u32], stack: &mut Vec<(u32, u32)>) -> R<()> {
        for start in 0..narrow(marks.len())? {
            if mark_of(marks, start)? != 0 {
                continue;
            }
            stack.clear();
            stack.push((start, self.frag(start)?.spreads.0));
            set_mark(marks, start, 1)?;
            while let Some(&(frag, next)) = stack.last() {
                if next >= self.frag(frag)?.spreads.1 {
                    set_mark(marks, frag, 2)?;
                    stack.pop();
                    continue;
                }
                if let Some(top) = stack.last_mut() {
                    top.1 = next.checked_add(1).ok_or(INTERNAL)?;
                }
                let target = self.spread(next)?.frag;
                match mark_of(marks, target)? {
                    0 => {
                        set_mark(marks, target, 1)?;
                        stack.push((target, self.frag(target)?.spreads.0));
                    }
                    1 => return Err(refuse(Rule::NoFragmentCycles)),
                    _ => {}
                }
            }
        }
        Ok(())
    }

    /// Marks with `stamp` every fragment the spreads `from` to `to` reach,
    /// and appends each fragment it marks to the reached fragments.
    fn reach(&mut self, from: u32, to: u32, stamp: u32) -> R<()> {
        let mut stack = mem::take(&mut self.b.scratch);
        stack.clear();
        stack.extend(from..to);
        let reached = self.reach_from(&mut stack, stamp);
        stack.clear();
        self.b.scratch = stack;
        reached
    }

    /// Marks with `stamp` every fragment the spreads on `stack` reach, and
    /// appends each fragment it marks to the reached fragments.
    fn reach_from(&mut self, stack: &mut Vec<u32>, stamp: u32) -> R<()> {
        while let Some(spread) = stack.pop() {
            let frag = self.spread(spread)?.frag;
            if mark_of(&self.b.marks, frag)? != stamp {
                set_mark(&mut self.b.marks, frag, stamp)?;
                self.b.reached.push(frag);
                let (a, b) = self.frag(frag)?.spreads;
                stack.extend(a..b);
            }
        }
        Ok(())
    }

    /// Checks that every fragment is reached from an operation.
    fn used_fragments(&mut self) -> R<()> {
        let n = self.b.frags.len();
        if n == 0 {
            return Ok(());
        }
        self.b.marks.clear();
        self.b.marks.resize(n, 0);
        for index in 0..self.b.ops.len() {
            let (a, b) = self.op(index)?.spreads;
            self.reach(a, b, 1)?;
        }
        if self.b.marks.iter().any(|&mark| mark != 1) {
            return Err(refuse(Rule::NoUnusedFragments));
        }
        Ok(())
    }

    /// Checks that every fragment spread can apply where it stands.
    fn possible_spreads(&self) -> R<()> {
        for spread in &self.b.spreads {
            if spread.parent != NONE
                && !self.overlap(spread.parent, self.frag(spread.frag)?.cond)?
            {
                return Err(refuse(Rule::PossibleFragmentSpreads));
            }
        }
        Ok(())
    }

    /// Computes the introspection depth and the field depth of every
    /// fragment through its spreads by one walk in post order, then checks
    /// the introspection depth of every spread inside a `__schema` or
    /// `__type` field and the field depth of every operation through its
    /// spreads. The graph of fragments has no cycle here.
    fn depths(&mut self) -> R<()> {
        let n = self.b.frags.len();
        if n == 0 {
            return Ok(());
        }
        self.b.marks.clear();
        self.b.marks.resize(n, 0);
        let mut stack = mem::take(&mut self.b.scratch);
        let walked = self.fragment_depths(&mut stack);
        stack.clear();
        self.b.scratch = stack;
        walked?;
        for spread in &self.b.spreads {
            if spread.intro != NOINTRO {
                let depth = spread
                    .intro
                    .checked_add(self.frag(spread.frag)?.max)
                    .ok_or(INTERNAL)?;
                if depth >= 3 {
                    return Err(refuse(Rule::MaxIntrospectionDepth));
                }
            }
        }
        for op in &self.b.ops {
            for index in op.spreads.0..op.spreads.1 {
                let spread = self.spread(index)?;
                let above = spread.fd.checked_sub(1).ok_or(INTERNAL)?;
                let depth = above.saturating_add(self.frag(spread.frag)?.fdm);
                if depth > self.o.max_depth {
                    return Err(defer(Reason::Depth));
                }
            }
        }
        Ok(())
    }

    /// Fills the depths of every `frag` entry through its spreads, by a walk in
    /// post order on `stack`, with the marks 0 for a fragment not done and 1
    /// for one done.
    ///
    /// A fragment that several spreads push before it is done stands on the
    /// stack more than once, and every copy after the first that is done
    /// leaves without reading its spreads again, so each fragment reads its
    /// spreads at most twice.
    fn fragment_depths(&mut self, stack: &mut Vec<u32>) -> R<()> {
        for start in 0..narrow(self.b.frags.len())? {
            if mark_of(&self.b.marks, start)? != 0 {
                continue;
            }
            stack.clear();
            stack.push(start);
            while let Some(&frag) = stack.last() {
                if mark_of(&self.b.marks, frag)? != 0 {
                    stack.pop();
                    continue;
                }
                let (a, b) = self.frag(frag)?.spreads;
                let mut pending = false;
                for index in a..b {
                    let target = self.spread(index)?.frag;
                    if mark_of(&self.b.marks, target)? == 0 {
                        stack.push(target);
                        pending = true;
                    }
                }
                if pending {
                    continue;
                }
                let own = self.frag(frag)?;
                let (mut max, mut fdm) = (own.local, own.fdl);
                for index in a..b {
                    let spread = self.spread(index)?;
                    let target = self.frag(spread.frag)?;
                    max = max.max(spread.lc.saturating_add(target.max));
                    let above = spread.fd.checked_sub(1).ok_or(INTERNAL)?;
                    fdm = fdm.max(above.saturating_add(target.fdm));
                }
                let slot = self.b.frags.get_mut(widen(frag)?).ok_or(INTERNAL)?;
                slot.max = max;
                slot.fdm = fdm;
                set_mark(&mut self.b.marks, frag, 1)?;
                stack.pop();
            }
        }
        Ok(())
    }

    /// Checks the variables of the operation `index` through every fragment
    /// it reaches: every use defined and allowed in its position, and every
    /// definition used.
    ///
    /// The walk of its spreads lists the fragments it reaches, so the check
    /// reads those fragments alone, as `getRecursiveVariableUsages` of the
    /// reference reads them, and the operations of a document cost the
    /// fragments each reaches and not every fragment of the document each.
    fn variables(&mut self, index: usize) -> R<()> {
        let op = self.op(index)?;
        let stamp = narrow(index)?.checked_add(2).ok_or(INTERNAL)?;
        let n = self.b.frags.len();
        if self.b.marks.len() < n {
            self.b.marks.resize(n, 0);
        }
        self.b.reached.clear();
        self.reach(op.spreads.0, op.spreads.1, stamp)?;
        let defined = widen(op.vars.1)?
            .checked_sub(widen(op.vars.0)?)
            .ok_or(INTERNAL)?;
        let base = self.b.scratch.len();
        self.b
            .scratch
            .resize(base.checked_add(defined).ok_or(INTERNAL)?, 0);
        // Above 8 definitions a use finds its definition in a sorted order,
        // the crossing of the names compared in pairs.
        let t0 = self.b.tmp.len();
        let sorted = defined > 8;
        if sorted {
            let mut tmp = mem::take(&mut self.b.tmp);
            tmp.extend(op.vars.0..op.vars.1);
            if let Some(order) = tmp.get_mut(t0..) {
                let vars = &self.b.vars;
                let name = |index: u32| {
                    let var = usize::try_from(index).ok().and_then(|i| vars.get(i));
                    var.and_then(|var| self.f.text(var.name))
                        .unwrap_or_default()
                };
                sort_by_bytes(order, &name);
            }
            self.b.tmp = tmp;
        }
        let scope = Scope {
            op,
            base,
            t0,
            sorted,
        };
        self.check_uses(scope, op.uses)?;
        for at in 0..self.b.reached.len() {
            let frag = *self.b.reached.get(at).ok_or(INTERNAL)?;
            let uses = self.frag(frag)?.uses;
            self.check_uses(scope, uses)?;
        }
        let unused = self.b.scratch.get(base..).ok_or(INTERNAL)?.contains(&0);
        if unused {
            return Err(refuse(Rule::NoUnusedVariables));
        }
        self.b.scratch.truncate(base);
        self.b.tmp.truncate(t0);
        Ok(())
    }

    /// Checks the uses `uses` of variables in the scope `scope`: each defined
    /// by the operation and allowed in its position, and marks the
    /// definitions they use.
    fn check_uses(&mut self, scope: Scope, uses: (u32, u32)) -> R<()> {
        for index in uses.0..uses.1 {
            let usage = *self.b.uses.get(widen(index)?).ok_or(INTERNAL)?;
            let k = self.definition_of(scope, usage.name)?;
            *self
                .b
                .scratch
                .get_mut(scope.base.checked_add(k).ok_or(INTERNAL)?)
                .ok_or(INTERNAL)? = 1;
            if usage.named == NONE {
                continue;
            }
            let at = widen(scope.op.vars.0)?.checked_add(k).ok_or(INTERNAL)?;
            let def = *self.b.vars.get(at).ok_or(INTERNAL)?;
            let vm = span(&self.b.mods, def.mods, def.nmods)?;
            let lm = span(&self.b.use_mods, usage.mods, usage.nmods)?;
            let default_non_null =
                def.default != NO_VALUE && self.val(def.default)?.kind != v::NULL;
            if !allowed(
                def.named,
                vm,
                default_non_null,
                usage.named,
                lm,
                usage.has_default,
            ) {
                return Err(refuse(Rule::VariablesInAllowedPosition));
            }
        }
        Ok(())
    }

    /// Returns the index among the definitions of the operation of `scope`
    /// of the one named by the token `name`.
    fn definition_of(&self, scope: Scope, name: Tok) -> R<usize> {
        let text = self.text(name)?;
        let first = scope.op.vars.0;
        if scope.sorted {
            let order = self.b.tmp.get(scope.t0..).ok_or(INTERNAL)?;
            let found = order.binary_search_by(|&index| {
                let var = usize::try_from(index).ok().and_then(|i| self.b.vars.get(i));
                var.and_then(|var| self.f.text(var.name))
                    .unwrap_or_default()
                    .cmp(text)
            });
            let at = found.map_err(|_| refuse(Rule::NoUndefinedVariables))?;
            let index = *order.get(at).ok_or(INTERNAL)?;
            return widen(index.checked_sub(first).ok_or(INTERNAL)?);
        }
        for index in first..scope.op.vars.1 {
            let var = self.b.vars.get(widen(index)?).ok_or(INTERNAL)?;
            if self.text(var.name)? == text {
                return widen(index - first);
            }
        }
        Err(refuse(Rule::NoUndefinedVariables))
    }
}

/// The operation whose variables are checked, and where its marks and its
/// sorted order stand in the buffers.
#[derive(Clone, Copy)]
struct Scope {
    /// The operation.
    op: Op,
    /// The first mark of its definitions in the scratch buffer.
    base: usize,
    /// The first entry of its sorted order in the sorting buffer.
    t0: usize,
    /// Whether its definitions are found through the sorted order.
    sorted: bool,
}

/// Returns the `count` bytes of `bytes` from `at`.
fn span(bytes: &[u8], at: u32, count: u16) -> R<&[u8]> {
    let at = widen(at)?;
    let end = at.checked_add(usize::from(count)).ok_or(INTERNAL)?;
    bytes.get(at..end).ok_or(INTERNAL)
}

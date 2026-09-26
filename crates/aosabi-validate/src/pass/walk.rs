// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass walk: the walk of the single pass over the tokens of
 * an executable document in the order of its grammar.
 *
 * No analogue in the reference.
 */

//! The walk of the single pass.
//!
//! The walk reads one definition after the other by recursive descent, as
//! the parser of the reference reads them, and each recursive step opens a
//! level of nesting, which [`Options::max_depth`](super::Options::max_depth)
//! bounds. Every check that one token decides stops the walk there with a
//! refusal or a defer, and the walk appends the plan of the document to the
//! buffers as it goes. The checks that span the document run after the last
//! token, in `after.rs` and `overlap.rs`.
//!
//! The walk reads the kind of each token once, when it moves to the token, and
//! the accessors of a token, a row or a buffer, each a few instructions, carry
//! `#[inline]`, since a build at opt-level z inlines little by itself and pays
//! a call for each where they stay out of line.

use core::mem;

use super::{
    ArgUse,
    Buffers,
    Cond,
    FIELD,
    Frag,
    INLINE,
    INTERNAL,
    NO_VALUE,
    Op,
    Options,
    R,
    Reason,
    Rule,
    SPREAD,
    SYNTAX,
    Sel,
    SpreadRec,
    Stop,
    UNTYPED,
    Use,
    Val,
    VarDef,
    feed::{
        Feed,
        Kind,
        Tok, //
    },
    narrow,
    op,
    scalars::{
        Answer,
        Literal,
        Scalars, //
    },
    sort_by_bytes,
    table::{
        ArgRow,
        FieldRow,
        Kind as TypeKind,
        META,
        META_SCHEMA,
        META_TYPE,
        META_TYPENAME,
        NONE,
        Scalar,
        Span,
        Table,
        TypeRow,
        fref,
        loc, //
    },
    v,
    widen, //
};

/// The introspection depth of a selection outside every `__schema` and
/// `__type` field.
pub(super) const NOINTRO: u32 = u32::MAX;

/// The most arguments, input fields or directives a bit set of the walk holds, 64.
const WIDE: usize = 64;

/// Returns the stop of a broken rule.
pub(super) const fn refuse(rule: Rule) -> Stop {
    Stop::Refuse(rule)
}

/// Returns the stop of a document the pass leaves to the rules.
pub(super) const fn defer(reason: Reason) -> Stop {
    Stop::Defer(reason)
}

/// The type of a position of a value: the named type and its wrappers.
#[derive(Clone, Copy, Debug)]
struct Pos<'m> {
    /// The named type.
    named: u16,
    /// The wrappers, outermost first.
    mods: &'m [u8],
}

/// The state of the walk over one document.
pub(super) struct Walker<'w, 't, F, S> {
    /// The table of the schema.
    pub(super) t: &'w Table<'t>,
    /// The feed of the document.
    pub(super) f: &'w mut F,
    /// The custom scalars of the table.
    pub(super) sc: &'w S,
    /// The bounds and the switches of the call.
    pub(super) o: Options,
    /// The buffers of the plan and of the checks.
    pub(super) b: &'w mut Buffers,
    /// The current token.
    pub(super) cur: Tok,
    /// The kind of the current token, read once when the walk moves to it.
    pub(super) cur_kind: Kind,
    /// The start of file token.
    pub(super) sof: Tok,
    /// The most introspection list fields on one path of the current
    /// definition.
    pub(super) local: u32,
    /// The depth of the deepest field of the current definition.
    pub(super) fdl: u32,
    /// The levels of nesting open at the current token.
    pub(super) depth: u32,
    /// The stamp of the last collection of the rule for overlapping fields.
    pub(super) stamp: u32,
    /// The lowest address of a local a recursive step of the walk took.
    #[cfg(feature = "pass-stack-mark")]
    pub(super) low: usize,
}

/// What one walk answers.
pub(super) struct Outcome {
    /// The result of the walk.
    pub(super) result: R<()>,
    /// The lowest address of the stack the walk reached.
    #[cfg(feature = "pass-stack-mark")]
    pub(super) low: usize,
}

/// Walks the document `feed` reads against `table` and runs the checks after
/// its last token.
pub(super) fn run<F: Feed, S: Scalars>(
    table: &Table<'_>,
    feed: &mut F,
    scalars: &S,
    options: Options,
    bufs: &mut Buffers,
) -> Outcome {
    let sof = feed.start();
    let mut walker = Walker {
        t: table,
        f: feed,
        sc: scalars,
        o: options,
        b: bufs,
        cur: sof,
        cur_kind: Kind::Sof,
        sof,
        local: 0,
        fdl: 0,
        depth: 0,
        stamp: 0,
        #[cfg(feature = "pass-stack-mark")]
        low: usize::MAX,
    };
    let result = walker.document();
    Outcome {
        result,
        #[cfg(feature = "pass-stack-mark")]
        low: walker.low,
    }
}

/// Returns whether a field name counts as a list of the rule for the
/// introspection depth, which counts fields by name.
#[inline]
fn is_list_name(name: &[u8]) -> bool {
    matches!(
        name,
        b"fields" | b"interfaces" | b"possibleTypes" | b"inputFields"
    )
}

/// Returns `mods` without its outermost non null wrapper.
#[inline]
fn nullable(mods: &[u8]) -> &[u8] {
    match mods.split_first() {
        Some((b'!', rest)) => rest,
        _ => mods,
    }
}

/// Returns whether an integer literal fits a signed 32 bit integer, the test
/// the literal parse of `Int` makes.
fn int_fits(text: &[u8]) -> bool {
    let (negative, digits) = match text.split_first() {
        Some((b'-', digits)) => (true, digits),
        _ => (false, text),
    };
    if digits.is_empty() || digits.len() > 10 {
        return false;
    }
    let mut n: u64 = 0;
    for &digit in digits {
        if !digit.is_ascii_digit() {
            return false;
        }
        n = n * 10 + u64::from(digit - b'0');
    }
    if negative {
        n <= 2_147_483_648
    } else {
        n <= 2_147_483_647
    }
}

/// Returns the bit of the entry `index` of a bit set, or the defer of a set
/// wider than 64 entries.
#[inline]
fn bit(index: usize) -> R<u64> {
    let shift = u32::try_from(index).map_err(|_| defer(Reason::Wide))?;
    1u64.checked_shl(shift).ok_or(defer(Reason::Wide))
}

impl<'t, F: Feed, S: Scalars> Walker<'_, 't, F, S> {
    /// Returns the kind of the current token.
    #[inline]
    fn kind(&self) -> Kind {
        self.cur_kind
    }

    /// Moves to the next token and reads its kind.
    #[inline]
    fn adv(&mut self) -> R<()> {
        self.cur = self.f.advance().ok_or(SYNTAX)?;
        self.cur_kind = self.f.kind(self.cur).ok_or(INTERNAL)?;
        Ok(())
    }

    /// Returns the text of `tok`.
    #[inline]
    pub(super) fn text(&self, tok: Tok) -> R<&[u8]> {
        self.f.text(tok).ok_or(INTERNAL)
    }

    /// Takes the current token when it is of `kind`.
    #[inline]
    fn expect(&mut self, kind: Kind) -> R<Tok> {
        if self.kind() != kind {
            return Err(SYNTAX);
        }
        let tok = self.cur;
        self.adv()?;
        Ok(tok)
    }

    /// Takes the current token when it is of `kind`, and tells whether it did.
    #[inline]
    fn eat(&mut self, kind: Kind) -> R<bool> {
        if self.kind() != kind {
            return Ok(false);
        }
        self.adv()?;
        Ok(true)
    }

    /// Returns whether the current token is the name `keyword`.
    #[inline]
    fn is_keyword(&self, keyword: &[u8]) -> R<bool> {
        Ok(self.kind() == Kind::Name && self.text(self.cur)? == keyword)
    }

    /// Opens one level of nesting.
    #[inline]
    fn open(&mut self) -> R<()> {
        self.depth = self.depth.checked_add(1).ok_or(INTERNAL)?;
        if self.depth > self.o.max_depth {
            return Err(defer(Reason::Depth));
        }
        Ok(())
    }

    /// Closes one level of nesting.
    #[inline]
    fn close(&mut self) -> R<()> {
        self.depth = self.depth.checked_sub(1).ok_or(INTERNAL)?;
        Ok(())
    }

    /// Records the address of a local of a recursive step as the lowest the
    /// stack reached, when it lies below the lowest so far.
    #[cfg(feature = "pass-stack-mark")]
    pub(super) fn mark(&mut self) {
        let probe = 0u8;
        let at = core::hint::black_box(core::ptr::addr_of!(probe)).addr();
        if at < self.low {
            self.low = at;
        }
    }

    /// Records nothing, the walk without the stack mark.
    #[cfg(not(feature = "pass-stack-mark"))]
    #[inline]
    // The form without the stack mark keeps every call site of the form with
    // it, so the receiver stays.
    #[expect(
        clippy::unused_self,
        reason = "the call sites are those of the form with the stack mark"
    )]
    pub(super) fn mark(&mut self) {}

    /// Returns the type `index` of the table.
    #[inline]
    pub(super) fn ty(&self, index: u16) -> R<&'t TypeRow> {
        self.t.ty(index).ok_or(INTERNAL)
    }

    /// Returns whether the types `a` and `b` share a possible object type.
    #[inline]
    pub(super) fn overlap(&self, a: u16, b: u16) -> R<bool> {
        self.t.overlap(a, b).ok_or(INTERNAL)
    }

    /// Returns the selection `index` of the plan.
    #[inline]
    pub(super) fn sel(&self, index: u32) -> R<Sel> {
        self.b.sels.get(widen(index)?).copied().ok_or(INTERNAL)
    }

    /// Returns the argument `index` of the plan.
    #[inline]
    pub(super) fn arg(&self, index: u32) -> R<ArgUse> {
        self.b.args.get(widen(index)?).copied().ok_or(INTERNAL)
    }

    /// Returns the value `index` of the plan.
    #[inline]
    pub(super) fn val(&self, index: u32) -> R<Val> {
        self.b.vals.get(widen(index)?).copied().ok_or(INTERNAL)
    }

    /// Returns the fragment `index` of the document.
    #[inline]
    pub(super) fn frag(&self, index: u32) -> R<Frag> {
        self.b.frags.get(widen(index)?).copied().ok_or(INTERNAL)
    }

    /// Returns the spread `index` of the document.
    #[inline]
    pub(super) fn spread(&self, index: u32) -> R<SpreadRec> {
        self.b.spreads.get(widen(index)?).copied().ok_or(INTERNAL)
    }

    /// Returns the operation `index` of the document.
    #[inline]
    pub(super) fn op(&self, index: usize) -> R<Op> {
        self.b.ops.get(index).copied().ok_or(INTERNAL)
    }

    /// Returns the field a field reference names.
    #[inline]
    pub(super) fn field_of(&self, reference: u32) -> R<&'t FieldRow> {
        self.t.field(reference).ok_or(INTERNAL)
    }

    /// Returns whether two of the names from `base` on are equal, comparing
    /// pairs up to 8 names and sorting above that, and drops those names.
    pub(super) fn duplicate_names(&mut self, base: usize) -> R<bool> {
        let mut names = mem::take(&mut self.b.names);
        let duplicate = self.duplicates(names.get_mut(base..));
        names.truncate(base);
        self.b.names = names;
        duplicate
    }

    /// Returns whether two names of `list` are equal.
    fn duplicates(&self, list: Option<&mut [Tok]>) -> R<bool> {
        let list = list.ok_or(INTERNAL)?;
        if list.len() <= 8 {
            for (i, a) in list.iter().enumerate() {
                let a = self.text(*a)?;
                for b in list.iter().skip(i + 1) {
                    if self.text(*b)? == a {
                        return Ok(true);
                    }
                }
            }
            return Ok(false);
        }
        let feed = &*self.f;
        sort_by_bytes(list, &|tok| feed.text(tok).unwrap_or_default());
        for pair in list.windows(2) {
            if let [a, b] = pair
                && self.text(*a)? == self.text(*b)?
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    /// Returns a selection of the parent type `parent` with every other
    /// field empty.
    #[inline]
    fn blank(&self, parent: u16) -> Sel {
        Sel {
            kind: FIELD,
            parent,
            cond: NONE,
            end: 0,
            field: UNTYPED,
            key: self.sof,
            name: self.sof,
            args: 0,
            nargs: 0,
            skip: Cond::None,
            include: Cond::None,
        }
    }

    /// Appends `sel` to the plan and returns its index.
    #[inline]
    fn push_sel(&mut self, sel: Sel) -> usize {
        self.b.sels.push(sel);
        self.b.sels.len() - 1
    }

    /// Sets the end of the selection `index` past the last selection so far.
    #[inline]
    fn end_sel(&mut self, index: usize) -> R<()> {
        let end = narrow(self.b.sels.len())?;
        self.b.sels.get_mut(index).ok_or(INTERNAL)?.end = end;
        Ok(())
    }

    /// Sets the end of the value `index` past the last value so far.
    #[inline]
    fn end_val(&mut self, index: usize) -> R<()> {
        let end = narrow(self.b.vals.len())?;
        self.b.vals.get_mut(index).ok_or(INTERNAL)?.end = end;
        Ok(())
    }

    /// Walks the whole document, `many(SOF, definition, EOF)` of the parser,
    /// and runs the checks after its last token.
    fn document(&mut self) -> R<()> {
        self.adv()?;
        loop {
            self.definition()?;
            if self.kind() == Kind::Eof {
                break;
            }
        }
        self.after()
    }

    /// Walks one definition.
    fn definition(&mut self) -> R<()> {
        match self.kind() {
            Kind::BraceL => self.operation(op::QUERY, true),
            Kind::String | Kind::BlockString => {
                // A description, which stands before an operation or a
                // fragment of an executable document.
                self.adv()?;
                if self.kind() != Kind::Name {
                    return Err(SYNTAX);
                }
                self.keyword_definition()
            }
            Kind::Name => self.keyword_definition(),
            _ => Err(SYNTAX),
        }
    }

    /// Walks a definition that opens with a keyword.
    fn keyword_definition(&mut self) -> R<()> {
        match self.text(self.cur)? {
            b"query" => self.operation(op::QUERY, false),
            b"mutation" => self.operation(op::MUTATION, false),
            // The rule for single field subscriptions checks an operation on a
            // schema with a subscription root type alone, and the pass leaves
            // such an operation to it.
            b"subscription" if self.t.subscription != NONE => Err(defer(Reason::Subscription)),
            b"subscription" => self.operation(op::SUBSCRIPTION, false),
            b"fragment" => self.fragment(),
            // A definition of a type system, which the parser takes and the
            // rule for executable definitions refuses.
            _ => Err(refuse(Rule::ExecutableDefinitions)),
        }
    }

    /// Walks an operation of `kind`, a shorthand query when `shorthand`.
    fn operation(&mut self, kind: u8, shorthand: bool) -> R<()> {
        let s0 = narrow(self.b.sels.len())?;
        let v0 = self.b.vars.len();
        let u0 = narrow(self.b.uses.len())?;
        let p0 = narrow(self.b.spreads.len())?;
        let root = match kind {
            op::QUERY => self.t.query,
            op::MUTATION => self.t.mutation,
            _ => self.t.subscription,
        };
        let mut name = self.sof;
        let mut named = false;
        self.local = 0;
        self.fdl = 0;
        if !shorthand {
            self.adv()?;
            if self.kind() == Kind::Name {
                name = self.cur;
                named = true;
                self.adv()?;
            }
            if self.kind() == Kind::ParenL {
                self.variable_definitions(v0)?;
            }
            let location = match kind {
                op::QUERY => loc::QUERY,
                op::MUTATION => loc::MUTATION,
                _ => loc::SUBSCRIPTION,
            };
            self.directives(location)?;
        }
        self.selection_set(root, 0, NOINTRO, 1)?;
        let op = Op {
            kind,
            name,
            named,
            root,
            sels: (s0, narrow(self.b.sels.len())?),
            vars: (narrow(v0)?, narrow(self.b.vars.len())?),
            uses: (u0, narrow(self.b.uses.len())?),
            spreads: (p0, narrow(self.b.spreads.len())?),
        };
        self.b.ops.push(op);
        Ok(())
    }

    /// Walks a fragment definition.
    fn fragment(&mut self) -> R<()> {
        self.adv()?;
        if self.kind() != Kind::Name || self.text(self.cur)? == b"on" {
            return Err(SYNTAX);
        }
        let name = self.cur;
        self.adv()?;
        if !self.is_keyword(b"on")? {
            return Err(SYNTAX);
        }
        self.adv()?;
        let condition = self.expect(Kind::Name)?;
        let cond = self.type_condition(condition)?;
        // The variables the directives of the fragment use are variables of
        // the fragment, as the rules collect the variables of its whole node.
        let s0 = narrow(self.b.sels.len())?;
        let u0 = narrow(self.b.uses.len())?;
        let p0 = narrow(self.b.spreads.len())?;
        self.directives(loc::FRAGMENT_DEFINITION)?;
        self.local = 0;
        self.fdl = 0;
        self.selection_set(cond, 0, NOINTRO, 1)?;
        let frag = Frag {
            name,
            cond,
            sels: (s0, narrow(self.b.sels.len())?),
            uses: (u0, narrow(self.b.uses.len())?),
            spreads: (p0, narrow(self.b.spreads.len())?),
            local: self.local,
            max: 0,
            fdl: self.fdl,
            fdm: 0,
        };
        self.b.frags.push(frag);
        Ok(())
    }

    /// Returns the type a type condition names, which is composite.
    fn type_condition(&self, condition: Tok) -> R<u16> {
        let cond = self
            .t
            .type_named(self.text(condition)?)
            .ok_or(refuse(Rule::KnownTypeNames))?;
        if !self.ty(cond)?.is_composite() {
            return Err(refuse(Rule::FragmentsOnCompositeTypes));
        }
        Ok(cond)
    }

    /// Walks the variable definitions of an operation whose first variable
    /// is the variable `v0` of the plan.
    // Out of line, so an operation with no variable compiles no walk of their definitions.
    #[inline(never)]
    fn variable_definitions(&mut self, v0: usize) -> R<()> {
        self.adv()?;
        loop {
            self.variable_definition()?;
            if self.eat(Kind::ParenR)? {
                break;
            }
        }
        let base = self.b.names.len();
        for index in v0..self.b.vars.len() {
            let name = self.b.vars.get(index).ok_or(INTERNAL)?.name;
            self.b.names.push(name);
        }
        if self.duplicate_names(base)? {
            return Err(refuse(Rule::UniqueVariableNames));
        }
        Ok(())
    }

    /// Walks one variable definition.
    fn variable_definition(&mut self) -> R<()> {
        // A description before the variable is checked as a value of the
        // type of the variable.
        let description = self.cur;
        let block = match self.kind() {
            Kind::String => Some(false),
            Kind::BlockString => Some(true),
            _ => None,
        };
        if block.is_some() {
            self.adv()?;
        }
        self.expect(Kind::Dollar)?;
        let name = self.expect(Kind::Name)?;
        self.expect(Kind::Colon)?;
        let m0 = self.b.mods.len();
        let type_name = self.type_ref()?;
        let m1 = self.b.mods.len();
        let named = self
            .t
            .type_named(self.text(type_name)?)
            .ok_or(refuse(Rule::KnownTypeNames))?;
        if !self.ty(named)?.is_input() {
            return Err(refuse(Rule::VariablesAreInputTypes));
        }
        if let Some(block) = block {
            let kind = if block { v::BLOCK } else { v::STRING };
            self.literal(Pos { named, mods: &[] }, kind, description)?;
        }
        let default = if self.eat(Kind::Equals)? {
            self.default_value(named, m0, m1)?
        } else {
            NO_VALUE
        };
        if self.kind() == Kind::At {
            self.directives(loc::VARIABLE_DEFINITION)?;
        }
        let nmods = u16::try_from(m1.checked_sub(m0).ok_or(INTERNAL)?).map_err(|_| INTERNAL)?;
        let var = VarDef {
            name,
            named,
            mods: narrow(m0)?,
            nmods,
            default,
        };
        self.b.vars.push(var);
        Ok(())
    }

    /// Walks the default value of a variable of the named type `named`,
    /// whose wrappers stand from `m0` to `m1` of the wrapper buffer, and
    /// returns its index.
    fn default_value(&mut self, named: u16, m0: usize, m1: usize) -> R<u32> {
        let at = narrow(self.b.vals.len())?;
        let mods = mem::take(&mut self.b.mods);
        let walked = match mods.get(m0..m1) {
            Some(wrappers) => self.value(
                Some(Pos {
                    named,
                    mods: wrappers,
                }),
                false,
                true,
            ),
            None => Err(INTERNAL),
        };
        self.b.mods = mods;
        walked.map(|()| at)
    }

    /// Walks a type reference, appends its wrappers outermost first and
    /// returns the token of its named type.
    fn type_ref(&mut self) -> R<Tok> {
        self.mark();
        if !self.eat(Kind::BracketL)? {
            let named = self.expect(Kind::Name)?;
            if self.eat(Kind::Bang)? {
                self.b.mods.push(b'!');
            }
            return Ok(named);
        }
        self.open()?;
        let at = self.b.mods.len();
        self.b.mods.push(b'[');
        let named = self.type_ref()?;
        self.expect(Kind::BracketR)?;
        if self.eat(Kind::Bang)? {
            if at > self.b.mods.len() {
                return Err(INTERNAL);
            }
            self.b.mods.insert(at, b'!');
        }
        self.close()?;
        Ok(named)
    }

    /// Walks the directives at `location` and returns `@skip` and `@include`.
    ///
    /// Most places carry no directive, so the walk of the directives is a
    /// step of its own that this one enters only at an `@`.
    #[inline]
    fn directives(&mut self, location: u16) -> R<(Cond, Cond)> {
        if self.cur_kind != Kind::At {
            return Ok((Cond::None, Cond::None));
        }
        self.directives_at(location)
    }

    /// Walks the directives at `location` from the `@` of the first one and
    /// returns `@skip` and `@include`.
    ///
    /// The step stays out of line, so the test of `directives` inlines at
    /// every place that may carry directives.
    #[inline(never)]
    fn directives_at(&mut self, location: u16) -> R<(Cond, Cond)> {
        let mut seen: u64 = 0;
        let mut skip = Cond::None;
        let mut include = Cond::None;
        while self.kind() == Kind::At {
            self.adv()?;
            let name = self.expect(Kind::Name)?;
            let index = self
                .t
                .directive_named(self.text(name)?)
                .ok_or(refuse(Rule::KnownDirectives))?;
            let directive = *self.t.directive(index).ok_or(INTERNAL)?;
            if directive.locations & location == 0 {
                return Err(refuse(Rule::KnownDirectives));
            }
            if location == loc::VARIABLE_DEFINITION {
                return Err(defer(Reason::VariableDirective));
            }
            if !directive.repeatable {
                let flag = bit(usize::from(index))?;
                if seen & flag != 0 {
                    return Err(refuse(Rule::UniqueDirectivesPerLocation));
                }
                seen |= flag;
            }
            let a0 = self.b.args.len();
            let count = self.arguments(Some(directive.args))?;
            if index == self.t.include {
                include = self.condition(a0, count)?;
            } else if index == self.t.skip {
                skip = self.condition(a0, count)?;
            }
        }
        Ok((skip, include))
    }

    /// Returns the condition of an `@include` or `@skip` whose `count`
    /// arguments start at the argument `a0` of the plan.
    fn condition(&self, a0: usize, count: usize) -> R<Cond> {
        let end = a0.checked_add(count).ok_or(INTERNAL)?;
        let mut cond = Cond::None;
        for arg in self.b.args.get(a0..end).ok_or(INTERNAL)? {
            let val = self.val(arg.val)?;
            cond = match val.kind {
                v::TRUE => Cond::Lit(true),
                v::FALSE => Cond::Lit(false),
                _ => Cond::Var(val.tok),
            };
        }
        Ok(cond)
    }

    /// Walks the arguments of a field or a directive with the definitions
    /// `defs`, `None` where there is no type, and returns their count.
    ///
    /// A field with no argument definitions and no `(` holds no argument and
    /// misses none, so the walk of the arguments is a step of its own that
    /// this one enters only where there is an argument or a definition.
    #[inline]
    fn arguments(&mut self, defs: Option<Span>) -> R<usize> {
        if self.cur_kind != Kind::ParenL && defs.is_none_or(|span| span.len == 0) {
            return Ok(0);
        }
        self.arguments_at(defs)
    }

    /// Walks the arguments of a field or a directive with the definitions
    /// `defs`, `None` where there is no type, from the token after its name,
    /// and returns their count.
    ///
    /// The step stays out of line, so the test of `arguments` inlines at
    /// every field and directive.
    #[inline(never)]
    fn arguments_at(&mut self, defs: Option<Span>) -> R<usize> {
        let a0 = self.b.args.len();
        let defs = match defs {
            Some(span) => Some(self.t.args(span).ok_or(INTERNAL)?),
            None => None,
        };
        if defs.is_some_and(|defs| defs.len() > WIDE) {
            return Err(defer(Reason::Wide));
        }
        let mut seen: u64 = 0;
        if self.eat(Kind::ParenL)? {
            loop {
                let name = self.expect(Kind::Name)?;
                self.expect(Kind::Colon)?;
                let val = narrow(self.b.vals.len())?;
                let arg = if let Some(defs) = defs {
                    let names = (Rule::KnownArgumentNames, Rule::UniqueArgumentNames);
                    let (index, def) = self.find_def(defs, name, &mut seen, names)?;
                    let mods = self.t.mods(def.ty.mods).ok_or(INTERNAL)?;
                    let pos = Pos {
                        named: def.ty.named,
                        mods,
                    };
                    self.value(Some(pos), def.default.is_some(), false)?;
                    u16::try_from(index).map_err(|_| INTERNAL)?
                } else {
                    self.value(None, false, false)?;
                    NONE
                };
                self.b.args.push(ArgUse { name, arg, val });
                if self.eat(Kind::ParenR)? {
                    break;
                }
            }
        }
        if let Some(defs) = defs {
            self.required(defs, seen, Rule::ProvidedRequiredArguments)?;
        } else {
            // Without a type the names are compared, sorted above 8.
            let base = self.b.names.len();
            for index in a0..self.b.args.len() {
                let name = self.b.args.get(index).ok_or(INTERNAL)?.name;
                self.b.names.push(name);
            }
            if self.duplicate_names(base)? {
                return Err(refuse(Rule::UniqueArgumentNames));
            }
        }
        self.b.args.len().checked_sub(a0).ok_or(INTERNAL)
    }

    /// Returns the definition among `defs` that `name` names, with its
    /// index, and marks it in `seen`.
    ///
    /// `rules` names the rule a name no definition carries breaks and the
    /// rule a name seen before breaks.
    fn find_def(
        &self,
        defs: &[ArgRow],
        name: Tok,
        seen: &mut u64,
        rules: (Rule, Rule),
    ) -> R<(usize, ArgRow)> {
        let (unknown, duplicate) = rules;
        let text = self.text(name)?;
        let index = defs
            .iter()
            .position(|def| self.t.bytes(def.name) == Some(text))
            .ok_or(refuse(unknown))?;
        let flag = bit(index)?;
        if *seen & flag != 0 {
            return Err(refuse(duplicate));
        }
        *seen |= flag;
        let def = *defs.get(index).ok_or(INTERNAL)?;
        Ok((index, def))
    }

    /// Refuses with `rule` when a required definition of `defs` is missing
    /// from `seen`.
    fn required(&self, defs: &[ArgRow], seen: u64, rule: Rule) -> R<()> {
        for (index, def) in defs.iter().enumerate() {
            if def.required(self.t) && seen & bit(index)? == 0 {
                return Err(refuse(rule));
            }
        }
        Ok(())
    }

    /// Walks a value at a position of type `ty`, `None` where there is no
    /// type, checks it as the rule for values of the correct type does and
    /// records every variable it uses. `has_default` tells whether the
    /// position has a default, and `konst` whether the value is constant.
    fn value(&mut self, ty: Option<Pos<'_>>, has_default: bool, konst: bool) -> R<()> {
        self.mark();
        match self.kind() {
            Kind::Dollar => self.variable(ty, has_default, konst),
            Kind::BracketL => self.list(ty, konst),
            Kind::BraceL => self.object(ty, konst),
            _ => self.scalar_value(ty),
        }
    }

    /// Walks a variable at a position of type `ty`.
    fn variable(&mut self, ty: Option<Pos<'_>>, has_default: bool, konst: bool) -> R<()> {
        if konst {
            return Err(SYNTAX);
        }
        let at = self.b.vals.len();
        self.adv()?;
        let name = self.expect(Kind::Name)?;
        let end = narrow(at.checked_add(1).ok_or(INTERNAL)?)?;
        self.b.vals.push(Val {
            kind: v::VAR,
            tok: name,
            end,
        });
        let (named, mods, nmods) = match ty {
            Some(pos) => {
                let m0 = narrow(self.b.use_mods.len())?;
                self.b.use_mods.extend_from_slice(pos.mods);
                let nmods = u16::try_from(pos.mods.len()).map_err(|_| INTERNAL)?;
                (pos.named, m0, nmods)
            }
            None => (NONE, 0, 0),
        };
        self.b.uses.push(Use {
            name,
            named,
            mods,
            nmods,
            has_default,
        });
        Ok(())
    }

    /// Returns the stop of a list or an object literal at a position of the
    /// named type `named`, which takes neither: a defer at a custom scalar,
    /// whose literal parse may take it, and a refusal at any other type.
    fn misfit(&self, named: u16) -> R<Stop> {
        Ok(match self.ty(named)?.scalar {
            Scalar::Custom(_) => defer(Reason::Scalar),
            _ => refuse(Rule::ValuesOfCorrectType),
        })
    }

    /// Walks a list literal at a position of type `ty`.
    fn list(&mut self, ty: Option<Pos<'_>>, konst: bool) -> R<()> {
        let at = self.b.vals.len();
        let tok = self.cur;
        self.adv()?;
        self.open()?;
        self.b.vals.push(Val {
            kind: v::LIST,
            tok,
            end: 0,
        });
        let item = match ty {
            Some(pos) => match nullable(pos.mods).split_first() {
                Some((b'[', inner)) => Some(Pos {
                    named: pos.named,
                    mods: inner,
                }),
                _ => return Err(self.misfit(pos.named)?),
            },
            None => None,
        };
        while !self.eat(Kind::BracketR)? {
            self.value(item, false, konst)?;
        }
        self.end_val(at)?;
        self.close()
    }

    /// Returns the input fields of the input object type `named`, or the stop
    /// of an object literal that the type does not take or the pass does not
    /// decide.
    fn input_fields(&self, named: u16) -> R<&'t [ArgRow]> {
        let row = self.ty(named)?;
        if row.kind != TypeKind::InputObject {
            return Err(self.misfit(named)?);
        }
        if row.one_of {
            return Err(defer(Reason::OneOf));
        }
        let inputs = self.t.args(row.inputs).ok_or(INTERNAL)?;
        if inputs.len() > WIDE {
            return Err(defer(Reason::Wide));
        }
        Ok(inputs)
    }

    /// Walks an object literal at a position of type `ty`.
    fn object(&mut self, ty: Option<Pos<'_>>, konst: bool) -> R<()> {
        let at = self.b.vals.len();
        let tok = self.cur;
        self.adv()?;
        self.open()?;
        self.b.vals.push(Val {
            kind: v::OBJECT,
            tok,
            end: 0,
        });
        let inputs = match ty {
            Some(pos) => Some(self.input_fields(pos.named)?),
            None => None,
        };
        let n0 = self.b.names.len();
        let mut seen: u64 = 0;
        while !self.eat(Kind::BraceR)? {
            let name = self.expect(Kind::Name)?;
            self.expect(Kind::Colon)?;
            let field_at = self.b.vals.len();
            self.b.vals.push(Val {
                kind: v::FIELD,
                tok: name,
                end: 0,
            });
            if let Some(inputs) = inputs {
                let names = (Rule::ValuesOfCorrectType, Rule::UniqueInputFieldNames);
                let (_, def) = self.find_def(inputs, name, &mut seen, names)?;
                let mods = self.t.mods(def.ty.mods).ok_or(INTERNAL)?;
                let pos = Pos {
                    named: def.ty.named,
                    mods,
                };
                self.value(Some(pos), def.default.is_some(), konst)?;
            } else {
                // Without a type the names are compared after the object,
                // sorted above 8.
                self.b.names.push(name);
                self.value(None, false, konst)?;
            }
            self.end_val(field_at)?;
        }
        if self.duplicate_names(n0)? {
            return Err(refuse(Rule::UniqueInputFieldNames));
        }
        if let Some(inputs) = inputs {
            self.required(inputs, seen, Rule::ValuesOfCorrectType)?;
        }
        self.end_val(at)?;
        self.close()
    }

    /// Walks a literal of a scalar kind or an enum value at a position of
    /// type `ty`.
    fn scalar_value(&mut self, ty: Option<Pos<'_>>) -> R<()> {
        let tok = self.cur;
        let kind = match self.kind() {
            Kind::Int => v::INT,
            Kind::Float => v::FLOAT,
            Kind::String => v::STRING,
            Kind::BlockString => v::BLOCK,
            Kind::Name => match self.text(tok)? {
                b"true" => v::TRUE,
                b"false" => v::FALSE,
                b"null" => v::NULL,
                _ => v::ENUM,
            },
            _ => return Err(SYNTAX),
        };
        let end = narrow(self.b.vals.len().checked_add(1).ok_or(INTERNAL)?)?;
        self.b.vals.push(Val { kind, tok, end });
        if let Some(pos) = ty {
            self.literal(pos, kind, tok)?;
        }
        self.adv()
    }

    /// Checks the literal `tok` of the value kind `kind` at the position
    /// `pos`, as the rule for values of the correct type checks it.
    fn literal(&self, pos: Pos<'_>, kind: u8, tok: Tok) -> R<()> {
        if kind == v::NULL {
            if pos.mods.first() == Some(&b'!') {
                return Err(refuse(Rule::ValuesOfCorrectType));
            }
            return Ok(());
        }
        let row = self.ty(pos.named)?;
        let text = self.text(tok)?;
        if let Scalar::Custom(scalar) = row.scalar {
            let literal = match kind {
                v::INT => Literal::Int(text),
                v::FLOAT => Literal::Float(text),
                v::STRING | v::BLOCK => Literal::String {
                    value: text,
                    block: kind == v::BLOCK,
                },
                v::TRUE | v::FALSE => Literal::Boolean(kind == v::TRUE),
                _ => Literal::Enum(text),
            };
            return match self.sc.literal(scalar, literal) {
                Answer::Accept => Ok(()),
                Answer::Refuse => Err(refuse(Rule::ValuesOfCorrectType)),
                Answer::Defer => Err(defer(Reason::Scalar)),
            };
        }
        let fits = match kind {
            v::INT => match row.scalar {
                Scalar::Int => int_fits(text),
                Scalar::Float | Scalar::Id => true,
                _ => false,
            },
            v::FLOAT => row.scalar == Scalar::Float,
            v::STRING | v::BLOCK => matches!(row.scalar, Scalar::String | Scalar::Id),
            v::TRUE | v::FALSE => row.scalar == Scalar::Boolean,
            _ => row.kind == TypeKind::Enum && self.is_enum_value(row, text)?,
        };
        if fits {
            Ok(())
        } else {
            Err(refuse(Rule::ValuesOfCorrectType))
        }
    }

    /// Returns whether `text` names a value of the enum type `row`.
    fn is_enum_value(&self, row: &TypeRow, text: &[u8]) -> R<bool> {
        let values = self.t.values(row.values).ok_or(INTERNAL)?;
        Ok(values
            .iter()
            .any(|value| self.t.bytes(*value) == Some(text)))
    }

    /// Walks a selection set whose fields lie `fd` deep, below a parent of
    /// the type `parent` with `lc` introspection list fields above it, of
    /// which the nearest `__schema` or `__type` field stands below `intro`.
    fn selection_set(&mut self, parent: u16, lc: u32, intro: u32, fd: u32) -> R<()> {
        self.mark();
        self.expect(Kind::BraceL)?;
        self.open()?;
        loop {
            if self.kind() == Kind::Spread {
                self.fragment_selection(parent, lc, intro, fd)?;
            } else {
                self.field(parent, lc, intro, fd)?;
            }
            if self.eat(Kind::BraceR)? {
                return self.close();
            }
        }
    }

    /// Walks an inline fragment or a fragment spread.
    fn fragment_selection(&mut self, parent: u16, lc: u32, intro: u32, fd: u32) -> R<()> {
        self.adv()?;
        if self.is_keyword(b"on")? {
            self.adv()?;
            let condition = self.expect(Kind::Name)?;
            let cond = self.type_condition(condition)?;
            if parent != NONE && !self.overlap(parent, cond)? {
                return Err(refuse(Rule::PossibleFragmentSpreads));
            }
            let (skip, include) = self.directives(loc::INLINE_FRAGMENT)?;
            let sel = Sel {
                kind: INLINE,
                cond,
                skip,
                include,
                ..self.blank(parent)
            };
            let index = self.push_sel(sel);
            self.selection_set(cond, lc, intro, fd)?;
            return self.end_sel(index);
        }
        if self.kind() == Kind::Name {
            let name = self.cur;
            self.adv()?;
            let (skip, include) = self.directives(loc::FRAGMENT_SPREAD)?;
            let sel = Sel {
                kind: SPREAD,
                key: name,
                skip,
                include,
                ..self.blank(parent)
            };
            let index = self.push_sel(sel);
            self.end_sel(index)?;
            let intro = if intro == NOINTRO {
                NOINTRO
            } else {
                lc.checked_sub(intro).ok_or(INTERNAL)?
            };
            self.b.spreads.push(SpreadRec {
                name,
                sel: narrow(index)?,
                frag: u32::MAX,
                parent,
                lc,
                intro,
                fd,
            });
            return Ok(());
        }
        let (skip, include) = self.directives(loc::INLINE_FRAGMENT)?;
        let sel = Sel {
            kind: INLINE,
            skip,
            include,
            ..self.blank(parent)
        };
        let index = self.push_sel(sel);
        self.selection_set(parent, lc, intro, fd)?;
        self.end_sel(index)
    }

    /// Returns the reference and the definition of the field named `text`
    /// on the type `parent`, the untyped reference where there is no type.
    fn field_def(&self, parent: u16, text: &[u8]) -> R<(u32, Option<FieldRow>)> {
        if parent == NONE {
            return Ok((UNTYPED, None));
        }
        let reference = if text == b"__typename" {
            fref(META, META_TYPENAME)
        } else if text == b"__schema" && parent == self.t.query {
            fref(META, META_SCHEMA)
        } else if text == b"__type" && parent == self.t.query {
            fref(META, META_TYPE)
        } else {
            let index = self
                .t
                .field_named(parent, text)
                .ok_or(refuse(Rule::FieldsOnCorrectType))?;
            fref(parent, index)
        };
        let def = *self.field_of(reference)?;
        if self.o.no_schema_introspection && self.ty(def.ty.named)?.introspection {
            return Err(refuse(Rule::NoSchemaIntrospectionCustom));
        }
        let meta = reference == fref(META, META_SCHEMA) || reference == fref(META, META_TYPE);
        if meta && self.t.meta_corner {
            return Err(defer(Reason::MetaCorner));
        }
        Ok((reference, Some(def)))
    }

    /// Walks a field below a parent of the type `parent`.
    fn field(&mut self, parent: u16, lc: u32, intro: u32, fd: u32) -> R<()> {
        let first = self.expect(Kind::Name)?;
        let name = if self.eat(Kind::Colon)? {
            self.expect(Kind::Name)?
        } else {
            first
        };
        let text = self.text(name)?;
        let list = is_list_name(text);
        let intro_field = text == b"__schema" || text == b"__type";
        let (field, def) = self.field_def(parent, text)?;

        // The rule for the introspection depth counts fields by name.
        let (mut lc2, mut intro2) = (lc, intro);
        if list {
            lc2 = lc.checked_add(1).ok_or(INTERNAL)?;
            if intro != NOINTRO && lc2.checked_sub(intro).ok_or(INTERNAL)? >= 3 {
                return Err(refuse(Rule::MaxIntrospectionDepth));
            }
            self.local = self.local.max(lc2);
        } else if intro == NOINTRO && intro_field {
            intro2 = lc;
        }
        self.fdl = self.fdl.max(fd);

        let sel = Sel {
            kind: FIELD,
            field,
            key: first,
            name,
            ..self.blank(parent)
        };
        let index = self.push_sel(sel);
        let a0 = narrow(self.b.args.len())?;
        let count = self.arguments(def.map(|def| def.args))?;
        let (skip, include) = self.directives(loc::FIELD)?;
        let sel = self.b.sels.get_mut(index).ok_or(INTERNAL)?;
        sel.args = a0;
        sel.nargs = u16::try_from(count).map_err(|_| INTERNAL)?;
        sel.skip = skip;
        sel.include = include;
        if self.kind() == Kind::BraceL {
            let sub = match def {
                Some(def) if def.leaf => return Err(refuse(Rule::ScalarLeafs)),
                Some(def) => def.ty.named,
                None => NONE,
            };
            let below = fd.checked_add(1).ok_or(INTERNAL)?;
            self.selection_set(sub, lc2, intro2, below)?;
        } else if def.is_some_and(|def| !def.leaf) {
            return Err(refuse(Rule::ScalarLeafs));
        }
        self.end_sel(index)
    }
}

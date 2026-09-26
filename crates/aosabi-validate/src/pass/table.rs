// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass table: the field table of one schema, which the single
 * pass reads in place of the schema.
 *
 * No analogue in the reference.
 */

//! The field table of the single pass.
//!
//! A [`Table`] is a set of slices of plain rows, so it has two forms with one
//! shape. A server builds the rows at its first validation of a schema from
//! the schema of the type layer, through
//! [`OwnedTable::from_schema`](super::from_schema::OwnedTable::from_schema),
//! and views them as a table while that schema lives. A device gets the same
//! rows as statics, which the primitive `aosabi-devtool pass --table` writes
//! for one schema. Every row names the rows it refers to by index and every
//! name by a [`Span`] of one arena of bytes, so neither form holds a reference
//! into itself.
//!
//! A name is found through a [`Lookup`]: the static form asks a function
//! written for its schema, a `match` over byte strings, and the boot form
//! bisects entries sorted by name. [`Table::dump`] prints the rows in one
//! text that both forms of a schema print byte for byte alike, which the
//! notebook of the pass holds.

use core::{
    cmp::Ordering,
    fmt,
    ops::Range, //
};

/// No type, no operation root and no directive.
pub const NONE: u16 = u16::MAX;

/// The type index of the three meta fields in a field reference.
pub const META: u16 = u16::MAX - 1;
/// `__typename`, the meta field index.
pub const META_TYPENAME: u16 = 0;
/// `__schema`, the meta field index.
pub const META_SCHEMA: u16 = 1;
/// `__type`, the meta field index.
pub const META_TYPE: u16 = 2;

/// The executable directive locations, as bits.
pub mod loc {
    /// `QUERY`.
    pub const QUERY: u16 = 1;
    /// `MUTATION`.
    pub const MUTATION: u16 = 2;
    /// `SUBSCRIPTION`.
    pub const SUBSCRIPTION: u16 = 4;
    /// `FIELD`.
    pub const FIELD: u16 = 8;
    /// `FRAGMENT_DEFINITION`.
    pub const FRAGMENT_DEFINITION: u16 = 16;
    /// `FRAGMENT_SPREAD`.
    pub const FRAGMENT_SPREAD: u16 = 32;
    /// `INLINE_FRAGMENT`.
    pub const INLINE_FRAGMENT: u16 = 64;
    /// `VARIABLE_DEFINITION`.
    pub const VARIABLE_DEFINITION: u16 = 128;

    /// The names of the bits, in the order of their values.
    pub const NAMES: [(u16, &str); 8] = [
        (QUERY, "QUERY"),
        (MUTATION, "MUTATION"),
        (SUBSCRIPTION, "SUBSCRIPTION"),
        (FIELD, "FIELD"),
        (FRAGMENT_DEFINITION, "FRAGMENT_DEFINITION"),
        (FRAGMENT_SPREAD, "FRAGMENT_SPREAD"),
        (INLINE_FRAGMENT, "INLINE_FRAGMENT"),
        (VARIABLE_DEFINITION, "VARIABLE_DEFINITION"),
    ];
}

/// A run of entries of one array of the table, its first index and its length.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Span {
    /// The index of the first entry.
    pub at: u32,
    /// The count of entries.
    pub len: u32,
}

impl Span {
    /// The span of no entry.
    pub const EMPTY: Self = Self { at: 0, len: 0 };

    /// Returns the span of `len` entries from `at`.
    #[must_use]
    pub const fn new(at: u32, len: u32) -> Self {
        Self { at, len }
    }

    /// Returns the indices the span covers, or `None` when they pass `usize`.
    #[must_use]
    #[inline]
    pub fn range(self) -> Option<Range<usize>> {
        let at = usize::try_from(self.at).ok()?;
        let len = usize::try_from(self.len).ok()?;
        Some(at..at.checked_add(len)?)
    }
}

/// What kind of named type a row is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A scalar type.
    Scalar,
    /// An object type.
    Object,
    /// An interface type.
    Interface,
    /// A union type.
    Union,
    /// An enum type.
    Enum,
    /// An input object type.
    InputObject,
}

impl Kind {
    /// Returns the word [`Table::dump`] prints for the kind.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Scalar => "scalar",
            Self::Object => "object",
            Self::Interface => "interface",
            Self::Union => "union",
            Self::Enum => "enum",
            Self::InputObject => "input",
        }
    }
}

/// Which literals a scalar type takes.
///
/// A scalar is one of the five specified scalars only when it is the very type
/// the type layer hands out for that name. Every other scalar, a scalar with a
/// binding laid over a specified name among them, is [`Scalar::Custom`] and is
/// asked through its own literal parse (see [`super::Scalars`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scalar {
    /// Not a scalar.
    None,
    /// `Int`, a signed 32 bit integer.
    Int,
    /// `Float`, a double.
    Float,
    /// `String`.
    String,
    /// `Boolean`.
    Boolean,
    /// `ID`, read from a string or an integer literal.
    Id,
    /// A custom scalar, by its index among the custom scalars of the table.
    Custom(u16),
}

/// A type reference: the named type and its wrappers from the outside in.
///
/// The wrappers hold `b'!'` for non null and `b'['` for a list, so
/// `[Reading!]!` is the named type `Reading` with the wrappers `b"![!"`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeRef {
    /// The index of the named type.
    pub named: u16,
    /// The wrappers in the wrapper bytes of the table.
    pub mods: Span,
}

/// A named type of the schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TypeRow {
    /// The name in the name bytes.
    pub name: Span,
    /// The kind.
    pub kind: Kind,
    /// Which literals a scalar takes, [`Scalar::None`] for every other kind.
    pub scalar: Scalar,
    /// The fields of an object or an interface type.
    pub fields: Span,
    /// The first word of its possible object types in the possible words:
    /// the type itself for an object, the members of a union and the
    /// implementations of an interface.
    pub possible: u32,
    /// The values of an enum type, each a name in the name bytes.
    pub values: Span,
    /// The fields of an input object type in the arguments of the table.
    pub inputs: Span,
    /// Whether an input object type carries `@oneOf`.
    pub one_of: bool,
    /// Whether the type is one of the eight introspection types.
    pub introspection: bool,
}

impl TypeRow {
    /// Returns whether values of the type are leaves.
    #[must_use]
    pub const fn is_leaf(&self) -> bool {
        matches!(self.kind, Kind::Scalar | Kind::Enum)
    }

    /// Returns whether the type is an object, an interface or a union.
    #[must_use]
    pub const fn is_composite(&self) -> bool {
        matches!(self.kind, Kind::Object | Kind::Interface | Kind::Union)
    }

    /// Returns whether the type is a scalar, an enum or an input object.
    #[must_use]
    pub const fn is_input(&self) -> bool {
        matches!(self.kind, Kind::Scalar | Kind::Enum | Kind::InputObject)
    }
}

/// A field of an object or an interface type, or a meta field.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FieldRow {
    /// The name in the name bytes.
    pub name: Span,
    /// The pre encoded key of the answer, `"name":`, in the name bytes.
    pub key: Span,
    /// The return type.
    pub ty: TypeRef,
    /// The arguments in definition order, in the arguments of the table.
    pub args: Span,
    /// Whether the named return type is a leaf, a scalar or an enum.
    pub leaf: bool,
}

/// An argument of a field or a directive, or a field of an input object type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ArgRow {
    /// The name in the name bytes.
    pub name: Span,
    /// The type.
    pub ty: TypeRef,
    /// The default value as JSON in the name bytes, `None` where the
    /// definition has none.
    pub default: Option<Span>,
}

impl ArgRow {
    /// Returns whether a document must give the argument, which is a non null
    /// type and no default, given the wrapper bytes `mods` of `table`.
    #[must_use]
    pub fn required(&self, table: &Table<'_>) -> bool {
        self.default.is_none() && table.mods(self.ty.mods).and_then(<[u8]>::first) == Some(&b'!')
    }
}

/// A directive of the schema.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DirectiveRow {
    /// The name in the name bytes.
    pub name: Span,
    /// The executable locations, the bits of [`loc`].
    pub locations: u16,
    /// The arguments in the arguments of the table.
    pub args: Span,
    /// Whether it may stand more than once at one location.
    pub repeatable: bool,
}

/// A name and the index of the row it names, an entry of a sorted lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name in the name bytes.
    pub name: Span,
    /// The index of the row.
    pub index: u16,
}

/// How a table finds a type or a directive by its name.
#[derive(Clone, Copy, Debug)]
pub enum Lookup<'t> {
    /// A function written for one schema, a `match` over byte strings.
    Static(fn(&[u8]) -> Option<u16>),
    /// Entries sorted by the bytes of their names, found by bisection.
    Sorted(&'t [Entry]),
}

/// How a table finds a field of a type by its name.
#[derive(Clone, Copy, Debug)]
pub enum FieldLookup<'t> {
    /// A function written for one schema over the type index and the name.
    Static(fn(u16, &[u8]) -> Option<u16>),
    /// Entries sorted by name per type, found by bisection.
    Sorted {
        /// The entries of each type, by type index.
        types: &'t [Span],
        /// The entries, each naming a field by its index in its type.
        entries: &'t [Entry],
    },
}

/// The possible object types of one type, as a bit set over the type indices.
#[derive(Clone, Copy, Debug)]
pub struct Possible<'t>(&'t [u64]);

impl Possible<'_> {
    /// Returns whether the two sets share a type.
    #[must_use]
    #[inline]
    pub fn intersects(self, other: Possible<'_>) -> bool {
        self.0.iter().zip(other.0).any(|(a, b)| a & b != 0)
    }

    /// Returns whether the set holds the type `index`.
    #[must_use]
    #[inline]
    pub fn contains(self, index: u16) -> bool {
        let word = usize::from(index / 64);
        let bit = 1u64 << (index % 64);
        self.0.get(word).is_some_and(|w| w & bit != 0)
    }
}

/// The field table of one schema.
#[derive(Clone, Copy, Debug)]
pub struct Table<'t> {
    /// The bytes of every name, key and default.
    pub names: &'t [u8],
    /// Every named type, in the order of the type map of the schema.
    pub types: &'t [TypeRow],
    /// Every field of every object and interface type, the three meta fields
    /// among them.
    pub fields: &'t [FieldRow],
    /// Every argument of a field or a directive and every input field.
    pub args: &'t [ArgRow],
    /// Every value of every enum type.
    pub values: &'t [Span],
    /// The wrappers of every type reference.
    pub mods: &'t [u8],
    /// The possible types of every type, [`Table::words`] words each.
    pub possible: &'t [u64],
    /// The words of one set of possible types, the count of types over 64
    /// rounded up.
    pub words: u32,
    /// Every directive.
    pub directives: &'t [DirectiveRow],
    /// The index of the first of the three meta fields in the fields.
    pub meta: u32,
    /// The query root type, or [`NONE`].
    pub query: u16,
    /// The mutation root type, or [`NONE`].
    pub mutation: u16,
    /// The subscription root type, or [`NONE`].
    pub subscription: u16,
    /// The directive `@include`, or [`NONE`].
    pub include: u16,
    /// The directive `@skip`, or [`NONE`].
    pub skip: u16,
    /// Whether the query root type is a possible type of an interface or a
    /// union, or the named return type of a field, the schema where the pass
    /// defers a `__schema` or `__type` field.
    pub meta_corner: bool,
    /// The lookup of a type by name.
    pub type_named: Lookup<'t>,
    /// The lookup of a field by type and name.
    pub field_named: FieldLookup<'t>,
    /// The lookup of a directive by name.
    pub directive_named: Lookup<'t>,
}

impl<'t> Table<'t> {
    /// Returns the bytes `span` names in the name bytes.
    #[must_use]
    #[inline]
    pub fn bytes(&self, span: Span) -> Option<&'t [u8]> {
        self.names.get(span.range()?)
    }

    /// Returns the wrappers `span` names in the wrapper bytes.
    #[must_use]
    #[inline]
    pub fn mods(&self, span: Span) -> Option<&'t [u8]> {
        self.mods.get(span.range()?)
    }

    /// Returns the type `index`.
    #[must_use]
    #[inline]
    pub fn ty(&self, index: u16) -> Option<&'t TypeRow> {
        self.types.get(usize::from(index))
    }

    /// Returns the arguments `span` names.
    #[must_use]
    #[inline]
    pub fn args(&self, span: Span) -> Option<&'t [ArgRow]> {
        self.args.get(span.range()?)
    }

    /// Returns the enum values `span` names.
    #[must_use]
    #[inline]
    pub fn values(&self, span: Span) -> Option<&'t [Span]> {
        self.values.get(span.range()?)
    }

    /// Returns the directive `index`.
    #[must_use]
    #[inline]
    pub fn directive(&self, index: u16) -> Option<&'t DirectiveRow> {
        self.directives.get(usize::from(index))
    }

    /// Returns the field a field reference names, see [`fref`].
    #[must_use]
    #[inline]
    pub fn field(&self, reference: u32) -> Option<&'t FieldRow> {
        let ty = u16::try_from(reference >> 16).ok()?;
        let index = u32::from(u16::try_from(reference & 0xffff).ok()?);
        let at = if ty == META {
            if index > u32::from(META_TYPE) {
                return None;
            }
            self.meta.checked_add(index)?
        } else {
            let fields = self.ty(ty)?.fields;
            if index >= fields.len {
                return None;
            }
            fields.at.checked_add(index)?
        };
        self.fields.get(usize::try_from(at).ok()?)
    }

    /// Returns the possible object types of the type `index`.
    #[must_use]
    #[inline]
    pub fn possible(&self, index: u16) -> Option<Possible<'t>> {
        let at = usize::try_from(self.ty(index)?.possible).ok()?;
        let words = usize::try_from(self.words).ok()?;
        Some(Possible(self.possible.get(at..at.checked_add(words)?)?))
    }

    /// Returns whether the types `a` and `b` share a possible object type,
    /// the `doTypesOverlap` of the reference over composite types.
    #[must_use]
    #[inline]
    pub fn overlap(&self, a: u16, b: u16) -> Option<bool> {
        if a == b {
            return Some(true);
        }
        Some(self.possible(a)?.intersects(self.possible(b)?))
    }

    /// Returns the type named `name`.
    #[must_use]
    #[inline]
    pub fn type_named(&self, name: &[u8]) -> Option<u16> {
        self.lookup(self.type_named, name)
    }

    /// Returns the directive named `name`.
    #[must_use]
    #[inline]
    pub fn directive_named(&self, name: &[u8]) -> Option<u16> {
        self.lookup(self.directive_named, name)
    }

    /// Returns the index of the field `name` of the type `ty` among its fields.
    #[must_use]
    #[inline]
    pub fn field_named(&self, ty: u16, name: &[u8]) -> Option<u16> {
        match self.field_named {
            FieldLookup::Static(find) => find(ty, name),
            FieldLookup::Sorted { types, entries } => {
                let span = types.get(usize::from(ty))?;
                self.bisect(entries.get(span.range()?)?, name)
            }
        }
    }

    /// Returns the index `lookup` answers for `name`.
    #[inline]
    fn lookup(&self, lookup: Lookup<'_>, name: &[u8]) -> Option<u16> {
        match lookup {
            Lookup::Static(find) => find(name),
            Lookup::Sorted(entries) => self.bisect(entries, name),
        }
    }

    /// Returns the index of the entry named `name` among `entries`, which are
    /// sorted by the bytes of their names.
    fn bisect(&self, entries: &[Entry], name: &[u8]) -> Option<u16> {
        let found = entries.binary_search_by(|entry| match self.bytes(entry.name) {
            Some(bytes) => bytes.cmp(name),
            None => Ordering::Less,
        });
        entries.get(found.ok()?).map(|entry| entry.index)
    }

    /// Prints every row of the table as text into `out`.
    ///
    /// The text names every type, field, argument, input field, enum value
    /// and directive with its type, its default and its flags, and ends with
    /// the count of names the lookups of the table find at their own row and
    /// of those they miss. Both forms of one schema print it byte for byte
    /// alike, because the text reads the rows and asks the lookups and never
    /// prints how a lookup is made.
    ///
    /// # Errors
    ///
    /// Returns the error of `out`.
    pub fn dump(&self, out: &mut impl fmt::Write) -> fmt::Result {
        self.dump_head(out)?;
        for (index, row) in self.types.iter().enumerate() {
            self.dump_type(out, index, row)?;
        }
        for index in [META_TYPENAME, META_SCHEMA, META_TYPE] {
            writeln!(out, "meta {index}")?;
            match self.field(fref(META, index)) {
                Some(field) => self.dump_field(out, "  field", field)?,
                None => writeln!(out, "  missing")?,
            }
        }
        for (index, row) in self.directives.iter().enumerate() {
            write!(out, "directive {index} @")?;
            self.text(out, row.name)?;
            write!(out, " on")?;
            for (bit, name) in loc::NAMES {
                if row.locations & bit != 0 {
                    write!(out, " {name}")?;
                }
            }
            writeln!(out, "{}", if row.repeatable { " repeatable" } else { "" })?;
            self.dump_args(out, "  arg", row.args)?;
        }
        self.dump_lookups(out)
    }

    /// Prints the counts, the roots and the flags of the table.
    fn dump_head(&self, out: &mut impl fmt::Write) -> fmt::Result {
        writeln!(
            out,
            "table types {} fields {} args {} values {} directives {} words {}",
            self.types.len(),
            self.fields.len(),
            self.args.len(),
            self.values.len(),
            self.directives.len(),
            self.words
        )?;
        for (word, root) in [
            ("query", self.query),
            ("mutation", self.mutation),
            ("subscription", self.subscription),
        ] {
            write!(out, "root {word} ")?;
            self.type_name(out, root)?;
            writeln!(out)?;
        }
        for (word, directive) in [("include", self.include), ("skip", self.skip)] {
            write!(out, "directive {word} ")?;
            match self.directive(directive) {
                Some(row) => self.text(out, row.name)?,
                None => write!(out, "none")?,
            }
            writeln!(out)?;
        }
        writeln!(
            out,
            "meta corner {}",
            if self.meta_corner { "yes" } else { "no" }
        )
    }

    /// Prints the type `row` at `index` with its fields, values and inputs.
    fn dump_type(&self, out: &mut impl fmt::Write, index: usize, row: &TypeRow) -> fmt::Result {
        write!(out, "type {index} ")?;
        self.text(out, row.name)?;
        write!(out, " {}", row.kind.word())?;
        match row.scalar {
            Scalar::None => {}
            Scalar::Int => write!(out, " int")?,
            Scalar::Float => write!(out, " float")?,
            Scalar::String => write!(out, " string")?,
            Scalar::Boolean => write!(out, " boolean")?,
            Scalar::Id => write!(out, " id")?,
            Scalar::Custom(custom) => write!(out, " custom {custom}")?,
        }
        if row.one_of {
            write!(out, " one-of")?;
        }
        if row.introspection {
            write!(out, " introspection")?;
        }
        write!(out, " possible")?;
        let possible = u16::try_from(index).ok().and_then(|i| self.possible(i));
        match possible {
            Some(set) => {
                for member in 0..self.types.len() {
                    if u16::try_from(member).is_ok_and(|m| set.contains(m)) {
                        write!(out, " {member}")?;
                    }
                }
            }
            None => write!(out, " missing")?,
        }
        writeln!(out)?;
        match self.values(row.values) {
            Some(values) => {
                for value in values {
                    write!(out, "  value ")?;
                    self.text(out, *value)?;
                    writeln!(out)?;
                }
            }
            None => writeln!(out, "  values missing")?,
        }
        self.dump_args(out, "  input", row.inputs)?;
        match row.fields.range().and_then(|range| self.fields.get(range)) {
            Some(fields) => {
                for field in fields {
                    self.dump_field(out, "  field", field)?;
                }
            }
            None => writeln!(out, "  fields missing")?,
        }
        Ok(())
    }

    /// Prints one field with its arguments, behind the label `label`.
    fn dump_field(&self, out: &mut impl fmt::Write, label: &str, field: &FieldRow) -> fmt::Result {
        write!(out, "{label} ")?;
        self.text(out, field.name)?;
        write!(out, " key ")?;
        self.text(out, field.key)?;
        write!(out, " type ")?;
        self.type_ref(out, field.ty)?;
        writeln!(out, "{}", if field.leaf { " leaf" } else { "" })?;
        self.dump_args(out, "    arg", field.args)
    }

    /// Prints the arguments `span` names, one line each behind `label`.
    fn dump_args(&self, out: &mut impl fmt::Write, label: &str, span: Span) -> fmt::Result {
        let Some(args) = self.args(span) else {
            return writeln!(out, "{label} missing");
        };
        for arg in args {
            write!(out, "{label} ")?;
            self.text(out, arg.name)?;
            write!(out, " type ")?;
            self.type_ref(out, arg.ty)?;
            if let Some(default) = arg.default {
                write!(out, " default ")?;
                self.text(out, default)?;
            }
            writeln!(out)?;
        }
        Ok(())
    }

    /// Prints the counts of names the lookups find at their own row and miss.
    fn dump_lookups(&self, out: &mut impl fmt::Write) -> fmt::Result {
        let (mut found, mut missed) = (0usize, 0usize);
        let mut count = |hit: bool| {
            if hit {
                found += 1;
            } else {
                missed += 1;
            }
        };
        for (index, row) in self.types.iter().enumerate() {
            let named = self.bytes(row.name).and_then(|name| self.type_named(name));
            count(named.map(usize::from) == Some(index));
            let Ok(ty) = u16::try_from(index) else {
                count(false);
                continue;
            };
            let fields = row.fields.range().and_then(|range| self.fields.get(range));
            for (at, field) in fields.unwrap_or_default().iter().enumerate() {
                let named = self
                    .bytes(field.name)
                    .and_then(|name| self.field_named(ty, name));
                count(named.map(usize::from) == Some(at));
            }
        }
        for (index, row) in self.directives.iter().enumerate() {
            let named = self
                .bytes(row.name)
                .and_then(|name| self.directive_named(name));
            count(named.map(usize::from) == Some(index));
        }
        writeln!(out, "lookups found {found} missed {missed}")
    }

    /// Prints the name of the type `index`, or `none`.
    fn type_name(&self, out: &mut impl fmt::Write, index: u16) -> fmt::Result {
        match self.ty(index) {
            Some(row) => self.text(out, row.name),
            None => write!(out, "none"),
        }
    }

    /// Prints a type reference as the SDL writes it, such as `[Reading!]!`.
    fn type_ref(&self, out: &mut impl fmt::Write, ty: TypeRef) -> fmt::Result {
        match self.mods(ty.mods) {
            Some(mods) => self.wrapped(out, ty.named, mods),
            None => write!(out, "missing"),
        }
    }

    /// Prints the named type `named` inside the wrappers `mods`.
    fn wrapped(&self, out: &mut impl fmt::Write, named: u16, mods: &[u8]) -> fmt::Result {
        match mods.split_first() {
            Some((b'!', rest)) => {
                self.wrapped(out, named, rest)?;
                write!(out, "!")
            }
            Some((_, rest)) => {
                write!(out, "[")?;
                self.wrapped(out, named, rest)?;
                write!(out, "]")
            }
            None => self.type_name(out, named),
        }
    }

    /// Prints the bytes `span` names as text, each byte that is no printable
    /// ASCII as an escape.
    fn text(&self, out: &mut impl fmt::Write, span: Span) -> fmt::Result {
        let Some(bytes) = self.bytes(span) else {
            return write!(out, "<missing>");
        };
        for &byte in bytes {
            if byte.is_ascii_graphic() || byte == b' ' {
                out.write_char(char::from(byte))?;
            } else {
                write!(out, "\\x{byte:02x}")?;
            }
        }
        Ok(())
    }
}

/// Returns the field reference of the field `index` of the type `ty`: the
/// type index and the field index in one word, with [`META`] as the type of
/// the meta fields.
#[must_use]
pub fn fref(ty: u16, index: u16) -> u32 {
    (u32::from(ty) << 16) | u32::from(index)
}

// SPDX-License-Identifier: MIT
/*
 * aosabi-validate pass from_schema: the field table of the single pass built
 * from a schema of the type layer, with the handles of its custom scalars.
 *
 * No analogue in the reference.
 */

//! The table of the single pass built from a schema, the boot form.
//!
//! [`OwnedTable::from_schema`] walks the type map, the fields, the possible
//! types, the enum values, the input fields, the directives and the meta
//! fields of a [`Schema`], owns one arena of bytes for every name, key and
//! default, one vector per kind of row and the sorted lookups, and hands out
//! [`Table`] views of them. It refuses nothing a schema holds: custom
//! scalars, a subscription root type, `@oneOf` and a directive on a variable
//! definition all enter the table, and the pass decides what it defers.
//!
//! A scalar is specified only when it is the very type the type layer hands
//! out for its name, and every other scalar is custom, a scalar with a
//! binding laid over a specified name among them. The table keeps the handle
//! of every custom scalar, and [`SchemaScalars`] asks it about a literal
//! through its own literal parse, as the rule for values of the correct type
//! asks it.
//!
//! A default value enters the arena as the JSON the writer of the workspace
//! writes for it, which is what the dump prints and what a device writes from.
//!
//! A build of the conventional validation builds no table. There the names
//! [`OwnedTable`](super::OwnedTable) and [`TableError`](super::TableError) of
//! the module stand for [`NoTable`], of which no value exists, and a string
//! ([`Carry`](super::Carry)), and [`NoTable::from_schema`] answers the
//! sentence [`NO_TABLE`] for every schema.

use alloc::{
    string::String,
    sync::Arc,
    vec,
    vec::Vec, //
};
use core::{
    convert::Infallible,
    fmt, //
};

use aosabi_json::JsValue;
use aosabi_lang::ast::{
    BooleanValueNode,
    EnumValueNode,
    FloatValueNode,
    IntValueNode,
    StringValueNode,
    ValueNode, //
};
use aosabi_type::{
    DefinitionError,
    Field,
    NamedType,
    ScalarType,
    Schema,
    Type,
    directives::DirectiveLocation,
    is_introspection_type,
    schema_meta_field_def,
    specified_scalar_types,
    type_meta_field_def,
    type_name_meta_field_def, //
};

use super::{
    scalars::{
        Answer,
        Literal,
        Scalars, //
    },
    table::{
        ArgRow,
        DirectiveRow,
        Entry,
        FieldLookup,
        FieldRow,
        Kind,
        Lookup,
        NONE,
        Scalar,
        Span,
        Table,
        TypeRef,
        TypeRow,
        loc, //
    },
};

/// Why a table cannot be built from a schema.
#[derive(Debug)]
pub enum TableError {
    /// A thunk of the schema throws, the error it throws.
    Definition(DefinitionError),
    /// The schema holds more types, fields or bytes than the indices of the
    /// table count.
    TooLarge,
    /// A type reference names a type the type map does not hold.
    Missing(String),
}

impl fmt::Display for TableError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Definition(error) => write!(f, "a thunk of the schema throws: {error}"),
            Self::TooLarge => f.write_str("the schema passes the indices of the table"),
            Self::Missing(name) => write!(f, "the type map holds no type {name}"),
        }
    }
}

impl From<DefinitionError> for TableError {
    fn from(error: DefinitionError) -> Self {
        Self::Definition(error)
    }
}

/// The rows of a table that a schema owns, and the handles of its custom
/// scalars.
#[derive(Debug, Default)]
pub struct OwnedTable {
    /// The bytes of every name, key and default.
    names: Vec<u8>,
    /// Every named type.
    types: Vec<TypeRow>,
    /// Every field and the three meta fields.
    fields: Vec<FieldRow>,
    /// Every argument and input field.
    args: Vec<ArgRow>,
    /// Every enum value.
    values: Vec<Span>,
    /// The wrappers of every type reference.
    mods: Vec<u8>,
    /// The possible types of every type.
    possible: Vec<u64>,
    /// The words of one set of possible types.
    words: u32,
    /// Every directive.
    directives: Vec<DirectiveRow>,
    /// The first meta field.
    meta: u32,
    /// The query root type.
    query: u16,
    /// The mutation root type.
    mutation: u16,
    /// The subscription root type.
    subscription: u16,
    /// The directive `@include`.
    include: u16,
    /// The directive `@skip`.
    skip: u16,
    /// Whether the query root type is a possible type of an abstract type or
    /// the named return type of a field.
    meta_corner: bool,
    /// The types sorted by name.
    type_entries: Vec<Entry>,
    /// The fields of each type sorted by name, by type index.
    field_spans: Vec<Span>,
    /// The entries those spans name.
    field_entries: Vec<Entry>,
    /// The directives sorted by name.
    directive_entries: Vec<Entry>,
    /// The custom scalars, by the index of [`Scalar::Custom`].
    scalars: Vec<Arc<ScalarType>>,
}

impl OwnedTable {
    /// Builds the table of `schema`.
    ///
    /// # Errors
    ///
    /// Returns [`TableError::Definition`] with what a thunk of the schema
    /// throws, [`TableError::TooLarge`] for a schema whose rows pass the
    /// indices of a table, and [`TableError::Missing`] for a type reference
    /// whose type the type map does not hold, which a schema the type layer
    /// built never has.
    pub fn from_schema(schema: &Schema) -> Result<Self, TableError> {
        let mut builder = Builder {
            schema,
            table: Self::default(),
        };
        builder.build()?;
        Ok(builder.table)
    }

    /// Returns the table these rows form.
    #[must_use]
    pub fn view(&self) -> Table<'_> {
        Table {
            names: &self.names,
            types: &self.types,
            fields: &self.fields,
            args: &self.args,
            values: &self.values,
            mods: &self.mods,
            possible: &self.possible,
            words: self.words,
            directives: &self.directives,
            meta: self.meta,
            query: self.query,
            mutation: self.mutation,
            subscription: self.subscription,
            include: self.include,
            skip: self.skip,
            meta_corner: self.meta_corner,
            type_named: Lookup::Sorted(&self.type_entries),
            field_named: FieldLookup::Sorted {
                types: &self.field_spans,
                entries: &self.field_entries,
            },
            directive_named: Lookup::Sorted(&self.directive_entries),
        }
    }

    /// Returns the custom scalars of the table, which answer for a literal
    /// through their own literal parse.
    #[must_use]
    pub fn scalars(&self) -> SchemaScalars<'_> {
        SchemaScalars {
            scalars: &self.scalars,
        }
    }
}

/// The table of a schema in a build of the conventional validation, of which
/// no value exists.
///
/// A build whose setting `AOSABI_VALIDATION` is `conventional` carries no
/// single pass ([`CARRIED`](super::CARRIED)), and the name
/// [`OwnedTable`](super::OwnedTable) of the module stands for this type there
/// ([`Carry`](super::Carry)). Its functions are those of [`OwnedTable`] that a
/// caller names, so every caller compiles in both builds, while no table is
/// built: [`NoTable::from_schema`] answers [`NO_TABLE`] without reading the
/// schema, and the field of [`Infallible`] leaves no value for the other two
/// to read. It has no default, since it has no value.
#[derive(Debug)]
pub struct NoTable(Infallible);

impl NoTable {
    /// Answers that a build of the conventional validation builds no table.
    ///
    /// # Errors
    ///
    /// Returns [`NO_TABLE`] for every schema.
    pub fn from_schema(_schema: &Schema) -> Result<Self, &'static str> {
        Err(NO_TABLE)
    }

    /// Returns the table these rows form, which no call reaches, since no
    /// value of the type exists.
    #[must_use]
    pub fn view(&self) -> Table<'_> {
        match self.0 {}
    }

    /// Returns the custom scalars of the table, which no call reaches, since
    /// no value of the type exists.
    #[must_use]
    pub fn scalars(&self) -> SchemaScalars<'_> {
        match self.0 {}
    }
}

/// Why a build of the conventional validation builds no table, the error of
/// [`NoTable::from_schema`].
///
/// The name [`TableError`](super::TableError) of the module stands for a
/// string in such a build ([`Carry`](super::Carry)), so a caller that prints
/// the error runs the `Display` of `str` in `core` and a caller that drops it
/// runs nothing, where a [`TableError`] holds the error a thunk throws or a
/// name.
pub const NO_TABLE: &str = "the build carries the conventional validation, which builds no table";

/// The custom scalars of a table built from a schema.
#[derive(Clone, Copy, Debug)]
pub struct SchemaScalars<'a> {
    /// The custom scalars, by the index of [`Scalar::Custom`].
    scalars: &'a [Arc<ScalarType>],
}

impl Scalars for SchemaScalars<'_> {
    fn literal(&self, scalar: u16, literal: Literal<'_>) -> Answer {
        let Some(ty) = self.scalars.get(usize::from(scalar)) else {
            return Answer::Defer;
        };
        let Some(node) = value_node(literal) else {
            return Answer::Defer;
        };
        // The rule reads a result of `undefined` and a thrown error alike as a
        // value of the wrong type.
        match ty.parse_literal(&node, None) {
            Ok(value) if value.is_undefined() => Answer::Refuse,
            Ok(_) => Answer::Accept,
            Err(_) => Answer::Refuse,
        }
    }
}

/// Returns the value node of the parser for `literal`, or `None` for text
/// that is not UTF-8, which no token of the lexer holds.
fn value_node(literal: Literal<'_>) -> Option<ValueNode> {
    let text = |bytes: &[u8]| core::str::from_utf8(bytes).ok().map(String::from);
    Some(match literal {
        Literal::Int(digits) => ValueNode::IntValue(IntValueNode {
            loc: None,
            value: text(digits)?,
        }),
        Literal::Float(digits) => ValueNode::FloatValue(FloatValueNode {
            loc: None,
            value: text(digits)?,
        }),
        Literal::String { value, block } => ValueNode::StringValue(StringValueNode {
            loc: None,
            value: text(value)?,
            block: Some(block),
        }),
        Literal::Boolean(value) => ValueNode::BooleanValue(BooleanValueNode { loc: None, value }),
        Literal::Enum(name) => ValueNode::EnumValue(EnumValueNode {
            loc: None,
            value: text(name)?,
        }),
    })
}

/// Returns `n` as a `u32`, or the error of a schema too large for a table.
fn index32(n: usize) -> Result<u32, TableError> {
    u32::try_from(n).map_err(|_| TableError::TooLarge)
}

/// Returns `n` as a type, field or directive index, below the two indices
/// the table reserves.
fn index16(n: usize) -> Result<u16, TableError> {
    u16::try_from(n)
        .ok()
        .filter(|&index| index < NONE - 1)
        .ok_or(TableError::TooLarge)
}

/// Returns the span of the entries of `vector` from `at` to its end.
fn tail<T>(vector: &[T], at: usize) -> Result<Span, TableError> {
    let len = vector.len().checked_sub(at).ok_or(TableError::TooLarge)?;
    Ok(Span::new(index32(at)?, index32(len)?))
}

/// The state of the build of one table.
struct Builder<'s> {
    /// The schema the table is built from.
    schema: &'s Schema,
    /// The table built so far.
    table: OwnedTable,
}

impl Builder<'_> {
    /// Builds every row and lookup of the table.
    fn build(&mut self) -> Result<(), TableError> {
        let map = self.schema.get_type_map();
        let count = map.len();
        let words = count.div_ceil(64);
        self.table.words = index32(words)?;
        let slots = count.checked_mul(words).ok_or(TableError::TooLarge)?;
        self.table.possible = vec![0; slots];
        for (index, named) in map.values().enumerate() {
            self.named_type(index, named)?;
        }
        self.meta_fields()?;
        self.directives()?;
        self.table.query = self.root(self.schema.get_query_type())?;
        self.table.mutation = self.root(self.schema.get_mutation_type())?;
        self.table.subscription = self.root(self.schema.get_subscription_type())?;
        self.table.meta_corner = self.meta_corner();
        self.lookups()
    }

    /// Returns the index of the type named `name` in the type map.
    fn type_index(&self, name: &str) -> Result<u16, TableError> {
        let index = self
            .schema
            .get_type_map()
            .get_index_of(name)
            .ok_or_else(|| TableError::Missing(String::from(name)))?;
        index16(index)
    }

    /// Returns the index of the root type `root`, or [`NONE`].
    fn root(&self, root: Option<&NamedType>) -> Result<u16, TableError> {
        root.map_or(Ok(NONE), |root| self.type_index(root.name()))
    }

    /// Appends `bytes` to the arena and returns their span.
    fn name(&mut self, bytes: &[u8]) -> Result<Span, TableError> {
        let at = self.table.names.len();
        self.table.names.extend_from_slice(bytes);
        tail(&self.table.names, at)
    }

    /// Appends the JSON of a default value to the arena and returns its
    /// span, or `None` for the undefined value of no default.
    fn default(&mut self, value: &JsValue) -> Result<Option<Span>, TableError> {
        let at = self.table.names.len();
        if aosabi_json::write(&mut self.table.names, value).is_none() {
            return Ok(None);
        }
        tail(&self.table.names, at).map(Some)
    }

    /// Returns the reference of the type `ty`, its named type and its
    /// wrappers appended to the wrapper bytes.
    fn type_ref(&mut self, ty: &Type) -> Result<TypeRef, TableError> {
        let at = self.table.mods.len();
        let mut current = ty;
        let named = loop {
            if let Some(non_null) = current.as_non_null() {
                self.table.mods.push(b'!');
                current = non_null.of_type();
            } else if let Some(list) = current.as_list() {
                self.table.mods.push(b'[');
                current = &list.of_type;
            } else {
                let named = current
                    .as_named()
                    .ok_or_else(|| TableError::Missing(String::from("a wrapped type")))?;
                break self.type_index(named.name())?;
            }
        };
        Ok(TypeRef {
            named,
            mods: tail(&self.table.mods, at)?,
        })
    }

    /// Returns the row of an argument or an input field.
    fn arg(&mut self, name: &str, ty: &Type, default: &JsValue) -> Result<ArgRow, TableError> {
        Ok(ArgRow {
            name: self.name(name.as_bytes())?,
            ty: self.type_ref(ty)?,
            default: self.default(default)?,
        })
    }

    /// Appends the rows of `args`, each its name, type and default, and
    /// returns their span.
    fn args<'a>(
        &mut self,
        args: impl Iterator<Item = (&'a str, &'a Type, &'a JsValue)>,
    ) -> Result<Span, TableError> {
        let mut rows = Vec::new();
        for (name, ty, default) in args {
            rows.push(self.arg(name, ty, default)?);
        }
        let at = self.table.args.len();
        self.table.args.extend(rows);
        tail(&self.table.args, at)
    }

    /// Returns the row of the field `field`.
    fn field(&mut self, field: &Field) -> Result<FieldRow, TableError> {
        let name = self.name(field.name.as_bytes())?;
        let mut key = Vec::with_capacity(field.name.len() + 3);
        key.push(b'"');
        key.extend_from_slice(field.name.as_bytes());
        key.extend_from_slice(b"\":");
        let key = self.name(&key)?;
        let ty = self.type_ref(&field.r#type)?;
        let leaf = self.named_is_leaf(ty.named)?;
        let args = self.args(
            field
                .args
                .iter()
                .map(|arg| (arg.name.as_str(), &arg.r#type, &arg.default_value)),
        )?;
        Ok(FieldRow {
            name,
            key,
            ty,
            args,
            leaf,
        })
    }

    /// Returns whether the named type `index` of the type map is a leaf.
    fn named_is_leaf(&self, index: u16) -> Result<bool, TableError> {
        let named = self
            .schema
            .get_type_map()
            .get_index(usize::from(index))
            .map(|(_, named)| named)
            .ok_or(TableError::TooLarge)?;
        Ok(matches!(named, NamedType::Scalar(_) | NamedType::Enum(_)))
    }

    /// Appends the rows of `fields` and returns their span.
    fn fields<'a>(&mut self, fields: impl Iterator<Item = &'a Field>) -> Result<Span, TableError> {
        let mut rows = Vec::new();
        for field in fields {
            rows.push(self.field(field)?);
        }
        if rows.len() >= usize::from(NONE - 1) {
            return Err(TableError::TooLarge);
        }
        let at = self.table.fields.len();
        self.table.fields.extend(rows);
        tail(&self.table.fields, at)
    }

    /// Sets the bit of the type `member` in the possible types of the type
    /// `index`.
    fn set_possible(&mut self, index: usize, member: u16) -> Result<(), TableError> {
        let words = usize::try_from(self.table.words).map_err(|_| TableError::TooLarge)?;
        let at = index
            .checked_mul(words)
            .and_then(|at| at.checked_add(usize::from(member / 64)))
            .ok_or(TableError::TooLarge)?;
        let word = self
            .table
            .possible
            .get_mut(at)
            .ok_or(TableError::TooLarge)?;
        *word |= 1u64 << (member % 64);
        Ok(())
    }

    /// Builds the row of the named type `named` at `index` of the type map.
    fn named_type(&mut self, index: usize, named: &NamedType) -> Result<(), TableError> {
        let name = self.name(named.name().as_bytes())?;
        let words = usize::try_from(self.table.words).map_err(|_| TableError::TooLarge)?;
        let possible = index32(index.checked_mul(words).ok_or(TableError::TooLarge)?)?;
        let mut row = TypeRow {
            name,
            kind: Kind::Scalar,
            scalar: Scalar::None,
            fields: Span::EMPTY,
            possible,
            values: Span::EMPTY,
            inputs: Span::EMPTY,
            one_of: false,
            introspection: is_introspection_type(named),
        };
        match named {
            NamedType::Scalar(scalar) => {
                row.scalar = self.scalar(scalar)?;
            }
            NamedType::Object(object) => {
                row.kind = Kind::Object;
                row.fields = self.fields(object.get_fields()?.values())?;
                self.set_possible(index, index16(index)?)?;
            }
            NamedType::Interface(interface) => {
                row.kind = Kind::Interface;
                row.fields = self.fields(interface.get_fields()?.values())?;
                self.possible_types(index, named)?;
            }
            NamedType::Union(_) => {
                row.kind = Kind::Union;
                self.possible_types(index, named)?;
            }
            NamedType::Enum(enum_type) => {
                row.kind = Kind::Enum;
                let mut values = Vec::new();
                for value in enum_type.get_values()? {
                    values.push(self.name(value.name.as_bytes())?);
                }
                let at = self.table.values.len();
                self.table.values.extend(values);
                row.values = tail(&self.table.values, at)?;
            }
            NamedType::InputObject(input) => {
                row.kind = Kind::InputObject;
                row.one_of = input.is_one_of;
                row.inputs = self.args(
                    input
                        .get_fields()?
                        .values()
                        .map(|field| (field.name.as_str(), &field.r#type, &field.default_value)),
                )?;
            }
        }
        self.table.types.push(row);
        Ok(())
    }

    /// Sets the possible types of the abstract type `named` at `index`.
    fn possible_types(&mut self, index: usize, named: &NamedType) -> Result<(), TableError> {
        let mut members = Vec::new();
        for member in self.schema.get_possible_types(named)? {
            members.push(self.type_index(member.name())?);
        }
        for member in members {
            self.set_possible(index, member)?;
        }
        Ok(())
    }

    /// Returns which literals the scalar `scalar` takes, and keeps the handle
    /// of a custom scalar.
    fn scalar(&mut self, scalar: &Arc<ScalarType>) -> Result<Scalar, TableError> {
        let specified = specified_scalar_types()
            .iter()
            .any(|specified| Arc::ptr_eq(specified, scalar));
        if specified {
            match scalar.name.as_str() {
                "Int" => return Ok(Scalar::Int),
                "Float" => return Ok(Scalar::Float),
                "String" => return Ok(Scalar::String),
                "Boolean" => return Ok(Scalar::Boolean),
                "ID" => return Ok(Scalar::Id),
                _ => {}
            }
        }
        let custom = index16(self.table.scalars.len())?;
        self.table.scalars.push(Arc::clone(scalar));
        Ok(Scalar::Custom(custom))
    }

    /// Appends the three meta fields, `__typename`, `__schema` and `__type`.
    fn meta_fields(&mut self) -> Result<(), TableError> {
        let meta = [
            type_name_meta_field_def(),
            schema_meta_field_def()?,
            type_meta_field_def()?,
        ];
        let span = self.fields(meta.iter().map(|field| &**field))?;
        self.table.meta = span.at;
        Ok(())
    }

    /// Appends the row of every directive and finds `@include` and `@skip`.
    fn directives(&mut self) -> Result<(), TableError> {
        let schema = self.schema;
        for directive in schema.get_directives() {
            let name = self.name(directive.name.as_bytes())?;
            let mut locations = 0;
            for location in &directive.locations {
                locations |= match location {
                    DirectiveLocation::Query => loc::QUERY,
                    DirectiveLocation::Mutation => loc::MUTATION,
                    DirectiveLocation::Subscription => loc::SUBSCRIPTION,
                    DirectiveLocation::Field => loc::FIELD,
                    DirectiveLocation::FragmentDefinition => loc::FRAGMENT_DEFINITION,
                    DirectiveLocation::FragmentSpread => loc::FRAGMENT_SPREAD,
                    DirectiveLocation::InlineFragment => loc::INLINE_FRAGMENT,
                    DirectiveLocation::VariableDefinition => loc::VARIABLE_DEFINITION,
                    _ => 0,
                };
            }
            let args = self.args(
                directive
                    .args
                    .iter()
                    .map(|arg| (arg.name.as_str(), &arg.r#type, &arg.default_value)),
            )?;
            self.table.directives.push(DirectiveRow {
                name,
                locations,
                args,
                repeatable: directive.is_repeatable,
            });
        }
        index16(self.table.directives.len())?;
        let position = |name: &str| {
            schema
                .get_directives()
                .iter()
                .position(|directive| directive.name == name)
                .map_or(Ok(NONE), index16)
        };
        self.table.include = position("include")?;
        self.table.skip = position("skip")?;
        Ok(())
    }

    /// Returns whether the pass defers a `__schema` or `__type` field on the
    /// schema: the query root type is a possible type of an interface or a
    /// union, or the named return type of a field.
    ///
    /// Two fields of one response key compare as fields of parents that
    /// exclude each other when their parents are two object types or when the
    /// fields that hold them do. A meta field of the query root type meets a
    /// field of another object type that way only inside a fragment on the
    /// query root type within an abstract type, or below a field that returns
    /// the query root type, since every field at the root of an operation has
    /// the root type as its parent.
    fn meta_corner(&self) -> bool {
        let view = self.table.view();
        let query = self.table.query;
        if query == NONE {
            return false;
        }
        let possible = self.table.types.iter().enumerate().any(|(index, row)| {
            matches!(row.kind, Kind::Interface | Kind::Union)
                && u16::try_from(index)
                    .ok()
                    .and_then(|index| view.possible(index))
                    .is_some_and(|set| set.contains(query))
        });
        let returns_query = |field: &FieldRow| field.ty.named == query;
        possible || self.table.fields.iter().any(returns_query)
    }

    /// Fills the sorted lookups of the types, the fields and the directives.
    fn lookups(&mut self) -> Result<(), TableError> {
        let mut types = Vec::with_capacity(self.table.types.len());
        for (index, row) in self.table.types.iter().enumerate() {
            types.push(Entry {
                name: row.name,
                index: index16(index)?,
            });
        }
        self.sort(&mut types);
        self.table.type_entries = types;

        let mut directives = Vec::with_capacity(self.table.directives.len());
        for (index, row) in self.table.directives.iter().enumerate() {
            directives.push(Entry {
                name: row.name,
                index: index16(index)?,
            });
        }
        self.sort(&mut directives);
        self.table.directive_entries = directives;

        let mut spans = Vec::with_capacity(self.table.types.len());
        let mut entries = Vec::new();
        for row in &self.table.types {
            let range = row.fields.range().ok_or(TableError::TooLarge)?;
            let fields = self.table.fields.get(range).ok_or(TableError::TooLarge)?;
            let at = entries.len();
            for (index, field) in fields.iter().enumerate() {
                entries.push(Entry {
                    name: field.name,
                    index: index16(index)?,
                });
            }
            let run = entries.get_mut(at..).ok_or(TableError::TooLarge)?;
            self.sort(run);
            spans.push(tail(&entries, at)?);
        }
        self.table.field_spans = spans;
        self.table.field_entries = entries;
        Ok(())
    }

    /// Sorts `entries` stably by the bytes of their names.
    ///
    /// The lookups of the types, the fields and the directives all sort
    /// here, so a build holds one instance of the stable sort for them, as
    /// the core files sort through `sort_by_bytes` of the module.
    fn sort(&self, entries: &mut [Entry]) {
        let names = &self.table.names;
        entries.sort_by(|a, b| bytes(names, a.name).cmp(bytes(names, b.name)));
    }
}

/// Returns the bytes `span` names in `names`, empty for a span outside them.
fn bytes(names: &[u8], span: Span) -> &[u8] {
    span.range()
        .and_then(|range| names.get(range))
        .unwrap_or_default()
}

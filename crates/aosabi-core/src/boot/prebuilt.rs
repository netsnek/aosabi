// SPDX-License-Identifier: MIT
/*
 * aosabi-core boot prebuilt: the schema record a build writes, which a boot
 * reads with no lexer, no parser and no validation of the schema.
 *
 * No analogue in the reference, whose Worker builds its schema from the SDL
 * text in its global scope at every start.
 */

//! The schema record a build writes and a boot reads.
//!
//! The boot of the SDL and the IR, [`super::canary_schema_record`], reads the
//! JSON text of the IR, parses the SDL to hold the IR to it, and
//! [`crate::create_schema()`] parses the SDL again, visits it with the rules
//! of the SDL, builds the types from the nodes of that parse and builds the
//! schema a second time from its own type map, which the first request then
//! validates. A build does all of that once with [`write()`], on the machine
//! that builds the app and through the same code, and writes what the boot
//! needs of it: the type resolvers of the IR, the members of each union and
//! interface the type resolvers of a canary build are held to, and every type,
//! field, argument, default value, directive and root of the schema that
//! `create_schema` builds, in the order of its type map. [`schema_record`]
//! then reads that record in one pass over its bytes, and
//! `create_schema` makes the types from it with the bindings of the app and
//! builds the schema once, known valid, so no lexer, no parser and no rule
//! of the SDL or of the schema runs at the boot.
//!
//! # The record
//!
//! The record is a text that opens with [`MAGIC`], which no JSON text opens
//! with, so a host hands it where it hands the JSON text of the IR and the
//! boot tells the two apart. A number is written in digits of five bits, the
//! lowest first, each digit but the last as the byte `]` and up and the last
//! as the byte `#` and up, so every byte of a number is a printable ASCII
//! character other than `"` and `\`, and a text is its length in bytes and
//! its UTF-8. So the whole record is UTF-8 and crosses a JavaScript string,
//! a module of JavaScript that exports it and the input buffer of the module
//! byte for byte. After the magic come the length and the digest of the SDL
//! the record was written from, then the type resolvers, then the members of
//! the abstract types, then the schema: its description, its types, each a
//! head of its kind, its name, its description, the URL of a scalar or the
//! flag `oneOf` of an input object and the length of its body, then the body,
//! then the three roots and the directives. A reference to a type is a list,
//! a non null type or the place of a named type, where the places count the
//! thirteen types every schema shares, `SHARED`, before the types of the
//! record.
//!
//! The boot holds the record it read for the schema build of its SDL, keyed
//! by the text it leaked, so only the build of that boot takes it, and the
//! SDL path stays for every other host and every other build.

mod write;

use std::{
    cell::RefCell,
    rc::Rc,
    sync::{
        Arc,
        OnceLock, //
    }, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_type::{
    ArgumentConfig,
    DefinitionError,
    Directive,
    DirectiveConfig,
    DirectiveLocation,
    EnumType,
    EnumTypeConfig,
    EnumValueConfig,
    FieldConfig,
    InputFieldConfig,
    InputObjectType,
    InputObjectTypeConfig,
    InterfaceType,
    InterfaceTypeConfig,
    ListType,
    NamedType,
    NonNullType,
    ObjMap,
    ObjectType,
    ObjectTypeConfig,
    ScalarType,
    ScalarTypeConfig,
    Schema,
    SchemaBindings,
    SchemaConfig,
    ThunkObjMap,
    ThunkReadonlyArray,
    Type,
    UnionType,
    UnionTypeConfig,
    introspection_schema,
    introspection_types,
    specified_directives,
    specified_scalar_types, //
};

pub use self::write::write;
use super::{
    CanaryResolver,
    SchemaRecord,
    ir::{
        Ir,
        Type as IrType,
        Union as IrUnion, //
    },
    resolve_type, //
};
use crate::{
    create_schema::{
        ResolverMap,
        make_executable_schema, //
    },
    glue::resolve_type::{
        ResolveTypeCheck,
        ResolveTypeRule,
        hold_canary_types, //
    },
    scalars::bound_scalars,
};

/// The text a schema record opens with, with the version of its form.
pub const MAGIC: &str = "#aosabi schema record 1\n";

/// The byte of the last digit of a number, to which the digit is added.
const LAST: u8 = b'#';

/// The byte of a digit another digit follows, to which the digit is added.
const MORE: u8 = b']';

/// The bits of one digit.
const BITS: u32 = 5;

/// The count of the values of one digit.
const DIGITS: u64 = 1 << BITS;

/// The types every schema shares, which a reference names by their place
/// here: the specified scalars and the types of the introspection.
const SHARED: [&str; 13] = [
    "String",
    "Int",
    "Float",
    "Boolean",
    "ID",
    "__Schema",
    "__Directive",
    "__DirectiveLocation",
    "__Type",
    "__Field",
    "__InputValue",
    "__EnumValue",
    "__TypeKind",
];

/// The code of a list in a type reference.
const LIST: u64 = 0;

/// The code of a non null type in a type reference.
const NON_NULL: u64 = 1;

/// The code of the first named type in a type reference, the place of the
/// type added to it.
const NAMED: u64 = 2;

/// The codes of a default value, in the order of [`JsValue`].
const UNDEFINED: u64 = 0;
const NULL: u64 = 1;
const FALSE: u64 = 2;
const TRUE: u64 = 3;
const NUMBER: u64 = 4;
const STRING: u64 = 5;
const ARRAY: u64 = 6;
const OBJECT: u64 = 7;

/// The deepest default value the record holds, the bound of the nesting of
/// the IR of a boot.
const DEPTH: usize = 64;

/// The code of a directive every schema shares, which the record names, and
/// of one the SDL defines, which the record writes whole.
const SPECIFIED: u64 = 0;
const DEFINED: u64 = 1;

/// The kind of a type of the record, its code being its place here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Scalar,
    Object,
    Interface,
    Union,
    Enum,
    InputObject,
}

impl Kind {
    /// Every kind, in the order of its code.
    const ALL: [Self; 6] = [
        Self::Scalar,
        Self::Object,
        Self::Interface,
        Self::Union,
        Self::Enum,
        Self::InputObject,
    ];

    /// Returns the kind of the code `code`.
    fn of(code: u64) -> Option<Self> {
        Self::ALL.get(usize::try_from(code).ok()?).copied()
    }

    /// Returns the code of the kind.
    fn code(self) -> u64 {
        Self::ALL
            .iter()
            .position(|kind| *kind == self)
            .map_or(0, |at| u64::try_from(at).unwrap_or(0))
    }
}

/// Returns whether `text`, the text a host hands in place of the JSON text
/// of the IR, is a schema record.
#[must_use]
pub fn is_record(text: &str) -> bool {
    text.starts_with(MAGIC)
}

/// Returns the digest of `text`, which the record keeps of the SDL it was
/// written from, so a boot refuses a record of another SDL.
///
/// The digest folds the text eight bytes at a time and is no hash an
/// adversary cannot meet, since the build writes the record and the SDL side
/// by side, and the boot holds a record to the SDL it is handed to tell a
/// stale record from a fresh one.
fn digest(text: &str) -> u64 {
    const PRIME: u64 = 0x0000_0100_0000_01b3;
    let (words, tail) = text.as_bytes().as_chunks::<8>();
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for word in words {
        hash = (hash ^ u64::from_le_bytes(*word))
            .wrapping_mul(PRIME)
            .rotate_left(29);
    }
    for byte in tail {
        hash = (hash ^ u64::from(*byte)).wrapping_mul(PRIME);
    }
    hash ^ u64::try_from(text.len()).unwrap_or(u64::MAX)
}

/// A cursor over the text of a record, which answers `None` past its end or
/// at a byte no field takes.
#[derive(Clone, Copy, Debug)]
struct Cursor {
    /// The text of the record, which lives as long as the module.
    text: &'static str,
    /// The place of the next byte.
    at: usize,
}

impl Cursor {
    /// Reads one byte.
    fn byte(&mut self) -> Option<u8> {
        let byte = *self.text.as_bytes().get(self.at)?;
        self.at += 1;
        Some(byte)
    }

    /// Reads a number.
    fn num(&mut self) -> Option<u64> {
        let mut value = 0;
        let mut shift = 0;
        loop {
            let byte = self.byte()?;
            let digit = u64::from(byte.wrapping_sub(LAST));
            if digit < DIGITS {
                return Some(value | digit.checked_shl(shift)?);
            }
            let digit = u64::from(byte.wrapping_sub(MORE));
            if digit >= DIGITS || shift >= u64::BITS - BITS {
                return None;
            }
            value |= digit << shift;
            shift += BITS;
        }
    }

    /// Reads a count or a length.
    fn count(&mut self) -> Option<usize> {
        usize::try_from(self.num()?).ok()
    }

    /// Reads a flag, a number 0 or 1.
    fn flag(&mut self) -> Option<bool> {
        match self.num()? {
            0 => Some(false),
            1 => Some(true),
            _ => None,
        }
    }

    /// Reads `len` bytes as a text.
    fn take(&mut self, len: usize) -> Option<&'static str> {
        let end = self.at.checked_add(len)?;
        let text = self.text.get(self.at..end)?;
        self.at = end;
        Some(text)
    }

    /// Reads a length and that many bytes as a text.
    fn text(&mut self) -> Option<&'static str> {
        let len = self.count()?;
        self.take(len)
    }

    /// Reads a text that may be absent, its length plus one or 0.
    // The outer option is the end of the record, as for every read of the
    // cursor, and the inner one the text the record leaves absent.
    #[expect(clippy::option_option)]
    fn optional(&mut self) -> Option<Option<&'static str>> {
        match self.count()? {
            0 => Some(None),
            len => self.take(len - 1).map(Some),
        }
    }

    /// Reads a count and that many texts.
    fn texts(&mut self) -> Option<Vec<&'static str>> {
        let count = self.count()?;
        let mut texts = Vec::with_capacity(count.min(self.text.len()));
        for _ in 0..count {
            texts.push(self.text()?);
        }
        Some(texts)
    }
}

/// The message of a record that ends early or holds a byte no field takes.
const BROKEN: &str = "the schema record is broken";

/// Returns the error of a record that ends early or holds a byte no field
/// takes, which no record a build wrote holds.
#[cold]
#[inline(never)]
fn broken() -> DefinitionError {
    DefinitionError::plain(BROKEN)
}

/// Returns the message of a record whose SDL is not the SDL `type_defs` the
/// host hands the boot.
#[cold]
#[inline(never)]
fn stale() -> String {
    String::from(
        "the schema record was written from another SDL than the build hands the boot, so the build writes it again",
    )
}

thread_local! {
    /// The record of the boot, with the SDL it leaked, which only the schema
    /// build of that SDL takes.
    static HELD: RefCell<Option<(&'static str, Rc<Prebuilt>)>> = const { RefCell::new(None) };
}

/// Returns the record the boot of `type_defs` read, where the boot read one
/// for that text, which it leaked.
pub(crate) fn held(type_defs: &str) -> Option<Rc<Prebuilt>> {
    HELD.with_borrow(|held| match held {
        Some((text, prebuilt)) if core::ptr::eq(*text, type_defs) => Some(Rc::clone(prebuilt)),
        _ => None,
    })
}

/// Reads the schema record `record` of the SDL `type_defs`, with the type
/// resolvers `canary` of a canary build in place of the rule the
/// IR gives each of their types, as [`super::canary_schema_record`] reads the
/// SDL and the IR, and holds its schema for the schema build of the SDL the
/// record names.
///
/// # Errors
///
/// Returns a message for a record that is broken or was written from
/// another SDL, or one that names a type or a member of `canary` that is no
/// union or interface of the record and no member of it.
pub fn schema_record(
    type_defs: &str,
    record: &str,
    canary: &[CanaryResolver],
) -> Result<SchemaRecord, String> {
    if !is_record(record) {
        return Err(String::from(BROKEN));
    }
    let text: &'static str = Box::leak(Box::<str>::from(record));
    let mut at = Cursor {
        text,
        at: MAGIC.len(),
    };
    let len = at.count().ok_or(BROKEN)?;
    if len != type_defs.len() || at.num().ok_or(BROKEN)? != digest(type_defs) {
        return Err(stale());
    }
    let mut rules = read_rules(&mut at).ok_or(BROKEN)?;
    let members = read_members(&mut at).ok_or(BROKEN)?;
    resolve_type::canary_rules(&members, canary, &mut rules)?;
    let prebuilt = Prebuilt::read(&mut at).ok_or(BROKEN)?;
    prebuilt.hold_to(&rules)?;
    hold_canary_types(
        canary
            .iter()
            .map(|resolver| resolver.type_name.clone())
            .collect(),
    );
    let type_defs: &'static str = Box::leak(Box::<str>::from(type_defs));
    HELD.with_borrow_mut(|held| *held = Some((type_defs, Rc::new(prebuilt))));
    Ok(SchemaRecord {
        type_defs,
        resolve_types: rules.leak(),
    })
}

/// Prints the schema the record `record` of the SDL `type_defs` builds and
/// the schema the SDL builds, each through the schema build of
/// [`crate::create_schema()`] with the scalars every boot binds, and the
/// schema the record builds with no bindings, so a test build holds the first
/// two to each other and the third to the SDL as the printer of graphql-js
/// writes it.
///
/// Each of the first two is the text of the printer of `aosabi-type` followed
/// by the names of the type map in its order and the names of the
/// directives, which the printer leaves out where they are specified. The
/// record is read as a boot reads it, so its schema is held for the text the
/// read leaked alone, and the build of the SDL takes the text of the caller,
/// which holds none.
///
/// # Errors
///
/// Returns what [`schema_record`] returns, or the message of an error of a
/// build or of the printer.
pub fn print(type_defs: &str, record: &str) -> Result<[String; 3], String> {
    let held = schema_record(type_defs, record, &[])?;
    let map = ResolverMap {
        scalars: bound_scalars(type_defs),
        ..ResolverMap::default()
    };
    let built = |text: &str, map: &ResolverMap| {
        make_executable_schema(text, map).map_err(|error| error.to_string())
    };
    let printed = |schema: &Schema| {
        aosabi_type::print_schema(schema)
            .map_err(|error| aosabi_lang::js_string::to_string_lossy(error.message()).into_owned())
    };
    let traced = |schema: &Schema| -> Result<String, String> {
        let types: Vec<&str> = schema.get_type_map().keys().map(String::as_str).collect();
        let directives: Vec<&str> = schema
            .get_directives()
            .iter()
            .map(|directive| directive.name.as_str())
            .collect();
        Ok(format!(
            "{}\n# the type map: {}\n# the directives: {}\n",
            printed(schema)?,
            types.join(" "),
            directives.join(" ")
        ))
    };
    Ok([
        traced(built(held.type_defs, &map)?.as_ref())?,
        traced(built(type_defs, &map)?.as_ref())?,
        printed(built(held.type_defs, &ResolverMap::default())?.as_ref())?,
    ])
}

/// Reads the type resolvers the build made of the IR, which live as long as
/// the module, as the rules of the SDL path do.
fn read_rules(at: &mut Cursor) -> Option<Vec<ResolveTypeRule>> {
    let count = at.count()?;
    let mut rules = Vec::with_capacity(count.min(at.text.len()));
    for _ in 0..count {
        let type_name = at.text()?;
        let valid_names = at.texts()?.leak();
        let checks = at.count()?;
        let mut held = Vec::with_capacity(checks.min(at.text.len()));
        for _ in 0..checks {
            let any = at.flag()?;
            let name = at.text()?;
            let fields = at.texts()?.leak();
            held.push(if any {
                ResolveTypeCheck::AnyField { fields, name }
            } else {
                ResolveTypeCheck::AllFields { fields, name }
            });
        }
        rules.push(ResolveTypeRule {
            type_name,
            valid_names,
            checks: held.leak(),
        });
    }
    Some(rules)
}

/// Reads the members of the unions and the interfaces of the IR, the part of
/// the model the type resolvers of a canary build are held to: the objects
/// with the interfaces each implements, the interfaces and the unions with
/// their members, each in the order of the model.
fn read_members(at: &mut Cursor) -> Option<Ir> {
    let type_of = |name: &str, implements: Vec<&str>| IrType {
        name: String::from(name),
        fields: Vec::new(),
        implements: implements.into_iter().map(String::from).collect(),
    };
    let count = at.count()?;
    let mut objects = Vec::with_capacity(count.min(at.text.len()));
    for _ in 0..count {
        let name = at.text()?;
        objects.push(type_of(name, at.texts()?));
    }
    let interfaces = at
        .texts()?
        .into_iter()
        .map(|name| type_of(name, Vec::new()))
        .collect();
    let count = at.count()?;
    let mut unions = Vec::with_capacity(count.min(at.text.len()));
    for _ in 0..count {
        let name = String::from(at.text()?);
        let members = at.texts()?.into_iter().map(String::from).collect();
        unions.push(IrUnion { name, members });
    }
    Some(Ir {
        objects,
        interfaces,
        unions,
        operations: Vec::new(),
    })
}

/// A type of the record: its head, read at the boot, and its body, which the
/// thunks of the type read when the schema build resolves them.
#[derive(Clone, Copy, Debug)]
struct Entry {
    /// The kind.
    kind: Kind,
    /// The name.
    name: &'static str,
    /// The description.
    description: Option<&'static str>,
    /// The URL of a scalar, `specifiedByURL`.
    url: Option<&'static str>,
    /// The flag `isOneOf` of an input object.
    one_of: bool,
    /// The cursor at the body.
    body: Cursor,
}

/// The schema of a record, its types and the cursors at its parts, which
/// [`Prebuilt::schema`] builds with the bindings of the app.
#[derive(Debug)]
pub(crate) struct Prebuilt {
    /// The description of the schema.
    description: Option<&'static str>,
    /// The types, in the order of the type map of the schema the SDL builds.
    types: Vec<Entry>,
    /// The places of the types the schema is built from, in the order of the
    /// type map of the second build of the SDL path.
    order: Vec<usize>,
    /// The places of the specified scalars the first build of the SDL holds,
    /// those the SDL declares among them, which the check of the resolver map
    /// reads.
    declared: Vec<usize>,
    /// The places of the roots `query`, `mutation` and `subscription`.
    roots: [Option<usize>; 3],
    /// The cursor at the directives.
    directives: Cursor,
}

impl Prebuilt {
    /// Reads the schema part of the record at the cursor of `at`.
    fn read(at: &mut Cursor) -> Option<Self> {
        let description = at.optional()?;
        let count = at.count()?;
        let mut types = Vec::with_capacity(count.min(at.text.len()));
        for _ in 0..count {
            types.push(Self::entry(at)?);
        }
        let count = at.count()?;
        let mut order = Vec::with_capacity(count.min(at.text.len()));
        for _ in 0..count {
            order.push(at.count()?);
        }
        let count = at.count()?;
        let mut declared = Vec::with_capacity(count.min(SHARED.len()));
        for _ in 0..count {
            declared.push(at.count()?);
        }
        let mut roots = [None; 3];
        for root in &mut roots {
            *root = match at.count()? {
                0 => None,
                place => Some(place - 1),
            };
        }
        Some(Self {
            description,
            types,
            order,
            declared,
            roots,
            directives: *at,
        })
    }

    /// Reads the head of one type at the cursor of `at` and passes its body.
    fn entry(at: &mut Cursor) -> Option<Entry> {
        let kind = Kind::of(at.num()?)?;
        let name = at.text()?;
        let description = at.optional()?;
        let url = if kind == Kind::Scalar {
            at.optional()?
        } else {
            None
        };
        let one_of = kind == Kind::InputObject && at.flag()?;
        let len = at.count()?;
        let body = *at;
        at.take(len)?;
        Some(Entry {
            kind,
            name,
            description,
            url,
            one_of,
            body,
        })
    }

    /// Holds `rules` to the types of the record, as the SDL path holds them
    /// to the SDL: each stands on a union or an interface, whose members are
    /// object types.
    fn hold_to(&self, rules: &[ResolveTypeRule]) -> Result<(), String> {
        let kind_of = |name: &str| {
            self.types
                .iter()
                .find(|entry| entry.name == name)
                .map(|entry| entry.kind)
        };
        for rule in rules {
            if !matches!(kind_of(rule.type_name), Some(Kind::Union | Kind::Interface)) {
                return Err(not_abstract(rule.type_name));
            }
            if let Some(name) = rule
                .valid_names
                .iter()
                .find(|name| kind_of(name) != Some(Kind::Object))
            {
                return Err(not_object(rule.type_name, name));
            }
        }
        Ok(())
    }

    /// Builds the schema of the record with `bindings` laid over its types,
    /// the schema `create_schema` builds from the SDL the record was written
    /// from with the same bindings, known valid, and returns it with the
    /// specified scalars the first build of that SDL holds, which the check of
    /// the resolver map reads.
    ///
    /// Each type takes the config the builders of `aosabi-type` give the
    /// definition it was written from, with no node of a parse, and its
    /// interfaces, its fields and its members are thunks, as there, which read
    /// the body of the type when the schema resolves them.
    ///
    /// # Errors
    ///
    /// Returns what a constructor of a type, of a directive or of the schema
    /// throws, or the error of a broken record.
    pub(crate) fn schema(
        &self,
        bindings: &SchemaBindings,
    ) -> Result<(Schema, Vec<NamedType>), DefinitionError> {
        let scope = Arc::new(Scope {
            table: OnceLock::new(),
            bindings: bindings.clone(),
        });
        let mut table = shared_types(&scope.bindings)?;
        table.reserve(self.types.len());
        for entry in &self.types {
            table.push(scope.named(entry)?);
        }
        let _ = scope.table.set(table);
        let mut types = Vec::with_capacity(self.order.len());
        for place in &self.order {
            types.push(scope.place(*place)?);
        }
        let directives = scope.directives(self.directives)?;
        let [query, mutation, subscription] =
            self.roots.map(|root| root.map(|place| scope.place(place)));
        let mut declared = Vec::with_capacity(self.declared.len());
        for place in &self.declared {
            declared.push(scope.place(*place)?);
        }
        let schema = Schema::new(
            SchemaConfig {
                description: self.description.map(String::from),
                query: query.transpose()?,
                mutation: mutation.transpose()?,
                subscription: subscription.transpose()?,
                types,
                directives: Some(directives),
                extensions: JsObject::new(),
                ast_node: None,
                extension_ast_nodes: Vec::new(),
                assume_valid: true,
            },
            &introspection_schema()?,
        )?;
        Ok((schema, declared))
    }
}

/// Returns the message of a type resolver on a type that is no union and no
/// interface of the record, the message of the SDL path.
#[cold]
#[inline(never)]
fn not_abstract(name: &str) -> String {
    format!("the IR resolves the type {name}, which is no union and no interface of the SDL")
}

/// Returns the message of a type resolver whose member is no object type of
/// the record, the message of the SDL path.
#[cold]
#[inline(never)]
fn not_object(type_name: &str, name: &str) -> String {
    format!("the IR resolves {type_name} to {name}, which is no object type of the SDL")
}

/// Returns the types every schema shares in the order of [`SHARED`]: each
/// specified scalar, or the copy of it with its binding laid over it where
/// `bindings` names it, as the standard type map of the builders holds them,
/// and the types of the introspection.
fn shared_types(bindings: &SchemaBindings) -> Result<Vec<NamedType>, DefinitionError> {
    let mut shared = Vec::with_capacity(SHARED.len() + 64);
    for name in SHARED {
        let scalar = specified_scalar_types()
            .iter()
            .find(|scalar| scalar.name == name);
        let named = match scalar {
            Some(scalar) => match bindings.scalars.get(name) {
                Some(binding) => NamedType::from(Arc::new(ScalarType::new(
                    binding.apply(scalar.to_config()),
                )?)),
                None => NamedType::from(Arc::clone(scalar)),
            },
            None => introspection_types()?
                .iter()
                .find(|ty| ty.name() == name)
                .cloned()
                .ok_or_else(broken)?,
        };
        shared.push(named);
    }
    Ok(shared)
}

/// What the thunks of the types of one schema build share: the table of the
/// types by their places, published once every type exists, and the
/// bindings, the scope of the builders of `aosabi-type`.
struct Scope {
    /// The shared types and then the types of the record.
    table: OnceLock<Vec<NamedType>>,
    /// The bindings laid over the types.
    bindings: SchemaBindings,
}

impl Scope {
    /// Returns the type at the place `place`.
    ///
    /// A thunk runs only after the table is published, so an empty table is
    /// the invariant of the builders of `aosabi-type`, whose message this is.
    fn place(&self, place: usize) -> Result<NamedType, DefinitionError> {
        let Some(table) = self.table.get() else {
            return Err(DefinitionError::plain("Unexpected invariant triggered."));
        };
        table.get(place).cloned().ok_or_else(broken)
    }

    /// Reads a type reference at the cursor of `at`.
    fn wrapped(&self, at: &mut Cursor) -> Result<Type, DefinitionError> {
        match at.num().ok_or_else(broken)? {
            LIST => Ok(Type::from(ListType::new(self.wrapped(at)?))),
            NON_NULL => Ok(Type::from(NonNullType::new(self.wrapped(at)?)?)),
            code => {
                let place = usize::try_from(code - NAMED).map_err(|_| broken())?;
                Ok(Type::from(self.place(place)?))
            }
        }
    }

    /// Reads a count and that many named types at the cursor of `at`.
    fn named_list(&self, at: &mut Cursor) -> Result<Vec<NamedType>, DefinitionError> {
        let count = at.count().ok_or_else(broken)?;
        let mut types = Vec::with_capacity(count.min(at.text.len()));
        for _ in 0..count {
            types.push(self.place(at.count().ok_or_else(broken)?)?);
        }
        Ok(types)
    }

    /// Makes the type of `entry` with the bindings of the scope.
    fn named(self: &Arc<Self>, entry: &Entry) -> Result<NamedType, DefinitionError> {
        let name = String::from(entry.name);
        let description = entry.description.map(String::from);
        Ok(match entry.kind {
            Kind::Scalar => {
                let config = ScalarTypeConfig {
                    name: name.clone(),
                    description,
                    specified_by_url: entry.url.map(String::from),
                    serialize: None,
                    parse_value: None,
                    parse_literal: None,
                    extensions: JsObject::new(),
                    ast_node: None,
                    extension_ast_nodes: Vec::new(),
                };
                NamedType::from(Arc::new(ScalarType::new(
                    self.bindings.bind_scalar(&name, config),
                )?))
            }
            Kind::Object => NamedType::from(Arc::new(ObjectType::new(ObjectTypeConfig {
                interfaces: self.interfaces(entry.body),
                fields: self.fields(entry.name, entry.body),
                is_type_of: self.bindings.is_type_of(&name, None),
                name,
                description,
                extensions: JsObject::new(),
                ast_node: None,
                extension_ast_nodes: Vec::new(),
            })?)),
            Kind::Interface => {
                NamedType::from(Arc::new(InterfaceType::new(InterfaceTypeConfig {
                    interfaces: self.interfaces(entry.body),
                    fields: self.fields(entry.name, entry.body),
                    resolve_type: self.bindings.resolve_type(&name, None),
                    name,
                    description,
                    extensions: JsObject::new(),
                    ast_node: None,
                    extension_ast_nodes: Vec::new(),
                })?))
            }
            Kind::Union => NamedType::from(Arc::new(UnionType::new(UnionTypeConfig {
                types: self.members(entry.body),
                resolve_type: self.bindings.resolve_type(&name, None),
                name,
                description,
                extensions: JsObject::new(),
                ast_node: None,
                extension_ast_nodes: Vec::new(),
            })?)),
            Kind::Enum => NamedType::from(Arc::new(EnumType::new(EnumTypeConfig {
                values: ThunkObjMap::Map(enum_values(entry.body)?),
                name,
                description,
                extensions: JsObject::new(),
                ast_node: None,
                extension_ast_nodes: Vec::new(),
            })?)),
            Kind::InputObject => {
                NamedType::from(Arc::new(InputObjectType::new(InputObjectTypeConfig {
                    fields: self.input_fields(entry.body),
                    name,
                    description,
                    extensions: JsObject::new(),
                    ast_node: None,
                    extension_ast_nodes: Vec::new(),
                    is_one_of: entry.one_of,
                })?))
            }
        })
    }

    /// Returns the thunk of the interfaces of an object or an interface whose
    /// body opens at `body`.
    fn interfaces(self: &Arc<Self>, body: Cursor) -> ThunkReadonlyArray<NamedType> {
        let scope = Arc::clone(self);
        ThunkReadonlyArray::thunk(move || scope.named_list(&mut { body }))
    }

    /// Returns the thunk of the members of a union whose body opens at
    /// `body`.
    fn members(self: &Arc<Self>, body: Cursor) -> ThunkReadonlyArray<NamedType> {
        let scope = Arc::clone(self);
        ThunkReadonlyArray::thunk(move || scope.named_list(&mut { body }))
    }

    /// Returns the thunk of the fields of the object or the interface `name`
    /// whose body opens at `body`, with the field bindings of the type laid
    /// over them.
    fn fields(self: &Arc<Self>, name: &'static str, body: Cursor) -> ThunkObjMap<FieldConfig> {
        let scope = Arc::clone(self);
        ThunkObjMap::thunk(move || {
            let mut at = body;
            let interfaces = at.count().ok_or_else(broken)?;
            for _ in 0..interfaces {
                at.num().ok_or_else(broken)?;
            }
            let mut fields = scope.field_map(&mut at)?;
            scope.bindings.bind_fields(name, &mut fields);
            Ok(fields)
        })
    }

    /// Reads a count and that many fields at the cursor of `at`.
    fn field_map(&self, at: &mut Cursor) -> Result<ObjMap<FieldConfig>, DefinitionError> {
        let count = at.count().ok_or_else(broken)?;
        let mut fields = ObjMap::default();
        for _ in 0..count {
            let name = at.text().ok_or_else(broken)?;
            let description = at.optional().ok_or_else(broken)?;
            let r#type = self.wrapped(at)?;
            let deprecation_reason = at.optional().ok_or_else(broken)?;
            let config = FieldConfig {
                r#type,
                description: description.map(String::from),
                args: self.arguments(at)?,
                deprecation_reason: deprecation_reason.map(String::from),
                ast_node: None,
                resolve: None,
                subscribe: None,
                extensions: JsObject::new(),
            };
            fields.insert(String::from(name), config);
        }
        Ok(fields)
    }

    /// Reads a count and that many arguments at the cursor of `at`.
    fn arguments(&self, at: &mut Cursor) -> Result<ObjMap<ArgumentConfig>, DefinitionError> {
        let count = at.count().ok_or_else(broken)?;
        let mut args = ObjMap::default();
        for _ in 0..count {
            let name = at.text().ok_or_else(broken)?;
            let (description, r#type, default_value, deprecation_reason) = self.input_value(at)?;
            let config = ArgumentConfig {
                description,
                default_value,
                deprecation_reason,
                ast_node: None,
                extensions: JsObject::new(),
                r#type,
            };
            args.insert(String::from(name), config);
        }
        Ok(args)
    }

    /// Reads the description, the type, the default value and the reason of
    /// the deprecation of an argument or an input field at the cursor of
    /// `at`.
    fn input_value(
        &self,
        at: &mut Cursor,
    ) -> Result<(Option<String>, Type, JsValue, Option<String>), DefinitionError> {
        let description = at.optional().ok_or_else(broken)?.map(String::from);
        let r#type = self.wrapped(at)?;
        let default_value = value(at, 0).ok_or_else(broken)?;
        let deprecation_reason = at.optional().ok_or_else(broken)?.map(String::from);
        Ok((description, r#type, default_value, deprecation_reason))
    }

    /// Returns the thunk of the fields of an input object whose body opens
    /// at `body`.
    fn input_fields(self: &Arc<Self>, body: Cursor) -> ThunkObjMap<InputFieldConfig> {
        let scope = Arc::clone(self);
        ThunkObjMap::thunk(move || {
            let mut at = body;
            let count = at.count().ok_or_else(broken)?;
            let mut fields = ObjMap::default();
            for _ in 0..count {
                let name = at.text().ok_or_else(broken)?;
                let (description, r#type, default_value, deprecation_reason) =
                    scope.input_value(&mut at)?;
                let config = InputFieldConfig {
                    description,
                    default_value,
                    deprecation_reason,
                    extensions: JsObject::new(),
                    ast_node: None,
                    r#type,
                };
                fields.insert(String::from(name), config);
            }
            Ok(fields)
        })
    }

    /// Reads the directives of the schema at the cursor `at`: a directive
    /// every schema shares as the one [`specified_directives`] holds, and one
    /// the SDL defines as its builder makes it.
    fn directives(&self, mut at: Cursor) -> Result<Vec<Arc<Directive>>, DefinitionError> {
        let count = at.count().ok_or_else(broken)?;
        let mut directives = Vec::with_capacity(count.min(at.text.len()));
        for _ in 0..count {
            let code = at.num().ok_or_else(broken)?;
            let name = at.text().ok_or_else(broken)?;
            let directive = match code {
                SPECIFIED => specified_directives()
                    .iter()
                    .find(|directive| directive.name == name)
                    .cloned()
                    .ok_or_else(broken)?,
                DEFINED => Arc::new(self.directive(name, &mut at)?),
                _ => return Err(broken()),
            };
            directives.push(directive);
        }
        Ok(directives)
    }

    /// Reads the rest of the directive `name` the SDL defines at the cursor
    /// of `at`.
    // Out of line, so a boot compiles the build of a directive where the SDL defines one.
    #[inline(never)]
    fn directive(&self, name: &str, at: &mut Cursor) -> Result<Directive, DefinitionError> {
        let description = at.optional().ok_or_else(broken)?.map(String::from);
        let names = at.texts().ok_or_else(broken)?;
        let mut locations = Vec::with_capacity(names.len());
        for location in names {
            locations.push(DirectiveLocation::from_name(location).ok_or_else(broken)?);
        }
        let is_repeatable = at.flag().ok_or_else(broken)?;
        Directive::new(DirectiveConfig {
            name: String::from(name),
            description,
            locations,
            location_names: None,
            is_repeatable,
            args: self.arguments(at)?,
            extensions: JsObject::new(),
            ast_node: None,
        })
    }
}

/// Reads the values of an enum whose body opens at `body`, each with the
/// value of its name, which the constructor of the enum gives a value the
/// config leaves undefined, as its builder leaves it.
fn enum_values(mut body: Cursor) -> Result<ObjMap<EnumValueConfig>, DefinitionError> {
    let at = &mut body;
    let count = at.count().ok_or_else(broken)?;
    let mut values = ObjMap::default();
    for _ in 0..count {
        let name = at.text().ok_or_else(broken)?;
        let description = at.optional().ok_or_else(broken)?.map(String::from);
        let deprecation_reason = at.optional().ok_or_else(broken)?.map(String::from);
        let config = EnumValueConfig {
            description,
            value: JsValue::Undefined,
            deprecation_reason,
            extensions: JsObject::new(),
            ast_node: None,
        };
        values.insert(String::from(name), config);
    }
    Ok(values)
}

/// Reads a default value at the cursor of `at`, nested `depth` levels deep.
fn value(at: &mut Cursor, depth: usize) -> Option<JsValue> {
    if depth > DEPTH {
        return None;
    }
    Some(match at.num()? {
        UNDEFINED => JsValue::Undefined,
        NULL => JsValue::Null,
        FALSE => JsValue::Bool(false),
        TRUE => JsValue::Bool(true),
        NUMBER => JsValue::Number(f64::from_bits(at.num()?)),
        STRING => JsValue::String(JsString::from(at.text()?)),
        ARRAY => {
            let count = at.count()?;
            let mut items = Vec::with_capacity(count.min(at.text.len()));
            for _ in 0..count {
                items.push(value(at, depth + 1)?);
            }
            JsValue::Array(items)
        }
        OBJECT => {
            let count = at.count()?;
            let mut object = JsObject::with_capacity(count.min(at.text.len()));
            for _ in 0..count {
                let key = at.text()?;
                object.insert(key, value(at, depth + 1)?);
            }
            JsValue::Object(object)
        }
        _ => return None,
    })
}

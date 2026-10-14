// SPDX-License-Identifier: MIT
/*
 * aoviewer-core read ir: the mapping of the ir.json of a build into the model.
 *
 * The mapping repeats the rules of the SDL printer of the reference, so the
 * model of ir.json has the types and the order the served schema prints.
 */

//! The ir.json reader.
//!
//! The reader holds the whole intermediate representation before the builder
//! sees a type, because the printer writes the root types first and drops an
//! interface by what every interface declares, whatever the order of the keys
//! of the file. The nine keys of `ir.json` are required and `version` is the
//! number 1, so a file of another kind is refused rather than read as an
//! empty schema. Inside them a list that is `null` or absent holds nothing,
//! and an unknown key is skipped, such as the `column` and `relation` of an
//! entity field, which the printer never reads.
//!
//! The model holds what the printer writes and no more: no introspection type
//! and no directive, because the `@inContext` block is appended to the served
//! schema after the printer.

use alloc::{
    borrow::Cow,
    boxed::Box,
    string::String,
    vec::Vec, //
};
use core::cmp::Ordering;

use super::{
    Cursor,
    ReadError, //
};
use crate::{
    json::Token,
    limits::ReadLimits,
    model::{
        Builder,
        Kind,
        Model,
        TypeId,
        TypeRefSpec, //
    },
};

/// The three operation roots in the order the printer groups them, which are
/// also the names of the root types it writes.
const ROOTS: [&str; 3] = ["Query", "Mutation", "Subscription"];

/// A named type of one of the six records, with the lists its kind carries.
#[derive(Debug, Default)]
struct IrType<'a> {
    name: Option<Cow<'a, str>>,
    description: Option<Cow<'a, str>>,
    fields: Vec<IrField<'a>>,
    implements: Vec<Cow<'a, str>>,
    members: Vec<Cow<'a, str>>,
    values: Vec<Cow<'a, str>>,
}

/// A field, an argument or an input field, which the representation holds in
/// one shape.
#[derive(Debug)]
struct IrField<'a> {
    name: Cow<'a, str>,
    type_ref: TypeRefSpec,
    exposed: bool,
    description: Option<Cow<'a, str>>,
    args: Vec<IrField<'a>>,
}

/// An operation of a root.
#[derive(Debug)]
struct IrOperation<'a> {
    root: Cow<'a, str>,
    name: Cow<'a, str>,
    args: Vec<IrField<'a>>,
    returns: TypeRefSpec,
    description: Option<Cow<'a, str>>,
}

/// The whole intermediate representation, each record in the order of its
/// keys, which is the order of `Object.values` for keys that are type names.
#[derive(Debug, Default)]
struct Ir<'a> {
    entities: Vec<IrType<'a>>,
    objects: Vec<IrType<'a>>,
    interfaces: Vec<IrType<'a>>,
    unions: Vec<IrType<'a>>,
    inputs: Vec<IrType<'a>>,
    enums: Vec<IrType<'a>>,
    scalars: Vec<Cow<'a, str>>,
    operations: Vec<IrOperation<'a>>,
}

/// Maps the `ir.json` of a build by the rules of the SDL printer,
/// descriptions as the intermediate representation carries them, types in
/// the order the printer writes them.
///
/// The types stand in the order the printer writes them: the roots `Query`,
/// `Mutation` and `Subscription` that have an operation, the entities and the
/// objects with an exposed field, the inputs, the unions, the interfaces with
/// an exposed field, the scalars and the enums. An object keeps its exposed
/// fields, an input all of its fields or the one field `_: String` when it
/// has none, and an `implements` list loses the dropped interfaces and is
/// sorted as `localeCompare` sorts it. A description is the text the
/// representation carries, an empty one is none, as the printer writes no
/// block for it, and the descriptions of arguments are kept, although the
/// printer leaves them out. A root is the type called `Query`, `Mutation` or
/// `Subscription`, which is how graphql-js reads a schema without a schema
/// definition.
///
/// # Errors
///
/// Returns [`ReadError::Json`] for bytes that are no JSON document within
/// `limits`, [`ReadError::Shape`] for a document that is no `ir.json`, and
/// [`ReadError::Model`] for a schema the builder refuses or a count past
/// `limits`.
pub fn from_ir_json(bytes: &[u8], limits: &ReadLimits) -> Result<Model, ReadError> {
    let mut c = Cursor::new(bytes, limits);
    let ir = document(&mut c)?;
    c.end()?;
    let mut b = Builder::new(limits);
    print(&ir, &mut b)?;
    Ok(b.finish()?)
}

/// Adds the types of `ir` to `b` in the order the printer writes them.
fn print(ir: &Ir<'_>, b: &mut Builder) -> Result<(), ReadError> {
    let mut dropped: Vec<&str> = ir
        .interfaces
        .iter()
        .filter(|i| !i.fields.iter().any(|f| f.exposed))
        .filter_map(|i| i.name.as_deref())
        .collect();
    dropped.sort_unstable();
    let mut p = Printer {
        b,
        roots: [false; 3],
        dropped,
    };
    for root in ROOTS {
        let mut ops = ir.operations.iter().filter(|o| o.root == root).peekable();
        if ops.peek().is_none() {
            continue;
        }
        let id = p.add_type(Kind::Object, root, None)?;
        for op in ops {
            let description = text(op.description.as_ref());
            p.field(id, &op.name, &op.returns, description, &op.args)?;
        }
    }
    for e in &ir.entities {
        // An entity prints no description.
        p.object(Kind::Object, e, None)?;
    }
    for o in &ir.objects {
        p.object(Kind::Object, o, description(o))?;
    }
    for input in &ir.inputs {
        let id = p.add_type(Kind::Input, name(input), description(input))?;
        if input.fields.is_empty() {
            let string = TypeRefSpec::Named(String::from("String"));
            p.field(id, "_", &string, None, &[])?;
        }
        for f in &input.fields {
            p.field(
                id,
                &f.name,
                &f.type_ref,
                text(f.description.as_ref()),
                &f.args,
            )?;
        }
    }
    for u in &ir.unions {
        let id = p.add_type(Kind::Union, name(u), description(u))?;
        for member in &u.members {
            p.b.add_member(id, member);
        }
    }
    for i in &ir.interfaces {
        if !p.is_dropped(name(i)) {
            p.object(Kind::Interface, i, description(i))?;
        }
    }
    for s in &ir.scalars {
        p.add_type(Kind::Scalar, s, None)?;
    }
    for e in &ir.enums {
        let id = p.add_type(Kind::Enum, name(e), description(e))?;
        for value in &e.values {
            p.b.add_value(id, value, None, None)?;
        }
    }
    let root = |slot: usize| p.roots[slot].then_some(ROOTS[slot]);
    let roots = [root(0), root(1), root(2)];
    p.b.roots(roots[0], roots[1], roots[2]);
    Ok(())
}

/// The builder while the types of `ir.json` are added, with the root names
/// seen so far and the interfaces the printer drops.
struct Printer<'b, 'd> {
    b: &'b mut Builder,
    /// Whether a type called `Query`, `Mutation` or `Subscription` was added.
    roots: [bool; 3],
    /// The names of the interfaces without an exposed field, sorted by bytes.
    dropped: Vec<&'d str>,
}

impl Printer<'_, '_> {
    /// Adds a type and notes when its name is the name of a root.
    fn add_type(
        &mut self,
        kind: Kind,
        name: &str,
        description: Option<&str>,
    ) -> Result<TypeId, ReadError> {
        let id = self.b.add_type(kind, name, description)?;
        if let Some(slot) = ROOTS.iter().position(|r| *r == name) {
            self.roots[slot] = true;
        }
        Ok(id)
    }

    /// Returns whether the printer drops the interface called `name`.
    fn is_dropped(&self, name: &str) -> bool {
        self.dropped.binary_search(&name).is_ok()
    }

    /// Adds a field to the type `on` with every argument of `args`.
    fn field(
        &mut self,
        on: TypeId,
        name: &str,
        type_ref: &TypeRefSpec,
        description: Option<&str>,
        args: &[IrField<'_>],
    ) -> Result<(), ReadError> {
        let field = self
            .b
            .add_field(on, name, type_ref, description, None, None)?;
        for a in args {
            let description = text(a.description.as_ref());
            self.b
                .add_arg(field, &a.name, &a.type_ref, description, None, None)?;
        }
        Ok(())
    }

    /// Adds an entity, an object or an interface with its exposed fields,
    /// unless it exposes none.
    fn object(
        &mut self,
        kind: Kind,
        t: &IrType<'_>,
        description: Option<&str>,
    ) -> Result<(), ReadError> {
        if !t.fields.iter().any(|f| f.exposed) {
            return Ok(());
        }
        let id = self.add_type(kind, name(t), description)?;
        for f in t.fields.iter().filter(|f| f.exposed) {
            self.field(
                id,
                &f.name,
                &f.type_ref,
                text(f.description.as_ref()),
                &f.args,
            )?;
        }
        let mut kept: Vec<&str> = t
            .implements
            .iter()
            .map(AsRef::as_ref)
            .filter(|n| !self.is_dropped(n))
            .collect();
        kept.sort_by(|x, y| locale_order(x, y));
        for interface in kept {
            self.b.add_implements(id, interface);
        }
        Ok(())
    }
}

/// Returns the description a type prints.
fn description<'t>(t: &'t IrType<'_>) -> Option<&'t str> {
    text(t.description.as_ref())
}

/// Returns the name of a type the reader has checked to carry one.
fn name<'t>(t: &'t IrType<'_>) -> &'t str {
    t.name.as_deref().unwrap_or_default()
}

/// Returns a description the printer writes, which is none for the empty
/// text.
fn text<'t>(description: Option<&'t Cow<'_, str>>) -> Option<&'t str> {
    description.map(AsRef::as_ref).filter(|d| !d.is_empty())
}

/// Compares two type names the way `a.localeCompare(b)` orders them under
/// the root collation of ICU.
///
/// Inside the alphabet `[_0-9A-Za-z]` of a name the collation has two levels.
/// The primary one puts `_` below the digits and the digits below the letters
/// and gives both cases of a letter one weight, a proper prefix sorting
/// first. The tertiary one decides between names that differ in case alone,
/// the lower case letter first. A byte outside the alphabet sorts below every
/// byte in it, which only keeps the order total, because the builder refuses
/// such a name.
fn locale_order(a: &str, b: &str) -> Ordering {
    a.bytes()
        .map(primary_weight)
        .cmp(b.bytes().map(primary_weight))
        .then_with(|| {
            a.bytes()
                .map(tertiary_weight)
                .cmp(b.bytes().map(tertiary_weight))
        })
}

/// Returns the primary weight of a byte of a name, or `None` outside the
/// alphabet.
fn primary_weight(byte: u8) -> Option<u8> {
    match byte {
        b'_' => Some(0),
        b'0'..=b'9' => Some(1 + (byte - b'0')),
        b'a'..=b'z' => Some(11 + (byte - b'a')),
        b'A'..=b'Z' => Some(11 + (byte - b'A')),
        _ => None,
    }
}

/// Returns the tertiary weight of a byte of a name, which puts a lower case
/// letter before its upper case form.
fn tertiary_weight(byte: u8) -> (bool, u8) {
    (byte.is_ascii_uppercase(), byte)
}

/// Reads the top level object of `ir.json`.
fn document<'a>(c: &mut Cursor<'a>) -> Result<Ir<'a>, ReadError> {
    c.object()?;
    let mut ir = Ir::default();
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "version" => c.member("version", 1 << 0, &mut seen, version)?,
            "entities" => ir.entities = c.member("entities", 1 << 1, &mut seen, record)?,
            "objects" => ir.objects = c.member("objects", 1 << 2, &mut seen, record)?,
            "interfaces" => ir.interfaces = c.member("interfaces", 1 << 3, &mut seen, record)?,
            "unions" => ir.unions = c.member("unions", 1 << 4, &mut seen, record)?,
            "inputs" => ir.inputs = c.member("inputs", 1 << 5, &mut seen, record)?,
            "enums" => ir.enums = c.member("enums", 1 << 6, &mut seen, record)?,
            "scalars" => c.member("scalars", 1 << 7, &mut seen, |c| {
                c.list(|c| {
                    c.count_type()?;
                    ir.scalars.push(c.string()?);
                    Ok(())
                })
            })?,
            "operations" => c.member("operations", 1 << 8, &mut seen, |c| {
                c.list(|c| {
                    c.count_member()?;
                    ir.operations.push(operation(c)?);
                    Ok(())
                })
            })?,
            _ => c.skip()?,
        }
    }
    if seen != (1 << 9) - 1 {
        return Err(c.shape("an object with the nine keys of ir.json"));
    }
    Ok(ir)
}

/// Reads the `version`, which is the number 1.
fn version(c: &mut Cursor<'_>) -> Result<(), ReadError> {
    match c.next()? {
        Token::Number("1") => Ok(()),
        _ => Err(c.shape("the number 1")),
    }
}

/// Reads a record of named types, `{"Name":{..},..}`, in the order of its
/// keys.
///
/// The key is not the name: the printer reads the `name` member of each
/// value.
fn record<'a>(c: &mut Cursor<'a>) -> Result<Vec<IrType<'a>>, ReadError> {
    c.object()?;
    let mut types = Vec::new();
    let mut keys = Vec::new();
    while let Some(key) = c.key()? {
        c.count_type()?;
        c.enter_record(key.clone());
        types.push(ir_type(c)?);
        c.leave();
        keys.push(key);
    }
    keys.sort_unstable();
    if let Some(pair) = keys.windows(2).find(|w| w[0] == w[1]) {
        c.enter_record(pair[1].clone());
        return Err(c.shape("a key given once"));
    }
    Ok(types)
}

/// Reads one named type of a record.
fn ir_type<'a>(c: &mut Cursor<'a>) -> Result<IrType<'a>, ReadError> {
    c.object()?;
    let mut t = IrType::default();
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "name" => t.name = Some(c.member("name", 1 << 0, &mut seen, Cursor::string)?),
            "description" => {
                t.description =
                    c.member("description", 1 << 1, &mut seen, Cursor::string_or_null)?;
            }
            "fields" => c.member("fields", 1 << 2, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    t.fields.push(field(c, true)?);
                    Ok(())
                })
            })?,
            "implements" => c.member("implements", 1 << 3, &mut seen, |c| {
                names(c, &mut t.implements, "interfaces of a type")
            })?,
            "members" => c.member("members", 1 << 4, &mut seen, |c| {
                names(c, &mut t.members, "members of a union")
            })?,
            "values" => c.member("values", 1 << 5, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    t.values.push(c.string()?);
                    Ok(())
                })
            })?,
            _ => c.skip()?,
        }
    }
    if t.name.is_none() {
        return Err(c.shape("a type with a name"));
    }
    Ok(t)
}

/// Reads a list of type names into `out`, naming the list by `what`.
fn names<'a>(
    c: &mut Cursor<'a>,
    out: &mut Vec<Cow<'a, str>>,
    what: &'static str,
) -> Result<(), ReadError> {
    c.list_or_null(|c| {
        c.count_name(out.len(), what)?;
        out.push(c.string()?);
        Ok(())
    })
}

/// Reads a field, or an argument when `with_args` is false, whose own
/// arguments the printer never writes and the reader skips.
fn field<'a>(c: &mut Cursor<'a>, with_args: bool) -> Result<IrField<'a>, ReadError> {
    c.object()?;
    let mut name = None;
    let mut type_ref = None;
    let mut exposed = None;
    let mut description = None;
    let mut args = Vec::new();
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "name" => name = Some(c.member("name", 1 << 0, &mut seen, Cursor::string)?),
            "type" => type_ref = Some(c.member("type", 1 << 1, &mut seen, ir_type_ref)?),
            "exposed" => {
                exposed = c.member("exposed", 1 << 2, &mut seen, Cursor::boolean_or_null)?;
            }
            "description" => {
                description = c.member("description", 1 << 3, &mut seen, Cursor::string_or_null)?;
            }
            "args" if with_args => c.member("args", 1 << 4, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    args.push(field(c, false)?);
                    Ok(())
                })
            })?,
            _ => c.skip()?,
        }
    }
    Ok(IrField {
        name: name.ok_or_else(|| c.shape("a field with a name"))?,
        type_ref: type_ref.ok_or_else(|| c.shape("a field with a type"))?,
        exposed: exposed == Some(true),
        description,
        args,
    })
}

/// Reads an operation of a root.
fn operation<'a>(c: &mut Cursor<'a>) -> Result<IrOperation<'a>, ReadError> {
    c.object()?;
    let mut root = None;
    let mut name = None;
    let mut args = Vec::new();
    let mut returns = None;
    let mut description = None;
    let mut seen = 0;
    while let Some(key) = c.key()? {
        match key.as_ref() {
            "root" => root = Some(c.member("root", 1 << 0, &mut seen, Cursor::string)?),
            "name" => name = Some(c.member("name", 1 << 1, &mut seen, Cursor::string)?),
            "args" => c.member("args", 1 << 2, &mut seen, |c| {
                c.list_or_null(|c| {
                    c.count_member()?;
                    args.push(field(c, false)?);
                    Ok(())
                })
            })?,
            "returns" => returns = Some(c.member("returns", 1 << 3, &mut seen, ir_type_ref)?),
            "description" => {
                description = c.member("description", 1 << 4, &mut seen, Cursor::string_or_null)?;
            }
            _ => c.skip()?,
        }
    }
    Ok(IrOperation {
        root: root.ok_or_else(|| c.shape("an operation with a root"))?,
        name: name.ok_or_else(|| c.shape("an operation with a name"))?,
        args,
        returns: returns.ok_or_else(|| c.shape("an operation with a type"))?,
        description,
    })
}

/// One object of a type reference while the reader is inside it.
#[derive(Debug, Default)]
struct RefLevel<'a> {
    kind: Option<Cow<'a, str>>,
    name: Option<Cow<'a, str>>,
    nullable: Option<bool>,
    of: Option<TypeRefSpec>,
    seen: u32,
}

/// Reads a type reference, `{"kind":..,"name":..,"nullable":..}` or
/// `{"kind":"list","of":..,"nullable":..}`.
///
/// The objects nest as deep as the lists do, and the parser caps that
/// nesting at [`ReadLimits::max_depth`], so the reader holds one level per
/// open object and needs no recursion.
fn ir_type_ref(c: &mut Cursor<'_>) -> Result<TypeRefSpec, ReadError> {
    c.object()?;
    let mut levels = alloc::vec![RefLevel::default()];
    loop {
        let Some(level) = levels.last_mut() else {
            return Err(c.shape("a type reference"));
        };
        if let Some(key) = c.key()? {
            match key.as_ref() {
                "kind" => {
                    level.kind = Some(c.member("kind", 1 << 0, &mut level.seen, Cursor::string)?);
                }
                "name" => {
                    level.name = Some(c.member("name", 1 << 1, &mut level.seen, Cursor::string)?);
                }
                "nullable" => {
                    level.nullable =
                        c.member("nullable", 1 << 2, &mut level.seen, Cursor::boolean_or_null)?;
                }
                "of" => {
                    c.enter("of");
                    if level.seen & 1 << 3 != 0 {
                        return Err(c.shape("a key given once"));
                    }
                    level.seen |= 1 << 3;
                    c.object()?;
                    levels.push(RefLevel::default());
                }
                _ => c.skip()?,
            }
            continue;
        }
        let level = levels.pop().unwrap_or_default();
        let spec = ref_level(c, level)?;
        match levels.last_mut() {
            Some(outer) => {
                c.leave();
                outer.of = Some(spec);
            }
            None => return Ok(spec),
        }
    }
}

/// Returns the reference one finished object of a type reference states.
///
/// A list wraps the reference of `of`, a scalar or a reference is its name,
/// and the reference is never null unless `nullable` is `true`, as
/// `renderType` writes it.
fn ref_level(c: &Cursor<'_>, level: RefLevel<'_>) -> Result<TypeRefSpec, ReadError> {
    let base = match level.kind.as_deref() {
        Some("list") => match level.of {
            Some(inner) => TypeRefSpec::List(Box::new(inner)),
            None => return Err(c.shape("a list with an of")),
        },
        Some("scalar" | "ref") => match level.name {
            Some(name) => TypeRefSpec::Named(String::from(name)),
            None => return Err(c.shape("a named type with a name")),
        },
        _ => return Err(c.shape("the kind of a type reference")),
    };
    Ok(if level.nullable == Some(true) {
        base
    } else {
        TypeRefSpec::NonNull(Box::new(base))
    })
}

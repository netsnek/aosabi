// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-ir: the SDL printer, the projection of an IR to the GraphQL schema
 * text that an Aosabi build writes.
 *
 * Derived from Pylon 2.9.5 by Nico Schett, packages/pylon/src/ir/sdl.ts,
 * Apache-2.0. Changed by the Aosabi authors.
 */

//! The printer of the intermediate representation.
//!
//! The reference renders a schema through one hand written printer and never
//! through `printSchema` of graphql-js, so the bytes of `schema.graphql` are
//! the bytes this module writes. The port keeps one function per function of
//! `sdl.ts`, in the order of that file, and every string literal is the one of
//! the reference.
//!
//! # JavaScript semantics the port reproduces
//!
//! - `Object.values` lists the values of a record in `OrdinaryOwnPropertyKeys`
//!   order, which is [`Record::values`].
//! - `!description` is true for `undefined` and for the empty string and for
//!   no other string.
//! - `x ?? []` takes the right side for `undefined` and `null` alone, which is
//!   the `None` of the model.
//! - `Array.prototype.sort` is stable and reads the sign of the comparator
//!   alone, which is the stable `sort_by` of a slice.
//! - `Set.prototype.has` compares strings by their code units, which is the
//!   equality of two well formed Rust strings.
//! - `Array.prototype.join` and a template literal concatenate strings and
//!   change no unit of them, so the printer indexes no string and no position
//!   arises in it. The one position of this crate is
//!   [`CollateError::offset`], a UTF-16 code unit offset.
//!
//! The judging notebook is `tests/notebooks/15_sdl_printer.ipynb`, which
//! drives `aosabi-devtool print-sdl` against the reference over the cases of
//! `tests/capture/15_corpus/`.
//!
//! [enumerable]: https://tc39.es/ecma262/#sec-enumerableownproperties
//! [to-boolean]: https://tc39.es/ecma262/#sec-toboolean
//! [coalesce]: https://tc39.es/ecma262/#sec-binary-logical-operators
//! [sort]: https://tc39.es/ecma262/#sec-array.prototype.sort
//! [same-value-zero]: https://tc39.es/ecma262/#sec-samevaluezero
//! [`Record::values`]: crate::record::Record::values

use alloc::{
    format,
    string::String,
    vec::Vec, //
};

use aosabi_json::{
    map::IndexSet,
    value::KeyHasherBuilder, //
};

use crate::{
    collate::{
        CollateError,
        check_name,
        compare, //
    },
    ir::{
        Field,
        InterfaceType,
        Operation,
        RootName,
        SchemaIr,
        TypeRef, //
    }, //
};

/// The names of the interfaces the printer drops, the `Set<string>`.
struct Dropped {
    /// The names in insertion order, as a JavaScript `Set` keeps them.
    names: IndexSet<String, KeyHasherBuilder>,
}

impl Dropped {
    /// Returns whether the set holds `name`, which is `Set.prototype.has`.
    fn has(&self, name: &str) -> bool {
        self.names.contains(name)
    }
}

/// The options of [`render_object_type`], the object literal type.
#[derive(Clone, Copy)]
struct ObjectOpts<'a> {
    /// The interface names the type implements.
    implements: Option<&'a [String]>,
    /// The description of the type.
    description: Option<&'a str>,
}

/// Renders a normalized type reference as GraphQL type syntax.
#[must_use]
pub fn render_type(t: &TypeRef) -> String {
    let bang = if t.nullable() { "" } else { "!" };
    match t {
        TypeRef::List { of, .. } => format!("[{}]{bang}", render_type(of)),
        TypeRef::Scalar { name, .. } | TypeRef::Ref { name, .. } => format!("{name}{bang}"),
    }
}

/// Renders a GraphQL description block, with no `indent` at header level and
/// two spaces at field level.
///
/// The text is inserted verbatim, with no escaping and no indentation of its
/// further lines, which is quirk `Q14_DESCRIPTION_VERBATIM`. `None` and the
/// empty string both render nothing, which is the `!description` test of :17.
fn render_description(description: Option<&str>, indent: &str) -> String {
    let Some(description) = description.filter(|text| !text.is_empty()) else {
        return String::new();
    };
    format!("{indent}\"\"\"\n{indent}{description}\n{indent}\"\"\"\n")
}

/// Renders one field line with its description block above it.
fn render_field(f: &Field) -> String {
    format!(
        "{}  {}{}: {}",
        render_description(f.description.as_deref(), "  "),
        f.name,
        render_args(f.args.as_deref().unwrap_or_default()),
        render_type(&f.r#type)
    )
}

/// Renders an argument list on one line, or nothing for no argument.
///
/// An argument renders its name and its type and nothing else, so a default
/// value and a description of an argument never reach the schema.
fn render_args(args: &[Field]) -> String {
    if args.is_empty() {
        return String::new();
    }
    let inner = args
        .iter()
        .map(|a| format!("{}: {}", a.name, render_type(&a.r#type)))
        .collect::<Vec<_>>()
        .join(", ");
    format!("({inner})")
}

/// Renders one field line of a root type with its description block above
/// it.
fn render_operation(op: &Operation) -> String {
    format!(
        "{}  {}{}: {}",
        render_description(op.description.as_deref(), "  "),
        op.name,
        render_args(&op.args),
        render_type(&op.returns)
    )
}

/// Renders the `implements` clause of a type, without the dropped interfaces
/// and sorted by the collation.
///
/// # Errors
///
/// Returns a [`CollateError`] when two or more names are kept and one of
/// them leaves the collation alphabet.
fn render_implements(r#impl: Option<&[String]>, dropped: &Dropped) -> Result<String, CollateError> {
    let kept: Vec<&str> = r#impl
        .unwrap_or_default()
        .iter()
        .map(String::as_str)
        .filter(|n| !dropped.has(n))
        .collect();
    if kept.is_empty() {
        return Ok(String::new());
    }
    let sorted = sort(kept)?;
    Ok(format!(" implements {}", sorted.join(" & ")))
}

/// The reference calls its comparator only when the list holds two or more
/// names, so a list of one name is returned unchecked. Every name of a longer
/// list takes part in at least one comparison of any comparison sort, so
/// checking each name first refuses exactly the lists the comparator would
/// refuse. It also names the first such name in list order, whatever order
/// the sort compares in, and it leaves the sort a comparator that cannot
/// fail.
///
/// # Errors
///
/// Returns the [`CollateError`] of the first name outside the collation
/// alphabet.
fn sort(mut names: Vec<&str>) -> Result<Vec<&str>, CollateError> {
    if names.len() < 2 {
        return Ok(names);
    }
    for name in &names {
        check_name(name)?;
    }
    names.sort_by(|a, b| compare(a, b));
    Ok(names)
}

/// Renders an interface block.
///
/// # Errors
///
/// Returns the [`CollateError`] of [`render_implements`].
fn render_interface(i: &InterfaceType, dropped: &Dropped) -> Result<String, CollateError> {
    let fields: Vec<String> = i
        .fields
        .iter()
        .filter(|f| f.exposed)
        .map(render_field)
        .collect();
    Ok(format!(
        "{}interface {}{} {{\n{}\n}}",
        render_description(i.description.as_deref(), ""),
        i.name,
        render_implements(i.implements.as_deref(), dropped)?,
        fields.join("\n")
    ))
}

/// Renders an object type block, for an entity and for a plain object alike.
///
/// # Errors
///
/// Returns the [`CollateError`] of [`render_implements`].
fn render_object_type(
    name: &str,
    fields: &[Field],
    opts: ObjectOpts<'_>,
    dropped: &Dropped,
) -> Result<String, CollateError> {
    let exposed: Vec<String> = fields
        .iter()
        .filter(|f| f.exposed)
        .map(render_field)
        .collect();
    Ok(format!(
        "{}type {}{} {{\n{}\n}}",
        render_description(opts.description, ""),
        name,
        render_implements(opts.implements, dropped)?,
        exposed.join("\n")
    ))
}

/// Projects an IR to a GraphQL SDL string.
///
/// The blocks stand in the order root types, entities, objects, inputs,
/// unions, interfaces, scalars and enums, they are joined by one blank line,
/// and the text ends in one newline. Every block reads the `name` member of
/// its value and never the key of the record.
///
/// # Errors
///
/// Returns a [`CollateError`] when an `implements` list keeps two or more
/// names and one of them leaves the alphabet `[_0-9A-Za-z]`. The reference has
/// no such refusal. Aosabi refuses at build time because the order of such a
/// list is not reproduced.
pub fn to_sdl(ir: &SchemaIr) -> Result<String, CollateError> {
    let mut blocks: Vec<String> = Vec::new();

    // A GraphQL interface declares at least one field, so an interface with
    // no exposed field is invalid SDL. It is dropped here and stripped from
    // every `implements` clause.
    let dropped = Dropped {
        names: ir
            .interfaces
            .values()
            .filter(|i| i.fields.iter().filter(|f| f.exposed).count() == 0)
            .map(|i| i.name.clone())
            .collect(),
    };

    for root in RootName::ALL {
        let ops: Vec<String> = ir
            .operations
            .iter()
            .filter(|o| o.root == root)
            .map(render_operation)
            .collect();
        if ops.is_empty() {
            continue;
        }
        blocks.push(format!("type {} {{\n{}\n}}", root.as_str(), ops.join("\n")));
    }

    for e in ir.entities.values() {
        if e.fields.iter().filter(|f| f.exposed).count() == 0 {
            continue;
        }
        let opts = ObjectOpts {
            implements: Some(&e.implements),
            description: None,
        };
        blocks.push(render_object_type(&e.name, &e.fields, opts, &dropped)?);
    }

    for o in ir.objects.values() {
        if o.fields.iter().filter(|f| f.exposed).count() == 0 {
            continue;
        }
        let opts = ObjectOpts {
            implements: o.implements.as_deref(),
            description: o.description.as_deref(),
        };
        blocks.push(render_object_type(&o.name, &o.fields, opts, &dropped)?);
    }

    // GraphQL requires at least one field of an input object, so an input
    // with none renders a placeholder, quirk `Q08_EMPTY_INPUT_PLACEHOLDER`.
    for input in ir.inputs.values() {
        let fields = if input.fields.is_empty() {
            String::from("  _: String")
        } else {
            let lines: Vec<String> = input.fields.iter().map(render_field).collect();
            lines.join("\n")
        };
        blocks.push(format!(
            "{}input {} {{\n{}\n}}",
            render_description(input.description.as_deref(), ""),
            input.name,
            fields
        ));
    }

    for u in ir.unions.values() {
        blocks.push(format!(
            "{}union {} = {}",
            render_description(u.description.as_deref(), ""),
            u.name,
            u.members.join(" | ")
        ));
    }

    for i in ir.interfaces.values() {
        if dropped.has(&i.name) {
            continue;
        }
        blocks.push(render_interface(i, &dropped)?);
    }

    for s in &ir.scalars {
        blocks.push(format!("scalar {s}"));
    }

    for en in ir.enums.values() {
        let values: Vec<String> = en.values.iter().map(|v| format!("  {v}")).collect();
        blocks.push(format!(
            "{}enum {} {{\n{}\n}}",
            render_description(en.description.as_deref(), ""),
            en.name,
            values.join("\n")
        ));
    }

    Ok(blocks.join("\n\n") + "\n")
}

// SPDX-License-Identifier: MIT
/*
 * aoviewer-docs input sdl: the SDL path from a schema file into the model.
 *
 * The file is parsed by aosabi-lang, built by aosabi-type with the SDL rules of
 * aosabi-validate and read through the introspection accessors.
 */

//! The SDL input.
//!
//! [`read`] builds the schema of an SDL file as graphql-js builds it and fills
//! a [`Builder`] with the answers the introspection accessors give, so the
//! model is the one an introspection result of the same schema reads into: the
//! types in the order of the type map, the introspection types among them,
//! every field, argument, input field and enum value with its deprecation, and
//! the directives in the order the schema lists them. The deprecation of a
//! directive argument is dropped, because [`Builder::add_directive_arg`] takes
//! none, as the introspection reader of the core drops it.
//!
//! The builder is handed back unfinished with the outline of the types that
//! are no `__` type, so the build path checks the schema against `ir.json` and
//! sets the argument descriptions before the model is finished.
//!
//! Neither the parser of `aosabi-lang` nor the schema builder of `aosabi-type`
//! caps what it reads, so the file is bounded before the schema is built, each
//! bound refused with its reason. The file holds at most
//! [`ReadLimits::max_input_bytes`] past a byte order mark, which the lexer
//! passes over, and is UTF-8. Its brackets nest at most
//! [`ReadLimits::max_depth`] deep, because the parser descends once per bracket
//! and a deep enough nesting overflows the stack, which no `Result` reports.
//! The parser stops after [`TOKENS_PER_ELEMENT`] tokens for each type and
//! member the limits admit. The parsed document defines at most
//! [`ReadLimits::max_types`] types and as many directives, at most
//! [`ReadLimits::max_members`] fields, arguments, input fields and enum values
//! together, and names at most [`ReadLimits::max_types`] interfaces or union
//! members for one type, because the build grows faster than the count of
//! types. The model builder holds the same limits again as it fills.
//!
//! The schema builder coerces each default value as it builds the fields of a
//! type, and an object of an input type in one makes it build the fields of
//! that type and coerce their default values first, then copy them in. A
//! default value that leads back to its own input type can so recurse without
//! end, a long chain of input types exhausts the stack, and the copies and the
//! wrappers of list types can exhaust the memory, none of which a `Result`
//! reports. So [`check_defaults`] walks every default value with its type
//! before the build and refuses a cycle, a coercion past
//! [`ReadLimits::max_depth`] levels and default values that together hold more
//! values than the parser reads tokens.
//!
//! Every loop of the scan reads one byte per iteration of a file that
//! [`ReadLimits::max_input_bytes`] caps, and the count runs once over the
//! definitions and members of the parsed document. The check of the default
//! values visits each value of a literal once, each input type once in a
//! search on a stack of its own and each wrapper of a type reference once,
//! which the nesting bound caps. Every other loop runs over a list of the
//! built schema, whose counts the builder caps as they grow, or over the
//! wrappers of one type reference, which [`ReadLimits::max_depth`] caps.

use std::path::Path;

use aosabi_lang::{
    ParseOptions,
    ast::{
        DefinitionNode,
        DocumentNode,
        FieldDefinitionNode,
        InputValueDefinitionNode,
        NullableTypeNode,
        TypeNode,
        ValueNode, //
    },
    js_string::to_string_lossy,
    location::get_location, //
};
use aosabi_type::introspection::{
    directive_args,
    directive_description,
    directive_is_repeatable,
    directive_locations,
    directive_name,
    enum_value_deprecation_reason,
    enum_value_description,
    enum_value_name,
    field_args,
    field_deprecation_reason,
    field_description,
    field_name,
    field_type,
    input_value_default_value,
    input_value_deprecation_reason,
    input_value_description,
    input_value_name,
    input_value_type,
    schema_description,
    schema_directives,
    schema_mutation_type,
    schema_query_type,
    schema_subscription_type,
    schema_types,
    type_description,
    type_enum_values,
    type_fields,
    type_input_fields,
    type_interfaces,
    type_kind,
    type_name,
    type_of_type,
    type_possible_types,
    type_specified_by_url, //
};
use aosabi_type::scalars::is_specified_scalar_type;
use aosabi_type::{
    BuildSchemaOptions,
    DefinitionError,
    Field,
    InputValue,
    NamedType,
    Schema,
    Type,
    TypeKind, //
};
use aoviewer_core::{
    limits::ReadLimits,
    model::{
        ArgId,
        Builder,
        Deprecation,
        Kind,
        ModelError,
        TypeId,
        TypeRefSpec, //
    },
};

use super::{
    BOM,
    Read, //
};
use crate::error::{
    DocsError,
    Position, //
};

/// The tokens the parser reads at most for each type and member the read
/// limits admit.
///
/// The SDL files of `tests/fixtures/viewer/corpus.toml` and the two synthetic
/// schemas of `tests/gen/synth_schema.py` spend at most 4.09 tokens per type,
/// field, argument, input field, enum value and directive of their model,
/// `synthetic-5000` the most, so the bound leaves about twice that.
const TOKENS_PER_ELEMENT: u32 = 8;

/// A schema read through the SDL path, before its model is finished.
#[derive(Debug)]
pub(super) struct Sdl {
    /// The builder, filled with the whole schema.
    pub(super) builder: Builder,
    /// The types that are no `__` type, in schema order.
    pub(super) outline: Vec<OutlineType>,
}

/// A type of the outline.
#[derive(Debug)]
pub(super) struct OutlineType {
    /// The name.
    pub(super) name: String,
    /// The kind.
    pub(super) kind: Kind,
    /// Whether the type is one of the five scalars of the specification,
    /// which graphql-js adds to a schema that names one without declaring
    /// it.
    pub(super) specified: bool,
    /// The fields of an object or an interface, or the input fields of an
    /// input type, in schema order.
    pub(super) fields: Vec<OutlineField>,
}

/// A field or an input field of the outline.
#[derive(Debug)]
pub(super) struct OutlineField {
    /// The name.
    pub(super) name: String,
    /// The type expression.
    pub(super) type_ref: TypeRefSpec,
    /// The arguments of a field of an object or an interface, in schema
    /// order.
    pub(super) args: Vec<OutlineArg>,
}

/// An argument of a field of the outline.
#[derive(Debug)]
pub(super) struct OutlineArg {
    /// The name.
    pub(super) name: String,
    /// The id the builder gave the argument.
    pub(super) id: ArgId,
    /// The type expression.
    pub(super) type_ref: TypeRefSpec,
}

/// Reads the SDL file `path`, of which the loader read `read`, into a
/// builder under `limits`.
///
/// # Errors
///
/// Returns [`DocsError::Sdl`] for a file longer than
/// [`ReadLimits::max_input_bytes`], whether its length or its bytes say so,
/// bytes that are no UTF-8, brackets nested deeper than
/// [`ReadLimits::max_depth`], default values that [`check_defaults`]
/// refuses, and what the parser or the schema builder refuses, the parser's
/// bound on tokens among it, and [`DocsError::Model`] for a count past the
/// limits and for what the model builder refuses.
pub(super) fn read(path: &Path, read: &Read, limits: &ReadLimits) -> Result<Sdl, DocsError> {
    let refuse = |position, message| DocsError::Sdl {
        path: path.to_path_buf(),
        position,
        message,
    };
    let too_long = || {
        let message = format!(
            "the file is longer than the read limit of {} bytes",
            limits.max_input_bytes
        );
        refuse(None, message)
    };
    let bytes = match read {
        Read::Bytes(bytes) => bytes.as_slice(),
        Read::TooLong(_) => return Err(too_long()),
    };
    let body = bytes.strip_prefix(BOM).unwrap_or(bytes);
    if body.len() > limits.max_input_bytes as usize {
        return Err(too_long());
    }
    let text = match std::str::from_utf8(bytes) {
        Ok(text) => text,
        Err(e) => {
            let at = position_of(bytes, e.valid_up_to());
            return Err(refuse(Some(at), String::from("the file is not UTF-8")));
        }
    };
    if let Some(at) = too_deep(bytes, usize::from(limits.max_depth)) {
        let message = format!(
            "the brackets nest deeper than the read limit of {}",
            limits.max_depth
        );
        return Err(refuse(Some(position_of(bytes, at)), message));
    }
    let options = ParseOptions {
        max_tokens: Some(f64::from(max_tokens(limits))),
        ..ParseOptions::default()
    };
    let document = aosabi_lang::parse(text, options).map_err(|e| {
        let e = DefinitionError::from(e);
        definition(path, &e)
    })?;
    check_counts(&document, limits)?;
    check_defaults(path, &document, limits)?;
    let schema = aosabi_validate::build_ast_schema(&document, BuildSchemaOptions::default())
        .map_err(|e| definition(path, &e))?;
    fill(path, &schema, limits)
}

/// Returns the most tokens the parser reads under `limits`.
fn max_tokens(limits: &ReadLimits) -> u32 {
    limits
        .max_types
        .saturating_add(limits.max_members)
        .saturating_mul(TOKENS_PER_ELEMENT)
}

/// Refuses a document that defines more types, directives or members than
/// `limits` admit, or names more interfaces or union members for one type.
///
/// A type extension adds members and no type, and the interfaces and union
/// members of a type are summed by its name over its definition and its
/// extensions, as the schema builder merges them.
fn check_counts(document: &DocumentNode, limits: &ReadLimits) -> Result<(), ModelError> {
    let (mut types, mut directives, mut members) = (0u64, 0u64, 0u64);
    let mut lists: Vec<(&str, u64)> = Vec::new();
    for d in &document.definitions {
        let (t, m, list) = match d {
            DefinitionNode::ObjectTypeDefinition(n) => (
                1,
                fields(n.fields.as_ref()),
                Some((&n.name, count(n.interfaces.as_ref()))),
            ),
            DefinitionNode::ObjectTypeExtension(n) => (
                0,
                fields(n.fields.as_ref()),
                Some((&n.name, count(n.interfaces.as_ref()))),
            ),
            DefinitionNode::InterfaceTypeDefinition(n) => (
                1,
                fields(n.fields.as_ref()),
                Some((&n.name, count(n.interfaces.as_ref()))),
            ),
            DefinitionNode::InterfaceTypeExtension(n) => (
                0,
                fields(n.fields.as_ref()),
                Some((&n.name, count(n.interfaces.as_ref()))),
            ),
            DefinitionNode::InputObjectTypeDefinition(n) => (1, count(n.fields.as_ref()), None),
            DefinitionNode::InputObjectTypeExtension(n) => (0, count(n.fields.as_ref()), None),
            DefinitionNode::EnumTypeDefinition(n) => (1, count(n.values.as_ref()), None),
            DefinitionNode::EnumTypeExtension(n) => (0, count(n.values.as_ref()), None),
            DefinitionNode::UnionTypeDefinition(n) => {
                (1, 0, Some((&n.name, count(n.types.as_ref()))))
            }
            DefinitionNode::UnionTypeExtension(n) => {
                (0, 0, Some((&n.name, count(n.types.as_ref()))))
            }
            DefinitionNode::ScalarTypeDefinition(_) => (1, 0, None),
            DefinitionNode::DirectiveDefinition(n) => {
                directives += 1;
                (0, count(n.arguments.as_ref()), None)
            }
            _ => (0, 0, None),
        };
        types += t;
        members += m;
        if let Some((name, len)) = list {
            lists.push((name.value(), len));
        }
    }
    let max_types = u64::from(limits.max_types);
    let limit = |what| ModelError::Limit {
        what,
        limit: max_types,
    };
    if types > max_types {
        return Err(limit("types"));
    }
    if directives > max_types {
        return Err(limit("directives"));
    }
    if longest_list(lists) > max_types {
        return Err(limit("interfaces or union members of one type"));
    }
    if members > u64::from(limits.max_members) {
        return Err(ModelError::Limit {
            what: "fields, arguments and values",
            limit: u64::from(limits.max_members),
        });
    }
    Ok(())
}

/// Returns the longest sum of the list lengths `lists` holds for one name.
fn longest_list(mut lists: Vec<(&str, u64)>) -> u64 {
    lists.sort_unstable_by(|a, b| a.0.as_bytes().cmp(b.0.as_bytes()));
    lists
        .chunk_by(|a, b| a.0 == b.0)
        .map(|run| run.iter().map(|&(_, len)| len).sum())
        .max()
        .unwrap_or(0)
}

/// Returns the count of the fields `list` defines with their arguments.
fn fields(list: Option<&Vec<FieldDefinitionNode>>) -> u64 {
    list.map_or(0, |list| {
        list.iter().map(|f| 1 + count(f.arguments.as_ref())).sum()
    })
}

/// Returns the length of `list` as a count of members.
fn count<T>(list: Option<&Vec<T>>) -> u64 {
    list.map_or(0, |list| list.len() as u64)
}

/// Refuses the default values whose coercion the schema builder might not
/// end or might not bound, before it coerces them.
///
/// The builder coerces the default value of an input field or an argument as it
/// builds the fields of the type or the directive that holds it. An object of
/// an input type in a default value makes it build the fields of that type
/// first, coercing their default values, and copy each of them into the object
/// where the object names no value. A value that is no list is wrapped once per
/// list of its type. None of it is capped, so the check refuses with its
/// position a default value whose objects lead back to the input type it
/// belongs to, a default value whose coercion reaches more than
/// [`ReadLimits::max_depth`] levels, and default values that hold more values
/// together than the parser reads tokens. The rule on cycles is conservative,
/// since it reads the shape of a value and not its validity. It also refuses an
/// invalid default value that the builder abandons before it reaches the cycle,
/// such as an object that lacks a non null field its type declares ahead of the
/// field that leads back. Each list and object is one level, and the levels of
/// the default values the builder coerces on the way count toward it, since its
/// recursion runs through them. Every value of a literal is at least one token,
/// so only default values that grow as they are filled in reach the bound on
/// values. The counts are upper bounds, since every default value of an input
/// type counts as copied into each of its objects.
fn check_defaults(
    path: &Path,
    document: &DocumentNode,
    limits: &ReadLimits,
) -> Result<(), DocsError> {
    let mut defaults = Defaults::new(path, document, limits);
    let reaches = defaults.input_types()?;
    for d in &document.definitions {
        let (owner, fields) = match d {
            DefinitionNode::ObjectTypeDefinition(n) => (n.name.value(), n.fields.as_deref()),
            DefinitionNode::ObjectTypeExtension(n) => (n.name.value(), n.fields.as_deref()),
            DefinitionNode::InterfaceTypeDefinition(n) => (n.name.value(), n.fields.as_deref()),
            DefinitionNode::InterfaceTypeExtension(n) => (n.name.value(), n.fields.as_deref()),
            DefinitionNode::DirectiveDefinition(n) => {
                for a in n.arguments.as_deref().unwrap_or_default() {
                    defaults.argument(a, &reaches, || {
                        format!("@{}({}:)", n.name.value(), a.name.value())
                    })?;
                }
                continue;
            }
            _ => continue,
        };
        for f in fields.unwrap_or_default() {
            for a in f.arguments.as_deref().unwrap_or_default() {
                defaults.argument(a, &reaches, || {
                    format!("{owner}.{}({}:)", f.name.value(), a.name.value())
                })?;
            }
        }
    }
    Ok(())
}

/// The input types of a document with their input fields, and the measure
/// of the default values checked so far.
struct Defaults<'a> {
    /// The file the document was read from.
    path: &'a Path,
    /// The names of the input types, sorted.
    names: Vec<&'a str>,
    /// The input fields of each input type over its definition and its
    /// extensions, sorted by name.
    fields: Vec<Vec<InputField<'a>>>,
    /// The levels a coercion may reach.
    max_depth: u64,
    /// The values all coerced default values may hold together.
    budget: u64,
    /// The values the default values checked so far hold.
    total: u64,
}

/// An input field with the shape of its type.
struct InputField<'a> {
    /// The definition.
    node: &'a InputValueDefinitionNode,
    /// The list wrappers of its type.
    lists: u64,
    /// The input type its type names, if the document defines one.
    named: Option<usize>,
}

/// What a coercion reaches.
#[derive(Clone, Copy, Debug, Default)]
struct Reach {
    /// The deepest level, each list and object one level deeper.
    depth: u64,
    /// The values, each list, object and leaf one.
    values: u64,
}

/// What a default value reaches before the default values of the input
/// types it meets are filled in.
#[derive(Debug, Default)]
struct Walk {
    /// The reach of the value itself.
    reach: Reach,
    /// The input types its objects stand at, each with the level of the
    /// object.
    met: Vec<(usize, u64)>,
}

impl Walk {
    /// Takes `values` values whose deepest stands at `level`.
    fn take(&mut self, level: u64, values: u64) {
        self.reach.depth = self.reach.depth.max(level);
        self.reach.values = self.reach.values.saturating_add(values);
    }

    /// Returns the reach once the default values of the input types it
    /// meets, whose reaches `reaches` holds, are filled in.
    fn filled(&self, reaches: &[Reach]) -> Reach {
        self.met.iter().fold(self.reach, |r, &(t, level)| Reach {
            depth: r.depth.max(level.saturating_add(reaches[t].depth)),
            values: r.values.saturating_add(reaches[t].values),
        })
    }
}

/// The state of an input type in the search for cycles.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mark {
    /// Not reached yet.
    Open,
    /// Its default values are being measured.
    Busy,
    /// Measured.
    Done,
}

impl<'a> Defaults<'a> {
    /// Indexes the input types of `document` with their input fields.
    fn new(path: &'a Path, document: &'a DocumentNode, limits: &ReadLimits) -> Self {
        let mut parts: Vec<(&str, &[InputValueDefinitionNode])> = document
            .definitions
            .iter()
            .filter_map(|d| match d {
                DefinitionNode::InputObjectTypeDefinition(n) => {
                    Some((n.name.value(), n.fields.as_deref().unwrap_or_default()))
                }
                DefinitionNode::InputObjectTypeExtension(n) => {
                    Some((n.name.value(), n.fields.as_deref().unwrap_or_default()))
                }
                _ => None,
            })
            .collect();
        parts.sort_by(|a, b| a.0.cmp(b.0));
        let mut names: Vec<&str> = parts.iter().map(|&(name, _)| name).collect();
        names.dedup();
        let mut defaults = Self {
            path,
            names,
            fields: Vec::new(),
            max_depth: u64::from(limits.max_depth),
            budget: u64::from(max_tokens(limits)),
            total: 0,
        };
        let mut fields: Vec<Vec<InputField<'a>>> =
            defaults.names.iter().map(|_| Vec::new()).collect();
        for (name, nodes) in parts {
            if let Some(t) = defaults.input_type(name) {
                fields[t].extend(nodes.iter().map(|node| defaults.input_field(node)));
            }
        }
        for list in &mut fields {
            list.sort_by(|a, b| a.node.name.value().cmp(b.node.name.value()));
        }
        defaults.fields = fields;
        defaults
    }

    /// Returns the index of the input type `name`, if the document defines
    /// one.
    fn input_type(&self, name: &str) -> Option<usize> {
        self.names.binary_search(&name).ok()
    }

    /// Returns the input field of the definition `node`.
    fn input_field(&self, node: &'a InputValueDefinitionNode) -> InputField<'a> {
        let (lists, name) = shape(&node.r#type);
        InputField {
            node,
            lists,
            named: self.input_type(name),
        }
    }

    /// Returns the input field `name` of the input type `t`.
    ///
    /// A name defined twice for one input type is refused by the SDL rules
    /// before the builder coerces a default value, so the first stands for it.
    fn field(&self, t: usize, name: &str) -> Option<&InputField<'a>> {
        let fields = &self.fields[t];
        let i = fields.partition_point(|f| f.node.name.value() < name);
        fields.get(i).filter(|f| f.node.name.value() == name)
    }

    /// Measures the default values of every input type and returns what
    /// those of each input type reach together.
    ///
    /// An input type is measured after the input types its default values
    /// meet, on a stack of the search's own, so no chain of input types
    /// deepens the stack of the process. An input type met while its own
    /// default values are being measured closes a cycle.
    fn input_types(&mut self) -> Result<Vec<Reach>, DocsError> {
        let walks: Vec<Vec<(usize, Walk)>> = self
            .fields
            .iter()
            .map(|fields| {
                fields
                    .iter()
                    .enumerate()
                    .filter_map(|(i, f)| {
                        let value = f.node.default_value.as_ref()?;
                        Some((i, self.walk(value, f.lists, f.named)))
                    })
                    .collect()
            })
            .collect();
        let mut marks = vec![Mark::Open; walks.len()];
        let mut reaches = vec![Reach::default(); walks.len()];
        let mut stack: Vec<(usize, usize, usize)> = Vec::new();
        for root in 0..walks.len() {
            if marks[root] != Mark::Open {
                continue;
            }
            marks[root] = Mark::Busy;
            stack.push((root, 0, 0));
            while let Some(top) = stack.last_mut() {
                let (t, w, m) = *top;
                let Some((field, walk)) = walks[t].get(w) else {
                    stack.pop();
                    reaches[t] = self.measure(t, &walks[t], &reaches)?;
                    marks[t] = Mark::Done;
                    continue;
                };
                let Some(&(u, _)) = walk.met.get(m) else {
                    *top = (t, w + 1, 0);
                    continue;
                };
                top.2 = m + 1;
                match marks[u] {
                    Mark::Busy => return Err(self.cycle(t, *field, u)),
                    Mark::Open => {
                        marks[u] = Mark::Busy;
                        stack.push((u, 0, 0));
                    }
                    Mark::Done => {}
                }
            }
        }
        Ok(reaches)
    }

    /// Measures the default values `walks` of the input type `t`, whose
    /// input types `reaches` holds measured, and returns what they reach
    /// together.
    fn measure(
        &mut self,
        t: usize,
        walks: &[(usize, Walk)],
        reaches: &[Reach],
    ) -> Result<Reach, DocsError> {
        let owner = self.names[t];
        let mut reach = Reach::default();
        for (i, walk) in walks {
            let node = self.fields[t][*i].node;
            let filled = walk.filled(reaches);
            self.admit(node, filled, || format!("{owner}.{}", node.name.value()))?;
            reach.depth = reach.depth.max(filled.depth);
            reach.values = reach.values.saturating_add(filled.values);
        }
        Ok(reach)
    }

    /// Measures the default value of the argument `node` once every input
    /// type is measured in `reaches`, `owner` naming the argument.
    fn argument(
        &mut self,
        node: &InputValueDefinitionNode,
        reaches: &[Reach],
        owner: impl Fn() -> String,
    ) -> Result<(), DocsError> {
        let Some(value) = &node.default_value else {
            return Ok(());
        };
        let (lists, name) = shape(&node.r#type);
        let walk = self.walk(value, lists, self.input_type(name));
        self.admit(node, walk.filled(reaches), owner)
    }

    /// Admits the default value of `node` whose coercion reaches `reach`, or
    /// refuses it for its depth or for the values all default values hold up
    /// to it, `owner` naming the input field or the argument.
    fn admit(
        &mut self,
        node: &InputValueDefinitionNode,
        reach: Reach,
        owner: impl Fn() -> String,
    ) -> Result<(), DocsError> {
        if reach.depth > self.max_depth {
            let message = format!(
                "the default value of {} nests deeper than the read limit of {} once the default values of its input objects are filled in",
                owner(),
                self.max_depth
            );
            return Err(self.refuse(node.default_value.as_ref(), message));
        }
        self.total = self.total.saturating_add(reach.values);
        if self.total > self.budget {
            let message = format!(
                "the default values up to {} hold more than {} values once the default values of their input objects are filled in",
                owner(),
                self.budget
            );
            return Err(self.refuse(node.default_value.as_ref(), message));
        }
        Ok(())
    }

    /// Returns the refusal of the default value of the input field `field`
    /// of the input type `t`, which holds an object of the input type `u`
    /// whose default values lead back to `t`.
    fn cycle(&self, t: usize, field: usize, u: usize) -> DocsError {
        let node = self.fields[t][field].node;
        let (owner, met) = (self.names[t], self.names[u]);
        let message = if t == u {
            format!(
                "the default value of {owner}.{} holds an object of {owner} itself, so the schema builder could coerce it without end",
                node.name.value()
            )
        } else {
            format!(
                "the default value of {owner}.{} holds an object of {met}, whose default values lead back to {owner}, so the schema builder could coerce it without end",
                node.name.value()
            )
        };
        self.refuse(node.default_value.as_ref(), message)
    }

    /// Returns the refusal of the file with `message` at the default value
    /// `value`.
    fn refuse(&self, value: Option<&ValueNode>, message: String) -> DocsError {
        let position = value.and_then(ValueNode::loc).map(|l| {
            let at = get_location(&l.source, l.start);
            Position {
                line: u32::try_from(at.line).unwrap_or(u32::MAX),
                column: u32::try_from(at.column).unwrap_or(u32::MAX),
            }
        });
        DocsError::Sdl {
            path: self.path.to_path_buf(),
            position,
            message,
        }
    }

    /// Walks the default value `value` of a type of `lists` list wrappers
    /// around the input type `named` as the builder coerces it.
    ///
    /// A list stands at its item type, and a value that is no list at a list
    /// type is wrapped once per wrapper. An object at an input type meets it
    /// and reads each key at the input field of that name, skipping a key
    /// the type lacks as the builder does, and any other object or list
    /// reads no input type. A null is one value, since the builder reads it
    /// before the type.
    fn walk(&self, value: &ValueNode, lists: u64, named: Option<usize>) -> Walk {
        let mut walk = Walk::default();
        let mut todo = vec![(value, lists, named, 1u64)];
        while let Some((value, lists, named, level)) = todo.pop() {
            match value {
                ValueNode::NullValue(_) => walk.take(level, 1),
                ValueNode::ListValue(list) => {
                    walk.take(level, 1);
                    let (lists, named) = match lists.checked_sub(1) {
                        Some(lists) => (lists, named),
                        None => (0, None),
                    };
                    todo.extend(list.values.iter().map(|v| (v, lists, named, level + 1)));
                }
                _ if lists > 0 => {
                    walk.take(level + lists - 1, lists);
                    todo.push((value, 0, named, level + lists));
                }
                ValueNode::ObjectValue(object) => {
                    walk.take(level, 1);
                    let Some(t) = named else {
                        todo.extend(object.fields.iter().map(|k| (&k.value, 0, None, level + 1)));
                        continue;
                    };
                    walk.met.push((t, level));
                    for key in &object.fields {
                        if let Some(f) = self.field(t, key.name.value()) {
                            todo.push((&key.value, f.lists, f.named, level + 1));
                        }
                    }
                }
                _ => walk.take(level, 1),
            }
        }
        walk
    }
}

/// Returns the list wrappers of the type `ty` and the name of the type they
/// wrap.
fn shape(ty: &TypeNode) -> (u64, &str) {
    let mut lists = 0;
    let mut cur = ty;
    loop {
        let inner = match cur {
            TypeNode::NamedType(n) => return (lists, n.name.value()),
            TypeNode::ListType(l) => l.r#type.as_ref(),
            TypeNode::NonNullType(n) => match &n.r#type {
                NullableTypeNode::NamedType(n) => return (lists, n.name.value()),
                NullableTypeNode::ListType(l) => l.r#type.as_ref(),
            },
        };
        lists += 1;
        cur = inner;
    }
}

/// Fills a builder with every type and directive of `schema` in schema order.
fn fill(path: &Path, schema: &Schema, limits: &ReadLimits) -> Result<Sdl, DocsError> {
    let mut b = Builder::new(limits);
    let mut outline = Vec::new();
    if let Some(text) = schema_description(schema) {
        b.schema_description(text);
    }
    for named in schema_types(schema) {
        let (kind, fields) = add_type(path, schema, &mut b, named, limits)?;
        let name = named.name();
        if !name.starts_with("__") {
            outline.push(OutlineType {
                name: String::from(name),
                kind,
                specified: is_specified_scalar_type(named),
                fields,
            });
        }
    }
    b.roots(
        schema_query_type(schema).map(NamedType::name),
        schema_mutation_type(schema).map(NamedType::name),
        schema_subscription_type(schema).map(NamedType::name),
    );
    for d in schema_directives(schema) {
        let locations: Vec<&str> = directive_locations(d)
            .iter()
            .map(|location| location.as_str())
            .collect();
        let id = b.add_directive(
            directive_name(d),
            directive_description(d),
            &locations,
            directive_is_repeatable(d),
        )?;
        for a in directive_args(d, true) {
            let a = InputValue::Argument(a);
            let type_ref = spec(path, input_value_type(a), limits)?;
            let default_value = input_value_default_value(a).map_err(|e| definition(path, &e))?;
            b.add_directive_arg(
                id,
                input_value_name(a),
                &type_ref,
                input_value_description(a),
                default_value.as_deref(),
            )?;
        }
    }
    Ok(Sdl {
        builder: b,
        outline,
    })
}

/// Adds the named type `named` with its members to `b` and returns its kind
/// and the outline of its fields.
fn add_type(
    path: &Path,
    schema: &Schema,
    b: &mut Builder,
    named: &NamedType,
    limits: &ReadLimits,
) -> Result<(Kind, Vec<OutlineField>), DocsError> {
    let ty = Type::from(named);
    let kind = kind(path, &ty)?;
    let id = b.add_type(kind, named.name(), type_description(&ty))?;
    let failed = |e: DefinitionError| definition(path, &e);
    let mut outline = Vec::new();
    match kind {
        Kind::Object | Kind::Interface => {
            for field in type_fields(&ty, true).map_err(failed)?.unwrap_or_default() {
                outline.push(add_field(path, b, id, field, limits)?);
            }
            for interface in type_interfaces(&ty).map_err(failed)?.unwrap_or_default() {
                b.add_implements(id, interface.name());
            }
        }
        Kind::Input => {
            for field in type_input_fields(&ty, true)
                .map_err(failed)?
                .unwrap_or_default()
            {
                outline.push(add_input_field(path, b, id, field.into(), limits)?);
            }
        }
        Kind::Enum => {
            for v in type_enum_values(&ty, true)
                .map_err(failed)?
                .unwrap_or_default()
            {
                b.add_value(
                    id,
                    enum_value_name(v),
                    enum_value_description(v),
                    deprecation(enum_value_deprecation_reason(v)),
                )?;
            }
        }
        Kind::Union => {
            for member in type_possible_types(&ty, schema)
                .map_err(failed)?
                .unwrap_or_default()
            {
                b.add_member(id, member.name());
            }
        }
        Kind::Scalar => {
            if let Some(url) = type_specified_by_url(&ty) {
                b.set_specified_by_url(id, url);
            }
        }
    }
    Ok((kind, outline))
}

/// Adds the field `field` of an object or an interface with its arguments to
/// the type `on` and returns its outline.
fn add_field(
    path: &Path,
    b: &mut Builder,
    on: TypeId,
    field: &Field,
    limits: &ReadLimits,
) -> Result<OutlineField, DocsError> {
    let type_ref = spec(path, field_type(field), limits)?;
    let id = b.add_field(
        on,
        field_name(field),
        &type_ref,
        field_description(field),
        None,
        deprecation(field_deprecation_reason(field)),
    )?;
    let mut args = Vec::new();
    for a in field_args(field, true) {
        let a = InputValue::Argument(a);
        let arg_ref = spec(path, input_value_type(a), limits)?;
        let default_value = input_value_default_value(a).map_err(|e| definition(path, &e))?;
        let arg_id = b.add_arg(
            id,
            input_value_name(a),
            &arg_ref,
            input_value_description(a),
            default_value.as_deref(),
            deprecation(input_value_deprecation_reason(a)),
        )?;
        args.push(OutlineArg {
            name: String::from(input_value_name(a)),
            id: arg_id,
            type_ref: arg_ref,
        });
    }
    Ok(OutlineField {
        name: String::from(field_name(field)),
        type_ref,
        args,
    })
}

/// Adds the input field `field` to the input type `on` and returns its
/// outline.
fn add_input_field(
    path: &Path,
    b: &mut Builder,
    on: TypeId,
    field: InputValue<'_>,
    limits: &ReadLimits,
) -> Result<OutlineField, DocsError> {
    let type_ref = spec(path, input_value_type(field), limits)?;
    let default_value = input_value_default_value(field).map_err(|e| definition(path, &e))?;
    b.add_field(
        on,
        input_value_name(field),
        &type_ref,
        input_value_description(field),
        default_value.as_deref(),
        deprecation(input_value_deprecation_reason(field)),
    )?;
    Ok(OutlineField {
        name: String::from(input_value_name(field)),
        type_ref,
        args: Vec::new(),
    })
}

/// Returns the model's kind of the named type `ty`.
///
/// A named type is never a list or a non null type, and such a type is
/// refused rather than read.
fn kind(path: &Path, ty: &Type) -> Result<Kind, DocsError> {
    match type_kind(ty) {
        TypeKind::Object => Ok(Kind::Object),
        TypeKind::Interface => Ok(Kind::Interface),
        TypeKind::Union => Ok(Kind::Union),
        TypeKind::Enum => Ok(Kind::Enum),
        TypeKind::InputObject => Ok(Kind::Input),
        TypeKind::Scalar => Ok(Kind::Scalar),
        TypeKind::List | TypeKind::NonNull => Err(DocsError::Sdl {
            path: path.to_path_buf(),
            position: None,
            message: format!("the type map holds the wrapping type {ty} under a name"),
        }),
    }
}

/// Returns the type expression of `ty` by name.
///
/// The wrappers are collected outside in and applied inside out, so no call
/// recurses, and more than [`ReadLimits::max_depth`] of them are refused as
/// the builder refuses them.
fn spec(path: &Path, ty: &Type, limits: &ReadLimits) -> Result<TypeRefSpec, DocsError> {
    let mut wrappers = Vec::new();
    let mut cur = ty;
    while let Some(inner) = type_of_type(cur) {
        if wrappers.len() >= usize::from(limits.max_depth) {
            return Err(DocsError::Model(ModelError::Limit {
                what: "wrappers of a type reference",
                limit: u64::from(limits.max_depth),
            }));
        }
        wrappers.push(type_kind(cur));
        cur = inner;
    }
    let Some(name) = type_name(cur) else {
        return Err(DocsError::Sdl {
            path: path.to_path_buf(),
            position: None,
            message: format!("the type expression {ty} names no type"),
        });
    };
    let mut type_ref = TypeRefSpec::Named(String::from(name));
    for wrapper in wrappers.into_iter().rev() {
        type_ref = match wrapper {
            TypeKind::NonNull => TypeRefSpec::NonNull(Box::new(type_ref)),
            _ => TypeRefSpec::List(Box::new(type_ref)),
        };
    }
    Ok(type_ref)
}

/// Returns the deprecation a deprecation reason states.
///
/// graphql-js answers `isDeprecated` as a reason that is not null, so the
/// reason alone decides.
fn deprecation(reason: Option<&str>) -> Option<Deprecation> {
    reason.map(|reason| Deprecation {
        reason: Some(String::from(reason)),
    })
}

/// Returns the refusal of the file `path` for what the parser or the schema
/// builder refuses, at the first location the error names.
fn definition(path: &Path, e: &DefinitionError) -> DocsError {
    let position = match e {
        DefinitionError::GraphQL(error) => error
            .locations
            .as_ref()
            .and_then(|locations| locations.first())
            .map(|l| Position {
                line: u32::try_from(l.line).unwrap_or(u32::MAX),
                column: u32::try_from(l.column).unwrap_or(u32::MAX),
            }),
        DefinitionError::Plain(_) => None,
    };
    DocsError::Sdl {
        path: path.to_path_buf(),
        position,
        message: to_string_lossy(e.message()).into_owned(),
    }
}

/// Returns the offset of the first bracket that opens deeper than `max`, if
/// any.
///
/// The scan follows the tokens of GraphQL that may hold a bracket as text, a
/// comment to the end of its line, a string to its closing quote past an
/// escaped one and a block string to its closing three quotes past an escaped
/// one, and counts the brackets `(`, `[` and `{` outside them. Every recursion
/// of the grammar opens one of them, so the count bounds the depth of the
/// parser.
///
/// [specification]: https://spec.graphql.org/October2021/
fn too_deep(bytes: &[u8], max: usize) -> Option<usize> {
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'#' => i = skip_comment(bytes, i),
            b'"' if bytes[i..].starts_with(b"\"\"\"") => i = skip_block_string(bytes, i + 3),
            b'"' => i = skip_string(bytes, i + 1),
            b'(' | b'[' | b'{' => {
                depth += 1;
                if depth > max {
                    return Some(i);
                }
                i += 1;
            }
            b')' | b']' | b'}' => {
                depth = depth.saturating_sub(1);
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// Returns the offset after the comment that starts at `i`, which is its
/// line terminator or the end.
fn skip_comment(bytes: &[u8], i: usize) -> usize {
    bytes[i..]
        .iter()
        .position(|&b| b == b'\n' || b == b'\r')
        .map_or(bytes.len(), |n| i + n)
}

/// Returns the offset after the string whose content starts at `i`, which is
/// past its closing quote, its line terminator or the end.
///
/// A string ends at a line terminator, after a backslash as well, which the
/// lexer refuses, so the scan goes on from there as the parser never would.
fn skip_string(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        match bytes[i] {
            b'\\' if matches!(bytes.get(i + 1), Some(b'\n' | b'\r')) => return i + 1,
            b'\\' => i += 2,
            b'"' => return i + 1,
            b'\n' | b'\r' => return i,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Returns the offset after the block string whose content starts at `i`,
/// which is past its closing three quotes or the end.
fn skip_block_string(bytes: &[u8], mut i: usize) -> usize {
    while i < bytes.len() {
        if bytes[i..].starts_with(b"\\\"\"\"") {
            i += 4;
        } else if bytes[i..].starts_with(b"\"\"\"") {
            return i + 3;
        } else {
            i += 1;
        }
    }
    bytes.len()
}

/// Returns the line and the column of the byte `offset` of `bytes`, both
/// from 1, with the column in UTF-16 code units of the text before it.
///
/// A line ends at `\r\n`, `\n` or `\r`, as the lexer of graphql-js ends one.
fn position_of(bytes: &[u8], offset: usize) -> Position {
    let before = &bytes[..offset.min(bytes.len())];
    let mut line = 1usize;
    let mut line_start = 0;
    let mut i = 0;
    while i < before.len() {
        match before[i] {
            b'\r' if before.get(i + 1) == Some(&b'\n') => {
                line += 1;
                i += 2;
                line_start = i;
            }
            b'\n' | b'\r' => {
                line += 1;
                i += 1;
                line_start = i;
            }
            _ => i += 1,
        }
    }
    let column: usize = String::from_utf8_lossy(&before[line_start..])
        .chars()
        .map(char::len_utf16)
        .sum();
    Position {
        line: u32::try_from(line).unwrap_or(u32::MAX),
        column: u32::try_from(column + 1).unwrap_or(u32::MAX),
    }
}

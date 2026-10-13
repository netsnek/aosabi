// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-query: the root type of the generated client, the text of types.ts.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/query/build/root-type.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The root type.
//!
//! [`generate_root_type()`] writes the authoring time `Data` root type of a
//! schema in the callable field style, where a field with arguments is a
//! call and every other field a property:
//!
//! ```text
//! data.me.name                    // field property, a scalar
//! data.posts({ first: 10 }).edges // arg field, a call
//! ```
//!
//! This is the type that makes `useData()` autocomplete before the analyzer
//! runs. The text is `types.ts` of the client: the object types, the enums as
//! `as const` objects beside a literal union type, the interfaces and unions
//! as unions of their members, the input objects the arguments reach, and
//! `Data` and `Mutations`, joined by blank lines.

use aosabi_json::{
    JsValue,
    map::IndexSet,
    value::KeyHasherBuilder, //
};
use aosabi_type::{
    DefinitionError,
    EnumType,
    InputObjectType,
    NamedType,
    ObjectType,
    Schema,
    Type,
    get_named_type,
    is_non_null_type, //
};

/// The TypeScript type of each built in scalar, `DEFAULT_SCALARS` of
/// root-type.ts:19-25.
///
/// The scalar types a caller hands [`generate_root_type()`] are merged over
/// these, a later pair winning over an earlier one with the same name.
pub const DEFAULT_SCALARS: &[(&str, &str)] = &[
    ("String", "string"),
    ("ID", "string"),
    ("Int", "number"),
    ("Float", "number"),
    ("Boolean", "boolean"),
];

/// The text a property of `Object.prototype` interpolates as, by name.
///
/// The reference looks a scalar name up as `scalars[named.name]` on an object
/// literal, so a name no pair carries that names an inherited property reads
/// that property, and its text stands where a TypeScript type belongs. Each
/// property is a native function but `__proto__`, whose getter answers the
/// prototype itself. The file the reference writes is then no valid
/// TypeScript, and its bundle fails.
const INHERITED: &[(&str, &str)] = &[
    ("constructor", "function Object() { [native code] }"),
    (
        "__defineGetter__",
        "function __defineGetter__() { [native code] }",
    ),
    (
        "__defineSetter__",
        "function __defineSetter__() { [native code] }",
    ),
    (
        "hasOwnProperty",
        "function hasOwnProperty() { [native code] }",
    ),
    (
        "__lookupGetter__",
        "function __lookupGetter__() { [native code] }",
    ),
    (
        "__lookupSetter__",
        "function __lookupSetter__() { [native code] }",
    ),
    (
        "isPrototypeOf",
        "function isPrototypeOf() { [native code] }",
    ),
    (
        "propertyIsEnumerable",
        "function propertyIsEnumerable() { [native code] }",
    ),
    ("toString", "function toString() { [native code] }"),
    ("valueOf", "function valueOf() { [native code] }"),
    ("__proto__", "[object Object]"),
    (
        "toLocaleString",
        "function toLocaleString() { [native code] }",
    ),
];

/// The message V8 throws when the mutation root has no field map.
///
/// The reference calls `mutationType.getFields()` on whatever named type the
/// schema names as its mutation root, and a scalar, an enum or a union has no
/// such method.
const GET_FIELDS_NOT_A_FUNCTION: &str = "mutationType.getFields is not a function";

/// A set of type names in insertion order, the `Set` of the reference.
type NameSet = IndexSet<String, KeyHasherBuilder>;

/// Generates the authoring time `Data` root type of `schema`.
///
/// `scalar_types` maps a GraphQL scalar name to a TypeScript type and is
/// merged over [`DEFAULT_SCALARS`]. A scalar that no pair names reads the
/// property of `Object.prototype` of its name when there is one, as the lookup
/// of the reference does, and `any` otherwise.
/// The parts are written in the order of the reference: every object type of
/// the type map, then every enum, then every interface and union, then the
/// input objects the arguments of the object types reach, taken from the end
/// of a queue, and last `Data` and `Mutations`. `Mutations` is emitted even
/// when the schema has no mutation, as `{}`, so the augmentation import never
/// breaks.
///
/// # Errors
///
/// Returns what a field, value or member thunk throws, which a schema built
/// from SDL has already run, and the `TypeError` of V8 when the mutation root
/// is a scalar, an enum or a union.
pub fn generate_root_type(
    schema: &Schema,
    scalar_types: &[(&str, &str)],
) -> Result<String, DefinitionError> {
    let scalars = Scalars { scalar_types };
    let type_map = schema.get_type_map();
    let mut out: Vec<String> = Vec::new();
    let mut needed_inputs = NameSet::default();

    for ty in type_map.values() {
        if let NamedType::Object(object) = ty
            && !object.name.starts_with("__")
        {
            out.push(render_object(object, &scalars, &mut needed_inputs)?);
        }
    }

    for ty in type_map.values() {
        if let NamedType::Enum(enum_type) = ty
            && !enum_type.name.starts_with("__")
        {
            out.push(render_enum(enum_type)?);
        }
    }

    // An interface maps to the union of its implementers and a union to its
    // members, so a field typed as either narrows by `__typename`.
    for ty in type_map.values() {
        if ty.name().starts_with("__") {
            continue;
        }
        let members = match ty {
            NamedType::Interface(_) => schema.get_possible_types(ty)?,
            NamedType::Union(union) => union.get_types()?,
            _ => continue,
        };
        out.push(format!("export type {} = {}", ty.name(), union_of(members)));
    }

    render_needed_inputs(schema, &scalars, needed_inputs, &mut out)?;

    let root_name = schema.get_query_type().map_or("Query", NamedType::name);
    out.push(format!("export type Data = {root_name}"));

    let mutations = match schema.get_mutation_type() {
        Some(mutation_type) if field_count(mutation_type)? > 0 => mutation_type.name(),
        _ => "{}",
    };
    out.push(format!("export type Mutations = {mutations}"));

    let mut text = out.join("\n\n");
    text.push('\n');
    Ok(text)
}

/// The merged scalar map of the reference, `{...DEFAULT_SCALARS, ...scalarTypes}`.
struct Scalars<'a> {
    /// The pairs of the caller, which win over the defaults.
    scalar_types: &'a [(&'a str, &'a str)],
}

impl Scalars<'_> {
    /// Returns the TypeScript type of the scalar `name`, `scalars[name] ?? 'any'`.
    ///
    /// A later pair of the caller wins, as the later key of a spread does, and
    /// a name no pair carries reads the inherited property of [`INHERITED`].
    fn get(&self, name: &str) -> &str {
        self.scalar_types
            .iter()
            .rev()
            .chain(DEFAULT_SCALARS)
            .chain(INHERITED)
            .find(|(key, _)| *key == name)
            .map_or("any", |(_, ts)| ts)
    }
}

/// Renders the input objects the queue built from `needed_inputs` reaches.
///
/// The queue is popped from its end, an input object is emitted once, and
/// rendering one pushes the input objects its fields name.
fn render_needed_inputs(
    schema: &Schema,
    scalars: &Scalars<'_>,
    needed_inputs: NameSet,
    out: &mut Vec<String>,
) -> Result<(), DefinitionError> {
    let mut emitted_inputs = NameSet::default();
    let mut queue: Vec<String> = needed_inputs.into_iter().collect();
    while let Some(name) = queue.pop() {
        if emitted_inputs.contains(name.as_str()) {
            continue;
        }
        let ty = schema.get_type(&name);
        emitted_inputs.insert(name);
        if let Some(NamedType::InputObject(input)) = ty {
            out.push(render_input(input, scalars, &mut queue)?);
        }
    }
    Ok(())
}

/// Renders an object type as an interface, a `__typename` literal first.
///
/// The literal makes a union of these objects a discriminated union. A scalar
/// leaf whose arguments are all optional is dual mode, usable as the bare
/// value or called in the authoring style, so its member is the intersection
/// of both.
fn render_object(
    ty: &ObjectType,
    scalars: &Scalars<'_>,
    needed_inputs: &mut NameSet,
) -> Result<String, DefinitionError> {
    let mut members = vec![format!("  __typename: {}", json_string(&ty.name))];
    for field in ty.get_fields()?.values() {
        let ret = render_output(&field.r#type, scalars);
        if field.args.is_empty() {
            members.push(format!("  {}: {ret}", field.name));
            continue;
        }
        let arg_members: Vec<String> = field
            .args
            .iter()
            .map(|a| {
                let optional = if is_non_null_type(&a.r#type) { "" } else { "?" };
                collect_inputs(&a.r#type, needed_inputs);
                format!(
                    "{}{optional}: {}",
                    a.name,
                    render_input_ref(&a.r#type, scalars)
                )
            })
            .collect();
        let args_obj = format!("{{ {} }}", arg_members.join("; "));
        let all_optional = field.args.iter().all(|a| !is_non_null_type(&a.r#type));
        let named = get_named_type(&field.r#type);
        let is_scalar_leaf = matches!(named, NamedType::Scalar(_) | NamedType::Enum(_));
        if is_scalar_leaf && all_optional {
            members.push(format!(
                "  {}: {ret} & ((args?: {args_obj}) => {ret})",
                field.name
            ));
        } else {
            let optional = if all_optional { "?" } else { "" };
            members.push(format!(
                "  {}(args{optional}: {args_obj}): {ret}",
                field.name
            ));
        }
    }
    Ok(format!(
        "export interface {} {{\n{}\n}}",
        ty.name,
        members.join("\n")
    ))
}

/// Renders an enum as an `as const` object and a literal union of the same name.
///
/// The object gives the runtime members, `Status.ACTIVE`, and the type alias
/// keeps the type a union of strings, so a raw literal still assigns. Value
/// and type share the name through declaration merging.
fn render_enum(ty: &EnumType) -> Result<String, DefinitionError> {
    let values = ty.get_values()?;
    let members: Vec<String> = values
        .iter()
        .map(|v| format!("  {}: {}", v.name, json_text(&v.value)))
        .collect();
    let union: Vec<String> = values.iter().map(|v| json_text(&v.value)).collect();
    Ok(format!(
        "export const {name} = {{\n{}\n}} as const\n\nexport type {name} = {}",
        members.join(",\n"),
        union.join(" | "),
        name = ty.name,
    ))
}

/// Renders an input object as an interface of its fields.
///
/// A field that names an input object pushes that name onto `queue`.
fn render_input(
    ty: &InputObjectType,
    scalars: &Scalars<'_>,
    queue: &mut Vec<String>,
) -> Result<String, DefinitionError> {
    let mut members: Vec<String> = Vec::new();
    for field in ty.get_fields()?.values() {
        let optional = if is_non_null_type(&field.r#type) {
            ""
        } else {
            "?"
        };
        // The reference collects into a fresh copy of the queue that it then
        // drops, so the call leaves the queue as it was.
        collect_inputs(&field.r#type, &mut queue.iter().cloned().collect());
        if let NamedType::InputObject(named) = get_named_type(&field.r#type) {
            queue.push(named.name.clone());
        }
        members.push(format!(
            "  {}{optional}: {}",
            field.name,
            render_input_ref(&field.r#type, scalars)
        ));
    }
    Ok(format!(
        "export interface {} {{\n{}\n}}",
        ty.name,
        members.join("\n")
    ))
}

/// Renders an output type, a nullable one with `| null`.
fn render_output(ty: &Type, scalars: &Scalars<'_>) -> String {
    match ty {
        Type::NonNull(non_null) => render_output_non_null(non_null.of_type(), scalars),
        Type::List(list) => format!("Array<{}> | null", render_output(&list.of_type, scalars)),
        _ => format!("{} | null", named_ref(ty, scalars)),
    }
}

/// Renders the type a non null output type wraps.
fn render_output_non_null(ty: &Type, scalars: &Scalars<'_>) -> String {
    match ty {
        Type::List(list) => format!("Array<{}>", render_output(&list.of_type, scalars)),
        _ => named_ref(ty, scalars),
    }
}

/// Renders an input type of an argument or an input field, a nullable one
/// with `| null`.
fn render_input_ref(ty: &Type, scalars: &Scalars<'_>) -> String {
    match ty {
        Type::NonNull(non_null) => render_input_ref_non_null(non_null.of_type(), scalars),
        Type::List(list) => format!("Array<{}> | null", render_input_ref(&list.of_type, scalars)),
        _ => format!("{} | null", named_ref(ty, scalars)),
    }
}

/// Renders the type a non null input type wraps.
fn render_input_ref_non_null(ty: &Type, scalars: &Scalars<'_>) -> String {
    match ty {
        Type::List(list) => format!("Array<{}>", render_input_ref(&list.of_type, scalars)),
        _ => named_ref(ty, scalars),
    }
}

/// Renders a named type, a scalar through the scalar map and any other type
/// by its name.
///
/// This is `namedOutput` and `namedInput` of the reference, whose bodies are
/// the same.
fn named_ref(ty: &Type, scalars: &Scalars<'_>) -> String {
    let named = get_named_type(ty);
    if let NamedType::Scalar(scalar) = &named {
        return scalars.get(&scalar.name).to_owned();
    }
    named.name().to_owned()
}

/// Adds the name of the input object `ty` names, if it names one, to `into`.
fn collect_inputs(ty: &Type, into: &mut NameSet) {
    if let NamedType::InputObject(named) = get_named_type(ty) {
        into.insert(named.name.clone());
    }
}

/// Returns the members of an abstract type joined into a TypeScript union,
/// `never` when it has none.
fn union_of(members: &[NamedType]) -> String {
    if members.is_empty() {
        return "never".to_owned();
    }
    let names: Vec<&str> = members.iter().map(NamedType::name).collect();
    names.join(" | ")
}

/// Returns the number of fields of the mutation root, `Object.keys(getFields())`.
///
/// # Errors
///
/// Returns what the field thunk throws, and the `TypeError` of V8 for a root
/// type that has no field map.
fn field_count(ty: &NamedType) -> Result<usize, DefinitionError> {
    match ty {
        NamedType::Object(object) => Ok(object.get_fields()?.len()),
        NamedType::Interface(interface) => Ok(interface.get_fields()?.len()),
        NamedType::InputObject(input) => Ok(input.get_fields()?.len()),
        NamedType::Scalar(_) | NamedType::Enum(_) | NamedType::Union(_) => {
            Err(DefinitionError::plain(GET_FIELDS_NOT_A_FUNCTION))
        }
    }
}

/// Returns `JSON.stringify(name)` of a type name.
fn json_string(name: &str) -> String {
    json_text(&JsValue::from(name))
}

/// Returns the text `JSON.stringify(value)` interpolates as in a template.
///
/// An undefined value has no JSON text, and the template then reads
/// `undefined`.
pub(crate) fn json_text(value: &JsValue) -> String {
    aosabi_json::to_string(value).unwrap_or_else(|| "undefined".to_owned())
}

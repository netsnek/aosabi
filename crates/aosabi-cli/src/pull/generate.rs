// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-cli: the Rust registry pull writes for a remote schema.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/pull/index.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! The registry.
//!
//! [`emit_registry`] renders the remote schema by this table:
//!
//! | Remote | Rust |
//! | --- | --- |
//! | `Query` and `Mutation` with a field | `pub mod <Root>` with one struct per field, its arguments as fields in order, and `impl Delegate` with `Registry`, `Return`, `PATH` and `arguments` |
//! | a root with no field, and `Subscription` | nothing |
//! | an object | a struct under `#[derive(Aosabi)]` and `#[aosabi(remote)]` with `__typename: TypeName` and its fields in order, a field with arguments typed `Callable<args::<Name>_<field>, <return>>`, the constant `TYPENAME`, `impl RemoteType` naming the file's `Registry` and `impl NeedsTarget` |
//! | an interface with possible types | a payload enum under `#[derive(Aosabi)]` and `#[aosabi(remote)]` of one variant per possible type, and `impl NeedsTarget` |
//! | an interface with none | the struct of an object without the inherent constant `TYPENAME` |
//! | an input object | a struct under `#[derive(Aosabi)]` and `impl IntoArgument` writing its fields in order |
//! | an enum | a unit enum under `#[derive(Aosabi)]`, `impl IntoArgument` writing the value name and `impl NeedsTarget` |
//! | a union | a payload enum under `#[derive(Aosabi)]` and `#[aosabi(remote)]` of one variant per member, and `impl NeedsTarget` |
//! | a scalar outside the twelve of `SCALARS` | `pub type <Name> = aosabi::Any;` |
//! | the arguments of a field | a struct of `args` under `#[derive(Aosabi)]` and `#[aosabi(arguments)]`, whose `Params` list them in order |
//!
//! `NeedsTarget` of `aosabi::federation` stands beside every struct and enum
//! a delegation can answer, because a blanket implementation over
//! `RemoteType` is refused outside the crate of the trait, so a pulled
//! registry compiles with the feature `federation` of the facade. A type, a
//! variant or a value whose Rust name is not its GraphQL name states the
//! GraphQL name with `typename` or `value`, so the description reads the
//! remote's name.
//!
//! A nullable position is `Option<T>`, a list is `Vec<T>` with the
//! nullability of its element dropped, and an
//! object, an interface, an input object or a union in a field of a struct
//! outside a list is `Box<T>`, so a type that reaches itself has a size. A
//! non null member of an object or an interface whose type has no value for
//! an absent member, an enum, an object, an interface, a union or `Date`, is
//! `Option<T>` as well, because the read of a pulled record takes a member
//! the remote leaves out as an absent one, and the remote leaves out every
//! member neither the client nor `needs` selects. The cases of `blog-app` and
//! of the corpus of `70_pull` pin it.
//!
//! `ID`, `Int`, `Float`, `Date` and `File` are the markers of the facade,
//! and the seven other known scalars are the aliases the file opens with.
//! Inside a root module and inside `args`, a type of
//! the remote is written `super::<Name>`, so a field named like a type does
//! not shadow it. A name that is a Rust keyword is written raw, and `self`,
//! `super`, `crate`, `Self` and `_`, which cannot be raw, take a trailing
//! underscore, while `arguments` and the enum values keep the GraphQL name.
//! The `use` line names exactly what the file uses, in the vertical layout of
//! `Documentation/coding-style.md` section 2.1, so an unused import never warns.
//!
//! The file allows the four lints of the plan and the eight of clippy that
//! the names and the shapes of a remote raise under `clippy::all` and
//! `clippy::pedantic`: `upper_case_acronyms` for an enum value in upper case,
//! `wildcard_imports` for the `use super::*;` of a module,
//! `enum_variant_names` for variants that repeat the name of their enum or
//! share a prefix or a suffix, `large_enum_variant` for a union whose members
//! differ in size, `pub_underscore_fields` for `__typename` in a registry a
//! library exports, `struct_field_names` for fields that start with the name
//! of their struct or share a prefix or a suffix, `too_many_lines` for a long
//! enum or input object and `type_complexity` for a deeply nested list. These
//! are the lints a pulled registry is known to raise, not every lint of the
//! two groups, so a service that lints with more allows the rest where it
//! declares the module. A list of keys takes the layout rustfmt gives it, so
//! `cargo fmt` leaves the list as it is.
//!
//! A remote type named `Registry`, `args` or `aosabi`, the crate whose paths
//! the file writes, a root name used as a type, a remote type named like an
//! item the file imports or a type of the prelude it writes, `Option`, `Vec`
//! and `Box`, a remote type named `str`, `bool` or `f64`, the primitive
//! types the file writes, a known scalar name that is no scalar, a field of a root named
//! like an alias, a marker or an item its module imports, and two items or
//! two fields of one struct that take one Rust name refuse with `AO-C0502`,
//! since the file would not compile.

use std::fmt::Write as _;

use aosabi_type::{
    Argument,
    DefinitionError,
    Field,
    InputField,
    NamedType,
    ObjMap,
    Schema,
    Type, //
};

use crate::{
    codes,
    report::CliError, //
};

/// The first line of the file.
const HEADER: &str = "// AUTO-GENERATED BY AOSABI INTROSPECT\n";

/// The lints the file allows, since every name is the remote's and the file
/// is generated: the four of the plan, and the eight of clippy that the names
/// and the shapes of a remote raise, which the documentation of the module
/// lists.
const ALLOW: &str = "#![allow(
    non_camel_case_types,
    non_snake_case,
    dead_code,
    missing_docs,
    clippy::enum_variant_names,
    clippy::large_enum_variant,
    clippy::pub_underscore_fields,
    clippy::struct_field_names,
    clippy::too_many_lines,
    clippy::type_complexity,
    clippy::upper_case_acronyms,
    clippy::wildcard_imports,
    reason = \"every name is the remote's GraphQL name verbatim, and the file is generated\"
)]
";

/// The seven aliases the file opens with, in the order of `SCALARS`.
const ALIASES: [(&str, &str); 7] = [
    ("String", "::std::string::String"),
    ("Boolean", "bool"),
    ("Number", "f64"),
    ("Any", "aosabi::Any"),
    ("Void", "()"),
    ("JSONObject", "aosabi::json::JsObject"),
    ("JSON", "aosabi::JSON"),
];

/// The markers of the facade, in the order of the `use` line.
const MARKERS: [&str; 5] = ["Date", "File", "Float", "ID", "Int"];

/// The derive of the facade, which the `use` line names before the markers.
const DERIVE: &str = "Aosabi";

/// The twelve known scalars.
const SCALARS: [&str; 12] = [
    "String",
    "ID",
    "Int",
    "Float",
    "Boolean",
    "Number",
    "Any",
    "Void",
    "JSONObject",
    "File",
    "Date",
    "JSON",
];

/// The three roots, which the file writes as no type.
const ROOTS: [&str; 3] = ["Query", "Mutation", "Subscription"];

/// The roots whose fields the `delegate` entries list, in their order.
const DELEGATE_ROOTS: [&str; 2] = ["Query", "Mutation"];

/// The names a remote type cannot take, the items the file writes or
/// imports, the types of the prelude it names and the primitive types it
/// writes, `str` in `&'static str` and `bool` and `f64` behind the aliases
/// `Boolean` and `Number`.
const TAKEN: [&str; 18] = [
    "Registry",
    "args",
    "RemoteRegistry",
    "Delegate",
    "IntoArgument",
    "TypeName",
    "Callable",
    "RemoteType",
    "NeedsTarget",
    "Aosabi",
    "JsObject",
    "JsValue",
    "Option",
    "Vec",
    "Box",
    "str",
    "bool",
    "f64",
];

/// The crate whose paths the file writes, its `use` line and the targets of
/// `Any`, `JSONObject` and `JSON` among them, which a remote type of that
/// name would shadow.
const CRATE: &str = "aosabi";

/// The keywords of Rust 2024, strict and reserved, which a name takes raw.
const KEYWORDS: [&str; 52] = [
    "abstract", "as", "async", "await", "become", "box", "break", "const", "continue", "crate",
    "do", "dyn", "else", "enum", "extern", "false", "final", "fn", "for", "gen", "if", "impl",
    "in", "let", "loop", "macro", "match", "mod", "move", "mut", "override", "priv", "pub", "ref",
    "return", "self", "Self", "static", "struct", "super", "trait", "true", "try", "type",
    "typeof", "unsafe", "unsized", "use", "virtual", "where", "while", "yield",
];

/// The names that cannot be raw, which take a trailing underscore.
const NOT_RAW: [&str; 5] = ["self", "super", "crate", "Self", "_"];

/// The gap of one level of the file.
const INDENT: &str = "    ";

/// The longest line of a list of keys, the `max_width` of rustfmt.
const MAX_WIDTH: usize = 100;

/// The widest run of keys rustfmt writes between the brackets of an array on
/// one line, its `array_width`.
const ARRAY_WIDTH: usize = 60;

/// The widest key, its quotes included, of which rustfmt packs several to a
/// line, its `short_array_element_width_threshold`.
const SHORT_KEY: usize = 10;

/// Where a type is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Scope {
    /// At the root of the file.
    Top,
    /// Inside a root module or `args`, where a type of the remote takes
    /// `super::`.
    Nested,
}

/// The names of `aosabi::json` the file may import, in the order of the
/// `use` line.
const JSON_NAMES: [&str; 2] = ["JsObject", "JsValue"];

/// The names of `aosabi::remote` the file may import, in the order of the
/// `use` line. The trait `Registry` is imported as `RemoteRegistry` by every
/// file, since the file names its own `Registry`.
const REMOTE_NAMES: [&str; 6] = [
    "Callable",
    "Delegate",
    "IntoArgument",
    "Registry as RemoteRegistry",
    "RemoteType",
    "TypeName",
];

/// The names of `aosabi::federation` the file may import.
const FEDERATION_NAMES: [&str; 1] = ["NeedsTarget"];

/// The import every file carries.
const REMOTE_REGISTRY: &str = "Registry as RemoteRegistry";

/// The writer of one registry.
struct Emitter<'a> {
    /// The remote schema.
    schema: &'a Schema,
    /// The names of the facade the file uses, which its `use` line imports.
    used: Vec<&'static str>,
    /// The items after the aliases, each with its blank line before it.
    items: String,
    /// The structs of `args`.
    args: Vec<String>,
    /// The Rust name of every item at the root of the file.
    names: Vec<String>,
}

/// Returns the Rust registry of `schema`, the text of `<name>.rs`.
///
/// # Errors
///
/// Returns `AO-C0502` for a name the file cannot write, and the failure of
/// a type of the schema whose fields or members do not read.
pub fn emit_registry(schema: &Schema) -> Result<String, CliError> {
    let mut emitter = Emitter {
        schema,
        used: vec![REMOTE_REGISTRY],
        items: String::new(),
        args: Vec::new(),
        names: ALIASES.iter().map(|(name, _)| (*name).to_owned()).collect(),
    };
    emitter.reserve(&["Registry", "args"])?;
    let mut types = Vec::new();
    for (name, named) in schema.get_type_map() {
        if name.starts_with("__") {
            continue;
        }
        check_name(name, named)?;
        emitter.emit_type(name, named)?;
        let listed = matches!(
            named,
            NamedType::Object(_) | NamedType::Interface(_) | NamedType::Union(_)
        );
        if listed && !ROOTS.contains(&name.as_str()) {
            types.push(name.clone());
        }
    }
    let mut delegate = Vec::new();
    for root in DELEGATE_ROOTS {
        if let Some(NamedType::Object(object)) = schema.get_type(root) {
            for field in object
                .get_fields()
                .map_err(|error| definition(&error))?
                .keys()
            {
                delegate.push(format!("{root}.{field}"));
            }
        }
    }
    Ok(emitter.finish(&delegate, &types))
}

impl Emitter<'_> {
    /// Records that the file uses the name `name` of the facade.
    fn use_name(&mut self, name: &'static str) {
        if !self.used.contains(&name) {
            self.used.push(name);
        }
    }

    /// Records the root names `names`, refusing one taken already.
    fn reserve(&mut self, names: &[&str]) -> Result<(), CliError> {
        for name in names {
            if self.names.iter().any(|known| known == name) {
                return Err(codes::registry_collision(name));
            }
            self.names.push((*name).to_owned());
        }
        Ok(())
    }

    /// Writes the item of the type `name` of the type map.
    fn emit_type(&mut self, name: &str, named: &NamedType) -> Result<(), CliError> {
        match named {
            NamedType::Object(object) if ROOTS.contains(&name) => {
                let fields = object.get_fields().map_err(|error| definition(&error))?;
                if DELEGATE_ROOTS.contains(&name) && !fields.is_empty() {
                    self.reserve(&[name])?;
                    self.root_module(name, fields)?;
                }
                Ok(())
            }
            NamedType::Object(object) => {
                let fields = object.get_fields().map_err(|error| definition(&error))?;
                self.object_struct(name, fields, true)
            }
            NamedType::Interface(interface) => {
                let possible = self
                    .schema
                    .get_possible_types(named)
                    .map_err(|error| definition(&error))?;
                if possible.is_empty() {
                    let fields = interface.get_fields().map_err(|error| definition(&error))?;
                    self.object_struct(name, fields, false)
                } else {
                    let members: Vec<String> = possible
                        .iter()
                        .map(|member| member.name().to_owned())
                        .collect();
                    self.variant_enum(name, &members)
                }
            }
            NamedType::Union(union) => {
                let members: Vec<String> = union
                    .get_types()
                    .map_err(|error| definition(&error))?
                    .iter()
                    .map(|member| member.name().to_owned())
                    .collect();
                self.variant_enum(name, &members)
            }
            NamedType::InputObject(input) => {
                let fields = input.get_fields().map_err(|error| definition(&error))?;
                self.input_struct(name, fields)
            }
            NamedType::Enum(enumeration) => {
                let values = enumeration
                    .get_values()
                    .map_err(|error| definition(&error))?;
                let value_names: Vec<&str> =
                    values.iter().map(|value| value.name.as_str()).collect();
                self.unit_enum(name, &value_names)
            }
            NamedType::Scalar(_) => {
                if !SCALARS.contains(&name) {
                    let ident = ident_of(name);
                    self.reserve(&[ident.as_str()])?;
                    let _ = writeln!(self.items, "\npub type {ident} = aosabi::Any;");
                }
                Ok(())
            }
        }
    }

    /// Writes the struct of the object or of the interface with no possible
    /// type `name` with its fields `fields`, under the derive with the key
    /// `remote`, with `impl RemoteType` and `impl NeedsTarget`, and the
    /// constant `TYPENAME` where `object` says it is an object.
    fn object_struct(
        &mut self,
        name: &str,
        fields: &ObjMap<Field>,
        object: bool,
    ) -> Result<(), CliError> {
        let ident = ident_of(name);
        self.reserve(&[ident.as_str()])?;
        self.use_name(DERIVE);
        self.use_name("TypeName");
        self.use_name("RemoteType");
        let keys = remote_keys(name, &ident);
        let mut text = format!(
            "\n#[derive(Aosabi)]\n#[aosabi({keys})]\npub struct {ident} {{\n{INDENT}pub __typename: TypeName,\n"
        );
        let mut seen = vec![String::from("__typename")];
        self.output_fields(name, fields, &mut text, &mut seen)?;
        text.push_str("}\n");
        if object {
            let _ = write!(
                text,
                "\nimpl {ident} {{\n{INDENT}pub const TYPENAME: &'static str = \"{name}\";\n}}\n"
            );
        }
        let _ = write!(
            text,
            "\nimpl RemoteType for {ident} {{\n{INDENT}type Registry = Registry;\n{INDENT}const TYPENAME: &'static str = \"{name}\";\n}}\n"
        );
        text.push_str(&self.needs_target(&ident));
        self.items.push_str(&text);
        Ok(())
    }

    /// Returns `impl NeedsTarget` of the type `ident`, its own record, which
    /// a delegation that answers it reads its `needs!` against.
    fn needs_target(&mut self, ident: &str) -> String {
        self.use_name("NeedsTarget");
        format!("\nimpl NeedsTarget for {ident} {{\n{INDENT}type Record = Self;\n}}\n")
    }

    /// Writes the struct of the input object `name` with its fields `fields`
    /// and its `impl IntoArgument`, which leaves an absent optional field out.
    fn input_struct(&mut self, name: &str, fields: &ObjMap<InputField>) -> Result<(), CliError> {
        let ident = ident_of(name);
        self.reserve(&[ident.as_str()])?;
        self.use_name(DERIVE);
        self.use_name("IntoArgument");
        self.use_name("JsObject");
        self.use_name("JsValue");
        let mut text = format!("\n#[derive(Aosabi)]\npub struct {ident} {{\n");
        let mut inserts = String::new();
        let mut seen = Vec::new();
        for (field_name, field) in fields {
            let field_ident = unique(&mut seen, ident_of(field_name), name)?;
            let rendered = self.render(&field.r#type, Scope::Top, true)?;
            let _ = writeln!(text, "{INDENT}pub {field_ident}: {rendered},");
            let _ = writeln!(
                inserts,
                "{INDENT}{INDENT}object.insert(\"{field_name}\", self.{field_ident}.into_argument());"
            );
        }
        text.push_str("}\n");
        let body = if fields.is_empty() {
            format!("{INDENT}{INDENT}JsValue::Object(JsObject::new())\n")
        } else {
            format!(
                "{INDENT}{INDENT}let mut object = JsObject::new();\n{inserts}{INDENT}{INDENT}JsValue::Object(object)\n"
            )
        };
        let _ = write!(
            text,
            "\nimpl IntoArgument for {ident} {{\n{INDENT}fn into_argument(&self) -> JsValue {{\n{body}{INDENT}}}\n}}\n"
        );
        self.items.push_str(&text);
        Ok(())
    }

    /// Writes the unit enum `name` with the values `values` under the
    /// derive, with its `impl IntoArgument`, which writes the name of the
    /// value, and its `impl NeedsTarget`.
    fn unit_enum(&mut self, name: &str, values: &[&str]) -> Result<(), CliError> {
        let ident = ident_of(name);
        self.reserve(&[ident.as_str()])?;
        self.use_name(DERIVE);
        self.use_name("IntoArgument");
        self.use_name("JsValue");
        let mut variants = String::new();
        let mut arms = String::new();
        let mut seen = Vec::new();
        for value in values {
            let variant = unique(&mut seen, ident_of(value), name)?;
            if variant.trim_start_matches("r#") != *value {
                let _ = writeln!(variants, "{INDENT}#[aosabi(value = \"{value}\")]");
            }
            let _ = writeln!(variants, "{INDENT}{variant},");
            let _ = writeln!(
                arms,
                "{INDENT}{INDENT}{INDENT}Self::{variant} => \"{value}\","
            );
        }
        let body = if values.is_empty() {
            format!("{INDENT}{INDENT}JsValue::from(match *self {{}})\n")
        } else {
            format!("{INDENT}{INDENT}JsValue::from(match self {{\n{arms}{INDENT}{INDENT}}})\n")
        };
        let typename = typename_key(name, &ident);
        let needs = self.needs_target(&ident);
        let _ = write!(
            self.items,
            "\n#[derive(Aosabi, Clone, Copy, Debug, PartialEq, Eq)]\n{typename}pub enum {ident} {{\n{variants}}}\n\n\
             impl IntoArgument for {ident} {{\n{INDENT}fn into_argument(&self) -> JsValue {{\n{body}{INDENT}}}\n}}\n{needs}"
        );
        Ok(())
    }

    /// Writes the fields `fields` of the object or interface `owner` into
    /// `text`, a field with arguments as a `Callable` over its arguments
    /// struct in `args`.
    fn output_fields(
        &mut self,
        owner: &str,
        fields: &ObjMap<Field>,
        text: &mut String,
        seen: &mut Vec<String>,
    ) -> Result<(), CliError> {
        for (field_name, field) in fields {
            let field_ident = unique(seen, ident_of(field_name), owner)?;
            if field.args.is_empty() {
                let rendered = self.render(&field.r#type, Scope::Top, true)?;
                let rendered = if reads_no_absent(&field.r#type) {
                    format!("Option<{rendered}>")
                } else {
                    rendered
                };
                let _ = writeln!(text, "{INDENT}pub {field_ident}: {rendered},");
                continue;
            }
            self.use_name("Callable");
            let args_name = ident_of(&format!("{owner}_{field_name}"));
            let taken = format!("pub struct {args_name} ");
            if self.args.iter().any(|known| known.contains(&taken)) {
                return Err(codes::registry_collision(&args_name));
            }
            let returned = self.render(&field.r#type, Scope::Top, false)?;
            let _ = writeln!(
                text,
                "{INDENT}pub {field_ident}: Callable<args::{args_name}, {returned}>,"
            );
            self.use_name(DERIVE);
            let body = self.arguments_body(&field.args, owner)?;
            let head = format!("#[derive(Aosabi)]\n{INDENT}#[aosabi(arguments)]\n{INDENT}");
            self.args.push(match body {
                Some(body) => format!("{head}pub struct {args_name} {body}"),
                None => format!("{head}pub struct {args_name};"),
            });
        }
        Ok(())
    }

    /// Returns the body of an arguments struct, `{ ... }` with one field per
    /// argument in order at the depth of a module, or `None` for no
    /// argument, a unit struct.
    fn arguments_body(
        &mut self,
        args: &[Argument],
        owner: &str,
    ) -> Result<Option<String>, CliError> {
        if args.is_empty() {
            return Ok(None);
        }
        let mut body = String::from("{\n");
        let mut seen = Vec::new();
        for argument in args {
            let argument_ident = unique(&mut seen, ident_of(&argument.name), owner)?;
            let rendered = self.render(&argument.r#type, Scope::Nested, true)?;
            let _ = writeln!(body, "{INDENT}{INDENT}pub {argument_ident}: {rendered},");
        }
        body.push_str(INDENT);
        body.push('}');
        Ok(Some(body))
    }

    /// Writes the module of the root `root` with one struct and one
    /// `Delegate` per field.
    fn root_module(&mut self, root: &str, fields: &ObjMap<Field>) -> Result<(), CliError> {
        self.use_name("Delegate");
        self.use_name("JsObject");
        let mut text = format!("\npub mod {root} {{\n{INDENT}use super::*;\n");
        let mut seen = Vec::new();
        for (field_name, field) in fields {
            let struct_ident = unique(&mut seen, ident_of(field_name), root)?;
            if TAKEN.contains(&struct_ident.as_str())
                || ALIASES.iter().any(|(alias, _)| *alias == struct_ident)
                || MARKERS.contains(&struct_ident.as_str())
            {
                return Err(codes::registry_collision(&struct_ident));
            }
            let declaration = match self.arguments_body(&field.args, root)? {
                Some(body) => format!("pub struct {struct_ident} {body}"),
                None => format!("pub struct {struct_ident};"),
            };
            let returned = self.render(&field.r#type, Scope::Nested, false)?;
            let arguments = if field.args.is_empty() {
                format!("{INDENT}{INDENT}{INDENT}JsObject::new()\n")
            } else {
                self.use_name("IntoArgument");
                let mut inserts =
                    format!("{INDENT}{INDENT}{INDENT}let mut args = JsObject::new();\n");
                let mut idents = Vec::new();
                for argument in &field.args {
                    let argument_ident = unique(&mut idents, ident_of(&argument.name), root)?;
                    let _ = writeln!(
                        inserts,
                        "{INDENT}{INDENT}{INDENT}args.insert(\"{}\", self.{argument_ident}.into_argument());",
                        argument.name
                    );
                }
                let _ = writeln!(inserts, "{INDENT}{INDENT}{INDENT}args");
                inserts
            };
            let _ = write!(
                text,
                "\n{INDENT}{declaration}\n\n{INDENT}impl Delegate for {struct_ident} {{\n\
                 {INDENT}{INDENT}type Registry = super::Registry;\n\
                 {INDENT}{INDENT}type Return = {returned};\n\
                 {INDENT}{INDENT}const PATH: &'static str = \"{root}.{field_name}\";\n\n\
                 {INDENT}{INDENT}fn arguments(&self) -> JsObject {{\n{arguments}{INDENT}{INDENT}}}\n{INDENT}}}\n"
            );
        }
        text.push_str("}\n");
        self.items.push_str(&text);
        Ok(())
    }

    /// Writes the payload enum of the union or the interface `name` under the
    /// derive with the key `remote`, with one variant per type of `members`
    /// and its `impl NeedsTarget`.
    fn variant_enum(&mut self, name: &str, members: &[String]) -> Result<(), CliError> {
        let ident = ident_of(name);
        self.reserve(&[ident.as_str()])?;
        self.use_name(DERIVE);
        let keys = remote_keys(name, &ident);
        let mut text = format!("\n#[derive(Aosabi)]\n#[aosabi({keys})]\npub enum {ident} {{\n");
        for member in members {
            self.refuse_root(member)?;
            let member_ident = ident_of(member);
            let _ = writeln!(text, "{INDENT}{member_ident}({member_ident}),");
        }
        text.push_str("}\n");
        text.push_str(&self.needs_target(&ident));
        self.items.push_str(&text);
        Ok(())
    }

    /// Refuses the root name `name` used as a type, which the file writes as
    /// no type.
    fn refuse_root(&self, name: &str) -> Result<(), CliError> {
        if ROOTS.contains(&name) && matches!(self.schema.get_type(name), Some(NamedType::Object(_)))
        {
            return Err(codes::registry_collision(name));
        }
        Ok(())
    }

    /// Returns the Rust type of `ty` in `scope`, `Option<T>` where it is
    /// nullable, and `Box<T>` around a composite type outside a list when
    /// `field` says it stands in a field of a struct.
    fn render(&mut self, ty: &Type, scope: Scope, field: bool) -> Result<String, CliError> {
        let (base, nullable) = self.resolve(ty, scope, field)?;
        Ok(if nullable {
            format!("Option<{base}>")
        } else {
            base
        })
    }

    /// Returns the type and the nullability of `ty`, `resolveType` of the
    /// reference.
    fn resolve(
        &mut self,
        ty: &Type,
        scope: Scope,
        boxed: bool,
    ) -> Result<(String, bool), CliError> {
        match ty {
            Type::NonNull(inner) => {
                let (base, _) = self.resolve(inner.of_type(), scope, boxed)?;
                Ok((base, false))
            }
            Type::List(list) => {
                let (base, _) = self.resolve(&list.of_type, scope, false)?;
                Ok((format!("Vec<{base}>"), true))
            }
            named => {
                let name = self.named(named, scope)?;
                let composite = matches!(
                    named,
                    Type::Object(_) | Type::Interface(_) | Type::Union(_) | Type::InputObject(_)
                );
                Ok((
                    if boxed && composite {
                        format!("Box<{name}>")
                    } else {
                        name
                    },
                    true,
                ))
            }
        }
    }

    /// Returns the Rust name of the named type `ty` in `scope`.
    fn named(&mut self, ty: &Type, scope: Scope) -> Result<String, CliError> {
        let name = ty
            .as_named()
            .map(|named| named.name().to_owned())
            .unwrap_or_default();
        if matches!(ty, Type::Scalar(_)) && SCALARS.contains(&name.as_str()) {
            if let Some(marker) = MARKERS.iter().find(|marker| **marker == name) {
                self.use_name(marker);
            }
            return Ok(name);
        }
        self.refuse_root(&name)?;
        let ident = ident_of(&name);
        Ok(match scope {
            Scope::Top => ident,
            Scope::Nested => format!("super::{ident}"),
        })
    }

    /// Returns the whole file, with the `delegate` keys `delegate` and the
    /// `types` keys `types`.
    fn finish(self, delegate: &[String], types: &[String]) -> String {
        let mut out = format!("{HEADER}\n{ALLOW}\n{}\n", self.use_line());
        for (alias, target) in ALIASES {
            let _ = writeln!(out, "pub type {alias} = {target};");
        }
        out.push_str(&self.items);
        if !self.args.is_empty() {
            let _ = write!(out, "\npub mod args {{\n{INDENT}use super::*;\n");
            for body in &self.args {
                let _ = writeln!(out, "\n{INDENT}{body}");
            }
            out.push_str("}\n");
        }
        out.push_str("\npub struct Registry;\n\nimpl RemoteRegistry for Registry {\n");
        out.push_str(&keys("DELEGATE", delegate));
        out.push_str(&keys("TYPES", types));
        out.push_str("}\n");
        out
    }

    /// Returns the `use` line of what the file uses.
    fn use_line(&self) -> String {
        let used = |names: &[&'static str]| -> Vec<&'static str> {
            names
                .iter()
                .copied()
                .filter(|name| self.used.contains(name))
                .collect()
        };
        let mut top: Vec<String> = used(&[DERIVE]).into_iter().map(str::to_owned).collect();
        top.extend(used(&MARKERS).into_iter().map(str::to_owned));
        top.extend(group("federation", &used(&FEDERATION_NAMES)));
        top.extend(group("json", &used(&JSON_NAMES)));
        top.extend(group("remote", &used(&REMOTE_NAMES)));
        if let [one] = top.as_slice() {
            return format!("use aosabi::{one};\n");
        }
        let mut out = String::from("use aosabi::{\n");
        for (at, item) in top.iter().enumerate() {
            let last = if at + 1 == top.len() { " //" } else { "" };
            let _ = writeln!(out, "{},{last}", indent_lines(item));
        }
        out.push_str("};\n");
        out
    }
}

/// Returns the group `module::{...}` of `names` in the vertical layout, one
/// name alone without braces, or `None` for no name.
fn group(module: &str, names: &[&str]) -> Option<String> {
    match names {
        [] => None,
        [one] => Some(format!("{module}::{one}")),
        many => {
            let mut out = format!("{module}::{{\n");
            for (at, name) in many.iter().enumerate() {
                let last = if at + 1 == many.len() { " //" } else { "" };
                let _ = writeln!(out, "{INDENT}{name},{last}");
            }
            out.push('}');
            Some(out)
        }
    }
}

/// Returns the keys of `#[aosabi(...)]` of the remote type `name` written
/// `ident`: `remote`, with the typename where the identifier is not the
/// name.
fn remote_keys(name: &str, ident: &str) -> String {
    if ident.trim_start_matches("r#") == name {
        String::from("remote")
    } else {
        format!("remote, typename = \"{name}\"")
    }
}

/// Returns the line `#[aosabi(typename = "<name>")]` of a unit enum written
/// `ident` whose identifier is not its name, and no line where it is.
fn typename_key(name: &str, ident: &str) -> String {
    if ident.trim_start_matches("r#") == name {
        String::new()
    } else {
        format!("#[aosabi(typename = \"{name}\")]\n")
    }
}

/// Returns `item` with every line indented by one level.
fn indent_lines(item: &str) -> String {
    format!("{INDENT}{}", item.replace('\n', &format!("\n{INDENT}")))
}

/// Returns the constant `name` of the registry with the keys `keys` in the
/// layout rustfmt gives it: on one line when that fits [`MAX_WIDTH`], else on
/// the next line when the keys fit [`ARRAY_WIDTH`], else several to a row
/// when no key is wider than [`SHORT_KEY`], else one key per line (,
/// rustfmt of 1.95.0 over random lists of keys).
fn keys(name: &str, keys: &[String]) -> String {
    let head = format!("{INDENT}const {name}: &'static [&'static str] =");
    let quoted: Vec<String> = keys.iter().map(|key| format!("\"{key}\"")).collect();
    let items = quoted.join(", ");
    let line = format!("{head} &[{items}];");
    if line.len() <= MAX_WIDTH {
        return format!("{line}\n");
    }
    let gap = INDENT.repeat(2);
    if items.len() <= ARRAY_WIDTH {
        return format!("{head}\n{gap}&[{items}];\n");
    }
    let mut out = format!("{head} &[\n");
    if quoted.iter().any(|key| key.len() > SHORT_KEY) {
        for key in &quoted {
            let _ = writeln!(out, "{gap}{key},");
        }
    } else if gap.len() + items.len() < MAX_WIDTH {
        let _ = writeln!(out, "{gap}{items},");
    } else {
        // A key and its comma end before the last column, which only a single
        // row may reach.
        let mut row = String::new();
        for key in &quoted {
            if !row.is_empty() && gap.len() + row.len() + key.len() + 2 >= MAX_WIDTH {
                let _ = writeln!(out, "{gap}{row}");
                row.clear();
            }
            if !row.is_empty() {
                row.push(' ');
            }
            row.push_str(key);
            row.push(',');
        }
        let _ = writeln!(out, "{gap}{row}");
    }
    let _ = writeln!(out, "{INDENT}];");
    out
}

/// Returns `name` pushed onto `seen`, refusing with `AO-C0502` a name
/// `owner` holds twice.
fn unique(seen: &mut Vec<String>, name: String, owner: &str) -> Result<String, CliError> {
    if seen.contains(&name) {
        return Err(codes::registry_collision(&format!("{owner}.{name}")));
    }
    seen.push(name.clone());
    Ok(name)
}

/// Returns whether `ty` is a non null type with no value for an absent member.
///
/// The read of a pulled record takes a member the remote leaves out, which it
/// leaves out wherever the client and `needs` do not select it, as the value
/// of an absent member, and an enum, an object, an interface, a union and
/// `Date` have none, so a member of such a type is written `Option<T>` and
/// reads as `None`.
fn reads_no_absent(ty: &Type) -> bool {
    let Type::NonNull(inner) = ty else {
        return false;
    };
    match inner.of_type() {
        Type::Enum(_) | Type::Object(_) | Type::Interface(_) | Type::Union(_) => true,
        named @ Type::Scalar(_) => named.as_named().is_some_and(|named| named.name() == "Date"),
        _ => false,
    }
}

/// Returns the Rust identifier of the GraphQL name `name`: raw for a
/// keyword, with a trailing underscore for a name that cannot be raw.
fn ident_of(name: &str) -> String {
    if NOT_RAW.contains(&name) {
        format!("{name}_")
    } else if is_keyword(name) {
        format!("r#{name}")
    } else {
        name.to_owned()
    }
}

/// Returns whether `name` is a keyword of Rust 2024, strict or reserved,
/// which a name takes raw.
pub(crate) fn is_keyword(name: &str) -> bool {
    KEYWORDS.contains(&name)
}

/// Refuses a remote type whose name the file cannot write: a name of
/// [`TAKEN`], the name of [`CRATE`], or a known scalar name that is no
/// scalar.
fn check_name(name: &str, named: &NamedType) -> Result<(), CliError> {
    let scalar = matches!(named, NamedType::Scalar(_));
    if TAKEN.contains(&name) || name == CRATE || (SCALARS.contains(&name) && !scalar) {
        return Err(codes::registry_collision(name));
    }
    Ok(())
}

/// Returns the failure of a type whose fields or members do not read.
fn definition(error: &DefinitionError) -> CliError {
    CliError::new(error.to_string())
}

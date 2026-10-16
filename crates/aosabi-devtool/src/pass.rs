// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool pass: checks documents through the single pass of
 * aosabi-validate, and writes and prints the field table it reads.
 *
 * No analogue in the reference. The crate under test is the module pass of
 * aosabi-validate, and the judging notebook is tests/notebooks/09_single_pass.ipynb,
 * which holds its verdicts to the rules of aosabi-validate and to the
 * validation of graphql-js through tests/sidecar/graphql-js.mjs.
 */

//! The `pass` primitive of the seam.
//!
//! `--requests PATH` is the check, the batch form of the seam, one response
//! line per request line `{"id":...,"op":"check","sdl":...,"document":...}`.
//! The document is parsed as the parse function of the engine parses it, with
//! the default options, and the pass reads the tokens of the text the parsed
//! document keeps, as it does in the seat of the validator. The response
//! carries `verdict` behind `id` and `op`: `accept`, `refuse` with the `rule`
//! the pass found broken, named as graphql-js names it, `defer` with the
//! `reason` the pass leaves the document to the rules for, or `unparsed` for
//! a text the parser refuses. The process builds one table per distinct SDL
//! and keeps the buffers of the pass from request to request. An SDL that
//! does not build, a schema that `assertValidSchema` refuses and a table that
//! does not build answer `fatal`.
//!
//! `--table PATH --out PATH` writes the field table of the SDL in the first
//! file as one Rust module of statics into the second, the static form of
//! the table, which a probe compiles beside aosabi-validate. Its lookups are
//! functions of a `match` over byte strings. The module opens with the line
//! `SPDX-License-Identifier: MIT`, as section 6 of CONTRIBUTING.md asks of
//! every source file, since the host probe `tests/budget/pass` commits the
//! module written for its device schema. `--dump PATH` prints the table
//! the pass builds for the SDL in the file, the boot form, in the text of
//! `Table::dump`, which the module of `--table` prints byte for byte alike
//! for an SDL built with no options.
//!
//! `--branded-scalars` builds every schema through `create_schema` of
//! aosabi-core, as the primitive `validate` does under the same flag, so the
//! scalars aosabi-core binds answer for their literals. The static form holds no
//! scalar handle, so `--table` refuses the flag. `--max-depth COUNT` sets the
//! bound on nesting of the check, `pass::MAX_DEPTH` when absent, 64.

use std::{
    fmt::Write as _,
    fs,
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    ParseOptions,
    Source, //
};
use aosabi_validate::pass::{
    self,
    Bufs,
    LangFeed,
    Options,
    OwnedTable,
    Reason,
    Verdict,
    table::{
        Kind,
        Scalar,
        Span,
        Table,
        TypeRef, //
    }, //
};

use crate::{
    batch,
    cli::{
        self,
        Grammar,
        Matches,
        Opt, //
    },
    json::reader,
    seam::{
        self,
        Answer,
        Failure, //
    },
    validate::{
        self,
        SCALARS,
        SchemaForm, //
    },
};

/// The name of the primitive.
const NAME: &str = "pass";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "PASS1";

/// The command line of the `pass` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        cli::REQUESTS,
        Opt::path(
            "table",
            "PATH",
            "Write the field table of the SDL in this file as one Rust module of statics",
        ),
        Opt::path("out", "PATH", "The file --table writes its module into"),
        Opt::path(
            "dump",
            "PATH",
            "Print the field table the single pass builds for the SDL in this file",
        ),
        SCALARS,
        Opt::number(
            "max-depth",
            "COUNT",
            "The bound on nesting of the check, 64 when absent",
        ),
    ],
    positional: None,
    conflicts: &[
        ("requests", &["table", "dump", "out"]),
        ("table", &["dump", "branded-scalars", "max-depth"]),
        ("dump", &["out", "max-depth"]),
    ],
};

/// What the `pass` primitive accepts.
#[derive(Debug)]
pub struct PassArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of request lines of the check.
    requests: Option<PathBuf>,
    /// The SDL file whose static table `--out` receives.
    table: Option<PathBuf>,
    /// The file the static table is written into.
    out: Option<PathBuf>,
    /// The SDL file whose boot table is printed.
    dump: Option<PathBuf>,
    /// How each schema is built.
    form: SchemaForm,
    /// The bound on nesting the command line names.
    max_depth: Option<f64>,
}

impl PassArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// # Errors
    ///
    /// Returns a [`cli::Refusal`] when the value of `--max-depth` is no
    /// number.
    pub fn read(matches: &Matches) -> Result<Self, cli::Refusal> {
        Ok(Self {
            smoke: matches.flag("smoke"),
            requests: matches.path("requests"),
            table: matches.path("table"),
            out: matches.path("out"),
            dump: matches.path("dump"),
            form: SchemaForm::of(matches),
            max_depth: matches.number("max-depth")?,
        })
    }
}

/// Runs the `pass` primitive.
pub fn run(args: &PassArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke_at(NAME, LANDS_AT);
    }
    if let Some(sdl) = &args.table {
        let written =
            seam::required(args.out.as_deref(), "--out").and_then(|out| write_table(sdl, out));
        return seam::finish(NAME, written);
    }
    if let Some(sdl) = &args.dump {
        return seam::finish(NAME, dump(sdl, args.form));
    }
    let outcome = options(args.max_depth).and_then(|options| {
        let requests = seam::required(args.requests.as_deref(), "--requests")?;
        check_all(requests, args.form, options)
    });
    seam::finish(NAME, outcome)
}

/// Returns the options of the check, with the bound on nesting `max_depth`.
///
/// # Errors
///
/// Returns [`Failure::Usage`] when the bound is no whole number from 0 to
/// 4,294,967,295.
fn options(max_depth: Option<f64>) -> Result<Options, Failure> {
    let mut options = Options::default();
    if let Some(bound) = max_depth {
        let whole = bound.fract() == 0.0 && (0.0..=f64::from(u32::MAX)).contains(&bound);
        if !whole {
            return Err(Failure::Usage(String::from(
                "--max-depth takes a whole number from 0 to 4294967295",
            )));
        }
        // The value is whole and inside the range of u32, which the test
        // above holds, so the cast neither truncates nor wraps.
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            reason = "the bound is whole and inside the range of u32"
        )]
        let bound = bound as u32;
        options.max_depth = bound;
    }
    Ok(options)
}

/// Builds the table of the SDL `sdl` in the form `form`.
///
/// # Errors
///
/// Returns the reason the schema does not build, is invalid or yields no
/// table.
fn table_of(sdl: Source, form: SchemaForm) -> Result<OwnedTable, JsString> {
    let schema = validate::build(sdl, form)?;
    aosabi_type::assert_valid_schema(&schema).map_err(|error| {
        let mut reason = aosabi_lang::JsStringBuilder::new();
        reason
            .push_str("the schema is invalid: ")
            .push(error.message());
        reason.finish()
    })?;
    OwnedTable::from_schema(&schema)
        .map_err(|error| JsString::from(format!("the table does not build: {error}")))
}

/// Returns the failure of a table that does not build for the file `path`.
fn table_failure(path: &Path, reason: &JsString) -> Failure {
    let reason = aosabi_lang::js_string::to_string_lossy(reason);
    Failure::Data(format!("{}: {reason}", path.display()))
}

/// Prints the boot table of the SDL in the file `path`.
fn dump(path: &Path, form: SchemaForm) -> Result<Answer, Failure> {
    let sdl = seam::read_input(path)?;
    let table = table_of(Source::new(sdl), form).map_err(|reason| table_failure(path, &reason))?;
    let mut text = String::new();
    table
        .view()
        .dump(&mut text)
        .map_err(|_| Failure::Data(String::from("the dump does not write")))?;
    Ok(Answer::text(text))
}

/// Writes the static table of the SDL in the file `sdl` into the file `out`.
fn write_table(sdl: &Path, out: &Path) -> Result<Answer, Failure> {
    let text = seam::read_input(sdl)?;
    let table = table_of(Source::new(text), SchemaForm::Sdl)
        .map_err(|reason| table_failure(sdl, &reason))?;
    let origin = sdl.file_name().map_or_else(
        || String::from("-"),
        |name| name.to_string_lossy().into_owned(),
    );
    let module = module(&table.view(), &origin);
    fs::write(out, module).map_err(|error| Failure::NoInput(out.to_path_buf(), error))?;
    Ok(Answer::Payload(Vec::new()))
}

/// The state of one check over a file of request lines.
struct Checks {
    /// How each schema is built.
    form: SchemaForm,
    /// The options of every check.
    options: Options,
    /// The table of every distinct SDL seen so far, or the reason it does
    /// not build.
    tables: Vec<(String, Result<OwnedTable, JsString>)>,
    /// The buffers of the pass, kept from request to request.
    bufs: Bufs,
}

/// Answers every request line of the file `path`.
fn check_all(path: &Path, form: SchemaForm, options: Options) -> Result<Answer, Failure> {
    let bytes = seam::read_bytes(path)?;
    let mut checks = Checks {
        form,
        options,
        tables: Vec::new(),
        bufs: Bufs::new(),
    };
    let mut out = Vec::new();
    for line in bytes.split(|&byte| byte == b'\n') {
        if line.trim_ascii().is_empty() {
            continue;
        }
        let response = checks.answer(line);
        aosabi_json::write(&mut out, &JsValue::Object(response));
        out.push(b'\n');
    }
    Ok(Answer::Payload(out))
}

/// Returns the response with `id` first, `op` second and the payload or the
/// `fatal` of `outcome` after them, the response of [`crate::batch`].
fn respond(id: JsValue, op: JsValue, outcome: Result<JsObject, JsString>) -> JsObject {
    let mut response = JsObject::new();
    response.insert("id", id);
    response.insert("op", op);
    match outcome {
        Ok(payload) => {
            for (key, value) in &payload {
                response.insert(key.clone(), value.clone());
            }
        }
        Err(fatal) => {
            response.insert("fatal", fatal);
        }
    }
    response
}

/// Returns the `fatal` of an `op` the primitive does not answer, as
/// [`crate::batch`] writes it.
fn unknown_op(op: Option<&JsValue>) -> JsString {
    let printed = op
        .and_then(aosabi_json::to_string)
        .unwrap_or_else(|| String::from("undefined"));
    JsString::from(format!("unknown op {printed}"))
}

impl Checks {
    /// Returns the response line of one request line, with the refusals of
    /// [`crate::batch`] in its order: a line that is not JSON, a line that is
    /// not an object, an unknown `op`, then a missing `id`.
    fn answer(&mut self, line: &[u8]) -> JsObject {
        let request = match reader::parse(line) {
            Ok(JsValue::Object(request)) => request,
            Ok(_) => {
                let fatal = JsString::from("the request line is not a JSON object");
                return respond(JsValue::Null, JsValue::Null, Err(fatal));
            }
            Err(error) => {
                let fatal = JsString::from(format!("the request line is not JSON: {error}"));
                return respond(JsValue::Null, JsValue::Null, Err(fatal));
            }
        };
        let id = request.get("id").cloned().unwrap_or(JsValue::Null);
        let op = match request.get("op") {
            Some(JsValue::String(op)) if *op == "check" => JsValue::String(op.clone()),
            Some(op @ JsValue::String(_)) => {
                let fatal = unknown_op(Some(op));
                return respond(id, op.clone(), Err(fatal));
            }
            other => return respond(id, JsValue::Null, Err(unknown_op(other))),
        };
        if !request.contains_key("id") {
            return respond(id, op, Err(JsString::from("the request carries no id")));
        }
        let outcome = self.check(&request);
        respond(id, op, outcome)
    }

    /// Answers the operation `check`.
    fn check(&mut self, request: &JsObject) -> Result<JsObject, JsString> {
        let sdl = batch::require_string(request, "sdl")?;
        let document = batch::require_source(request, "document")?;
        let index = self.table_index(sdl);
        let Some((_, built)) = self.tables.get(index) else {
            return Err(JsString::from("check: the table is gone"));
        };
        let table = match built {
            Ok(table) => table,
            Err(reason) => {
                let mut fatal = aosabi_lang::JsStringBuilder::new();
                fatal.push_str("check: ").push(reason);
                return Err(fatal.finish());
            }
        };
        let mut payload = JsObject::with_capacity(2);
        let Ok(parsed) = aosabi_lang::parse(document, ParseOptions::default()) else {
            payload.insert("verdict", "unparsed");
            return Ok(payload);
        };
        let verdict = match LangFeed::of(&parsed) {
            Some(mut feed) => pass::check(
                &table.view(),
                &mut feed,
                &table.scalars(),
                self.options,
                &mut self.bufs,
            ),
            None => Verdict::Defer(Reason::Internal),
        };
        payload.insert("verdict", verdict.word());
        match verdict {
            Verdict::Accept => {}
            Verdict::Refuse(rule) => {
                payload.insert("rule", rule.name());
            }
            Verdict::Defer(reason) => {
                payload.insert("reason", reason.name());
            }
        }
        Ok(payload)
    }

    /// Returns the index of the table of `sdl`, which it builds on first use.
    fn table_index(&mut self, sdl: &str) -> usize {
        if let Some(index) = self.tables.iter().position(|(text, _)| text == sdl) {
            return index;
        }
        let built = table_of(Source::new(String::from(sdl)), self.form);
        self.tables.push((String::from(sdl), built));
        self.tables.len() - 1
    }
}

/// Returns the Rust module of statics that holds `table`, a table the SDL
/// file named `origin` built.
fn module(table: &Table<'_>, origin: &str) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "// SPDX-License-Identifier: MIT\n\
         // @generated by aosabi-devtool pass --table from {origin}. Do not edit.\n\
         //! The field table of the single pass for one schema, the static form, written by\n\
         //! `aosabi-devtool pass --table`.\n"
    );
    out.push_str(
        "// The module is written by a program for one schema, so its length, its\n\
         // literals and its form follow the schema and not the style of the workspace.\n\
         #![allow(clippy::all, clippy::pedantic)]\n\n",
    );
    out.push_str(
        "use aosabi_validate::pass::table::{\n    ArgRow,\n    DirectiveRow,\n    FieldLookup,\n    \
         FieldRow,\n    Kind,\n    Lookup,\n    Scalar,\n    Span,\n    Table,\n    TypeRef,\n    \
         TypeRow, //\n};\n\n",
    );
    module_bytes(
        &mut out,
        "NAMES",
        "The bytes of every name, key and default.",
        table.names,
    );
    module_bytes(
        &mut out,
        "MODS",
        "The wrappers of every type reference.",
        table.mods,
    );
    module_types(&mut out, table);
    module_fields(&mut out, table);
    module_sets(&mut out, table);
    module_lookups(&mut out, table);
    let _ = writeln!(
        out,
        "/// The table.\n#[rustfmt::skip]\npub static TABLE: Table<'static> = Table {{\n    \
         names: NAMES,\n    types: &TYPES,\n    fields: &FIELDS,\n    args: &ARGS,\n    \
         values: &VALUES,\n    mods: MODS,\n    possible: &POSSIBLE,\n    words: {},\n    \
         directives: &DIRECTIVES,\n    meta: {},\n    query: {},\n    mutation: {},\n    \
         subscription: {},\n    include: {},\n    skip: {},\n    meta_corner: {},\n    \
         type_named: Lookup::Static(type_named),\n    \
         field_named: FieldLookup::Static(field_named),\n    \
         directive_named: Lookup::Static(directive_named),\n}};",
        table.words,
        table.meta,
        table.query,
        table.mutation,
        table.subscription,
        table.include,
        table.skip,
        table.meta_corner
    );
    out
}

/// Appends a static of the byte string `bytes` named `name`.
fn module_bytes(out: &mut String, name: &str, doc: &str, bytes: &[u8]) {
    let _ = write!(
        out,
        "/// {doc}\n#[rustfmt::skip]\npub static {name}: &[u8] = "
    );
    byte_string(out, bytes);
    out.push_str(";\n\n");
}

/// Appends `bytes` as a Rust byte string literal, each byte outside
/// printable ASCII and each quote and backslash as an escape.
fn byte_string(out: &mut String, bytes: &[u8]) {
    out.push_str("b\"");
    for &byte in bytes {
        match byte {
            b'"' => out.push_str("\\\""),
            b'\\' => out.push_str("\\\\"),
            0x20..=0x7e => out.push(char::from(byte)),
            _ => {
                let _ = write!(out, "\\x{byte:02x}");
            }
        }
    }
    out.push('"');
}

/// Returns a span as a Rust expression.
fn span(span: Span) -> String {
    format!("Span {{ at: {}, len: {} }}", span.at, span.len)
}

/// Returns a type reference as a Rust expression.
fn type_ref(ty: TypeRef) -> String {
    format!("TypeRef {{ named: {}, mods: {} }}", ty.named, span(ty.mods))
}

/// Appends the static of the type rows of `table`.
fn module_types(out: &mut String, table: &Table<'_>) {
    let _ = writeln!(
        out,
        "/// Every named type, in the order of the type map.\n#[rustfmt::skip]\n\
         pub static TYPES: [TypeRow; {}] = [",
        table.types.len()
    );
    for row in table.types {
        let kind = match row.kind {
            Kind::Scalar => "Scalar",
            Kind::Object => "Object",
            Kind::Interface => "Interface",
            Kind::Union => "Union",
            Kind::Enum => "Enum",
            Kind::InputObject => "InputObject",
        };
        let scalar = match row.scalar {
            Scalar::None => String::from("Scalar::None"),
            Scalar::Int => String::from("Scalar::Int"),
            Scalar::Float => String::from("Scalar::Float"),
            Scalar::String => String::from("Scalar::String"),
            Scalar::Boolean => String::from("Scalar::Boolean"),
            Scalar::Id => String::from("Scalar::Id"),
            Scalar::Custom(index) => format!("Scalar::Custom({index})"),
        };
        let _ = writeln!(
            out,
            "    TypeRow {{ name: {}, kind: Kind::{kind}, scalar: {scalar}, fields: {}, possible: {}, \
             values: {}, inputs: {}, one_of: {}, introspection: {} }},",
            span(row.name),
            span(row.fields),
            row.possible,
            span(row.values),
            span(row.inputs),
            row.one_of,
            row.introspection
        );
    }
    out.push_str("];\n\n");
}

/// Appends the statics of the field and argument rows of `table`.
fn module_fields(out: &mut String, table: &Table<'_>) {
    let _ = writeln!(
        out,
        "/// Every field and the three meta fields.\n#[rustfmt::skip]\n\
         pub static FIELDS: [FieldRow; {}] = [",
        table.fields.len()
    );
    for field in table.fields {
        let _ = writeln!(
            out,
            "    FieldRow {{ name: {}, key: {}, ty: {}, args: {}, leaf: {} }},",
            span(field.name),
            span(field.key),
            type_ref(field.ty),
            span(field.args),
            field.leaf
        );
    }
    out.push_str("];\n\n");
    let _ = writeln!(
        out,
        "/// Every argument and input field.\n#[rustfmt::skip]\n\
         pub static ARGS: [ArgRow; {}] = [",
        table.args.len()
    );
    for arg in table.args {
        let default = arg.default.map_or_else(
            || String::from("None"),
            |default| format!("Some({})", span(default)),
        );
        let _ = writeln!(
            out,
            "    ArgRow {{ name: {}, ty: {}, default: {default} }},",
            span(arg.name),
            type_ref(arg.ty)
        );
    }
    out.push_str("];\n\n");
}

/// Appends the statics of the enum values, the possible types and the
/// directives of `table`.
fn module_sets(out: &mut String, table: &Table<'_>) {
    let _ = writeln!(
        out,
        "/// Every enum value.\n#[rustfmt::skip]\npub static VALUES: [Span; {}] = [",
        table.values.len()
    );
    for value in table.values {
        let _ = writeln!(out, "    {},", span(*value));
    }
    out.push_str("];\n\n");
    let _ = writeln!(
        out,
        "/// The possible types of every type.\n#[rustfmt::skip]\npub static POSSIBLE: [u64; {}] = [",
        table.possible.len()
    );
    for word in table.possible {
        let _ = writeln!(
            out,
            "    0x{:04x}_{:04x}_{:04x}_{:04x},",
            word >> 48,
            (word >> 32) & 0xffff,
            (word >> 16) & 0xffff,
            word & 0xffff
        );
    }
    out.push_str("];\n\n");
    let _ = writeln!(
        out,
        "/// Every directive.\n#[rustfmt::skip]\npub static DIRECTIVES: [DirectiveRow; {}] = [",
        table.directives.len()
    );
    for directive in table.directives {
        let _ = writeln!(
            out,
            "    DirectiveRow {{ name: {}, locations: {}, args: {}, repeatable: {} }},",
            span(directive.name),
            directive.locations,
            span(directive.args),
            directive.repeatable
        );
    }
    out.push_str("];\n\n");
}

/// Appends the three lookup functions of `table`, each a `match` over byte
/// strings.
fn module_lookups(out: &mut String, table: &Table<'_>) {
    let bytes = |span: Span| table.bytes(span).unwrap_or_default();
    out.push_str(
        "/// Returns the type named `name`.\n#[rustfmt::skip]\n\
         fn type_named(name: &[u8]) -> Option<u16> {\n    match name {\n",
    );
    for (index, row) in table.types.iter().enumerate() {
        out.push_str("        ");
        byte_string(out, bytes(row.name));
        let _ = writeln!(out, " => Some({index}),");
    }
    out.push_str("        _ => None,\n    }\n}\n\n");
    out.push_str(
        "/// Returns the index of the field `name` of the type `ty` among its fields.\n\
         #[rustfmt::skip]\nfn field_named(ty: u16, name: &[u8]) -> Option<u16> {\n    match ty {\n",
    );
    for (index, row) in table.types.iter().enumerate() {
        let fields = row
            .fields
            .range()
            .and_then(|range| table.fields.get(range))
            .unwrap_or_default();
        if fields.is_empty() {
            continue;
        }
        let _ = writeln!(out, "        {index} => match name {{");
        for (at, field) in fields.iter().enumerate() {
            out.push_str("            ");
            byte_string(out, bytes(field.name));
            let _ = writeln!(out, " => Some({at}),");
        }
        out.push_str("            _ => None,\n        },\n");
    }
    out.push_str("        _ => None,\n    }\n}\n\n");
    out.push_str(
        "/// Returns the directive named `name`.\n#[rustfmt::skip]\n\
         fn directive_named(name: &[u8]) -> Option<u16> {\n    match name {\n",
    );
    for (index, row) in table.directives.iter().enumerate() {
        out.push_str("        ");
        byte_string(out, bytes(row.name));
        let _ = writeln!(out, " => Some({index}),");
    }
    out.push_str("        _ => None,\n    }\n}\n\n");
}

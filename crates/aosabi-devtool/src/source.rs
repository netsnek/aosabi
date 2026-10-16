// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool source: reads the plain source of a crate, its entry, the
 * roots it registers, what they reach, and the names of the source path.
 *
 * No analogue in the reference. The crate under test is aosabi-build with its
 * feature source, the reader of the source path of aosabi-builder, which
 * stands in for the program the TypeScript checker builds for the
 * builder. The judging notebook is tests/notebooks/12_authoring_parity.ipynb.
 */

//! The `source` primitive of the seam.
//!
//! Its one form is `--requests PATH`, the batch form of [`crate::batch`].
//!
//! | `op` | Request fields | Payload |
//! | --- | --- | --- |
//! | `read` | `package`, optional `start` with `registered`, `described` and `names`, optional `target` and `release` | the crate, its entry, its registrations, its refusals, its macro calls and its reach, or `error` |
//! | `names` | `kind`, `rust` | `graphql` |
//! | `build` | `package`, optional `target` and `release` | `plain`, and the artifacts of the source path, `schema`, `ir`, `resolvers`, `descriptors` and `authoring`, with `warnings` and `invalid`, or `refused`, or `error` |
//!
//! The operations `read` and `build` read a crate as rustc compiles it for
//! the target the string `target` names, the host where the request names
//! none, in the release profile of the card server where `release` is `true`,
//! with `-C opt-level=3` and `-C panic=abort`, and in the dev profile else,
//! over the active set and the target information the rustc of `RUSTC`, or
//! `rustc` from the path, prints in the working directory with no flag of the
//! environment, [`aosabi_build::source::Cfg::from_rustc`], asked once per
//! target and profile in a run.
//!
//! The operation `read` reads the library crate of the package in the
//! directory `package` through [`aosabi_build::source::read`], with the
//! library of every path dependency the manifests name, and walks the reach
//! from the roots `start` names, by default the roots the entry registers.
//! Its payload holds `crate`, `root`, `files`, `entry`, `registered`,
//! `refused`, `macros`, `roots`, `unmatched`, `reached`, `members`, `inputs`
//! and `unresolved`, every place as a file relative to the package and a
//! line, and `error` in their place when the reader reads no crate. The
//! operation `names` takes the Rust names of the array `rust`, of
//! the kind `kind`, `field`, `method`, `argument`, `variant` or `type`,
//! through [`graphql_name`] and answers the GraphQL names in the same order.
//!
//! The operation `build` runs the source path over the package in the
//! directory `package` through [`aosabi_build::build_source`], the reader,
//! the third type source and the inference, and answers the bytes of the five
//! artifacts as the command line writes them, `schema`, `ir`, `resolvers`,
//! `descriptors` and `authoring`, each a string, with the warnings of the
//! inference under `warnings` and the message of the validation under
//! `invalid` when the schema is no valid schema, which the command line
//! refuses before it writes a byte. A crate the builder refuses answers
//! `refused`, each refusal with its file, its line and its message, and a
//! crate the reader reads no source of answers `error`. Every answer holds
//! `plain`, whether the crate is plain, which no fragment of `graphql!`
//! holds.
//!
//! The reader is a feature of `aosabi-build` that the seam turns on for every
//! target but `wasm32`, so this primitive exists there alone, and the build
//! of the seam for `wasm32-unknown-unknown` leaves `syn` out.

use std::{
    ffi::OsString,
    path::Path,
    process::{
        Command,
        ExitCode, //
    },
    sync::Mutex,
};

use aosabi_build::{
    BuildError,
    source::{
        self,
        Cfg,
        Crate,
        Place,
        Reach,
        Start,
        names::{
            NameKind,
            graphql_name, //
        }, //
    },
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    seam::{
        self,
        BatchArgs, //
    }, //
};

/// The name of the primitive.
const NAME: &str = "source";

/// The milestone the primitive lands at.
const LANDS_AT: &str = "APL1";

/// The operations of the batch form.
const OPERATIONS: &Operations = &[("read", op_read), ("names", op_names), ("build", op_build)];

/// A target, `None` for the host, and whether the profile is the release
/// profile, which an active set belongs to.
type Build = (Option<String>, bool);

/// The active sets a run asked rustc for, by target and profile.
static ACTIVE_SETS: Mutex<Vec<(Build, Cfg)>> = Mutex::new(Vec::new());

/// Runs the `source` primitive, whose one form is the batch form.
pub fn run(args: &BatchArgs) -> ExitCode {
    seam::run_batch(NAME, LANDS_AT, OPERATIONS, args)
}

/// Answers the operation `read`.
fn op_read(request: &JsObject) -> Result<JsObject, JsString> {
    let package = batch::require_string(request, "package")?;
    let start = start(request)?;
    let cfg = active_set(request)?;
    let mut payload = JsObject::new();
    match source::read(Path::new(package), &cfg) {
        Ok(krate) => {
            write_crate(&mut payload, &krate);
            write_reach(&mut payload, &krate.reach(&start));
        }
        Err(error) => {
            payload.insert("error", error.to_string());
        }
    }
    Ok(payload)
}

/// Answers the operation `build`.
fn op_build(request: &JsObject) -> Result<JsObject, JsString> {
    let package = Path::new(batch::require_string(request, "package")?);
    let cfg = active_set(request)?;
    let mut payload = JsObject::new();
    let plain = source::read(package, &cfg).is_ok_and(|krate| krate.is_plain());
    payload.insert("plain", plain);
    match aosabi_build::build_source(package, &cfg) {
        Ok(made) => {
            let built = &made.built;
            payload.insert("schema", built.type_defs.as_str());
            payload.insert("ir", format!("{}\n", aosabi_ir::to_json(&built.ir)));
            payload.insert("resolvers", aosabi_build::resolvers_js(built));
            payload.insert("descriptors", format!("{}\n", made.descriptors));
            payload.insert("authoring", made.authoring.as_str());
            let warnings: Vec<JsValue> = built
                .warnings
                .iter()
                .map(|warning| JsValue::from(warning.as_str()))
                .collect();
            payload.insert("warnings", warnings);
            if let Err(invalid) = aosabi_build::assert_schema_is_valid(&built.type_defs) {
                payload.insert("invalid", invalid.to_string());
            }
        }
        Err(BuildError::Refused(refusals)) => {
            let refused: Vec<JsValue> = refusals
                .iter()
                .map(|refusal| {
                    let mut object = JsObject::new();
                    place_into(&mut object, &refusal.place);
                    object.insert("message", refusal.message.as_str());
                    JsValue::Object(object)
                })
                .collect();
            payload.insert("refused", refused);
        }
        Err(error) => {
            payload.insert("error", error.to_string());
        }
    }
    Ok(payload)
}

/// Answers the operation `names`.
fn op_names(request: &JsObject) -> Result<JsObject, JsString> {
    let kind = match batch::require_string(request, "kind")? {
        "field" => NameKind::Field,
        "method" => NameKind::Method,
        "argument" => NameKind::Argument,
        "variant" => NameKind::Variant,
        "type" => NameKind::Type,
        other => {
            return Err(JsString::from(format!(
                "names: the kind {other} is none of field, method, argument, variant and type"
            )));
        }
    };
    let spellings = strings(request, "rust")?;
    let graphql: Vec<JsValue> = spellings
        .iter()
        .map(|rust| JsValue::from(graphql_name(kind, rust)))
        .collect();
    let mut payload = JsObject::with_capacity(1);
    payload.insert("graphql", graphql);
    Ok(payload)
}

/// Returns the active set the build `request` names, of the target of its
/// field `target`, the host where it names none, in the release profile where
/// its field `release` is `true`, which the rustc of `RUSTC`, or `rustc` from
/// the path, prints in the working directory, asked once per target and
/// profile in a run.
fn active_set(request: &JsObject) -> Result<Cfg, JsString> {
    let triple = match request.get("target") {
        None => None,
        Some(JsValue::String(text)) => Some(
            text.as_str()
                .map(String::from)
                .ok_or_else(|| JsString::from("the field target holds a lone surrogate"))?,
        ),
        Some(_) => return Err(JsString::from("the field target is no string")),
    };
    let release = match request.get("release") {
        None => false,
        Some(JsValue::Bool(release)) => *release,
        Some(_) => return Err(JsString::from("the field release is no boolean")),
    };
    let key = (triple, release);
    let mut sets = ACTIVE_SETS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some((_, cfg)) = sets.iter().find(|(known, _)| *known == key) {
        return Ok(cfg.clone());
    }
    let rustc = std::env::var_os("RUSTC")
        .filter(|rustc| !rustc.is_empty())
        .unwrap_or_else(|| OsString::from("rustc"));
    // The release profile is the one of the server package, which aborts on a
    // panic and passes `-C panic=abort` as a flag of its build as well.
    let flags: Vec<String> = if release {
        vec![String::from("-C"), String::from("panic=abort")]
    } else {
        Vec::new()
    };
    let cfg = Cfg::from_rustc(
        &|| Command::new(&rustc),
        key.0.as_deref(),
        &Cfg::profile_flags(release, release),
        &flags,
    )
    .map_err(JsString::from)?;
    sets.push((key, cfg.clone()));
    Ok(cfg)
}

/// Returns the start of the reach `request` names, the registered roots by
/// default.
fn start(request: &JsObject) -> Result<Start, JsString> {
    let Some(value) = request.get("start") else {
        return Ok(Start {
            registered: true,
            ..Start::default()
        });
    };
    let JsValue::Object(fields) = value else {
        return Err(JsString::from("read: the field start is no object"));
    };
    let flag = |name: &str| matches!(fields.get(name), Some(JsValue::Bool(true)));
    let names = if fields.contains_key("names") {
        strings(fields, "names")?
    } else {
        Vec::new()
    };
    Ok(Start {
        registered: flag("registered"),
        described: flag("described"),
        names,
    })
}

/// Returns the strings of the array field `name` of `object`.
fn strings(object: &JsObject, name: &str) -> Result<Vec<String>, JsString> {
    let Some(JsValue::Array(values)) = object.get(name) else {
        return Err(JsString::from(format!("the field {name} is no array")));
    };
    values
        .iter()
        .map(|value| match value {
            JsValue::String(text) => text
                .as_str()
                .map(String::from)
                .ok_or_else(|| JsString::from(format!("the field {name} holds a lone surrogate"))),
            _ => Err(JsString::from(format!("the field {name} holds no string"))),
        })
        .collect()
}

/// Writes what `krate` states into `payload`.
fn write_crate(payload: &mut JsObject, krate: &Crate) {
    payload.insert("crate", krate.name());
    payload.insert("root", path_text(krate.root()));
    let files: Vec<JsValue> = krate
        .files()
        .iter()
        .map(|file| JsValue::from(path_text(file)))
        .collect();
    payload.insert("files", files);
    let entry = krate.entry().map_or(JsValue::Null, |entry| {
        let mut object = JsObject::new();
        object.insert("function", entry.function.as_str());
        place_into(&mut object, &entry.place);
        JsValue::Object(object)
    });
    payload.insert("entry", entry);
    let registered: Vec<JsValue> = krate
        .registrations()
        .iter()
        .map(|registration| {
            let mut object = JsObject::new();
            object.insert("root", registration.kind.as_str());
            object.insert("type", registration.path.as_str());
            place_into(&mut object, &registration.place);
            let through: Vec<JsValue> = registration
                .through
                .iter()
                .map(|path| JsValue::from(path.as_str()))
                .collect();
            object.insert("through", through);
            JsValue::Object(object)
        })
        .collect();
    payload.insert("registered", registered);
    let refused: Vec<JsValue> = krate
        .refusals()
        .iter()
        .map(|refusal| {
            let mut object = JsObject::new();
            place_into(&mut object, &refusal.place);
            object.insert("message", refusal.message.as_str());
            JsValue::Object(object)
        })
        .collect();
    payload.insert("refused", refused);
    let macros: Vec<JsValue> = krate
        .macro_calls()
        .iter()
        .map(|(path, place)| {
            let mut object = JsObject::new();
            object.insert("macro", path.as_str());
            place_into(&mut object, place);
            JsValue::Object(object)
        })
        .collect();
    payload.insert("macros", macros);
}

/// Writes the reach `reach` into `payload`.
fn write_reach(payload: &mut JsObject, reach: &Reach) {
    let roots: Vec<JsValue> = reach
        .roots
        .iter()
        .map(|root| {
            let mut object = JsObject::new();
            object.insert("type", root.path.as_str());
            object.insert("from", root.from.as_str());
            place_into(&mut object, &root.place);
            JsValue::Object(object)
        })
        .collect();
    payload.insert("roots", roots);
    let unmatched: Vec<JsValue> = reach
        .unmatched
        .iter()
        .map(|name| JsValue::from(name.as_str()))
        .collect();
    payload.insert("unmatched", unmatched);
    let reached: Vec<JsValue> = reach
        .reached
        .iter()
        .map(|reached| {
            let mut object = JsObject::new();
            object.insert("kind", reached.kind.as_str());
            object.insert("type", reached.path.as_str());
            place_into(&mut object, &reached.place);
            JsValue::Object(object)
        })
        .collect();
    payload.insert("reached", reached);
    let members: Vec<JsValue> = reach
        .members
        .iter()
        .map(|member| {
            let mut object = JsObject::new();
            object.insert("key", member.key.as_str());
            object.insert("kind", kind_text(member.kind));
            object.insert("graphql", member.graphql.as_str());
            place_into(&mut object, &member.place);
            let docs: Vec<JsValue> = member
                .docs
                .iter()
                .map(|line| JsValue::from(line.as_str()))
                .collect();
            object.insert("docs", docs);
            object.insert("async", member.asynchronous);
            JsValue::Object(object)
        })
        .collect();
    payload.insert("members", members);
    let inputs: Vec<JsValue> = reach
        .inputs
        .iter()
        .map(|input| {
            let mut object = JsObject::new();
            object.insert("key", input.key.as_str());
            object.insert("type", input.path.as_str());
            object.insert("graphql", input.graphql.as_str());
            JsValue::Object(object)
        })
        .collect();
    payload.insert("inputs", inputs);
    let unresolved: Vec<JsValue> = reach
        .unresolved
        .iter()
        .map(|unresolved| {
            let mut object = JsObject::new();
            object.insert("site", unresolved.site.as_str());
            object.insert("type", unresolved.ty.as_str());
            object.insert("reason", unresolved.reason.as_str());
            object.insert("detail", unresolved.detail.as_str());
            place_into(&mut object, &unresolved.place);
            JsValue::Object(object)
        })
        .collect();
    payload.insert("unresolved", unresolved);
}

/// Adds the file and the line of `place` to `object`.
fn place_into(object: &mut JsObject, place: &Place) {
    object.insert("file", path_text(&place.file));
    object.insert("line", u64::try_from(place.line).unwrap_or(u64::MAX));
}

/// Returns a path relative to the package with `/` between its parts.
fn path_text(path: &Path) -> String {
    path.components()
        .map(|part| part.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Returns the word of a name kind in an answer.
fn kind_text(kind: NameKind) -> &'static str {
    match kind {
        NameKind::Field => "field",
        NameKind::Method => "method",
        NameKind::Argument => "argument",
        NameKind::Variant => "variant",
        NameKind::Type => "type",
    }
}

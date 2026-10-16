// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool validate: validates a document against a schema and reports
 * the errors graphql-js reports.
 *
 * No analogue in the reference. The crate under test is aosabi-validate, and
 * the judging notebook is tests/notebooks/02_language.ipynb, which compares
 * the bytes against a node sidecar running the pinned graphql-js.
 */

//! The `validate` primitive of the seam.
//!
//! `--schema PATH` names an SDL file that is built the way `buildSchema`
//! builds it with no options, and `--document PATH` names the document. One of
//! the two paths may be `-`, which is stdin. The payload is the error array of
//! `validate` in the `toJSON` shape of the reference, `[]` for a valid
//! document, and the exit code is 0 either way, because the array is the
//! answer.
//!
//! All specified rules run, which is the default of the reference.
//! `--rule NAME` runs the one specified rule of that name alone, where the
//! name is the function name of the reference, such as
//! `FieldsOnCorrectTypeRule`.
//!
//! An SDL that does not build, a document that does not parse, a rule name no
//! specified rule carries and a throw of `validate` itself are harness
//! mistakes here and exit 65, because `build-schema` and `parse` are the
//! primitives that ask those questions.
//!
//! `--requests PATH` is the batch form, which answers the operation
//! `validate` of [`crate::batch`].
//!
//! `--branded-scalars` builds the schema through `create_schema` of
//! aosabi-core with empty resolver maps, in both forms, which binds its
//! scalars over every schema, `ID`, `GID` where the SDL declares it, `Date`,
//! `JSON`, `JSONObject`, `Void` and `Number`. A schema built from its SDL
//! gives every custom scalar a literal parse that takes every literal, while a
//! bound scalar refuses what it refuses on a served path, so the flag holds
//! the rules to those refusals.

use std::{
    path::PathBuf,
    process::ExitCode,
    sync::Arc, //
};

use aosabi_core::{
    ResolverMap,
    create_schema, //
};
use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    JsStringBuilder,
    ParseOptions,
    Source,
    js_string::to_string_lossy, //
};
use aosabi_type::{
    BuildSchemaOptions,
    Schema, //
};
use aosabi_validate::{
    ValidateOptions,
    ValidationRule, //
};

use crate::{
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Grammar,
        Matches,
        Opt,
        Refusal, //
    },
    seam::{
        self,
        Answer,
        Failure, //
    }, //
};

/// The name of the primitive, which opens every refusal it writes.
const NAME: &str = "validate";

/// The operation of the protocol that reaches aosabi-validate.
const OPERATIONS: &Operations = &[("validate", op_validate)];

/// The operation of the protocol under `--branded-scalars`.
const BOUND_OPERATIONS: &Operations = &[("validate", op_validate_bound)];

/// The flag that builds a schema with the scalars aosabi-core binds.
pub const SCALARS: Opt = Opt::flag(
    "branded-scalars",
    "Build the schema through create_schema of aosabi-core, which binds its scalars",
);

/// The command line of the `validate` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::path(
            "schema",
            "PATH",
            "Read the SDL of the schema from this file, or from stdin when it is `-`",
        ),
        Opt::path(
            "document",
            "PATH",
            "Read the GraphQL document from this file, or from stdin when it is `-`",
        ),
        Opt::value(
            "rule",
            "NAME",
            "Run this one specified rule alone, named as graphql-js names it",
        ),
        cli::REQUESTS,
        SCALARS,
    ],
    positional: None,
    conflicts: &[("requests", &["schema", "document", "rule"])],
};

/// What the `validate` primitive accepts.
#[derive(Debug)]
pub struct ValidateArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of the SDL, where `-` is stdin.
    schema: Option<PathBuf>,
    /// The file of the document, where `-` is stdin.
    document: Option<PathBuf>,
    /// The one specified rule to run alone.
    rule: Option<String>,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
    /// How the schema is built.
    form: SchemaForm,
}

impl ValidateArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when the name of `--rule` is not UTF-8.
    pub fn read(matches: &Matches) -> Result<Self, Refusal> {
        Ok(Self {
            smoke: matches.flag("smoke"),
            schema: matches.path("schema"),
            document: matches.path("document"),
            rule: matches.text("rule")?,
            requests: matches.path("requests"),
            form: SchemaForm::of(matches),
        })
    }
}

/// How a primitive builds the schema of an SDL text.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SchemaForm {
    /// `buildSchema` of the SDL with no options, whose custom scalars take
    /// every literal.
    Sdl,
    /// `create_schema` of aosabi-core with empty resolver maps, which binds
    /// its scalars over every schema.
    Bound,
}

impl SchemaForm {
    /// Returns the form the flag `--branded-scalars` of `matches` names.
    pub fn of(matches: &Matches) -> Self {
        if matches.flag("branded-scalars") {
            Self::Bound
        } else {
            Self::Sdl
        }
    }
}

/// Builds the schema of the SDL `sdl` in the form `form`.
///
/// # Errors
///
/// Returns `the sdl does not build: ` followed by the message of what the
/// build throws, and the same head for an SDL that carries a lone surrogate,
/// which the bound form cannot read.
pub fn build(sdl: Source, form: SchemaForm) -> Result<Arc<Schema>, JsString> {
    match form {
        SchemaForm::Sdl => aosabi_validate::build_schema(
            sdl,
            BuildSchemaOptions::default(),
            ParseOptions::default(),
        )
        .map(Arc::new)
        .map_err(|error| reason("the sdl does not build: ", error.message())),
        SchemaForm::Bound => {
            let Ok(text) = core::str::from_utf8(sdl.bytes()) else {
                return Err(JsString::from(
                    "the sdl does not build: it carries a lone surrogate",
                ));
            };
            create_schema(text, ResolverMap::default(), ResolverMap::default())
                .map_err(|error| reason("the sdl does not build: ", error.message()))
        }
    }
}

/// Runs the `validate` primitive.
pub fn run(args: &ValidateArgs) -> ExitCode {
    if args.smoke {
        return seam::smoke(NAME);
    }
    if let Some(requests) = &args.requests {
        let operations = match args.form {
            SchemaForm::Sdl => OPERATIONS,
            SchemaForm::Bound => BOUND_OPERATIONS,
        };
        return seam::finish(NAME, batch::run(operations, requests));
    }
    seam::finish(NAME, answer(args))
}

/// Returns the error array of the one call the flags name.
fn answer(args: &ValidateArgs) -> Result<Answer, Failure> {
    let schema = seam::required(args.schema.as_deref(), "--schema")?;
    let document = seam::required(args.document.as_deref(), "--document")?;
    seam::refuse_shared_stdin(schema, document)?;
    let call = Call {
        sdl: seam::read_input(schema)?.into(),
        text: seam::read_input(document)?.into(),
        rule: args.rule.as_deref(),
        parse_options: ParseOptions::default(),
        validate_options: ValidateOptions::default(),
        form: args.form,
    };
    validate(call)
        .map(Answer::errors)
        .map_err(|reason| Failure::Data(to_string_lossy(&reason).into_owned()))
}

/// One call of `validate`, which is everything `opValidate` of the oracle
/// reads out of a request.
struct Call<'r> {
    /// The SDL of the schema, built with no options.
    sdl: Source,
    /// The document.
    text: Source,
    /// The name of the one rule to run, every specified rule when absent.
    rule: Option<&'r str>,
    /// The parser options of the document.
    parse_options: ParseOptions,
    /// The options of the validation itself.
    validate_options: ValidateOptions,
    /// How the schema is built.
    form: SchemaForm,
}

/// Validates the document of `call` against the schema of its SDL.
///
/// The steps and the refusals are those of `opValidate` in
/// tests/sidecar/graphql-js.mjs.
///
/// # Errors
///
/// Returns the reason a step before the error array failed, without the name
/// of the operation in front of it.
fn validate(call: Call<'_>) -> Result<Vec<JsValue>, JsString> {
    let schema = build(call.sdl, call.form)?;
    let document = aosabi_lang::parse(call.text, call.parse_options)
        .map_err(|error| reason("the document does not parse: ", &error.message))?;
    let rule = call.rule.map(rule_named).transpose()?;
    let rules = rule.as_ref().map(core::slice::from_ref);
    let errors = aosabi_validate::validate(&schema, &document, rules, call.validate_options, None)
        .map_err(|error| error.to_string())?;
    Ok(errors.iter().map(seam::graphql_error_json).collect())
}

/// Returns `head` followed by `message`, the reason a step refused.
///
/// The oracle writes the `message` of what the step threw behind the name of
/// the step, and the message of a syntax error is a JavaScript string that may
/// end a quote on a leading surrogate, so the reason keeps its code units.
fn reason(head: &str, message: &JsString) -> JsString {
    let mut reason = JsStringBuilder::new();
    reason.push_str(head).push(message);
    reason.finish()
}

/// Returns the validation options of `request`, which travel as `options`.
///
/// The oracle hands the one `options` object to `parse` and to `validate`, and
/// `validate` reads `maxErrors` out of it. The reference reads
/// `options?.maxErrors ?? 100`, so an absent limit, `undefined` and `null` all
/// name the default.
///
/// # Errors
///
/// Returns a `fatal` when `options` is not an object or `maxErrors` is not a
/// number.
fn validate_options(request: &JsObject) -> Result<ValidateOptions, String> {
    let limit = batch::options_of(request)?.and_then(|options| options.get("maxErrors"));
    let max_errors = match limit {
        None | Some(JsValue::Undefined | JsValue::Null) => None,
        Some(JsValue::Number(limit)) => Some(error_limit(*limit)),
        Some(_) => return Err(format!("{NAME}: options.maxErrors is not a number")),
    };
    Ok(ValidateOptions { max_errors })
}

/// Returns the count of errors at which the limit `limit` aborts.
///
/// The reference tests `errors.length >= maxErrors` on a number. A count
/// reaches a fractional limit exactly when it reaches its ceiling, and it
/// reaches every negative limit at zero. A comparison with `NaN` is false, so
/// `NaN` never aborts, which is the largest count here, the reading
/// [`ValidateOptions::max_errors`] documents.
fn error_limit(limit: f64) -> usize {
    if limit.is_nan() {
        return usize::MAX;
    }
    // The cast saturates, and a limit that large is never reached by an
    // error list that fits in memory.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let limit = limit.ceil().max(0.0) as usize;
    limit
}

/// Returns the specified rule named `name`.
fn rule_named(name: &str) -> Result<ValidationRule, String> {
    aosabi_validate::specified_rule_named(name)
        .ok_or_else(|| format!("no specified rule is named {name}"))
}

/// Answers the operation `validate`.
fn op_validate(request: &JsObject) -> Result<JsObject, JsString> {
    op_validate_in(request, SchemaForm::Sdl)
}

/// Answers the operation `validate` with the scalars aosabi-core binds.
fn op_validate_bound(request: &JsObject) -> Result<JsObject, JsString> {
    op_validate_in(request, SchemaForm::Bound)
}

/// Answers the operation `validate` over a schema built in the form `form`.
fn op_validate_in(request: &JsObject, form: SchemaForm) -> Result<JsObject, JsString> {
    let sdl = batch::require_source(request, "sdl")?;
    let text = batch::require_source(request, "document")?;
    let rule = match request.get("rule") {
        None | Some(JsValue::Undefined) => None,
        Some(_) => Some(batch::require_string(request, "rule")?),
    };
    let call = Call {
        sdl,
        text,
        rule,
        parse_options: batch::parse_options(request)?,
        validate_options: validate_options(request)?,
        form,
    };
    let errors = validate(call).map_err(|refused| reason(&format!("{NAME}: "), &refused))?;
    let mut payload = JsObject::with_capacity(1);
    payload.insert("errors", errors);
    Ok(payload)
}

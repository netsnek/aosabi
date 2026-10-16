// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool parse: parses a GraphQL document and reports what graphql-js
 * reports, as a tree or as printed text.
 *
 * No analogue in the reference. The crate under test is aosabi-lang, and the
 * judging notebook is tests/notebooks/02_language.ipynb, which compares the
 * bytes against a node sidecar running the pinned graphql-js.
 */

//! The `parse` and `print` primitives of the seam.
//!
//! Both read one document, named by `--document PATH`, where the path `-` is
//! stdin. `--no-location`, `--allow-legacy-fragment-variables` and
//! `--max-tokens COUNT` are the three parser options a command line can
//! carry.
//!
//! - `parse` writes the tree as `JSON.stringify` writes the tree of the
//!   reference parser, through [`crate::ast_json`], and exits 0.
//! - `print` parses the same way and writes the text of the reference printer
//!   over that tree, and exits 0.
//!
//! A document that does not parse writes a JSON array that holds the one
//! syntax error in the `toJSON` shape of the reference, and exits 1. The
//! other exit codes are those of [`crate::seam`].
//!
//! `--requests PATH` is the batch form, which answers the operations `parse`
//! and `print` of [`crate::batch`] from either sub command, and two more of
//! the location arithmetic, `locate` and `print_location`, which hold the
//! table of line starts of a source and the excerpt of an error to the
//! oracle.

use std::{
    path::PathBuf,
    process::ExitCode, //
};

use aosabi_json::{
    JsObject,
    JsString,
    JsValue, //
};
use aosabi_lang::{
    DocumentNode,
    GraphQLError,
    ParseOptions,
    Source,
    SourceLocation,
    location::get_location_by_walk, //
};

use crate::{
    ast_from_json,
    ast_json,
    batch::{
        self,
        Operations, //
    },
    cli::{
        self,
        Dash,
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

/// The operations of the protocol that reach aosabi-lang.
const OPERATIONS: &Operations = &[
    ("parse", op_parse),
    ("print", op_print),
    ("locate", op_locate),
    ("print_location", op_print_location),
];

/// The command line of the `parse` and `print` primitives.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::path(
            "document",
            "PATH",
            "Read the GraphQL document from this file, or from stdin when it is `-`",
        ),
        Opt::flag(
            "no-location",
            "Build the tree without locations, the noLocation option",
        ),
        Opt::flag(
            "allow-legacy-fragment-variables",
            "Parse variable definitions on a fragment definition, the allowLegacyFragmentVariables option",
        ),
        Opt::number(
            "max-tokens",
            "COUNT",
            "Refuse a document of more tokens than this, the maxTokens option",
        )
        .dash(Dash::Numbers),
        cli::REQUESTS,
    ],
    positional: None,
    conflicts: &[("requests", &["document"])],
};

/// What the `parse` and `print` primitives accept.
#[derive(Debug)]
pub struct ParseArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The file of the document, where `-` is stdin.
    document: Option<PathBuf>,
    /// The noLocation option.
    no_location: bool,
    /// The allowLegacyFragmentVariables option.
    allow_legacy_fragment_variables: bool,
    /// The maxTokens option.
    max_tokens: Option<f64>,
    /// The file of request lines of the batch form.
    requests: Option<PathBuf>,
}

impl ParseArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when the budget of `--max-tokens` is not a
    /// number.
    pub fn read(matches: &Matches) -> Result<Self, Refusal> {
        Ok(Self {
            smoke: matches.flag("smoke"),
            document: matches.path("document"),
            no_location: matches.flag("no-location"),
            allow_legacy_fragment_variables: matches.flag("allow-legacy-fragment-variables"),
            max_tokens: matches.number("max-tokens")?,
            requests: matches.path("requests"),
        })
    }

    /// Returns the parser options the flags name.
    const fn options(&self) -> ParseOptions {
        ParseOptions {
            no_location: self.no_location,
            max_tokens: self.max_tokens,
            allow_legacy_fragment_variables: self.allow_legacy_fragment_variables,
        }
    }

    /// Parses the document the call names.
    fn parse(&self) -> Result<Result<DocumentNode, GraphQLError>, Failure> {
        let path = seam::required(self.document.as_deref(), "--document")?;
        let text = seam::read_input(path)?;
        Ok(aosabi_lang::parse(text, self.options()))
    }
}

/// Runs the `parse` primitive.
pub fn run(args: &ParseArgs) -> ExitCode {
    run_with("parse", args, |document| {
        // An object always has a JSON form, so the writer's `None` arm, which
        // is the undefined top level value, is unreachable here.
        aosabi_json::to_bytes(&ast_json::to_json(document)).unwrap_or_default()
    })
}

/// Runs the `print` primitive.
pub fn run_print(args: &ParseArgs) -> ExitCode {
    run_with("print", args, |document| {
        aosabi_lang::print(document).into_bytes()
    })
}

/// Runs the primitive `name`, whose payload over a parsed document is `render`.
fn run_with(name: &str, args: &ParseArgs, render: fn(&DocumentNode) -> Vec<u8>) -> ExitCode {
    if args.smoke {
        return seam::smoke(name);
    }
    if let Some(requests) = &args.requests {
        return seam::finish(name, batch::run(OPERATIONS, requests));
    }
    let answer = args.parse().map(|parsed| match parsed {
        Ok(document) => Answer::Payload(render(&document)),
        Err(error) => Answer::refused(vec![seam::graphql_error_json(&error)]),
    });
    seam::finish(name, answer)
}

/// Returns the payload that carries the one syntax error `error`.
fn errors_payload(error: &GraphQLError) -> JsObject {
    let mut payload = JsObject::with_capacity(1);
    payload.insert("errors", vec![seam::graphql_error_json(error)]);
    payload
}

/// Answers the operation `parse`.
///
/// `ast` is a string, the bytes `JSON.stringify` writes for the tree, so the
/// bytes of the tree survive the transport unchanged.
fn op_parse(request: &JsObject) -> Result<JsObject, JsString> {
    let source = batch::require_source(request, "document")?;
    let options = batch::parse_options(request)?;
    let document = match aosabi_lang::parse(source, options) {
        Ok(document) => document,
        Err(error) => return Ok(errors_payload(&error)),
    };
    let mut payload = JsObject::with_capacity(1);
    // An object always has a JSON form, so the default is never taken.
    let ast = aosabi_json::to_string(&ast_json::to_json(&document)).unwrap_or_default();
    payload.insert("ast", ast);
    Ok(payload)
}

/// Answers the operation `print`.
///
/// The tree comes from `ast` when the request carries one, as an object or as
/// the string a `parse` response carries, and from parsing `document`
/// otherwise, which is the order of `opPrint` in the oracle.
fn op_print(request: &JsObject) -> Result<JsObject, JsString> {
    let mut payload = JsObject::with_capacity(1);
    match request.get("ast") {
        None | Some(JsValue::Undefined) => {}
        Some(ast) => {
            let node = ast_from_json::node_of(ast).map_err(|reason| format!("print: {reason}"))?;
            payload.insert("printed", aosabi_lang::print(&node));
            return Ok(payload);
        }
    }
    let source = batch::require_source(request, "document")?;
    let options = batch::parse_options(request)?;
    match aosabi_lang::parse(source, options) {
        Ok(document) => payload.insert("printed", aosabi_lang::print(&document)),
        Err(error) => return Ok(errors_payload(&error)),
    };
    Ok(payload)
}

/// Answers the operation `locate`.
///
/// `positions` are code unit offsets into `document`, and `locations` holds
/// the line and the column `getLocation` answers for each, as a pair, in the
/// order of the positions. With `walk` true the line and the column come from
/// the walk of the body the dev feature `exec-walk-location` of aosabi-lang
/// keeps, the second judge of the table of line starts, which the oracle does
/// not hold and leaves unread.
fn op_locate(request: &JsObject) -> Result<JsObject, JsString> {
    let source = batch::require_source(request, "document")?;
    let walk = matches!(request.get("walk"), Some(JsValue::Bool(true)));
    let locations = positions_of(request)?
        .into_iter()
        .map(|position| {
            let location = if walk {
                get_location_by_walk(&source, position)
            } else {
                aosabi_lang::get_location(&source, position)
            };
            JsValue::from(vec![
                JsValue::from(location.line as u64),
                JsValue::from(location.column as u64),
            ])
        })
        .collect::<Vec<JsValue>>();
    let mut payload = JsObject::with_capacity(1);
    payload.insert("locations", locations);
    Ok(payload)
}

/// Answers the operation `print_location`.
///
/// Each position of `positions` is located in `document` and printed as
/// `printSourceLocation` prints it, so `printed` holds what `printLocation`
/// prints for a node that starts there. A request that carries `name` and
/// `locationOffset` builds the source with them, as
/// `new Source(body, name, locationOffset)` does.
fn op_print_location(request: &JsObject) -> Result<JsObject, JsString> {
    let source = source_of(request)?;
    let printed = positions_of(request)?
        .into_iter()
        .map(|position| {
            let location = aosabi_lang::get_location(&source, position);
            JsValue::from(aosabi_lang::print_source_location(&source, location))
        })
        .collect::<Vec<JsValue>>();
    let mut payload = JsObject::with_capacity(1);
    payload.insert("printed", printed);
    Ok(payload)
}

/// Returns the source of `document` with the `name` and the `locationOffset` the request carries.
///
/// A request without `locationOffset` builds the source at line 1, column
/// 1, and one without `name` takes the default name, the defaults of the
/// reference constructor.
fn source_of(request: &JsObject) -> Result<Source, String> {
    let Some(JsValue::String(body)) = request.get("document") else {
        return Err(String::from(
            "print_location: the request carries no string field document",
        ));
    };
    let name = match request.get("name") {
        None | Some(JsValue::Undefined) => aosabi_lang::source::DEFAULT_NAME,
        Some(_) => batch::require_string(request, "name")?,
    };
    let offset = match request.get("locationOffset") {
        None | Some(JsValue::Undefined) => SourceLocation { line: 1, column: 1 },
        Some(JsValue::Object(offset)) => SourceLocation {
            line: whole_number(offset.get("line"))
                .ok_or("print_location: locationOffset.line is not a whole number")?,
            column: whole_number(offset.get("column"))
                .ok_or("print_location: locationOffset.column is not a whole number")?,
        },
        Some(_) => {
            return Err(String::from(
                "print_location: locationOffset is not an object",
            ));
        }
    };
    Source::with_location_offset(body.clone(), name, offset).map_err(|error| error.to_string())
}

/// Returns the code unit offsets of the array field `positions` of `request`.
///
/// # Errors
///
/// Returns a `fatal` when the field is missing, is not an array, or holds a
/// value that is not a whole number from zero up.
fn positions_of(request: &JsObject) -> Result<Vec<usize>, String> {
    let Some(JsValue::Array(positions)) = request.get("positions") else {
        return Err(String::from("the request carries no array field positions"));
    };
    positions
        .iter()
        .map(|position| {
            whole_number(Some(position))
                .ok_or_else(|| String::from("positions holds a value that is no code unit offset"))
        })
        .collect()
}

/// Returns `value` as a whole number from zero up, or `None` for any other value.
///
/// A number of the wire is a double, and every whole number below 2^53 is
/// one exactly, so the conversion keeps it where `usize` holds it.
fn whole_number(value: Option<&JsValue>) -> Option<usize> {
    let Some(&JsValue::Number(number)) = value else {
        return None;
    };
    if number < 0.0 || number.fract() != 0.0 || number >= 9_007_199_254_740_992.0 {
        return None;
    }
    // The cast is exact, because the value is a whole number from zero below
    // 2^53, which a double carries without a gap.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let whole = number as u64;
    usize::try_from(whole).ok()
}

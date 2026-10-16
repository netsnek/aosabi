// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool json: writes a value as bytes the way node writes it.
 *
 * No analogue in the reference. The primitive behind it is aosabi-json, the
 * only crate that turns a number, a string or an object into bytes, and the
 * judging notebook is tests/notebooks/01_json_writer.ipynb, which drives this
 * sub command against node JSON.stringify over the same inputs.
 */

//! The `json` primitive of the seam.
//!
//! Four inputs reach the writer, one per call, so a notebook can hand node the
//! same value and compare the bytes.
//!
//! - `--f64-bits HEX` writes the double whose IEEE 754 bit pattern is the
//!   sixteen hexadecimal digits `HEX`. A notebook produces them in Python with
//!   `struct.pack("<d", value)` read back as `<Q` and formatted `016x`.
//! - `--f64-bits-file PATH` is the batch form of the same input, so that a
//!   hundred thousand doubles do not cost a hundred thousand processes. The
//!   file is UTF-8 with one `HEX` per line, surrounding whitespace ignored and
//!   blank lines skipped, and the output is one number per line, each line
//!   terminated by a newline. A number carries no newline of its own, so the
//!   split is unambiguous.
//! - `--string-file PATH` writes the content of the file as one JSON string.
//!   The file is WTF-8, which is UTF-8 in which a lone surrogate code unit is
//!   encoded as its three byte sequence, the form Python writes with
//!   `value.encode("utf-8", "surrogatepass")`. Two such sequences that form a
//!   pair are read as the pair, because they are one code point in JavaScript.
//! - `--json-file PATH` reads a UTF-8 JSON document with [`reader`], which
//!   keeps key order and takes the last value of a repeated key at the position
//!   of the first, the way `JSON.parse` does, and writes the value back.
//!
//! The option `--space SPACE` writes every input as
//! `JSON.stringify(value, null, space)` writes it in node, which changes the
//! bytes of an object or an array alone, so it reaches the document of
//! `--json-file`. The writer of aosabi-json takes the gap, and this primitive
//! derives it from `SPACE` by the rule of the function. A value `f64::from_str`
//! reads is the number of step 7, whose integer part gives at most ten spaces
//! and none below one, any other value is the string of step 8, whose first ten
//! UTF-16 code units are the gap, and a call with no `--space` takes the empty
//! gap of step 9. The text `2` is therefore the number 2, and a gap of the
//! character `2` cannot be named.
//!
//! Two kinds of value are refused. For the first no gap the writer takes gives
//! the bytes node writes for a document with an object or an array, and the
//! refusal holds for every input for simplicity, although a scalar document
//! reads the same on both. A number strictly between 0 and 1 has the empty gap
//! by step 7, and the empty gap writes no line break, but V8 keeps a gap of no
//! spaces for every number above 0 and writes the line breaks and the space
//! after a colon whenever a gap exists. So node writes `{"a":[1]}` with the
//! space 0.5 as `{\n"a": [\n1\n]\n}`, while Bun writes `{"a":[1]}` as the step
//! does. A text whose tenth code unit is the first half of a surrogate pair
//! would give a gap with a lone surrogate, which the writer cannot take.
//!
//! The output goes to stdout as bytes with no trailing newline, except in the
//! batch form. The exit codes are the ones of `sysexits.h`: 64 for a call with
//! no input or more than one and for a `--space` the primitive refuses, 65 for
//! an input that does not parse, 66 for a file that cannot be read, 74 for a
//! write to stdout that fails. A failure writes one line to stderr and no byte
//! to stdout.

pub mod reader;

use std::{
    fmt,
    fs,
    io::{
        self,
        Write, //
    },
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode, //
};

use aosabi_json::{
    JsString,
    number,
    string, //
};

use crate::{
    cli::{
        self,
        Dash,
        Grammar,
        Matches,
        Opt,
        Refusal, //
    },
    stub, //
};

/// The exit code of a call with no input or more than one, or with a space
/// the primitive refuses, `EX_USAGE`.
const EX_USAGE: u8 = 64;
/// The exit code of an input that does not parse, `EX_DATAERR`.
const EX_DATAERR: u8 = 65;
/// The exit code of a file that cannot be read, `EX_NOINPUT`.
const EX_NOINPUT: u8 = 66;
/// The exit code of a write to stdout that fails, `EX_IOERR`.
const EX_IOERR: u8 = 74;

/// The longest gap `JSON.stringify` writes, in spaces or in code units.
const GAP_MAX: u8 = 10;

/// The command line of the `json` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[
        cli::SMOKE,
        Opt::value(
            "f64-bits",
            "HEX",
            "Write the double whose IEEE 754 bits are these sixteen hex digits",
        ),
        Opt::path(
            "f64-bits-file",
            "PATH",
            "Write one double per line of the file, one hex pattern per line",
        ),
        Opt::path(
            "string-file",
            "PATH",
            "Write the WTF-8 content of the file as one JSON string",
        ),
        Opt::path(
            "json-file",
            "PATH",
            "Read a JSON document from the file and write it back",
        ),
        Opt::value(
            "space",
            "SPACE",
            "Write as JSON.stringify(value, null, space) does, a number of spaces or the gap",
        )
        .dash(Dash::Numbers),
    ],
    positional: None,
    conflicts: &[],
};

/// What the `json` primitive accepts.
#[derive(Debug)]
pub struct JsonArgs {
    /// Whether the call asks for the interface line.
    smoke: bool,
    /// The sixteen hexadecimal digits of one double.
    f64_bits: Option<String>,
    /// The file of such patterns, one per line.
    f64_bits_file: Option<PathBuf>,
    /// The file whose WTF-8 content is one string.
    string_file: Option<PathBuf>,
    /// The file holding one JSON document.
    json_file: Option<PathBuf>,
    /// The space argument of `JSON.stringify`, as the command line gives it.
    space: Option<String>,
}

impl JsonArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when the digits of `--f64-bits` or the value of
    /// `--space` are not UTF-8.
    pub fn read(matches: &Matches) -> Result<Self, Refusal> {
        Ok(Self {
            smoke: matches.flag("smoke"),
            f64_bits: matches.text("f64-bits")?,
            f64_bits_file: matches.path("f64-bits-file"),
            string_file: matches.path("string-file"),
            json_file: matches.path("json-file"),
            space: matches.text("space")?,
        })
    }

    /// Returns the one input the call names.
    ///
    /// # Errors
    ///
    /// Returns [`Failure::Usage`] when the call names no input or more than
    /// one.
    fn input(&self) -> Result<Input<'_>, Failure> {
        let mut named = [
            self.f64_bits.as_deref().map(Input::Bits),
            self.f64_bits_file.as_deref().map(Input::BitsFile),
            self.string_file.as_deref().map(Input::StringFile),
            self.json_file.as_deref().map(Input::JsonFile),
        ]
        .into_iter()
        .flatten();
        match (named.next(), named.next()) {
            (Some(input), None) => Ok(input),
            _ => Err(Failure::Usage),
        }
    }
}

/// The one input a call names, borrowed from the arguments.
#[derive(Debug, Clone, Copy)]
enum Input<'a> {
    /// The sixteen hexadecimal digits of one double.
    Bits(&'a str),
    /// A file with one such pattern per line.
    BitsFile(&'a Path),
    /// A file whose WTF-8 content is one string.
    StringFile(&'a Path),
    /// A file holding one JSON document.
    JsonFile(&'a Path),
}

/// Why a call wrote no bytes.
#[derive(Debug)]
enum Failure {
    /// The call named no input or more than one.
    Usage,
    /// The value of `--space` gives no gap that writes the bytes of node, with
    /// the reason.
    Space(String),
    /// An input did not parse, with the reason.
    Data(String),
    /// A file could not be read, with the reason.
    NoInput(PathBuf, io::Error),
    /// Stdout refused the bytes.
    Io(io::Error),
}

impl Failure {
    /// Returns the exit code the failure earns.
    fn exit_code(&self) -> u8 {
        match self {
            Self::Usage | Self::Space(_) => EX_USAGE,
            Self::Data(_) => EX_DATAERR,
            Self::NoInput(..) => EX_NOINPUT,
            Self::Io(_) => EX_IOERR,
        }
    }
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Usage => write!(
                f,
                "json takes exactly one of --f64-bits, --f64-bits-file, --string-file or --json-file"
            ),
            Self::Space(reason) | Self::Data(reason) => write!(f, "{reason}"),
            Self::NoInput(path, error) => write!(f, "cannot read {}: {error}", path.display()),
            Self::Io(error) => write!(f, "cannot write stdout: {error}"),
        }
    }
}

/// Runs the `json` primitive.
///
/// A smoke call prints the interface line and exits 0 before any input is
/// read, so the environment notebook can smoke the seam with no file on disk.
/// Any other call writes the bytes of the one input it names.
pub fn run(args: &JsonArgs) -> ExitCode {
    if args.smoke {
        println!("{}", stub::interface_line("json", true, "M1"));
        return ExitCode::SUCCESS;
    }
    let outcome = gap(args.space.as_deref()).and_then(|gap| {
        args.input()
            .and_then(|input| produce(input, &gap))
            .and_then(|bytes| emit(&bytes))
    });
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(failure) => {
            eprintln!("aosabi-devtool json: {failure}");
            ExitCode::from(failure.exit_code())
        }
    }
}

/// Returns the bytes of one input, an object or an array written with the
/// gap `gap`.
fn produce(input: Input<'_>, gap: &str) -> Result<Vec<u8>, Failure> {
    let mut out = Vec::new();
    match input {
        Input::Bits(hex) => number::write_f64(&mut out, parse_bits(hex)?),
        Input::BitsFile(path) => write_bits_lines(&mut out, path)?,
        Input::StringFile(path) => write_string_file(&mut out, path)?,
        Input::JsonFile(path) => write_json_file(&mut out, path, gap)?,
    }
    Ok(out)
}

/// Returns the gap `JSON.stringify` derives from the space argument `space`,
/// the empty gap when the call names none.
///
/// A value `f64::from_str` reads is the number, any other value the text.
///
/// # Errors
///
/// Returns [`Failure::Space`] for a number strictly between 0 and 1, which
/// node writes with the line breaks of a gap where the empty gap writes none,
/// and for a text whose gap ends inside a surrogate pair.
fn gap(space: Option<&str>) -> Result<String, Failure> {
    let Some(space) = space else {
        return Ok(String::new());
    };
    if let Ok(number) = space.parse::<f64>() {
        if number > 0.0 && number < 1.0 {
            return Err(Failure::Space(format!(
                "--space {space:?} lies between 0 and 1, where node writes line breaks with an empty gap and ECMA-262 writes none"
            )));
        }
        return Ok(" ".repeat(spaces(number)));
    }
    let units: Vec<u16> = space.encode_utf16().take(usize::from(GAP_MAX)).collect();
    String::from_utf16(&units).map_err(|_| {
        Failure::Space(format!(
            "--space {space:?} gives a gap that ends inside a surrogate pair, which the writer cannot take"
        ))
    })
}

/// Returns how many spaces the number `space` gives, which is
/// `min(10, ToIntegerOrInfinity(space))` and none below one.
///
/// The count is the largest of 1 to 10 that `space` reaches, which is that
/// minimum for every value, `NaN` and the negative numbers reaching none.
fn spaces(space: f64) -> usize {
    (1..=GAP_MAX)
        .rev()
        .find(|&count| space >= f64::from(count))
        .map_or(0, usize::from)
}

/// Writes the bytes to stdout with no trailing newline.
fn emit(bytes: &[u8]) -> Result<(), Failure> {
    let mut stdout = io::stdout().lock();
    stdout
        .write_all(bytes)
        .and_then(|()| stdout.flush())
        .map_err(Failure::Io)
}

/// Appends one number per pattern line of the file, each followed by a newline.
fn write_bits_lines(out: &mut Vec<u8>, path: &Path) -> Result<(), Failure> {
    let text = read_text(path)?;
    for (line_number, line) in (1..).zip(text.lines()) {
        let hex = line.trim();
        if hex.is_empty() {
            continue;
        }
        let value = parse_bits(hex)
            .map_err(|failure| Failure::Data(format!("line {line_number}: {failure}")))?;
        number::write_f64(out, value);
        out.push(b'\n');
    }
    Ok(())
}

/// Appends the WTF-8 content of the file as one JSON string.
fn write_string_file(out: &mut Vec<u8>, path: &Path) -> Result<(), Failure> {
    let bytes = read_bytes(path)?;
    let text = JsString::from_wtf8(&bytes).map_err(|error| data_error(path, &error))?;
    string::escape(out, &text);
    Ok(())
}

/// Appends the document of the file, written back by the writer with the gap
/// `gap`.
fn write_json_file(out: &mut Vec<u8>, path: &Path, gap: &str) -> Result<(), Failure> {
    let bytes = read_bytes(path)?;
    let value = reader::parse(&bytes).map_err(|error| data_error(path, &error))?;
    aosabi_json::write_with_gap(out, &value, gap);
    Ok(())
}

/// Returns the failure of an input file that does not parse, naming the file.
fn data_error(path: &Path, error: &dyn fmt::Display) -> Failure {
    Failure::Data(format!("{}: {error}", path.display()))
}

/// Reads a whole file, naming it in the failure.
fn read_bytes(path: &Path) -> Result<Vec<u8>, Failure> {
    fs::read(path).map_err(|error| Failure::NoInput(path.to_path_buf(), error))
}

/// Reads a whole file as UTF-8, naming it in the failure.
fn read_text(path: &Path) -> Result<String, Failure> {
    String::from_utf8(read_bytes(path)?)
        .map_err(|error| Failure::Data(format!("{}: not UTF-8: {error}", path.display())))
}

/// Parses sixteen hexadecimal digits as the bits of a double.
///
/// The digits are checked before they are parsed, because the integer parser
/// would also accept a leading sign, and only the sixteen digit form is the
/// contract.
fn parse_bits(hex: &str) -> Result<f64, Failure> {
    let well_formed = hex.len() == 16 && hex.bytes().all(|byte| byte.is_ascii_hexdigit());
    let bits = if well_formed {
        u64::from_str_radix(hex, 16).ok()
    } else {
        None
    };
    bits.map(f64::from_bits)
        .ok_or_else(|| Failure::Data(format!("{hex:?} is not sixteen hexadecimal digits")))
}

// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool provenance: recomputes every hash of the provenance lock over
 * the extracted files.
 *
 * No analogue in the reference. tools/provenance/extract.py is the only writer
 * of the extracted literals and records each one in
 * tools/provenance/provenance.lock with its mode, its SHA-256 and its byte
 * length. This primitive reads the lock back and hashes the files on disk, so
 * a hand edit of a vendored literal, a missing extraction and a stale lock are
 * three distinguishable states rather than one silent pass. The seven literals
 * and why each mode is what it is are docs/harness.md section 10.
 */

//! The `provenance` primitive of the seam.
//!
//! The report is one table row per literal, carrying the identifier, the mode,
//! the recorded hash, the recomputed hash and one of the states `ok`, `drift`
//! and `missing`, then a summary line with the counts. A literal in `drift` or
//! `missing` state is followed by indented lines saying what was found. The
//! exit code is 0 when every literal is ok and the declared count, when there
//! is one, equals the number of rows, and 1 otherwise. A lock that cannot be
//! read at all is one line on stderr and exit 1.
//!
//! The lock is read with the JSON reader of `aosabi-json` and every file is
//! hashed with [`sha256`], so the check reaches no outside crate. The
//! simulator of `homa-sim` hashes its fills and its links with the same
//! function.

pub mod sha256;

use std::{
    fmt,
    fs,
    path::{
        Path,
        PathBuf, //
    },
    process::ExitCode, //
};

use aosabi_json::JsValue;

use crate::{
    cli::{
        Grammar,
        Matches,
        Opt, //
    },
    json::reader, //
};

/// What a hash column shows for a literal that carries no hash.
const NOT_HASHED: &str = "not hashed";

/// The width of the identifier column.
const ID_WIDTH: usize = 30;

/// The width of the mode column.
const MODE_WIDTH: usize = 18;

/// The width of a hash column, which is a SHA-256 written as hex.
const HASH_WIDTH: usize = 64;

/// The command line of the `provenance` primitive.
pub static GRAMMAR: Grammar = Grammar {
    options: &[Opt::path(
        "root",
        "ROOT",
        "The repository root the lock and the extracted files are read under",
    )
    .default(".")],
    positional: None,
    conflicts: &[],
};

/// What `provenance` accepts.
#[derive(Debug)]
pub struct ProvenanceArgs {
    /// The repository root the lock and the extracted files are read under.
    root: PathBuf,
}

impl ProvenanceArgs {
    /// Reads the arguments `matches` holds for [`GRAMMAR`].
    pub fn read(matches: &Matches) -> Self {
        Self {
            root: matches.path("root").unwrap_or_default(),
        }
    }
}

/// Why the lock could not be read at all.
#[derive(Debug)]
enum LockError {
    /// The file is not there or is not readable as UTF-8.
    Unreadable(PathBuf),
    /// The file is not a JSON document.
    NotJson(PathBuf),
    /// The document carries no `literals` array.
    NoLiterals(PathBuf),
}

impl fmt::Display for LockError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Unreadable(path) => write!(f, "cannot read {}", path.display()),
            Self::NotJson(path) => write!(f, "{} is not valid JSON", path.display()),
            Self::NoLiterals(path) => write!(f, "{} carries no literals", path.display()),
        }
    }
}

/// The parts of the lock this primitive reads.
struct Lock {
    /// The count the lock declares, when it declares one.
    count: Option<u64>,
    /// The literal entries in lock order.
    literals: Vec<JsValue>,
}

/// The fields of one lock entry, each empty when the lock omits it.
struct Entry<'a> {
    /// The identifier of the literal.
    id: &'a str,
    /// The extraction mode.
    mode: &'a str,
    /// The state the lock records for it.
    state: &'a str,
    /// The extracted file, relative to the root.
    target: &'a str,
    /// The recorded hash as lowercase hex, or empty when there is none.
    sha256: &'a str,
    /// The recorded byte length, when there is one.
    bytes: Option<u64>,
}

impl<'a> Entry<'a> {
    /// Reads the fields of one entry, treating a missing or mistyped field as
    /// empty.
    fn read(value: &'a JsValue) -> Self {
        let field = |name: &str| match value {
            JsValue::Object(entry) => entry.get(name),
            _ => None,
        };
        let text = |name: &str| match field(name) {
            Some(JsValue::String(text)) => text.as_str().unwrap_or(""),
            _ => "",
        };
        Self {
            id: text("id"),
            mode: text("mode"),
            state: text("state"),
            target: text("target"),
            sha256: text("sha256"),
            bytes: field("bytes").and_then(count),
        }
    }

    /// Returns whether `len` agrees with the recorded length, which an entry
    /// without one always does.
    fn length_matches(&self, len: usize) -> bool {
        self.bytes
            .is_none_or(|recorded| u64::try_from(len) == Ok(recorded))
    }
}

/// What one literal turned out to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum State {
    /// The recomputed hash equals the recorded one, or the literal is ported
    /// code and carries no hash at all.
    Ok,
    /// The file is there and its bytes are not the recorded bytes.
    Drift,
    /// The file the lock names is not on disk, or the lock records no hash.
    Missing,
}

impl State {
    /// Returns the word the state column shows.
    fn label(self) -> &'static str {
        match self {
            Self::Ok => "ok",
            Self::Drift => "drift",
            Self::Missing => "missing",
        }
    }
}

/// The outcome of one report.
#[derive(Debug, Default)]
struct Outcome {
    /// How many literals are ok.
    ok: usize,
    /// How many literals drifted.
    drift: usize,
    /// How many literals are missing.
    missing: usize,
    /// Whether the declared count, when there is one, equals the rows.
    count_matches: bool,
}

impl Outcome {
    /// Counts one more literal in `state`.
    fn add(&mut self, state: State) {
        match state {
            State::Ok => self.ok += 1,
            State::Drift => self.drift += 1,
            State::Missing => self.missing += 1,
        }
    }

    /// Returns whether the lock is clean, which is the exit status.
    fn is_clean(&self) -> bool {
        self.drift == 0 && self.missing == 0 && self.count_matches
    }
}

/// Runs the `provenance` primitive.
pub fn run(args: &ProvenanceArgs) -> ExitCode {
    match read_lock(&args.root) {
        Ok(lock) if report(&args.root, &lock).is_clean() => ExitCode::SUCCESS,
        Ok(_) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("provenance: {error}");
            ExitCode::FAILURE
        }
    }
}

/// Reads the lock under `root`.
///
/// # Errors
///
/// Returns [`LockError`] when the lock is not there, is not JSON or carries no
/// `literals` array.
fn read_lock(root: &Path) -> Result<Lock, LockError> {
    let path = root
        .join("tools")
        .join("provenance")
        .join("provenance.lock");
    let Ok(text) = fs::read_to_string(&path) else {
        return Err(LockError::Unreadable(path));
    };
    let Ok(lock) = reader::parse(text.as_bytes()) else {
        return Err(LockError::NotJson(path));
    };
    let JsValue::Object(mut lock) = lock else {
        return Err(LockError::NoLiterals(path));
    };
    let count = lock.get("count").and_then(count);
    match lock.remove("literals") {
        Some(JsValue::Array(literals)) => Ok(Lock { count, literals }),
        _ => Err(LockError::NoLiterals(path)),
    }
}

/// Returns `value` as a count, when it is a whole number from 0 that a double
/// holds without a gap.
fn count(value: &JsValue) -> Option<u64> {
    /// The largest integer n such that n and n + 1 are both exact doubles,
    /// which is 2^53 less 1, the `Number.MAX_SAFE_INTEGER` of ECMA-262.
    const MAX_SAFE_INTEGER: f64 = 9_007_199_254_740_991.0;
    let JsValue::Number(number) = *value else {
        return None;
    };
    if number.fract() != 0.0 || !(0.0..=MAX_SAFE_INTEGER).contains(&number) {
        return None;
    }
    // The cast is exact, because the value is a whole number inside the
    // range a double carries without a gap.
    #[expect(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(number as u64)
}

/// Prints the report of every literal and returns its outcome.
fn report(root: &Path, lock: &Lock) -> Outcome {
    print_row("literal", "mode", "expected", "actual", "state");
    let mut outcome = Outcome::default();
    for literal in &lock.literals {
        outcome.add(check(root, &Entry::read(literal)));
    }
    let counted = lock.literals.len();
    outcome.count_matches = lock
        .count
        .is_none_or(|declared| u64::try_from(counted) == Ok(declared));
    if let Some(declared) = lock.count
        && !outcome.count_matches
    {
        println!("    the lock declares {declared} literals and carries {counted}");
    }
    println!(
        "provenance: {counted} literals, {} ok, {} drift, {} missing",
        outcome.ok, outcome.drift, outcome.missing
    );
    outcome
}

/// Checks one literal, prints its row and returns its state.
///
/// A ported literal is code rather than text, so nothing is extracted and
/// nothing is hashed. It is ok and says so in both hash columns.
fn check(root: &Path, entry: &Entry<'_>) -> State {
    if entry.mode == "ported" {
        print_entry(entry, NOT_HASHED, NOT_HASHED, State::Ok);
        return State::Ok;
    }
    if entry.sha256.is_empty() {
        print_entry(entry, NOT_HASHED, NOT_HASHED, State::Missing);
        println!(
            "    the lock records no hash, its state is {:?}",
            entry.state
        );
        return State::Missing;
    }
    let Ok(bytes) = fs::read(root.join(entry.target)) else {
        print_entry(entry, entry.sha256, NOT_HASHED, State::Missing);
        println!("    {} is not on disk", entry.target);
        return State::Missing;
    };
    let actual = sha256::hex(&bytes);
    if actual == entry.sha256 && entry.length_matches(bytes.len()) {
        print_entry(entry, entry.sha256, &actual, State::Ok);
        return State::Ok;
    }
    print_entry(entry, entry.sha256, &actual, State::Drift);
    println!("    {}", entry.target);
    if let Some(recorded) = entry.bytes {
        println!(
            "    {} bytes on disk, the lock records {recorded}",
            bytes.len()
        );
    }
    State::Drift
}

/// Prints the row of one entry.
fn print_entry(entry: &Entry<'_>, expected: &str, actual: &str, state: State) {
    print_row(entry.id, entry.mode, expected, actual, state.label());
}

/// Prints one row of the table, which the header row shares.
fn print_row(id: &str, mode: &str, expected: &str, actual: &str, state: &str) {
    println!(
        "{id:<ID_WIDTH$}  {mode:<MODE_WIDTH$}  {expected:<HASH_WIDTH$}  {actual:<HASH_WIDTH$}  {state}"
    );
}

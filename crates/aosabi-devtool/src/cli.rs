// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool cli: the command line parser of the seam.
 *
 * No analogue in the reference, whose command line runs on commander. The seam
 * reads its own command line over std::env::args_os, because the rule of
 * docs/decisions/ADR-0024-dependencies.md takes the standard library first and
 * code written in the project second, and a parser of long options meets none
 * of the four reasons that admit a crate.
 */

//! The command line of the seam.
//!
//! A [`Program`] lists its sub commands. Each [`Command`] carries its
//! [`Grammar`], the table of its options and of its positional values, and the
//! function that reads the [`Matches`] into the arguments of its primitive and
//! runs it. [`run`] finds the sub command the first argument names, matches
//! the rest against its grammar and calls that function.
//!
//! The grammar is the part of the GNU conventions the seam uses. Every option
//! is long, `--name` for a flag and `--name VALUE` or `--name=VALUE` for an
//! option that takes a value, and a command line names each option at most
//! once. The [`Kind`] of a value decides which values the option refuses, the
//! same in both forms, see [`Kind`] for a value that is not UTF-8. The parser
//! reads the shape of an argument over its bytes, so an argument that is not
//! UTF-8 is an option, short flags or a value as its bytes say, and a text
//! or a positional value that is not UTF-8 is refused before a primitive
//! runs. A value that stands apart and begins with a dash is
//! taken when it is `-` alone, which names stdin, or when the option takes
//! such values, see [`Dash`]. The argument `--` ends the options, so every
//! argument behind it is a positional value. An argument of one dash and more
//! is read as short flags from its first letter on, and the only short flags
//! are `-h` and, for the program, `-V`. The arguments `-h` and `--help` print
//! the help of the program or of the sub command to stdout and exit 0, and
//! `-V` and `--version` print the version of the program.
//!
//! An option named with no value attached waits for the next argument. When
//! that argument is not a value the option takes, the parser reads it as an
//! argument of its own, so an unknown argument behind the option is refused
//! as unknown, and the option is refused for its missing value when the next
//! known argument or the end of the command line is reached. Two arguments
//! the grammar says exclude each other are refused under the name of the one
//! the command line names first.
//!
//! A command line the grammar refuses writes `error: ` and the reason to
//! stderr, with the usage it breaks and a pointer to the help, writes no byte
//! to stdout and exits 2. The primitives keep the codes of `sysexits.h` for a
//! call the grammar takes and the primitive refuses, so the two never share a
//! code. A program called with no argument at all, or with `--` alone, prints
//! its help to stderr and exits 2 as well.

use std::{
    borrow::Cow,
    ffi::{
        OsStr,
        OsString, //
    },
    io::{
        self,
        Write, //
    },
    path::PathBuf,
    process::ExitCode, //
};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

/// The exit code of a command line the grammar refuses.
const EXIT_REFUSED: u8 = 2;

/// The flag of every primitive that reports its interface.
pub const SMOKE: Opt = Opt::flag("smoke", "Report the interface and exit 0");

/// The option of every primitive that has a batch form.
pub const REQUESTS: Opt = Opt::path(
    "requests",
    "PATH",
    "Answer every request line of this file, one response line each",
);

/// The command line of a primitive whose one form is the batch form, `--smoke` and
/// `--requests PATH`.
pub static BATCH_GRAMMAR: Grammar = Grammar {
    options: &[SMOKE, REQUESTS],
    positional: None,
    conflicts: &[],
};

/// A program and the sub commands it offers.
#[derive(Debug)]
pub struct Program {
    /// The name the usage line calls the program by.
    pub name: &'static str,
    /// The line the help of the program opens with.
    pub about: &'static str,
    /// The version `--version` prints behind the name.
    pub version: &'static str,
    /// The sub commands, in the order the help lists them.
    pub commands: &'static [Command],
}

/// One sub command.
#[derive(Debug)]
pub struct Command {
    /// The name the first argument calls it by.
    pub name: &'static str,
    /// The line the help of the program lists it with, which opens its own
    /// help as well.
    pub about: &'static str,
    /// Its options and positional values.
    pub grammar: &'static Grammar,
    /// Reads the matches into the arguments of the primitive and runs it.
    pub run: fn(&Matches) -> Result<ExitCode, Refusal>,
}

/// The options and the positional values of one sub command.
#[derive(Debug)]
pub struct Grammar {
    /// The options, in the order the help lists them.
    pub options: &'static [Opt],
    /// The positional values, when the sub command takes any.
    pub positional: Option<Positional>,
    /// Each option or positional value, named by its long name or by the
    /// value name of [`Positional`], with the ones it excludes. The parser
    /// reads each pair both ways.
    pub conflicts: &'static [(&'static str, &'static [&'static str])],
}

/// One option of a sub command, which is always long.
#[derive(Debug)]
pub struct Opt {
    /// The name behind the two dashes.
    pub long: &'static str,
    /// The value it takes, or `None` for a flag.
    pub value: Option<Value>,
    /// The line the help lists it with.
    pub help: &'static str,
    /// The values beginning with a dash it takes as a separate argument.
    pub dash: Dash,
    /// The value the primitive reads when the command line names none.
    pub default: Option<&'static str>,
}

impl Opt {
    /// Returns the flag `--long`, which takes no value.
    pub const fn flag(long: &'static str, help: &'static str) -> Self {
        Self {
            long,
            value: None,
            help,
            dash: Dash::Stdin,
            default: None,
        }
    }

    /// Returns the option `--long VALUE`, whose value is text the help calls
    /// `value`.
    pub const fn value(long: &'static str, value: &'static str, help: &'static str) -> Self {
        Self::of(long, value, Kind::Text, help)
    }

    /// Returns the option `--long PATH`, whose value is a path the help calls
    /// `value`.
    pub const fn path(long: &'static str, value: &'static str, help: &'static str) -> Self {
        Self::of(long, value, Kind::Path, help)
    }

    /// Returns the option `--long COUNT`, whose value is a double the help
    /// calls `value`.
    pub const fn number(long: &'static str, value: &'static str, help: &'static str) -> Self {
        Self::of(long, value, Kind::Number, help)
    }

    /// Returns the option `--long CHOICE`, whose value is one of `choices`
    /// and which the help calls `value`.
    pub const fn choice(
        long: &'static str,
        value: &'static str,
        help: &'static str,
        choices: &'static [Choice],
    ) -> Self {
        Self::of(long, value, Kind::Choice(choices), help)
    }

    /// Returns the option `--long`, whose value is of `kind` and which the
    /// help calls `name`.
    const fn of(long: &'static str, name: &'static str, kind: Kind, help: &'static str) -> Self {
        Self {
            long,
            value: Some(Value { name, kind }),
            help,
            dash: Dash::Stdin,
            default: None,
        }
    }

    /// Returns the option with the values beginning with a dash it takes.
    pub const fn dash(self, dash: Dash) -> Self {
        Self { dash, ..self }
    }

    /// Returns the option with the value a primitive reads in its absence.
    pub const fn default(self, default: &'static str) -> Self {
        Self {
            default: Some(default),
            ..self
        }
    }

    /// Returns how the help and a refusal write the option.
    fn display(&self) -> String {
        match self.value {
            Some(Value { name, .. }) => format!("--{} <{name}>", self.long),
            None => format!("--{}", self.long),
        }
    }

    /// Returns whether the option takes `next`, the argument behind it, as
    /// its value.
    fn takes(&self, next: &OsStr) -> bool {
        let bytes = next.as_encoded_bytes();
        let [b'-', _, ..] = bytes else {
            return true;
        };
        match self.dash {
            Dash::Stdin => false,
            Dash::Numbers => negative_number(bytes),
            Dash::Any => true,
        }
    }

    /// Refuses `value` when the kind of the option does not take it.
    fn check(&self, value: &OsStr) -> Result<(), Refusal> {
        let Some(Value { kind, .. }) = self.value else {
            return Ok(());
        };
        match kind {
            Kind::Path | Kind::Choice(_) if value.is_empty() => Err(required(self)),
            Kind::Text if value.to_str().is_none() => Err(invalid_utf8()),
            Kind::Text | Kind::Path => Ok(()),
            Kind::Number => number(self, value).map(|_| ()),
            Kind::Choice(choices) if choices.iter().any(|choice| value == choice.name) => Ok(()),
            Kind::Choice(_) => {
                let shown = value.to_string_lossy();
                let display = self.display();
                let possible = possible_values(self);
                Err(Refusal(format!(
                    "invalid value '{shown}' for '{display}'{possible}"
                )))
            }
        }
    }
}

/// The value of an option.
#[derive(Debug, Clone, Copy)]
pub struct Value {
    /// Its name in the help and in a refusal.
    pub name: &'static str,
    /// What it is, which decides the values the option refuses.
    pub kind: Kind,
}

/// What the value of an option is.
///
/// The parser holds a value to its kind whether the command line attaches it
/// with `=` or gives it as the next argument, so both forms take and refuse
/// the same values. On Unix that holds for a value that is not UTF-8 as well,
/// because the parser splits `--name=VALUE` at its first `=` over the bytes
/// of the argument and keeps every byte of the value through the extension of
/// Unix. The portable `OsStr::slice_encoded_bytes` is unstable in Rust 1.95.0
/// and the parser uses no extension of another platform, so there a value
/// that is not UTF-8 is taken only given apart and refused behind `=`.
#[derive(Debug, Clone, Copy)]
pub enum Kind {
    /// Text in UTF-8, which may be empty.
    Text,
    /// A path, which is never empty and may be any argument the platform
    /// passes.
    Path,
    /// A double, in the text `f64::from_str` reads.
    Number,
    /// One of these values, which are never empty.
    Choice(&'static [Choice]),
}

/// One value of an option that takes only some values.
#[derive(Debug)]
pub struct Choice {
    /// The value as the command line writes it.
    pub name: &'static str,
    /// What the value asks for, as the help describes it.
    pub help: &'static str,
}

/// The values beginning with a dash an option takes as a separate argument.
///
/// Every option takes such a value attached, as in `--name=-5`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dash {
    /// None but `-` alone, which names stdin.
    Stdin,
    /// A negative number as well, such as `-5`, `-0.5` or `-1e3`.
    Numbers,
    /// Every value.
    Any,
}

/// The positional values of a sub command.
#[derive(Debug)]
pub struct Positional {
    /// The name of one value in the help.
    pub value: &'static str,
    /// How many values the sub command takes at most.
    pub most: usize,
    /// The line the help lists them with.
    pub help: &'static str,
}

impl Positional {
    /// Returns how the help and a refusal write the values.
    fn display(&self) -> String {
        format!("[{}]...", self.value)
    }
}

/// What the parser matched on the command line of one sub command.
#[derive(Debug)]
pub struct Matches {
    /// The grammar the command line was matched against.
    grammar: &'static Grammar,
    /// The value of each option of the grammar, by its position there. A flag
    /// the command line names holds an empty value.
    found: Vec<Option<OsString>>,
    /// The positional values, in their order.
    positional: Vec<OsString>,
    /// The position in the grammar of the option named with no value
    /// attached, while it waits for the next argument.
    waiting: Option<usize>,
    /// The long names of the options and the value name of the positional
    /// values, in the order the command line names them.
    named: Vec<&'static str>,
}

impl Matches {
    /// Returns whether the command line names the flag `long`.
    ///
    /// # Panics
    ///
    /// Panics in a debug build when the grammar holds no option `long`, which
    /// is a fault of the primitive that asks and not of the command line.
    pub fn flag(&self, long: &str) -> bool {
        self.found(long).is_some()
    }

    /// Returns the value of the option `long` as a path.
    ///
    /// # Panics
    ///
    /// Panics in a debug build when the grammar holds no option `long`, as
    /// [`Matches::flag`] does.
    pub fn path(&self, long: &str) -> Option<PathBuf> {
        self.value(long).map(PathBuf::from)
    }

    /// Returns the value of the option `long` as text.
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when the value is not UTF-8, which the parser
    /// refuses before a primitive runs for an option of [`Kind::Text`].
    ///
    /// # Panics
    ///
    /// Panics in a debug build when the grammar holds no option `long`, as
    /// [`Matches::flag`] does.
    pub fn text(&self, long: &str) -> Result<Option<String>, Refusal> {
        self.value(long).map(utf8).transpose()
    }

    /// Returns the value of the option `long` as a double.
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when the value is not the text of a double,
    /// which the parser refuses before a primitive runs for an option of
    /// [`Kind::Number`].
    ///
    /// # Panics
    ///
    /// Panics in a debug build when the grammar holds no option `long`, as
    /// [`Matches::flag`] does.
    pub fn number(&self, long: &str) -> Result<Option<f64>, Refusal> {
        match (self.option(long), self.value(long)) {
            (Some(option), Some(value)) => number(option, &value).map(Some),
            _ => Ok(None),
        }
    }

    /// Returns the positional values as text.
    ///
    /// # Errors
    ///
    /// Returns a [`Refusal`] when a value is not UTF-8, which the parser
    /// refuses before a primitive runs.
    pub fn texts(&self) -> Result<Vec<String>, Refusal> {
        self.positional.iter().cloned().map(utf8).collect()
    }

    /// Returns the value of the option `long`, or its default when the command
    /// line names none.
    fn value(&self, long: &str) -> Option<OsString> {
        self.found(long).cloned().or_else(|| {
            self.option(long)
                .and_then(|option| option.default)
                .map(OsString::from)
        })
    }

    /// Returns what the command line gives the option `long`.
    fn found(&self, long: &str) -> Option<&OsString> {
        let index = self.index(long)?;
        self.found[index].as_ref()
    }

    /// Returns the option `long` of the grammar.
    fn option(&self, long: &str) -> Option<&'static Opt> {
        self.index(long).map(|index| &self.grammar.options[index])
    }

    /// Returns the position of the option `long` in the grammar.
    ///
    /// A primitive asks only for options its own grammar holds, so a debug
    /// build stops at a name the grammar lacks rather than reading it as
    /// absent.
    fn index(&self, long: &str) -> Option<usize> {
        let index = self
            .grammar
            .options
            .iter()
            .position(|option| option.long == long);
        debug_assert!(index.is_some(), "the grammar holds no option --{long}");
        index
    }

    /// Returns how a refusal writes the option `long`.
    fn display(&self, long: &str) -> String {
        self.option(long)
            .map_or_else(|| format!("--{long}"), Opt::display)
    }

    /// Matches the long option `name` with the bytes of the value attached
    /// behind `=`, or leaves it waiting for its value.
    ///
    /// A known option first settles the positional values and the option
    /// that waits, so their refusals come before its own.
    fn take_option(&mut self, name: &str, attached: Option<&[u8]>) -> Result<(), Refusal> {
        let Some(index) = self
            .grammar
            .options
            .iter()
            .position(|option| option.long == name)
        else {
            return Err(match attached {
                Some(value) if name == "help" => unexpected_value(&lossy(value), "--help"),
                _ => unexpected_argument(&format!("--{name}")),
            });
        };
        let option = &self.grammar.options[index];
        if let (None, Some(value)) = (option.value, attached) {
            return Err(unexpected_value(&lossy(value), &option.display()));
        }
        self.settle_positional()?;
        self.settle_waiting()?;
        match attached {
            None if option.value.is_some() => {
                self.waiting = Some(index);
                Ok(())
            }
            _ => {
                let value = attached.map(attached_value).transpose()?;
                self.settle(index, value)
            }
        }
    }

    /// Gives `arg` to the option that waits for its value when that option
    /// takes it, and returns `arg` when no option takes it.
    fn give_waiting(&mut self, arg: OsString) -> Result<Option<OsString>, Refusal> {
        match self.waiting {
            Some(index) if self.grammar.options[index].takes(&arg) => {
                self.waiting = None;
                self.settle(index, Some(arg)).map(|()| None)
            }
            _ => Ok(Some(arg)),
        }
    }

    /// Refuses the option that waits for its value, when one waits, because
    /// the argument the parser reads next or the end of the command line
    /// leaves it without one.
    fn settle_waiting(&mut self) -> Result<(), Refusal> {
        self.waiting
            .take()
            .map_or(Ok(()), |index| self.settle(index, None))
    }

    /// Records `value` for the option at `index` of the grammar, or refuses
    /// the option.
    ///
    /// An option that takes a value and gets none is refused first, then an
    /// option the command line names a second time, then a value the kind of
    /// the option does not take.
    fn settle(&mut self, index: usize, value: Option<OsString>) -> Result<(), Refusal> {
        let option = &self.grammar.options[index];
        let value = match (option.value, value) {
            (None, _) => OsString::new(),
            (Some(_), Some(value)) => value,
            (Some(_), None) => return Err(required(option)),
        };
        if self.found[index].is_some() {
            let option = option.display();
            return Err(Refusal(format!(
                "the argument '{option}' cannot be used multiple times"
            )));
        }
        option.check(&value)?;
        self.found[index] = Some(value);
        self.named.push(option.long);
        Ok(())
    }

    /// Matches `arg` as a positional value.
    fn take_positional(&mut self, arg: &OsStr) -> Result<(), Refusal> {
        let grammar = self.grammar;
        let Some(positional) = &grammar.positional else {
            return Err(unexpected_argument(&arg.to_string_lossy()));
        };
        self.settle_waiting()?;
        if self.positional.len() == positional.most {
            let shown = arg.to_string_lossy();
            return Err(unexpected_value(&shown, &positional.display()));
        }
        if self.positional.is_empty() {
            self.named.push(positional.value);
        }
        self.positional.push(arg.to_os_string());
        Ok(())
    }

    /// Refuses a positional value that is not UTF-8.
    ///
    /// The parser checks the positional values when it reads the next known
    /// option, a request for help or the end of the command line, so a value
    /// too many or an unknown argument behind them is refused first.
    fn settle_positional(&self) -> Result<(), Refusal> {
        if self.positional.iter().all(|value| value.to_str().is_some()) {
            Ok(())
        } else {
            Err(invalid_utf8())
        }
    }

    /// Returns how a refusal writes `name`, an option or the value name of
    /// the positional values.
    fn display_name(&self, name: &str) -> String {
        match &self.grammar.positional {
            Some(positional) if positional.value == name => positional.display(),
            _ => self.display(name),
        }
    }

    /// Returns whether the grammar says `one` and `other` exclude each other.
    fn excludes(&self, one: &str, other: &str) -> bool {
        self.grammar.conflicts.iter().any(|(name, excluded)| {
            (*name == one && excluded.contains(&other))
                || (*name == other && excluded.contains(&one))
        })
    }

    /// Refuses two names the grammar says exclude each other, under the name
    /// the command line names first, with every name it excludes.
    fn check_conflicts(&self) -> Result<(), Refusal> {
        for name in &self.named {
            let excluded: Vec<String> = self
                .named
                .iter()
                .filter(|other| self.excludes(name, other))
                .map(|other| self.display_name(other))
                .collect();
            let with = match excluded.as_slice() {
                [] => continue,
                [other] => format!(" '{other}'"),
                others => format!(":\n  {}", others.join("\n  ")),
            };
            let name = self.display_name(name);
            return Err(Refusal(format!(
                "the argument '{name}' cannot be used with{with}"
            )));
        }
        Ok(())
    }
}

/// Why a command line is refused, as the line behind `error: `.
#[derive(Debug)]
pub struct Refusal(String);

/// Why the parser ends the process before a primitive runs.
enum Stop {
    /// The command line asks for this text on stdout, which exits 0.
    Print(String),
    /// The command line names no sub command, so the help goes to stderr.
    Bare,
    /// The command line is refused, with the sub command it names, if any.
    Refused(Option<&'static Command>, Refusal),
}

/// Parses `args`, the command line without the name of the program, and runs
/// the sub command of `program` it names.
pub fn run(program: &'static Program, args: impl IntoIterator<Item = OsString>) -> ExitCode {
    match parse(program, args) {
        Ok((command, matches)) => (command.run)(&matches)
            .unwrap_or_else(|refusal| refuse(program, Some(command), &refusal)),
        Err(Stop::Print(text)) => {
            put(io::stdout().lock(), &text);
            ExitCode::SUCCESS
        }
        Err(Stop::Bare) => {
            put(io::stderr().lock(), &program_help(program));
            ExitCode::from(EXIT_REFUSED)
        }
        Err(Stop::Refused(command, refusal)) => refuse(program, command, &refusal),
    }
}

/// Finds the sub command `args` names and matches the rest of `args`.
fn parse(
    program: &'static Program,
    args: impl IntoIterator<Item = OsString>,
) -> Result<(&'static Command, Matches), Stop> {
    let mut args = args.into_iter();
    let Some(first) = args.next() else {
        return Err(Stop::Bare);
    };
    if let Some(stop) = program_argument(program, &first, &mut args) {
        return Err(stop);
    }
    let Some(command) = program
        .commands
        .iter()
        .find(|command| first == command.name)
    else {
        let shown = first.to_string_lossy();
        let refusal = Refusal(format!("unrecognized subcommand '{shown}'"));
        return Err(Stop::Refused(None, refusal));
    };
    let matches = match_command(program, command, args)?;
    Ok((command, matches))
}

/// Returns how `program` answers `first`, its first argument, when that
/// argument is an option or `--` rather than the name of a sub command.
fn program_argument(
    program: &Program,
    first: &OsStr,
    rest: &mut impl Iterator<Item = OsString>,
) -> Option<Stop> {
    let help = || Stop::Print(program_help(program));
    let version = || Stop::Print(format!("{} {}\n", program.name, program.version));
    let refused = |refusal| Some(Stop::Refused(None, refusal));
    match Shape::of(first) {
        // The program takes no positional value, so `--` alone names no sub
        // command and anything behind it is refused.
        Shape::End => match rest.next() {
            None => Some(Stop::Bare),
            Some(next) => refused(unexpected_argument(&next.to_string_lossy())),
        },
        Shape::Long(name, None) if name == "help" => Some(help()),
        Shape::Long(name, None) if name == "version" => Some(version()),
        Shape::Long(name, Some(value)) if name == "help" || name == "version" => {
            refused(unexpected_value(&lossy(value), &format!("--{name}")))
        }
        Shape::Long(name, _) => refused(unexpected_argument(&format!("--{name}"))),
        Shape::Short(Ok('h')) => Some(help()),
        Shape::Short(Ok('V')) => Some(version()),
        Shape::Short(Ok(flag)) => refused(unexpected_argument(&format!("-{flag}"))),
        Shape::Short(Err(flags)) => refused(unexpected_argument(&format!("-{flags}"))),
        Shape::Value => None,
    }
}

/// Matches `args` against the grammar of `command`.
fn match_command(
    program: &Program,
    command: &'static Command,
    mut args: impl Iterator<Item = OsString>,
) -> Result<Matches, Stop> {
    let grammar = command.grammar;
    let mut matches = Matches {
        grammar,
        found: vec![None; grammar.options.len()],
        positional: Vec::new(),
        waiting: None,
        named: Vec::new(),
    };
    let refused = |refusal| Stop::Refused(Some(command), refusal);
    while let Some(arg) = args.next() {
        let Some(arg) = matches.give_waiting(arg).map_err(refused)? else {
            continue;
        };
        match Shape::of(&arg) {
            Shape::End => {
                for arg in args.by_ref() {
                    matches.take_positional(&arg).map_err(refused)?;
                }
            }
            Shape::Long(name, None) if name == "help" => {
                return Err(help(program, command, &mut matches));
            }
            Shape::Short(Ok('h')) => return Err(help(program, command, &mut matches)),
            Shape::Long(name, attached) => {
                matches.take_option(&name, attached).map_err(refused)?;
            }
            Shape::Short(Ok(flag)) => {
                return Err(refused(unexpected_argument(&format!("-{flag}"))));
            }
            Shape::Short(Err(flags)) => {
                return Err(refused(unexpected_argument(&format!("-{flags}"))));
            }
            Shape::Value => matches.take_positional(&arg).map_err(refused)?,
        }
    }
    matches.settle_waiting().map_err(refused)?;
    matches.settle_positional().map_err(refused)?;
    matches.check_conflicts().map_err(refused)?;
    Ok(matches)
}

/// Returns the help of `command` to print, or the refusal of the positional
/// values or of the option that waits for its value, which are settled first.
fn help(program: &Program, command: &'static Command, matches: &mut Matches) -> Stop {
    match matches
        .settle_positional()
        .and_then(|()| matches.settle_waiting())
    {
        Ok(()) => Stop::Print(command_help(program, command)),
        Err(refusal) => Stop::Refused(Some(command), refusal),
    }
}

/// The shape of one argument, which the parser reads over its bytes, so an
/// argument that is not UTF-8 takes the shape its bytes give it.
///
/// The bytes are those of [`OsStr::as_encoded_bytes`], in which an ASCII byte
/// stands for its ASCII character on every platform.
enum Shape<'a> {
    /// The argument `--`, which ends the options.
    End,
    /// A long option, with its name and the bytes of the value behind its
    /// first `=`. The name holds U+FFFD for each byte that is not UTF-8, so it
    /// names no option of a grammar.
    Long(Cow<'a, str>, Option<&'a [u8]>),
    /// One dash and more, with the first short flag, or with the flags as
    /// text when the first is not UTF-8.
    Short(Result<char, Cow<'a, str>>),
    /// A value, which `-` alone is as well.
    Value,
}

impl<'a> Shape<'a> {
    /// Returns the shape of `arg`.
    fn of(arg: &'a OsStr) -> Self {
        let bytes = arg.as_encoded_bytes();
        match bytes {
            b"--" => Self::End,
            [b'-', b'-', body @ ..] => match body.iter().position(|&byte| byte == b'=') {
                Some(at) => Self::Long(lossy(&body[..at]), Some(&body[at + 1..])),
                None => Self::Long(lossy(body), None),
            },
            [b'-', flags @ ..] if !flags.is_empty() => Self::Short(first_flag(flags)),
            _ => Self::Value,
        }
    }
}

/// Returns the first short flag of `flags`, the bytes behind one dash, which
/// is the flag the parser reads first, or `flags` as text when that flag is
/// not UTF-8.
fn first_flag(flags: &[u8]) -> Result<char, Cow<'_, str>> {
    flags
        .utf8_chunks()
        .next()
        .and_then(|chunk| chunk.valid().chars().next())
        .ok_or_else(|| lossy(flags))
}

/// Returns `bytes` as text, with U+FFFD for each byte that is not UTF-8.
fn lossy(bytes: &[u8]) -> Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

/// Returns the value whose bytes the command line attaches behind `=`.
///
/// Unix passes an argument as bytes, so the value keeps every byte, as a
/// value given apart does.
// The conversion cannot fail on Unix. It returns a `Result` because the one
// of the other platforms can, and both are called alike.
#[cfg(unix)]
#[expect(clippy::unnecessary_wraps)]
fn attached_value(bytes: &[u8]) -> Result<OsString, Refusal> {
    Ok(OsStr::from_bytes(bytes).to_os_string())
}

/// Returns the value whose bytes the command line attaches behind `=`, or
/// the refusal of a value that is not UTF-8.
///
/// The portable `OsStr::slice_encoded_bytes` is unstable in Rust 1.95.0 and
/// the parser keeps the bytes through the extension of Unix alone, so on a
/// platform other than Unix only a value in UTF-8 is taken behind `=`.
#[cfg(not(unix))]
fn attached_value(bytes: &[u8]) -> Result<OsString, Refusal> {
    std::str::from_utf8(bytes)
        .map(OsString::from)
        .map_err(|_| invalid_utf8())
}

/// Returns whether `bytes` is a negative number an option of
/// [`Dash::Numbers`] takes as a separate argument.
///
/// That is a minus sign and digits, with at most one point after the first
/// digit and at most one exponent `e` or `E` after the first digit, which
/// digits follow and no sign. So `-5`, `-5.`, `-0.5` and `-1e3` are numbers,
/// while `-.5`, `-1e-3`, `-inf` and `-nan` are not.
fn negative_number(bytes: &[u8]) -> bool {
    let [b'-', digits @ ..] = bytes else {
        return false;
    };
    let mut point = false;
    let mut exponent = None;
    for (index, &byte) in digits.iter().enumerate() {
        match byte {
            b'0'..=b'9' => {}
            b'.' if !point && exponent.is_none() && index > 0 => point = true,
            b'e' | b'E' if exponent.is_none() && index > 0 => exponent = Some(index),
            _ => return false,
        }
    }
    exponent.is_none_or(|index| index + 1 < digits.len())
}

/// Returns `value` of `option` as a double, or the refusal of a value that is
/// not the text of a double `f64::from_str` reads.
fn number(option: &Opt, value: &OsStr) -> Result<f64, Refusal> {
    let text = utf8(value.to_os_string())?;
    text.parse::<f64>().map_err(|error| {
        let display = option.display();
        Refusal(format!("invalid value '{text}' for '{display}': {error}"))
    })
}

/// Returns the refusal of `option` named with no value.
fn required(option: &Opt) -> Refusal {
    let display = option.display();
    let possible = possible_values(option);
    Refusal(format!(
        "a value is required for '{display}' but none was supplied{possible}"
    ))
}

/// Returns the refusal of `value` given where `target` takes no more.
fn unexpected_value(value: &str, target: &str) -> Refusal {
    Refusal(format!(
        "unexpected value '{value}' for '{target}' found, no more were expected"
    ))
}

/// Returns the refusal of `shown`, an argument the grammar does not know.
fn unexpected_argument(shown: &str) -> Refusal {
    Refusal(format!("unexpected argument '{shown}' found"))
}

/// Returns the refusal of a text or a positional value that is not UTF-8.
fn invalid_utf8() -> Refusal {
    Refusal(String::from(
        "invalid UTF-8 was detected in one or more arguments",
    ))
}

/// Returns the line that names the values of `option` behind a line break,
/// when the option takes only some, or nothing.
fn possible_values(option: &Opt) -> String {
    let Some(Value {
        kind: Kind::Choice(choices),
        ..
    }) = option.value
    else {
        return String::new();
    };
    let names: Vec<&str> = choices.iter().map(|choice| choice.name).collect();
    format!("\n  [possible values: {}]", names.join(", "))
}

/// Returns `value` as text, or the refusal of a value that is not UTF-8.
fn utf8(value: OsString) -> Result<String, Refusal> {
    value.into_string().map_err(|_| invalid_utf8())
}

/// Writes the refusal to stderr and returns the exit code of a refused
/// command line.
fn refuse(program: &Program, command: Option<&Command>, refusal: &Refusal) -> ExitCode {
    let usage = match command {
        Some(command) => command_usage(program, command),
        None => format!("{} <COMMAND>", program.name),
    };
    let text = format!(
        "error: {}\n\nUsage: {usage}\n\nFor more information, try '--help'.\n",
        refusal.0
    );
    put(io::stderr().lock(), &text);
    ExitCode::from(EXIT_REFUSED)
}

/// Writes `text` to `out` and lets a failed write pass.
///
/// The help, the version and a refusal end the process with the exit code of
/// the command line whether or not the text reaches its reader, so a pipe
/// whose reader has closed leaves that code as it is.
fn put(mut out: impl Write, text: &str) {
    let _ = out.write_all(text.as_bytes()).and_then(|()| out.flush());
}

/// Returns the usage line of `command`.
fn command_usage(program: &Program, command: &Command) -> String {
    let mut usage = format!("{} {} [OPTIONS]", program.name, command.name);
    if let Some(positional) = &command.grammar.positional {
        usage.push(' ');
        usage.push_str(&positional.display());
    }
    usage
}

/// Returns the help of `program`, whose list of sub commands the notebooks
/// read the roster of the seam from.
fn program_help(program: &Program) -> String {
    let commands: Vec<(String, String)> = program
        .commands
        .iter()
        .map(|command| (command.name.to_owned(), command.about.to_owned()))
        .collect();
    let options = [
        (String::from("-h, --help"), String::from("Print help")),
        (String::from("-V, --version"), String::from("Print version")),
    ];
    format!(
        "{}\n\nUsage: {} <COMMAND>\n\nCommands:\n{}\nOptions:\n{}",
        program.about,
        program.name,
        table(&commands),
        table(&options),
    )
}

/// Returns the help of `command`, which `-h` and `--help` print alike.
fn command_help(program: &Program, command: &Command) -> String {
    let grammar = command.grammar;
    let mut help = format!(
        "{}\n\nUsage: {}\n\n",
        command.about,
        command_usage(program, command)
    );
    if let Some(positional) = &grammar.positional {
        let rows = [(positional.display(), positional.help.to_owned())];
        help.push_str("Arguments:\n");
        help.push_str(&table(&rows));
        help.push('\n');
    }
    let mut rows: Vec<(String, String)> = grammar
        .options
        .iter()
        .map(|option| (format!("    {}", option.display()), option_help(option)))
        .collect();
    rows.push((String::from("-h, --help"), String::from("Print help")));
    help.push_str("Options:\n");
    help.push_str(&table(&rows));
    help
}

/// Returns the line the help lists `option` with, and below it a line for
/// each value when the option takes only some.
fn option_help(option: &Opt) -> String {
    let mut first = vec![option.help.to_owned()];
    if let Some(default) = option.default {
        first.push(format!("[default: {default}]"));
    }
    let mut lines = vec![first.join(" ")];
    if let Some(Value {
        kind: Kind::Choice(choices),
        ..
    }) = option.value
    {
        let width = choices
            .iter()
            .map(|choice| choice.name.len() + 1)
            .max()
            .unwrap_or(0);
        lines.push(String::from("Possible values:"));
        lines.extend(choices.iter().map(|choice| {
            let name = format!("{}:", choice.name);
            format!("- {name:<width$} {}", choice.help)
        }));
    }
    lines.join("\n")
}

/// Returns `rows` as lines of two columns, the first padded to its widest
/// cell, each line indented by two spaces and ended by a newline. A line
/// break in the second column goes on under that column.
fn table(rows: &[(String, String)]) -> String {
    let width = rows.iter().map(|(left, _)| left.len()).max().unwrap_or(0);
    let under = format!("\n{}", " ".repeat(width + 4));
    let lines: Vec<String> = rows
        .iter()
        .map(|(left, right)| {
            let right = right.replace('\n', &under);
            format!("  {left:<width$}  {right}\n")
        })
        .collect();
    lines.concat()
}

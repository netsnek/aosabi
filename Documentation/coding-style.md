# Coding style

Aosabi follows the coding style of the Linux kernel, in Rust as in C. This page writes that rule
down from the kernel's own documents and says what each part means for a crate of this workspace.
[CONTRIBUTING.md](../CONTRIBUTING.md) is the rulebook of a change, and this page is the part of it
that governs the inside of a source file.

The kernel documents it follows are `Documentation/rust/coding-guidelines.rst`, which applies as
written because Aosabi is Rust, and `Documentation/process/coding-style.rst`, the C style, which
supplies the spirit wherever the Rust guidelines are silent: how long a function is, how deep it
nests, what a name says and what a comment is for. A C rule that exists only because C is C is left
out.

## 1. Formatting

The code is formatted by `rustfmt` with its default settings, and `cargo fmt --check` passes on
every commit. The file `rustfmt.toml` mirrors the kernel's `.rustfmt.toml` with the edition of this
workspace, 2024. The width is rustfmt's default of 100 columns.

A string a user sees, such as a log line or an error message, stays one literal whatever its length,
because a reader finds it with grep.

Imports take the vertical layout of the kernel, one item per line with braces as soon as a list has
more than one item, held in place by an empty trailing comment:

```rust
use crate::{
    example1,
    example2::{
        example3,
        example4, //
    },
    example5, //
};
```

## 2. Naming

Names follow the usual Rust conventions: a public item carries a descriptive name, a binding inside
a function stays short, so `i` and `tmp` are the right names for a loop counter and a temporary, and
a name that encodes its own type is a review finding. The path of a module is not repeated in the
name of an item, so a printer in the module `sdl` is `sdl::print`.

In prose the framework is written Aosabi, with a capital A, and as an identifier it stays lower
case, as in `aosabi-json`, `cargo aosabi build` and `aosabi.org`.

## 3. Functions

A function is short and does one thing, and a complex function is held to that all the more. More
than three levels of nesting inside a body, or more than about ten local variables, is the signal to
split it into helpers with descriptive names. The centralized exit of the kernel is `?` in Rust, and
an early return is preferred over another level of nesting. A function whose name is an imperative
returns `Result`, and a function whose name is a predicate returns `bool`.

## 4. Comments

- A plain `//` comment is written in Markdown as a doc comment is, starts with a capital letter and
  ends with a period, even when it is one sentence.
- A comment says why the code is the way it is, in the present tense, and never how the code came to
  be. A comment never names a previous attempt, a date or a count of tries, because that record is
  the history.
- A comment that repeats the line below it is removed in review.
- A tagged comment takes the kernel's forms, `// TODO:` and `// FIXME:`, and names what is missing
  rather than who will do it.
- An `#[allow]` or `#[expect]` carries a comment above it that says why the lint does not apply, and
  `expect` is preferred over `allow` unless the warning depends on the configuration.

## 5. Documentation

Every item a reader can reach carries a rustdoc comment, and so does every private item whose
purpose its name does not make plain. The workspace warns on a public item with none.

- The first paragraph is one sentence in the third person that says what the item does, such as
  "Appends a double the way ECMA-262 `Number::toString` does."
- An item of the workspace is linked by writing it in backticks inside square brackets.
- The sections take the kernel's names, `# Panics` and `# Examples`, and `# Errors` for the failure
  cases of a `Result`, which the lints of the workspace ask for.
- An example in a doc comment is marked `ignore` or `text` and asserts nothing, since every test is
  a notebook.

```rust
/// Appends a double the way ECMA-262 `Number::toString` does.
///
/// The digits go into `out`, and the shortest form that round trips is chosen.
///
/// # Errors
///
/// Returns [`WriteError::Full`] when `out` has no room left for the digits.
```

## 6. The header of a file

Every source file opens with its license line and a block comment that says what the file is, in one
sentence, before anything else:

```rust
// SPDX-License-Identifier: MIT
/*
 * aosabi-json: the writer of JSON bytes, the way V8 writes them.
 */
```

A file that a command of Aosabi writes into a user's project, such as the scaffold of `aosabi new`,
carries no license line, because the license of this repository is not the license of the user's
code.

## 7. Errors

A library crate never panics on input. It returns a `Result` with its own error type, and the
decision to end the process belongs to the binary at the top. `unwrap`, `expect` and `panic!` stay
out of library code, and a condition the author believes cannot happen is still an error variant.
Where a panic stays reachable, the conditions stand under `# Panics`. A compile time assertion is
welcome.

## 8. Features and conditional compilation

A cargo feature is the Kconfig symbol of this workspace. `#[cfg(feature = "x")]` stands on a whole
item and never inside an expression, so a conditional branch is factored into a function that takes
the attribute. `cfg!(feature = "x")` inside a plain `if` is preferred wherever the code compiles in
both settings, since the compiler keeps checking it. A definition that is always unused is deleted
rather than gated.

## 9. Lints

The table `[workspace.lints]` of `Cargo.toml` and the two lists of `clippy.toml` hold for every
crate and are never relaxed in one. They forbid `unsafe_code`, warn on a public item with no doc
comment, take the `pedantic` group of clippy, refuse `HashMap` and `BTreeMap`, and refuse every way
the standard library offers to start a thread.

## 10. Where Aosabi differs from the kernel

- The license line reads `MIT`, because Aosabi is MIT licensed.
- No `// SAFETY:` comment and no `# Safety` section exist in the workspace, because it forbids
  `unsafe_code`.
- A doc comment's example carries no assertion, because every test is a Jupyter notebook.
- The build is a cargo workspace, so `make rustfmt`, `make rustdoc` and `CLIPPY=1` read as
  `cargo fmt`, `cargo doc` and `cargo clippy`.
- Application code, the scaffold of `aosabi new` and the services under `examples/`, takes the style
  a Rust programmer writes a service in: rustfmt's default layout of imports, the glob
  `use aosabi::prelude::*;`, and doc comments that are the descriptions of the schema.

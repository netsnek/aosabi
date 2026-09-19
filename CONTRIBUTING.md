# Contributing to Aosabi

Thank you for helping. This file is the rulebook of a change: how it is signed off, how its commits
read, what style its code and its prose follow, how it is tested and which dependencies it may take.
Every rule here holds for every change, and a reviewer points at the rule a change breaks.

A larger change starts as an issue, so its design is agreed before its code is written. A
vulnerability goes to the private reporting that [SECURITY.md](SECURITY.md) describes, never to a
public issue.

## 1. Signed-off-by

Every commit carries a `Signed-off-by` line with the name and the mail address of its author, as the
Linux kernel asks. By the line the author certifies the
[Developer Certificate of Origin 1.1](https://developercertificate.org/), which says that they wrote
the change or have the right to submit it under the license of this repository. The command
`git commit -s` adds the line with the name and the address of the author's git configuration. A
commit of the maintainer ends so:

```text
Signed-off-by: Florian Kleber <kleber@snek.at>
```

A commit without the line is not merged.

## 2. Commit messages

A commit message has a typed summary, a prose body and the sign off:

```text
feat(validate): defer a literal of a oneOf input to the rules

The single pass reads no oneOf input, so it hands such a literal to the
validation rules, which write the error graphql-js writes.

Signed-off-by: Florian Kleber <kleber@snek.at>
```

- The summary is `type(scope): summary`, imperative, in lower case, with no full stop at its end.
- The type is one of `feat`, `fix`, `docs`, `refactor`, `perf`, `test`, `build`, `ci`, `chore` and
  `revert`.
- The scope is the crate without its `aosabi-` prefix, such as `validate` or `serve`, or a top level
  folder, such as `tests` or `Documentation`.
- The body is prose wrapped at 72 columns that says what the change does and why. It is no list of
  files and no restatement of the diff.
- A commit is small and does one thing. A refactor never rides along with a change of behaviour.

## 3. Style

The code follows the coding style of the Linux kernel, which
[Documentation/coding-style.md](Documentation/coding-style.md) writes down for this workspace.
`cargo fmt --check` and `cargo clippy` with the lints of the workspace pass on every commit, and a
source file opens with its `SPDX-License-Identifier` line and a block comment that says what the
file is.

Prose in the repository, in comments, documentation and commit messages, follows the same rules:

- It is English, in the present tense, and says what is, never how it came to be.
- Every sentence, list item, table cell and label starts with a capital letter.
- It uses no dashes as punctuation, no semicolons and no exclamation marks. A sentence that seems to
  need one becomes two sentences.
- A paragraph starts with a real sentence, never with a short bold label.
- People are they and them.
- Markdown is wrapped at 100 columns.
- A measured number names how it was measured: the notebook, the machine and the day.

## 4. Tests are notebooks

Every test of Aosabi is a Jupyter notebook in Python under [tests/notebooks/](tests/notebooks/), run
by papermill. The workspace holds no `#[test]` and no doc test that asserts behaviour, and a change
adds none.

- A change that fixes or adds behaviour adds or extends the notebook cell that proves it.
- The internals of the engine are reached through `aosabi-devtool`, one byte level primitive per
  subcommand, so a notebook checks one primitive at a time.
- A notebook has a `parameters` cell that papermill overrides, and it captures the output of every
  process it drives, so a failing run explains itself.
- Executed notebooks land under `tests/notebooks/out/` and are never committed, and a notebook is
  committed with its outputs stripped.

[Documentation/guides/conformance.md](Documentation/guides/conformance.md) says how to run the
suite.

## 5. Dependencies

Every crate from outside the project is code that ships in Aosabi and a part of its supply chain, so
a dependency is a weighing, in this order:

1. The standard library first, then code written in the project.
2. An outside crate only where it is more secure than own code, clearly faster, carried by the Linux
   kernel, or impossible to write within the rules of the workspace, which forbid unsafe code.
3. Then only a crate that is well known, well maintained and really needed.

A pull request that adds an outside crate says why it meets the rule and what it costs a deployed
build, and the counts of the [security guide](Documentation/guides/security.md) change with it. A
crate that runs on the build host alone counts in no deployed build, but follows the same rule. A
change that adds unsafe code, telemetry or a JavaScript runtime to a crate is rejected on that
ground alone.

## 6. Nothing private

No credential, token, key, private host name, internal address or mail address of a third party
enters the repository, in any file, an output cell of a notebook included. A recording made against
a real service stays with that service.

## 7. Pull requests

A pull request comes from a branch of a fork, holds the commits of one change, and says in its
description what it changes, why, and which notebooks it ran with their result. The checks of the
repository run on every pull request, and a maintainer merges it once they pass and the review
agrees.

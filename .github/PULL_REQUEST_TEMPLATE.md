<!-- Thank you for the change. CONTRIBUTING.md says how a change is written, tested and signed
off. -->

## What it changes

<!-- What the change does and why, in a few sentences, and the issue it closes if there is one. -->

## How it was tested

<!-- The notebooks you ran and the counts line that tests/run.py printed, for example after
python tests/run.py --only 04_aoframe. -->

## Checklist

- [ ] Every commit ends with a Signed-off-by line, which `git commit -s` adds. The line certifies
      the [Developer Certificate of Origin](https://developercertificate.org/) for the commit.
- [ ] Every commit message has the form `type(scope): summary` and a body in prose.
- [ ] Every commit builds on its own and does one thing, so the history stays bisectable.
- [ ] The formatting check `cargo fmt --check` passes.
- [ ] The lint `cargo clippy --locked --workspace --all-targets -- -D warnings` passes.
- [ ] The build `cargo build --locked --workspace` passes.
- [ ] A fix or a feature adds or extends the notebook cell under `tests/notebooks/` that proves
      it, and every notebook the change touches passes.
- [ ] The change takes no new dependency, or the description says why the standard library and
      the code of Aosabi cannot do the work.
- [ ] Every new source file opens with its `SPDX-License-Identifier` line.

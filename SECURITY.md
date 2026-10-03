# Security policy

Aosabi puts security before features, and a report of a vulnerability is the most welcome
contribution there is. This page says how to report one and what happens after.

## Reporting a vulnerability

Report a vulnerability through the private vulnerability reporting of GitHub on this repository:
open the tab Security of [netsnek/aosabi](https://github.com/netsnek/aosabi) and choose Report a
vulnerability, or open
[the form of a new advisory](https://github.com/netsnek/aosabi/security/advisories/new) directly.
The report stays private between you and the maintainers until a fix is out.

Please do not report a vulnerability in a public issue, a pull request or a discussion.

A good report names:

- The version or the commit, and the part it concerns, such as a crate, an npm package, the command
  line or a carrier.
- The build: the card or the target, the features of the facade, and for a TypeScript app its host.
- The steps that reproduce it, ideally as a request or a small service, and what an attacker gains.
- A fix or a mitigation, where you have one in mind.

## What happens next

The maintainers confirm the report, reproduce it, and prepare the fix in the private fork of the
advisory, where you are welcome to review it. The fix ships in a release, and the advisory is
published with it, crediting you unless you ask not to be named.

## Supported versions

Aosabi is before 1.0, and the latest release receives security fixes.

| Version | Security fixes |
| --- | --- |
| 0.1.x | Yes |

## Scope

The report covers the code of this repository: the crates, the plugins, the npm packages, the
command line and the build. A vulnerability in a fork under `third_party/` is reported here as well,
since the fork is built from this tree, and in the upstream project where it holds there too.

The defaults Aosabi ships with, TLS 1.3 alone, no unsafe block in its own code and every optional
feature off until a service names it, stand in the [security
guide](Documentation/guides/security.md).

// SPDX-License-Identifier: MIT
/*
 * aosabi: the program of the command line under the name of the project.
 *
 * No analogue in the reference. The commands inspect, verify, mcp, pull and
 * new are spelled with this name, and the library aosabi_cli answers it, as
 * it answers cargo-aosabi.
 */

//! The program `aosabi`.

fn main() -> std::process::ExitCode {
    aosabi_cli::main()
}

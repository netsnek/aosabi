// SPDX-License-Identifier: MIT
/*
 * cargo-aosabi: the program cargo runs for cargo aosabi.
 *
 * No analogue in the reference. Cargo runs an external sub command
 * `cargo aosabi` as the program cargo-aosabi with aosabi as its first
 * argument, and the library aosabi_cli answers it.
 */

//! The program `cargo-aosabi`.

fn main() -> std::process::ExitCode {
    aosabi_cli::main()
}

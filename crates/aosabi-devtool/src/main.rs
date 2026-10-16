// SPDX-License-Identifier: MIT
/*
 * aosabi-devtool: the seam through which a Python notebook reaches the Rust
 * internals, one byte level primitive per sub command.
 *
 * No analogue in the reference. Every test case of this project is a Python
 * notebook and none is written in Rust, so a notebook drives this binary as a
 * subprocess and judges one primitive per call, which is what lets a byte
 * difference bisect to one crate. The eight primitives of the crate map are
 * json, parse, validate, print-schema, execute, infer, collate and provenance.
 * The language and type layers answer through four further sub commands, print,
 * build-schema, validate-schema and natural-compare, one per operation of the
 * graphql-js oracle. The schema layer answers through one more, print-sdl,
 * because the printer of the intermediate representation is judged with no
 * inference in front of it. The crate homa answers through homa-packet,
 * homa-frame and homa-sim, one per byte level primitive of its wire forms, its
 * framing and its engine, and through homa-unix, its engine over a real Unix
 * Socket against a peer in another process, and through homa-ip, its engine and
 * the carrier Network over raw IP 146 between network namespaces. The carrier
 * LoRa answers through lora, its frame, its seal, its counters, its ledger and
 * its time on air. The case conversions of aosabi-json that keep the mappings to
 * ASCII alone answer through case. The subscriptions answer through two more,
 * pubsub and push: pubsub runs scripts of the pub sub and the repeater on one
 * agent, and push the hub of the kernel's publish primitive with no agent. The
 * loader of plugins answers through plugins, its sort, its role gate, both
 * passes of its boot and the mapping of a forbidden error. The primitives pull
 * and dotenv reach the command line, which the seam names on a Unix host alone.
 * The provider half of the Zitadel integration answers through zitadel, its
 * principal, its token, its roles and its introspection body. The queue of an
 * agent that a host owns answers through host-queue, which a build with the
 * feature host-queue alone holds, so the default build and every primitive in it
 * run the agent without that queue. The single pass of aosabi-validate answers
 * through pass, its check, its static table and the dump of its table.
 */

//! The Python seam of Aosabi.
//!
//! Each sub command is one module, which declares the grammar of its command
//! line for [`cli`], the parser of the seam, and reads what it matched into
//! its arguments. Every primitive answers one interface line under `--smoke`
//! through [`stub`], so a notebook can smoke every sub command with one cell.
//!
//! The primitives of the language, type and validation layers share their
//! input, output and exit code rules in [`seam`], and each of them answers a
//! whole file of requests through [`batch`]. The primitives of the schema
//! layer, which are `infer`, `print-sdl` and `collate`, follow the same rules,
//! and the two that read no file of their own answer through [`batch`] too.
//!
//! Every primitive runs on a thread whose stack this binary sizes, see
//! [`STACK_SIZE`], so the depth of nesting a primitive carries is a number of
//! the seam and not the resource limit of the shell that starts it.

mod app_model;
mod ast_from_json;
mod ast_json;
mod batch;
mod build_schema;
#[cfg(target_os = "linux")]
mod carrier;
mod case;
mod cli;
mod client;
mod client_schema;
mod collate;
#[cfg(unix)]
mod dotenv;
mod execute;
mod federation;
mod graphtp;
mod homa_frame;
#[cfg(target_os = "linux")]
mod homa_ip;
mod homa_packet;
mod homa_sim;
#[cfg(target_os = "linux")]
mod homa_unix;
#[cfg(feature = "host-queue")]
mod host_queue;
mod http;
mod infer;
mod json;
mod lora;
mod natural_compare;
mod parse;
mod pass;
mod plugins;
mod print_schema;
mod print_sdl;
mod provenance;
mod pubsub;
#[cfg(unix)]
mod pull;
mod push;
mod seam;
#[cfg(not(target_arch = "wasm32"))]
mod source;
mod stub;
mod validate;
mod validate_schema;
mod viewer;
mod yoga;
mod zitadel;

use std::{
    env,
    panic,
    process::ExitCode,
    thread, //
};

use crate::{
    build_schema::BuildSchemaArgs,
    case::CaseArgs,
    cli::{
        BATCH_GRAMMAR,
        Command,
        Program, //
    },
    collate::CollateArgs,
    execute::ExecuteArgs,
    federation::FederationArgs,
    graphtp::GraphtpArgs,
    homa_frame::HomaFrameArgs,
    homa_packet::HomaPacketArgs,
    homa_sim::HomaSimArgs,
    http::HttpArgs,
    infer::InferArgs,
    json::JsonArgs,
    lora::LoraArgs,
    natural_compare::NaturalCompareArgs,
    parse::ParseArgs,
    pass::PassArgs,
    plugins::PluginsArgs,
    print_schema::PrintSchemaArgs,
    print_sdl::PrintSdlArgs,
    provenance::ProvenanceArgs,
    pubsub::PubsubArgs,
    push::PushArgs,
    seam::BatchArgs,
    validate::ValidateArgs,
    validate_schema::ValidateSchemaArgs,
    viewer::ViewerArgs,
    yoga::YogaArgs, //
};

/// The command line of the seam, one sub command per primitive.
///
/// The about line of a command is the line the help of the program lists it
/// with, and `tests/notebooks/00_environment.ipynb` reads the roster of the
/// seam from that list, so it is part of the command line contract the
/// notebooks see.
static PROGRAM: Program = Program {
    name: "aosabi-devtool",
    about: "The Python seam. One byte level primitive per sub command.",
    version: env!("CARGO_PKG_VERSION"),
    commands: &[
        Command {
            name: "json",
            about: "Write values as bytes the way node writes them",
            grammar: &json::GRAMMAR,
            run: |matches| Ok(json::run(&JsonArgs::read(matches)?)),
        },
        Command {
            name: "parse",
            about: "Parse a GraphQL document and write the tree graphql-js builds",
            grammar: &parse::GRAMMAR,
            run: |matches| Ok(parse::run(&ParseArgs::read(matches)?)),
        },
        Command {
            name: "print",
            about: "Parse a GraphQL document and print it back as graphql-js prints it",
            grammar: &parse::GRAMMAR,
            run: |matches| Ok(parse::run_print(&ParseArgs::read(matches)?)),
        },
        Command {
            name: "validate",
            about: "Validate a document against a schema",
            grammar: &validate::GRAMMAR,
            run: |matches| Ok(validate::run(&ValidateArgs::read(matches)?)),
        },
        Command {
            name: "pass",
            about: "Check documents through the single pass and write or print its field table",
            grammar: &pass::GRAMMAR,
            run: |matches| Ok(pass::run(&PassArgs::read(matches)?)),
        },
        Command {
            name: "build-schema",
            about: "Build a schema from SDL and report what buildSchema throws",
            grammar: &build_schema::GRAMMAR,
            run: |matches| Ok(build_schema::run(&BuildSchemaArgs::read(matches))),
        },
        Command {
            name: "validate-schema",
            about: "Validate the schema of an SDL text the way the Aosabi build does",
            grammar: &validate_schema::GRAMMAR,
            run: |matches| Ok(validate_schema::run(&ValidateSchemaArgs::read(matches))),
        },
        Command {
            name: "print-schema",
            about: "Print a schema as SDL",
            grammar: &print_schema::GRAMMAR,
            run: |matches| Ok(print_schema::run(&PrintSchemaArgs::read(matches))),
        },
        Command {
            name: "natural-compare",
            about: "Order two strings the way naturalCompare orders them",
            grammar: &natural_compare::GRAMMAR,
            run: |matches| Ok(natural_compare::run(&NaturalCompareArgs::read(matches)?)),
        },
        Command {
            name: "client-schema",
            about: "Build the client schema of an introspection result and write the introspection query",
            grammar: &BATCH_GRAMMAR,
            run: |matches| Ok(client_schema::run(&BatchArgs::read(matches))),
        },
        Command {
            name: "execute",
            about: "Execute an operation without HTTP",
            grammar: &execute::GRAMMAR,
            run: |matches| Ok(execute::run(&ExecuteArgs::read(matches)?)),
        },
        Command {
            name: "infer",
            about: "Infer the intermediate representation from a descriptor dump",
            grammar: &infer::GRAMMAR,
            run: |matches| Ok(infer::run(&InferArgs::read(matches)?)),
        },
        Command {
            name: "print-sdl",
            about: "Print an intermediate representation as SDL",
            grammar: &print_sdl::GRAMMAR,
            run: |matches| Ok(print_sdl::run(&PrintSdlArgs::read(matches))),
        },
        Command {
            name: "collate",
            about: "Collate two names the way the reference collates them",
            grammar: &collate::GRAMMAR,
            run: |matches| Ok(collate::run(&CollateArgs::read(matches)?)),
        },
        Command {
            name: "app-model",
            about: "Form the canonical model of a raw model and write it as inspect --json prints it",
            grammar: &BATCH_GRAMMAR,
            run: |matches| Ok(app_model::run(&BatchArgs::read(matches))),
        },
        Command {
            name: "case",
            about: "Map text to the lower or upper case of JavaScript as far as it holds ASCII",
            grammar: &case::GRAMMAR,
            run: |matches| Ok(case::run(&CaseArgs::read(matches))),
        },
        Command {
            name: "yoga",
            about: "Answer requests through the Yoga of the capture schemas in process",
            grammar: &yoga::GRAMMAR,
            run: |matches| Ok(yoga::run(&YogaArgs::read(matches))),
        },
        Command {
            name: "http",
            about: "Print the tables of Node and drive the Node connection in process",
            grammar: &http::GRAMMAR,
            run: |matches| Ok(http::run(&HttpArgs::read(matches)?)),
        },
        Command {
            name: "pubsub",
            about: "Run scripts of the pub sub and the repeater on one agent with virtual timers",
            grammar: &pubsub::GRAMMAR,
            run: |matches| Ok(pubsub::run(&PubsubArgs::read(matches))),
        },
        #[cfg(feature = "host-queue")]
        Command {
            name: "host-queue",
            about: "Run the scenarios of a queue a host owns, with the feature host-queue",
            grammar: &host_queue::GRAMMAR,
            run: |matches| Ok(host_queue::run(&host_queue::HostQueueArgs::read(matches))),
        },
        Command {
            name: "push",
            about: "Run the hub of the publish primitive over the op list of the push model",
            grammar: &push::GRAMMAR,
            run: |matches| Ok(push::run(&PushArgs::read(matches))),
        },
        Command {
            name: "federation",
            about: "Transform, delegate and serialize the requests of Federation",
            grammar: &federation::GRAMMAR,
            run: |matches| Ok(federation::run(&FederationArgs::read(matches))),
        },
        Command {
            name: "graphtp",
            about: "Write and read the heads, the status bytes and the credential of GraphTP/1",
            grammar: &graphtp::GRAMMAR,
            run: |matches| Ok(graphtp::run(&GraphtpArgs::read(matches))),
        },
        Command {
            name: "homa-packet",
            about: "Encode and decode one Homa packet in either wire form",
            grammar: &homa_packet::GRAMMAR,
            run: |matches| Ok(homa_packet::run(&HomaPacketArgs::read(matches))),
        },
        Command {
            name: "homa-frame",
            about: "Frame, check and unframe Homa packets on a byte stream",
            grammar: &homa_frame::GRAMMAR,
            run: |matches| Ok(homa_frame::run(&HomaFrameArgs::read(matches))),
        },
        Command {
            name: "homa-sim",
            about: "Run a scenario of Homa RPCs over a simulated link",
            grammar: &homa_sim::GRAMMAR,
            run: |matches| Ok(homa_sim::run(&HomaSimArgs::read(matches))),
        },
        #[cfg(target_os = "linux")]
        Command {
            name: "homa-unix",
            about: "Serve or send Homa RPCs over a real Unix Socket",
            grammar: &homa_unix::GRAMMAR,
            run: |matches| Ok(homa_unix::run(&homa_unix::HomaUnixArgs::read(matches))),
        },
        #[cfg(target_os = "linux")]
        Command {
            name: "homa-ip",
            about: "Serve, load and flood GraphTP over Homa on raw IP 146",
            grammar: &homa_ip::GRAMMAR,
            run: |matches| Ok(homa_ip::run(&homa_ip::HomaIpArgs::read(matches))),
        },
        #[cfg(target_os = "linux")]
        Command {
            name: "carrier",
            about: "Serve GraphTP over Homa on Serial Line, USB and Bluetooth, and AoFrame alone",
            grammar: &carrier::GRAMMAR,
            run: |matches| Ok(carrier::run(&carrier::CarrierArgs::read(matches))),
        },
        Command {
            name: "lora",
            about: "Seal, open and time LoRa frames, and keep the airtime ledger",
            grammar: &lora::GRAMMAR,
            run: |matches| Ok(lora::run(&LoraArgs::read(matches))),
        },
        Command {
            name: "plugins",
            about: "Sort, gate, boot and remap plugins as the reference loader does, in process",
            grammar: &plugins::GRAMMAR,
            run: |matches| Ok(plugins::run(&PluginsArgs::read(matches))),
        },
        Command {
            name: "zitadel",
            about: "Map, read and introspect the tokens of the Zitadel provider in process",
            grammar: &BATCH_GRAMMAR,
            run: |matches| Ok(zitadel::run(&BatchArgs::read(matches))),
        },
        Command {
            name: "client",
            about: "Write the three files of the TypeScript client a build writes for an SDL text",
            grammar: &BATCH_GRAMMAR,
            run: |matches| Ok(client::run(&BatchArgs::read(matches))),
        },
        #[cfg(unix)]
        Command {
            name: "pull",
            about: "Write the Rust registry aosabi pull writes for an introspection answer",
            grammar: &BATCH_GRAMMAR,
            run: |matches| Ok(pull::run(&BatchArgs::read(matches))),
        },
        #[cfg(unix)]
        Command {
            name: "dotenv",
            about: "Parse the text of a .env file the way the command line reads it",
            grammar: &BATCH_GRAMMAR,
            run: |matches| Ok(dotenv::run(&BatchArgs::read(matches))),
        },
        #[cfg(not(target_arch = "wasm32"))]
        Command {
            name: "source",
            about: "Read the entry, the roots, the reach and the names of the plain source of a crate, and build its schema",
            grammar: &BATCH_GRAMMAR,
            run: |matches| Ok(source::run(&BatchArgs::read(matches))),
        },
        Command {
            name: "provenance",
            about: "Recompute every hash of the provenance lock over the extracted files",
            grammar: &provenance::GRAMMAR,
            run: |matches| Ok(provenance::run(&ProvenanceArgs::read(matches))),
        },
        Command {
            name: "viewer",
            about: "Judge the shared core of aoviewer and the documentation generator",
            grammar: &viewer::GRAMMAR,
            run: |matches| Ok(viewer::run(&ViewerArgs::read(matches))),
        },
    ],
};

/// The stack of the thread the primitives run on, in bytes.
///
/// The parser under test is recursive descent, as the reference parser is, and
/// the printer and the tree writer of the seam recurse over the tree the same
/// way, so a document spends stack for every level it nests. A JavaScript
/// runtime that meets its stack throws, which the oracle answers as one more
/// response line. A Rust process that meets its stack aborts, and every request
/// behind the document in the batch stays unanswered.
///
/// The main thread takes its stack from the resource limit of the shell, 8 MiB
/// by default on Linux, where the debug build carries 400 levels of nested
/// object values and not 500, while the reference answers a tree for all 1000
/// levels of the corpus. That is more than 16 KiB a level. The size here is 32
/// times that default, which is room for more than ten thousand such levels, ten
/// times the deepest document of the corpus. The pages are reserved and not
/// touched, so a shallow document pays nothing for it.
const STACK_SIZE: usize = 256 * 1024 * 1024;

/// Runs the primitive the command line names on a thread of [`STACK_SIZE`].
///
/// A host that refuses the thread runs the primitive on the main thread, so
/// the seam answers every document the smaller stack carries. A panic of the
/// primitive continues on the main thread, which keeps the exit code a panic
/// of a Rust process has.
fn main() -> ExitCode {
    // The bulk driver of USB moves each endpoint file of FunctionFS through a
    // relay that is this program started again, which answers before it
    // reads its command line.
    #[cfg(target_os = "linux")]
    if let Some(code) = aosabi_carrier_usb::relay_from_env() {
        return code;
    }
    // The devtool is a tool of the build host and part of no served binary, so
    // the thread rule of Documentation/architecture.md section 14 does not reach it, and it
    // needs a stack of STACK_SIZE for the deep documents of the corpus.
    #[expect(
        clippy::disallowed_methods,
        reason = "a tool of the build host that needs a 256 MiB stack for deep documents"
    )]
    let worker = thread::Builder::new()
        .name(String::from("aosabi-devtool"))
        .stack_size(STACK_SIZE)
        .spawn(run);
    match worker {
        Ok(worker) => worker
            .join()
            .unwrap_or_else(|payload| panic::resume_unwind(payload)),
        Err(_) => run(),
    }
}

/// Parses the command line and runs the primitive it names.
fn run() -> ExitCode {
    cli::run(&PROGRAM, env::args_os().skip(1))
}

// SPDX-License-Identifier: MIT
/*
 * aosabi-js abi: the exports of the module and of the native library, and the
 * one import block of the functions of the host.
 *
 * No analogue in the reference.
 */

//! The exports the wrapper calls and the functions of the host the engine
//! calls.
//!
//! A wasm export needs its symbol name, and in edition 2024 the attribute
//! that keeps it is unsafe, so each export line carries an expectation of the
//! lint `unsafe_code`, which the crate denies in place of forbidding. A wasm
//! import is an item of an `extern` block, which edition 2024 writes as
//! `unsafe extern`, so the one block carries one expectation as well. Every
//! item of the block is a `safe fn` whose arguments and answer are numbers
//! alone, so a call of it needs no unsafe block. No import shares its name
//! with an export, since a native linker binds an import and an export of
//! one name to each other.
//!
//! The native library exports the same lines, each under its own
//! expectation, and its `boot` takes the functions of the host as typed
//! `extern "C"` function pointers beside its two numbers, which the module
//! `native` keeps and calls under the names of the import block, so
//! the library holds no import block and no import at all. A call through
//! such a pointer is safe code, since its type is no `unsafe fn`, and the
//! host that hands the pointer keeps the promise a `safe fn` import makes.
//!
//! A `safe fn` import promises that every call is sound, and the module and
//! the host keep that promise together under three conditions. The host runs
//! the JavaScript of every import inside a `try` and answers a throw as a
//! tagged record, or answers the request of an import that answers none, such
//! as `host_enqueue`, with a failure once the export that called it has
//! returned, so no exception of the host crosses a frame of the module. The
//! host drops an instance after a trap, answers every request that waits on
//! it with a failure and calls it no more, since a trap leaves the state of
//! the module as it stood, and a panic of the native library ends the process
//! under the panic strategy `abort` of the profile, so the container restarts
//! where a wasm instance is replaced. And every range the host writes is a
//! range of a vector the module owns, which [`crate::host::alloc`] exposes
//! through `as_mut_ptr`, written while no reference of the module to it is
//! alive and never kept across calls. A range the host reads in place, the
//! bytes of a file of a multipart body, is a part of the body the entry of its
//! request keeps or the bytes of its own a file of a part in base64 holds,
//! never written, which the host reads before the call that names it returns.
//!
//! A request is named by the number the host gives it, and request 0 is the
//! boot. A handle is an index into the table of one request on the host, and
//! the table of the boot holds the copy of the graphql object of the app and
//! its roots. An address is a `usize`, four bytes in the module and eight in
//! the library, and every other number is a `u32`. An import that answers a
//! record writes it into the input buffer through the export `alloc` and
//! returns its length. An import that runs code of the app answers a
//! completion, a normal one or a throw, as the module [`crate::value`] reads
//! it, followed by the settlements callbacks of the app made inside the
//! import, which the module applies as the import returns, and each such
//! import but `js_error`, whose record names the form of its reads alone,
//! takes the context of its call as the first value record of the record it
//! is handed. The host calls the `then` of a thenable while it writes a
//! record, so no import hands it a promise, and the export `settle` answers
//! every other settlement.
//!
//! The exports are the boot, the request loop, the promises of the host and
//! the schema record of a build, which a build asks the module for once on
//! the machine that builds the app and a boot reads in place of the IR.
//! The export `fetch` answers a request at once or answers 0, and the host
//! then waits for the import `host_done`, which a later export calls once
//! the request has answered: `settle` when a promise of the host settles,
//! `run_job` when the microtask of a job of the faithful mode runs, `resume`
//! when the microtask the host reserved for a settlement carried on the
//! answer of an import runs, `wake` when the timer of the host the import
//! `host_timer` asked for fires and `publish` when the app publishes to the
//! pub sub of the engine, each of which drives the request again. The
//! export `release` drops a request once the host has read its answer.
//!
//! An answer whose body is a stream of a result processor of Yoga, the
//! events of a subscription, answers its head alone, and the module pushes
//! each piece of its body through the import `host_push` as the stream
//! gives it, then its end, and last the release of the request, after which
//! the host drops the table of the request. The export `release` of such a
//! request cancels its body, as a reader of the host that goes away cancels
//! it, and the module pushes the release once the cancel has run.

#[cfg(not(target_arch = "wasm32"))]
pub use crate::native::import;

/// The functions of the host, the module `aosabi_host` of the import object
/// the wrapper hands the module.
#[cfg(target_arch = "wasm32")]
pub mod import {
    #[link(wasm_import_module = "aosabi_host")]
    #[expect(
        unsafe_code,
        reason = "a wasm import is an item of an extern block, an unsafe form in edition 2024"
    )]
    unsafe extern "C" {
        /// Projects the object `handle` of the table `table` by the record at
        /// `at`, `len` bytes long, and returns the length of the record of its
        /// completion and the settlements its getters made.
        ///
        /// The record is the context of the call and one byte of its form.
        /// The form 0 answers the own enumerable keys of the object. The form
        /// 1 follows with a count and the members a projection selects, each
        /// its name and the byte of how the projection reads it, and answers
        /// the reads of each member in the order and the count of the
        /// projection, each a test or a value with its completion, until a
        /// member whose value the projection may project before the next one
        /// or a read that throws, a getter run at each read and a function
        /// member bound to its object at the read of its value. Every object,
        /// function and promise of the answer takes a handle in the table of
        /// `request`.
        pub safe fn js_project(table: u32, handle: u32, at: usize, len: u32, request: u32) -> u32;

        /// Reads the member of the object `handle` of the table `table` that
        /// the record at `record`, `len` bytes long, names after the context
        /// of the call, and returns the length of the record of its completion
        /// and the settlements its getter made.
        ///
        /// The byte after the name asks for the value, 0, or for a test, 1,
        /// which answers whether the member is `undefined`, false, true or a
        /// function and hands over no value. A function member is bound to its
        /// object at the read of its value, as the projection of
        /// `wrapResolver` binds it, but a member of a root of the table 0,
        /// which stays unbound, since `wrapResolver` calls a member of a root with no
        /// `this`.
        pub safe fn js_get(table: u32, handle: u32, record: usize, len: u32, request: u32) -> u32;

        /// Calls the function `handle` of the table `table` with the record
        /// of its context and its arguments at `args`, `len` bytes long, and
        /// returns the length of the record of its completion, a normal one
        /// or a throw, and the settlements the call made.
        pub safe fn js_call(table: u32, handle: u32, args: usize, len: u32, request: u32) -> u32;

        /// Reads the error `handle` of the table `table` in the form the
        /// record at `record`, `len` bytes long, names, and returns the length
        /// of the record of its completion and the settlements its reads made.
        ///
        /// The form 0 reads a thrown error as the executor reads it
        /// while it executes, and its record follows with a count and the
        /// byte of each level of the handlers that meet the error, the form 1
        /// an error inside a value and the form 2 the original error of a
        /// `GraphQLError`. The forms 3 and 4 make the reads the logger and the mask make of a
        /// thrown error at the end of the execution, by the handle of the
        /// record the host kept of it: its original error, and then, after a
        /// byte that is 1 where the mask masks it, the reads of the mask or of
        /// the writer of the result, as the module [`crate::value`] reads each
        /// record. The answer of a thrown error opens with the brand of the
        /// error, the class of the wrapper that made it, and an
        /// `AggregateError` crosses with the errors of it the executor
        /// handles. No stack crosses. The host reads the error inside the
        /// store of its request with no locale, since the record takes no
        /// context.
        pub safe fn js_error(table: u32, handle: u32, record: usize, len: u32, request: u32)
        -> u32;

        /// Iterates the value `handle` of the table `table` in the form the
        /// record at `record`, `len` bytes long, names after the context of
        /// the call, and returns the length of the record of its completion
        /// and the settlements the code of the app made.
        ///
        /// The form 0 pulls the next chunk of a reader, the body of a web
        /// standard `Response` a route answered, and answers the promise of
        /// the chunk, which settles with its bytes or with `null` at the end.
        /// The form 1 tests the member `[Symbol.asyncIterator]` of an object,
        /// as `isAsyncIterable` reads it, and answers whether it is a
        /// function as a test of `js_get` answers. The form 2 calls that
        /// member, `object[Symbol.asyncIterator]()`, and answers the iterator
        /// it returns, an object whose `next`, `return` and `throw` the engine
        /// reads and calls as it reads and calls every member of an object of
        /// the host.
        pub safe fn iter_next(
            table: u32,
            handle: u32,
            record: usize,
            len: u32,
            request: u32,
        ) -> u32;

        /// Asks the host for one microtask that runs one job of the agent of
        /// `request` through the export `run_job`, the faithful job mode.
        pub safe fn host_enqueue(request: u32);

        /// Tells the host that `request` has answered, with the answer at
        /// `answer` in the output buffer, as the export `fetch` answers it.
        pub safe fn host_done(request: u32, answer: usize);

        /// Writes the line at `at`, `len` bytes of UTF-8 long, to the stream
        /// `stream` of the host, 1 for `console.log`, where the logger of the
        /// engine writes a record, and 2 for `console.error`, where the
        /// logger of Yoga writes a warning and an error.
        pub safe fn host_write(stream: u32, at: usize, len: u32);

        /// Settles the promise the function `next` of the continuation
        /// `next` of `request` answered, with the completion record at `at`,
        /// `len` bytes long, once the rest of the chain has run.
        pub safe fn js_next(request: u32, next: u32, at: usize, len: u32);

        /// Writes `len` random bytes of the host into the input buffer, and
        /// returns the length of the record, the randomness of a request id.
        pub safe fn host_random(len: u32) -> u32;

        /// Pushes a record of the kind `kind` to the body of the answer of
        /// `request` whose head the host holds, with the bytes at `at`, `len`
        /// long: a piece of the body, its end, its failure with the message,
        /// or the release of the request once the module holds nothing of it.
        pub safe fn host_push(request: u32, kind: u32, at: usize, len: u32);

        /// Returns the milliseconds of the monotonic clock of the host, which
        /// the timers of an agent of the engine read.
        pub safe fn host_now() -> f64;

        /// Asks the host to call the export `wake` for `request` once `delay`
        /// milliseconds have passed, where the least timer of its agent is due.
        pub safe fn host_timer(request: u32, delay: u32);

        /// Hands the host the outbound request of `request` at `at`, `len`
        /// bytes long, its method, its URL, its header fields and its body, and
        /// returns the length of the record of its completion, the promise of
        /// the answer of the `fetch` of the host, which settles with the
        /// status, the status text, the header fields and the body of the
        /// response, or with the message of the failure.
        pub safe fn host_fetch(request: u32, at: usize, len: u32) -> u32;

        /// Hands the host the call of its crypto at `at`, `len` bytes long:
        /// the SHA-256 digest, the RSASSA-PKCS1-v1_5 signature with SHA-256
        /// under a key, the read of a key file, the clock or the import of a
        /// key. Returns the length of the record of the answer, a byte 1
        /// with its bytes or a byte 0 where the host answers none.
        pub safe fn host_crypto(at: usize, len: u32) -> u32;
    }
}

/// Sizes the input buffer to `len` zero bytes and returns its address, where
/// the host writes a record, or 0 when no buffer of that length is made.
// Exports under `#[unsafe(no_mangle)]` are smaller than the glue of
// `wasm-bindgen`.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: u32) -> usize {
    crate::host::alloc(len)
}

/// Releases both buffers.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn dealloc() {
    crate::host::dealloc();
}

/// Boots the app from the boot record in the first `len` bytes of the input
/// buffer and the handle `graphql` of the table of the boot, the copy of the
/// graphql object of the app. Returns 0, or the address of the message of
/// the failure in the output buffer.
///
/// The native library takes the fifteen functions of the host beside the
/// two numbers, in the order of the import block, and keeps them in its table
/// before the boot reads the app, so a pointer the host leaves null fails the
/// boot with its message and the boot calls no function of the host.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn boot(
    len: u32,
    graphql: u32,
    #[cfg(not(target_arch = "wasm32"))] js_project: Option<crate::native::Record>,
    #[cfg(not(target_arch = "wasm32"))] js_get: Option<crate::native::Record>,
    #[cfg(not(target_arch = "wasm32"))] js_call: Option<crate::native::Record>,
    #[cfg(not(target_arch = "wasm32"))] js_error: Option<crate::native::Record>,
    #[cfg(not(target_arch = "wasm32"))] iter_next: Option<crate::native::Record>,
    #[cfg(not(target_arch = "wasm32"))] host_enqueue: Option<crate::native::Enqueue>,
    #[cfg(not(target_arch = "wasm32"))] host_done: Option<crate::native::Done>,
    #[cfg(not(target_arch = "wasm32"))] host_write: Option<crate::native::Write>,
    #[cfg(not(target_arch = "wasm32"))] js_next: Option<crate::native::JsNext>,
    #[cfg(not(target_arch = "wasm32"))] host_random: Option<crate::native::Random>,
    #[cfg(not(target_arch = "wasm32"))] host_push: Option<crate::native::Push>,
    #[cfg(not(target_arch = "wasm32"))] host_now: Option<crate::native::Now>,
    #[cfg(not(target_arch = "wasm32"))] host_timer: Option<crate::native::Timer>,
    #[cfg(not(target_arch = "wasm32"))] host_fetch: Option<crate::native::Outbound>,
    #[cfg(not(target_arch = "wasm32"))] host_crypto: Option<crate::native::Crypto>,
) -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    if let Err(message) = crate::native::install(crate::native::Functions {
        js_project,
        js_get,
        js_call,
        js_error,
        iter_next,
        host_enqueue,
        host_done,
        host_write,
        js_next,
        host_random,
        host_push,
        host_now,
        host_timer,
        host_fetch,
        host_crypto,
    }) {
        return crate::host::output(message.as_bytes());
    }
    crate::boot::boot(len, graphql)
}

/// Answers the step of the schema record in the first `len` bytes of the
/// input buffer, the schema record of the SDL and the IR of a build, which a
/// build writes beside its glue and a boot reads in place of the IR, and
/// returns the address of the answer in the output buffer.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn schema_record(len: u32) -> usize {
    crate::prebuilt::schema_record(len)
}

/// Answers the request record in the first `len` bytes of the input buffer
/// as the request `request`. Returns the address of the answer in the output
/// buffer, or 0 while the request waits on the host, whose last call hands
/// the answer to the import `host_done`.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn fetch(len: u32, request: u32) -> usize {
    crate::request::fetch(len, request)
}

/// Runs the jobs the oldest reservation of the host for the request
/// `request` keeps, in the microtask the host reserved at the call that
/// settled a promise inside an import, and drives the request again.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn resume(request: u32) {
    crate::request::resume(request);
}

/// Answers the call the record in the first `len` bytes of the input buffer
/// names on the request `request`, its opening and its end, a read of the
/// context the engine runs it in or a call of a plugin of the engine on it,
/// and returns the address of the completion record in the output buffer.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn ctx(request: u32, len: u32) -> usize {
    crate::context::ctx(request, len)
}

/// Answers the call of the logger the record in the first `len` bytes of
/// the input buffer names, a record the engine renders and writes through
/// the import `host_write`, or a logger it derives, and returns the address
/// of the completion record in the output buffer. The name is no symbol of
/// the C library or of its mathematics, whose `log` a native linker would
/// bind to an export of that name.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn logger(len: u32) -> usize {
    crate::log::log(len)
}

/// Settles the promise `handle` of the table of `request` with the value
/// record in the first `len` bytes of the input buffer, fulfilled when bit 0
/// of `ok` is set and rejected when it is clear, through the resolving
/// functions the host handed a thenable it adopts when bit 1 is set, and
/// drives the request.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn settle(request: u32, handle: u32, ok: u32, len: u32) {
    let (fulfilled, adopted) = crate::value::settlement_of(ok);
    crate::request::settle(request, handle, fulfilled, adopted, len);
}

/// Runs the first job the host was asked to run for `request`, one microtask
/// of the host in the faithful mode, inside a poll of the request that drives
/// it.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn run_job(request: u32) {
    crate::request::run_job(request);
}

/// Drops the request `request` with every promise it waits on and every job
/// the host holds of it, or cancels the body the module pushes for it, whose
/// release the module pushes once the cancel has run.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn release(request: u32) {
    crate::request::release_by_host(request);
}

/// Drives the request `request` at a deadline of the timers of its agent,
/// in the task of the timer of the host the import `host_timer` asked for.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn wake(request: u32) {
    crate::request::wake(request);
}

/// Publishes the record in the first `len` bytes of the input buffer to the
/// pub sub of the engine it names, `pubSub.publish(routingKey, ...args)`, and
/// drives every request whose body the module pushes.
#[expect(
    unsafe_code,
    reason = "an export needs its symbol name, an unsafe attribute in edition 2024"
)]
#[unsafe(no_mangle)]
pub extern "C" fn publish(len: u32) {
    crate::pubsub::publish(len);
}

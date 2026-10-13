// SPDX-License-Identifier: MIT
/*
 * aosabi-js: the build script that gives the module core.wasm its stack.
 *
 * No analogue in the reference. It is the stack the Worker glue of the
 * gateway card takes in tests/fixtures/rs/gateway-worker-probe/build.rs.
 */

//! The build script of the module `core.wasm`.
//!
//! It gives the module of `wasm32-unknown-unknown` a stack of 4 MiB in its
//! linear memory in place of the 1 MiB rustc gives such a module, so a
//! federation of the app serves the default value of a remote nested as deep as
//! the reference builds one, 1,746 lists, as the Worker glue of the gateway
//! card takes it. On workerd the stack of 1 MiB traps on the four crafted
//! introspections at the last depth the reference serves and the stack of 4 MiB
//! serves each. On Bun the frames of the module stand on the stack of the
//! thread, whose bound `JavaScriptCore` keeps, so `gw4-44` meets that bound
//! first there, which the same page names. The reference refuses one level more
//! through the bound the port keeps for the end of its stack, so the larger
//! stack serves no level the reference refuses. The native library of the
//! feature `native` runs on the stack of the thread of its host and takes no
//! link argument.

/// The bytes of the stack of the module, 4 MiB.
const STACK: u32 = 4 * 1024 * 1024;

fn main() {
    println!("cargo::rerun-if-changed=build.rs");
    if std::env::var("CARGO_CFG_TARGET_ARCH").as_deref() == Ok("wasm32") {
        println!("cargo::rustc-cdylib-link-arg=-zstack-size={STACK}");
    }
}

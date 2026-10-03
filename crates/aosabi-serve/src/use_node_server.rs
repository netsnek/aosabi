// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-serve: the serving plugin.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/plugins/use-node-server.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 */

//! `use_node_server`.
//!
//! The plugin is named `node-server`, runs in the pass of the strategy `last`
//! and in the role `web` alone, and binds the port in its setup, so the port
//! binds only after every route is mounted. A native build of Aosabi is always
//! the Node case of `isNodeRuntime`, since a Worker build links no serving
//! crate at all.
//!
//! The listener serves TLS when the boot read a certificate from
//! `AOSABI_TLS_CERT` and `AOSABI_TLS_KEY`, and the line then names `https`,
//! which a build without the feature `tls` never prints.

use std::rc::Rc;

use aosabi_core::{
    App,
    Plugin,
    PluginObject,
    PluginRole,
    Strategy,
    Thrown,
    app::env_alias,
    console_log,
    glue::__wrap, //
};

use crate::{
    listener::TcpListen,
    serve::boot_tls, //
};

/// The name of the plugin, `name: 'node-server'`.
pub const PLUGIN_NAME: &str = "node-server";

/// The port the rule answers when neither the options nor `PORT` name one.
pub const DEFAULT_PORT: u16 = 3_000;

/// The options of the plugin, `NodeServerOptions`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NodeServerOptions {
    /// The port to bind, `port?`.
    pub port: Option<u16>,
}

/// Returns the plugin with no option, `useNodeServer()`.
#[must_use]
pub fn use_node_server() -> Rc<dyn Plugin> {
    use_node_server_with(NodeServerOptions::default())
}

/// Returns the plugin with `options`, `useNodeServer(options)`.
///
/// The object of the reference carries the keys `name`, `strategy`, `roles` and
/// `setup`. The literal writes the same four fields, so its members are `name`,
/// `roles` and `setup`, in the order of `MEMBER_ORDER` of the core, and its
/// strategy `last` stands in the field of its own of the boot report, since the
/// boot writes a strategy onto every plugin and `strategy` is never a member.
#[must_use]
pub fn use_node_server_with(options: NodeServerOptions) -> Rc<dyn Plugin> {
    Rc::new(PluginObject {
        name: __wrap::name(PLUGIN_NAME),
        strategy: Some(Strategy::Last),
        roles: __wrap::roles([PluginRole::Web]),
        setup: __wrap::setup(move |app: &mut App| setup(app, options)),
        ..PluginObject::default()
    })
}

/// The setup of the plugin.
fn setup(app: &mut App, options: NodeServerOptions) -> aosabi_core::Result<()> {
    if env_alias::var("NODE_ENV").as_deref() == Some("development") {
        return Ok(());
    }
    let port = port_rule(&options);
    let tls = boot_tls();
    let scheme = if tls.is_some() { "https" } else { "http" };
    let listener = TcpListen::bind(port, tls).map_err(|error| Thrown::error(&error.to_string()))?;
    let bound = listener.local_addr().map_or(port, |addr| addr.port());
    console_log!("Aosabi running at {scheme}://localhost:{bound}");
    app.attach_listener(Box::new(listener));
    Ok(())
}

/// Returns the port to bind, `options.port ?? (Number(process.env.PORT) || 3000)`.
///
/// The variable `PORT` is read through the alias table, so `AOSABI_PORT` wins
/// when both are set, and a value `Number` reads as 0 or `NaN`, an empty or a
/// wrong one among them, falls back to 3000 as `||` does.
#[must_use]
pub fn port_rule(options: &NodeServerOptions) -> u16 {
    if let Some(port) = options.port {
        return port;
    }
    env_alias::var("PORT")
        .and_then(|value| js_number_port(&value))
        .unwrap_or(DEFAULT_PORT)
}

/// Returns the port `Number(value)` gives, `None` for 0, `NaN` and a number
/// that is no port.
///
/// The function `Number` trims white space, reads the empty string as 0, a
/// decimal with a fraction or an exponent, and the prefixes `0x`, `0o` and
/// `0b`. A number that is not a whole number from 1 to 65535 falls back as
/// well, where Node's `listen` refuses it and the setup of the reference
/// throws, a divergence no fixture reaches. A number that passes the checks is
/// a whole number in the range of a port, so a cast gives the port exactly and
/// no float is ever formatted.
fn js_number_port(value: &str) -> Option<u16> {
    let text = value.trim();
    let number = if let Some(hex) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
        u32::from_str_radix(hex, 16).ok().map(f64::from)
    } else if let Some(octal) = text.strip_prefix("0o").or_else(|| text.strip_prefix("0O")) {
        u32::from_str_radix(octal, 8).ok().map(f64::from)
    } else if let Some(binary) = text.strip_prefix("0b").or_else(|| text.strip_prefix("0B")) {
        u32::from_str_radix(binary, 2).ok().map(f64::from)
    } else if text
        .bytes()
        .all(|byte| byte.is_ascii_digit() || b".eE+-".contains(&byte))
    {
        text.parse::<f64>().ok()
    } else {
        None
    }?;
    if number.fract() != 0.0 || !(1.0..=65_535.0).contains(&number) {
        return None;
    }
    // The checks above leave a whole number from 1 to 65535, which the cast
    // keeps as it is.
    #[expect(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the number is a whole number from 1 to 65535"
    )]
    let port = number as u16;
    Some(port)
}

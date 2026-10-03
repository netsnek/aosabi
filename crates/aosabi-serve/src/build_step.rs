// SPDX-License-Identifier: MIT AND Apache-2.0
/*
 * aosabi-serve: the build step and the abort of a configuration that throws.
 *
 * Derived from Pylon 2.9.5 by Nico Schett,
 * packages/pylon/src/cli/builder/bundler/bundler.ts, Apache-2.0.
 * Changed by the Aosabi authors.
 *
 * The abort message is the literal file config_abort_message.rs.
 */

//! `run_build_step`, `run_app_build_step`, `load_app` and the abort message
//! with its exit.
//!
//! The build step imports the configuration, calls it when it is
//! a factory, and stops the build with the constant head of its message and
//! the cause when that throws, because the app would otherwise run with no
//! plugins. A configuration of Aosabi throws by panicking, so the step runs
//! `config()` under a panic hook that prints the pinned constant, then the
//! payload of the panic as the cause, to standard error, and exits 1 from the
//! hook, so the status is 1 under both panic strategies.
//! The boot evaluates the configuration under the same hook.
//!
//! An app of the builder of the facade carries its configuration, the plugins
//! and the options of the chain of its entry, so [`load_app`] evaluates the
//! entry under the same hook, and an entry that answers an error prints the
//! abort message with the text of the error as the cause and exits with
//! [`CONFIG_ABORT_EXIT`], the exit and the message of a configuration that
//! throws at load, which is the form of the fixture `badconfig-app`.
//! [`config_abort`] is that abort, the one place the boot, the build step and
//! the mode `--requests` print it and take its exit from.

use std::{
    panic::{
        self,
        PanicHookInfo, //
    },
    process::{
        self,
        ExitCode, //
    },
};

use aosabi_core::{
    App,
    AppConfig,
    BuildContext,
    IntoApp as _,
    IntoConfig,
    IntoEntry,
    PluginError, //
};

use crate::runtime::Runtime;

/// The constant head of the abort message.
pub const CONFIG_ABORT_MESSAGE: &str = include_str!("config_abort_message.rs");

/// The variable that runs the build step in place of the server.
pub const BUILD_ENV: &str = "AOSABI_BUILD";

/// The exit of a configuration that throws at load, the status node exits with
/// on the rejected top level await of the generated entry, which the abort
/// hook and [`config_abort`] exit with.
pub const CONFIG_ABORT_EXIT: u8 = 1;

/// Returns the head of the abort message of Aosabi, the constant.
#[must_use]
pub fn abort_message() -> String {
    CONFIG_ABORT_MESSAGE.to_owned()
}

/// Returns the cause a panic carries, its message.
fn cause(info: &PanicHookInfo<'_>) -> String {
    let payload = info.payload();
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_owned();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    String::from("the configuration panicked")
}

/// Prints the abort message with `cause` as its cause and answers the exit
/// of a configuration that throws at load, as the abort hook does for a
/// configuration that panics.
#[must_use]
pub fn config_abort(cause: &str) -> ExitCode {
    eprintln!("{}{cause}", abort_message());
    ExitCode::from(CONFIG_ABORT_EXIT)
}

/// Installs the abort hook, which prints the abort message and exits with
/// [`CONFIG_ABORT_EXIT`] when the code it guards panics, and answers the hook
/// it replaced.
///
/// A binary calls it from the one loader of the configuration its main
/// names, [`load_config`] or [`load_app`], so it is inlined into that loader.
#[inline]
fn install_abort_hook() -> Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send + 'static> {
    let previous = panic::take_hook();
    panic::set_hook(Box::new(|info| {
        eprintln!("{}{}", abort_message(), cause(info));
        process::exit(i32::from(CONFIG_ABORT_EXIT));
    }));
    previous
}

/// Restores the hook `previous` that [`install_abort_hook`] replaced, inlined
/// into its one loader as [`install_abort_hook`] is.
#[inline]
fn restore_hook(previous: Box<dyn Fn(&PanicHookInfo<'_>) + Sync + Send + 'static>) {
    let _ = panic::take_hook();
    panic::set_hook(previous);
}

/// Evaluates `config` under the abort hook, which prints the abort message
/// and exits with [`CONFIG_ABORT_EXIT`] when it panics, and restores the hook
/// before it answers.
pub async fn load_config<C: IntoConfig>(config: fn() -> C) -> AppConfig {
    let previous = install_abort_hook();
    let loaded = config().into_config().await;
    restore_hook(previous);
    loaded
}

/// Evaluates `entry` under the abort hook and answers its app with the
/// configuration the app carries, the default where it carries none.
///
/// # Errors
///
/// Answers the abort of [`config_abort`] with the text of the error an entry
/// answered as its cause, as the hook ends an entry that panics.
pub fn load_app<E: IntoEntry>(entry: fn() -> E) -> Result<(App, AppConfig), ExitCode> {
    let previous = install_abort_hook();
    let answered = entry().into_entry();
    restore_hook(previous);
    let mut app = answered.map_err(|cause| config_abort(&cause))?.into_app();
    let config = app.take_config().unwrap_or_default();
    Ok((app, config))
}

/// Runs the build step: evaluates `config` under the abort hook and calls the
/// `build` hook of every configured plugin once, `initBuildPlugins`.
///
/// The step answers 0 when every hook answered, and 1 when one failed, whose
/// error goes to standard error.
pub fn run_build_step<C: IntoConfig>(runtime: &Runtime, config: fn() -> C) -> ExitCode {
    runtime.block_on(async move { build_hooks(&load_config(config).await).await })
}

/// Runs the build step over the configuration the app of `entry` carries, as
/// [`run_build_step`] runs it over a configuration of its own, and answers
/// the abort of [`config_abort`] for an entry that answers an error.
pub fn run_app_build_step<E: IntoEntry>(runtime: &Runtime, entry: fn() -> E) -> ExitCode {
    runtime.block_on(async move {
        match load_app(entry) {
            Ok((_, config)) => build_hooks(&config).await,
            Err(code) => code,
        }
    })
}

/// Calls the `build` hook of every plugin of `config` once, in order, and
/// answers 0 when every hook answered and 1 when one failed, whose error goes
/// to standard error.
async fn build_hooks(config: &AppConfig) -> ExitCode {
    let context = BuildContext {
        mode: String::from("build"),
        ..BuildContext::default()
    };
    let Some(plugins) = &config.plugins else {
        return ExitCode::SUCCESS;
    };
    for (index, slot) in plugins.slots().iter().enumerate() {
        let Some(build) = slot.plugin.build(&context) else {
            continue;
        };
        if let Err(cause) = build.await {
            eprintln!("{}", PluginError::of(slot.plugin.name(), index, cause));
            return ExitCode::from(1);
        }
    }
    ExitCode::SUCCESS
}

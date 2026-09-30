// SPDX-License-Identifier: MIT
/*
 * aosabi-core: the logger.
 *
 * No file of its own on the reference side.
 */

//! The logger.
//!
//! | Module | Translates | Holds |
//! | --- | --- | --- |
//! | `logger` | `core/logger.ts` | [`Logger`], [`LogLevel`], [`LogRecord`], [`LogSink`], [`LoggerConfig`], [`LevelSpec`], [`LogFormat`], [`LoggerOption`], [`logger()`], [`get_logger`], [`get_root_logger`], [`create_logger`], [`configure_logger`], [`parse_level_spec`], [`render_line`] and [`normalize_error`] |
//! | `pretty` | `core/logger-pretty.ts` | [`pretty_lines`], [`pretty_sink`] and [`devtools_sink`] |
//! | `registry` | no file | the table of bound loggers, [`run_with_logger`], [`access_log_enabled`], [`set_access_log`] and [`job_log_level`] |

pub mod pretty;
pub mod registry;

#[expect(
    clippy::module_inception,
    reason = "the file keeps the name of core/logger.ts, which it translates"
)]
pub mod logger;

pub use self::{
    logger::{
        IntoLogFields,
        LevelSpec,
        LogFormat,
        LogLevel,
        LogRecord,
        LogSink,
        Logger,
        LoggerConfig,
        LoggerOption,
        configure_logger,
        create_logger,
        get_logger,
        get_root_logger,
        logger,
        normalize_error,
        parse_level_spec,
        render_line, //
    },
    pretty::{
        devtools_sink,
        pretty_lines,
        pretty_sink, //
    },
    registry::{
        WithLogger,
        access_log_enabled,
        job_log_level,
        run_with_logger,
        set_access_log, //
    },
};

#[cfg(feature = "host-fields")]
pub use self::logger::HostField;

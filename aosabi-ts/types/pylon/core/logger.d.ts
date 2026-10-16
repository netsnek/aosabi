export type LogLevel = 'trace' | 'debug' | 'info' | 'warn' | 'error' | 'fatal';
export type LogFields = Record<string, unknown>;
export interface LogRecord {
    time: number;
    level: LogLevel;
    msg: string;
    tag?: string;
    [key: string]: unknown;
}
export interface Logger {
    trace(msg: string, fields?: LogFields): void;
    debug(msg: string, fields?: LogFields): void;
    info(msg: string, fields?: LogFields): void;
    warn(msg: string, fields?: LogFields): void;
    error(msg: string, fields?: LogFields): void;
    fatal(msg: string, fields?: LogFields): void;
    /** A logger that always carries `bindings` on every record (e.g. `{requestId}`). */
    child(bindings: LogFields): Logger;
    /** A logger tagged with a hierarchical component label; composes with `:` (a → a:b). */
    withTag(tag: string): Logger;
    /** A logger that ALSO fans each record (at or above `minLevel`, default `info`) to `sink` —
     *  e.g. a BullMQ `job.log`. The primary stdout sink + level gate are unchanged. */
    tee(sink: LogSink, minLevel?: LogLevel): Logger;
    /** The effective minimum level for THIS logger's tag (resolved against per-tag config). */
    readonly level: LogLevel;
}
/** Where a built record goes. Default emits JSON to stdout; override for pino/OTel/tests. */
export type LogSink = (record: LogRecord) => void;
/** Render a record as a single human line (dev format + BullMQ job.log dashboard lines). */
export declare const renderLine: (record: LogRecord) => string;
/** Parse `LOG_LEVEL` — a scalar (`info`) or a comma map (`info,db=debug,queue:email=trace`). */
export declare function parseLevelSpec(spec: string): LogLevel | Record<string, LogLevel>;
/** Build a logger with an explicit level/sink/tag/bindings (used for the root, config, tests). */
export declare const createLogger: (opts?: {
    level?: LogLevel | Record<string, LogLevel>;
    sink?: LogSink;
    tag?: string;
    bindings?: LogFields;
}) => Logger;
/** The process-wide root logger (used outside any request/job scope). */
export declare const getRootLogger: () => Logger;
/** @internal Replaces the root (used by `configureLogger` and tests). Not part of the public API. */
export declare const __setRootLogger: (logger: Logger) => void;
/** The current scoped logger (request/job), or the root logger outside a scope. */
export declare const getLogger: () => Logger;
/** Run `fn` with `log` as the active logger for its whole (a)sync execution. */
export declare const runWithLogger: <T>(log: Logger, fn: () => T) => T;
/**
 * A module-scoped, lazy, tagged logger. Assign it once at the top of a file; it re-resolves the
 * current scoped logger on every call, so it stays correlated AND carries the tag. Prefer this (or
 * `getLogger()` at call time) over capturing `const log = getLogger()` at import — a snapshot taken
 * at module load has no request context.
 */
export declare const logger: (tag: string) => Logger;
export interface LoggerConfig {
    /** A scalar level, or a per-tag map (`{'*':'info', db:'debug'}`). Env `LOG_LEVEL` overrides. */
    level?: LogLevel | Record<string, LogLevel>;
    /** `'json'` (prod default) · `'pretty'` (ANSI terminal line) · `'devtools'` (CSS headline + an
     *  expandable record object for Chrome DevTools) · `'auto'` (json in prod; in dev, `devtools`
     *  under `--inspect`, else `pretty`). */
    format?: 'json' | 'pretty' | 'devtools' | 'auto';
    /** Fields added to every record (service, version, region…). */
    base?: LogFields;
    /** Dotted paths to mask before the sink (`authorization`, `user.password`). */
    redact?: string[];
    /** Override the destination entirely (pino, OTel, a file, …). Wins over `format`. */
    sink?: LogSink;
    /** Queue jobs also tee their logs to BullMQ's persisted per-job log (dashboard). This is its
     *  threshold — separate from stdout — so debug-on-stdout doesn't bloat Redis. Default `info`. */
    job?: {
        level?: LogLevel;
    };
}
/** The threshold for the queue `job.log()` tee (from `config.logger.job.level`). */
export declare const jobLogLevel: () => LogLevel;
/**
 * Apply `config.logger` at boot. `false` → access line off (root unchanged). `true`/absent →
 * defaults. An object → rebuild the root from it. Env (`LOG_LEVEL`, `PYLON_LOG_FORMAT`) overrides
 * so ops can change verbosity/format without a redeploy.
 */
export declare const configureLogger: (cfg: boolean | LoggerConfig | undefined) => void;
/** Enable/disable the per-request access line (wired from `config.logger` at boot). */
export declare const setAccessLog: (on: boolean) => void;
export declare const accessLogEnabled: () => boolean;

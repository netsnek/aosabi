// SPDX-License-Identifier: MIT
/*
 * sentry.d.ts: the types of the entry aosabi/sentry/node of the package, the shape of the members of
 * @sentry/node 8.55.0 that lib/sentry.js exports.
 *
 * The entry exports init alone, the member the apps call, so the declaration is the shape
 * of init of @sentry/node with the keys of its options the engine reads, which every other key of
 * those options may stand beside with no effect, while a key whose value is a function or holds one
 * throws (docs/ts1-surface.md of the Aosabi repository, section 14).
 */

/** The options of init the client of the engine takes, beside any other key of @sentry/node. */
export interface NodeOptions {
  /** The project the events go to, SENTRY_DSN of the process where the options name none. */
  dsn?: string
  /** The environment of an event, SENTRY_ENVIRONMENT of the process where the options name none. */
  environment?: string
  /**
   * The release of an event, where the options name none SENTRY_RELEASE of the process or the variable
   * of a provider of CI that getSentryRelease of @sentry/node reads.
   */
  release?: string
  /** The share of the errors the client sends, every one where the options name none. */
  sampleRate?: number
  [key: string]: unknown
}

/** Initializes the client of Sentry of the app, init of @sentry/node. */
export declare function init(options?: NodeOptions): void

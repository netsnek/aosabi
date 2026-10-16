// SPDX-License-Identifier: MIT
/*
 * aosabi-compat: the register of Pylon defects Aosabi reproduces on purpose,
 * and the list of reference behaviour Aosabi deliberately does not port.
 *
 * No analogue in the reference. A reproduced defect carries an id, the line of
 * the reference that decides it, and one comparison case pinned to it in the
 * harness. A defect without an id is a bug in Aosabi. Reproducing is the
 * default, because the acceptance rule is byte equality and a fixed defect is a
 * byte difference.
 *
 * Q02 is retired and is never reused: it names the unmatched brace inside the
 * Voyager stylesheet, and Aosabi ships no Voyager stylesheet, so there is no
 * stray brace to reproduce. The quirks of Node and llhttp, Q18 to Q60, live
 * with the HTTP layer and its table in docs/byte-equality.md section 13.1. The
 * tenant null of the provider takes Q85, the next free id after Q83 and Q84.
 */

//! The register of Pylon defects Aosabi reproduces on purpose.
//!
//! Reproducing a defect is the default, because the acceptance rule is byte
//! equality and a fixed defect is a byte difference. [`NOT_PORTED`] carries the
//! other direction, which is reference behaviour Aosabi deliberately leaves
//! out, each with its reason.
//!
//! Byte equality with Pylon is a phase of the project, so an entry may carry
//! the condition on which it drops and what Aosabi does then.

#![no_std]

/// How well a register entry is established against the reference tree.
///
/// The two values carry the meaning CONTRIBUTING.md section 1 gives them
/// everywhere else in the repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Evidence {
    /// Read in the cited source at the cited lines.
    Proven,
    /// Stated in documentation or in a source comment, without the
    /// implementing code being read.
    Documented,
}

/// One Pylon defect or asymmetry that Aosabi reproduces on purpose.
#[derive(Debug, Clone, Copy)]
pub struct Quirk {
    /// The stable identifier a failing comparison cites.
    pub id: &'static str,
    /// What the reference does, in one sentence.
    pub what: &'static str,
    /// The reference path and line range that decides it, under.
    pub source: &'static str,
    /// How well the entry is established.
    pub evidence: Evidence,
    /// When the entry drops and what Aosabi does then, for an entry that
    /// carries a drop condition.
    pub drops: Option<&'static str>,
}

/// The reproduced behaviour of the reference, in identifier order.
///
/// The length of this slice is the count the environment notebook checks.
pub const QUIRKS: &[Quirk] = &[
    Quirk {
        id: "Q01_DUPLICATE_PLUGIN_WRITEBACK",
        what: "the configuration step writes the built plugin array back onto the configuration, so the four always on plugins appear twice in config.plugins after boot",
        source: "packages/pylon/src/app/pylon-handler.ts:185",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of the boot report and of inspect, and then the boot leaves config.plugins as the application wrote it, so config_plugins_after_boot of the boot report lists each plugin once, while the arrays of each pass stay in their own fields",
        ),
    },
    Quirk {
        id: "Q04_LANDING_VERSION_STRING",
        what: "the landing page renders the Pylon package version, which is 2.9.5 at the reference commit",
        source: "packages/pylon/src/plugins/use-unhandled-route.ts:3",
        evidence: Evidence::Proven,
        drops: None,
    },
    Quirk {
        id: "Q07_NO_DEPRECATED",
        what: "there is no @deprecated support, the JSDoc tag folds into the description",
        source: "packages/pylon/src/cli/builder/schema/type-definition-builder.ts",
        evidence: Evidence::Documented,
        drops: None,
    },
    Quirk {
        id: "Q08_EMPTY_INPUT_PLACEHOLDER",
        what: "an input object with no fields renders a single placeholder field named _ of type String",
        source: "packages/pylon/src/ir/sdl.ts:111",
        evidence: Evidence::Proven,
        drops: None,
    },
    Quirk {
        id: "Q09_INPUT_UNION_FIRST_MEMBER",
        what: "a union in input position warns and collapses to its first member",
        source: "packages/pylon/src/cli/builder/schema/type-definition-builder.ts",
        evidence: Evidence::Documented,
        drops: None,
    },
    Quirk {
        id: "Q11_IN_CONTEXT_EM_DASH",
        what: "the @inContext directive description carries the code point U+2014",
        source: "packages/pylon/src/core/in-context.ts:41",
        evidence: Evidence::Proven,
        drops: None,
    },
    Quirk {
        id: "Q13_VIEWER_ALWAYS_ON",
        what: "the viewer plugin slot survives graphiql: false and cannot be disabled, so it is always first in the array. Aosabi keeps the slot with the aoviewer plugin in it under its own key viewer: true alone, divergence D48, which keeps the array and the hook order equal under that key. The page that route serves is out of contract",
        source: "packages/pylon/src/app/pylon-handler.ts:146-150",
        evidence: Evidence::Proven,
        drops: None,
    },
    Quirk {
        id: "Q14_DESCRIPTION_VERBATIM",
        what: "descriptions are inserted with no escaping and no re-indentation",
        source: "packages/pylon/src/ir/sdl.ts",
        evidence: Evidence::Documented,
        drops: None,
    },
    Quirk {
        id: "Q15_INPUT_DESCRIPTION_FROM_TYPE",
        what: "an input field description comes from the field's type symbol while an output field description comes from the property symbol",
        source: "packages/pylon/src/cli/builder/schema/type-definition-builder.ts",
        evidence: Evidence::Documented,
        drops: None,
    },
    Quirk {
        id: "Q16_UNLABELED_REST_DROPPED",
        what: "an unlabeled rest parameter warns and is dropped. Not expressible in Rust, kept so the register is complete",
        source: "packages/pylon/src/cli/builder/schema/schema-parser.ts",
        evidence: Evidence::Documented,
        drops: None,
    },
    Quirk {
        id: "Q61_SUBSCRIPTION_OBJECT_PAYLOAD_PROJECTED",
        what: "the root resolver projects what a function returns onto the selected fields of an object return type, so a Repeater returned for an object typed subscription field becomes a plain object and the answer is the error Subscription field must return Async Iterable. Received: {}.",
        source: "packages/pylon/src/core/define-pylon.ts:278-288",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of subscriptions, and then a returned Repeater is left unprojected, so an object typed payload streams",
        ),
    },
    Quirk {
        id: "Q62_IN_CONTEXT_NOT_ON_SUBSCRIPTION",
        what: "the plugin useInContext reads @inContext in onExecute alone, which a subscription never reaches, so the directive declared on SUBSCRIPTION sets no locale and no context for one",
        source: "packages/pylon/src/plugins/use-in-context.ts:19",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of subscriptions, and then @inContext applies to a subscription as to a query",
        ),
    },
    Quirk {
        id: "Q63_ERROR_LOGGER_SILENT_ON_SUBSCRIPTION",
        what: "the plugin useGraphqlErrorLogger hooks onExecute alone, so an error of a subscription writes no graphql error line",
        source: "packages/pylon/src/plugins/use-graphql-error-logger.ts:14-37",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of subscriptions, and then a subscription error writes the graphql error line",
        ),
    },
    Quirk {
        id: "Q64_PUBSUB_NULLISH_PAYLOAD_TAKES_ID",
        what: "publish takes args[1] ?? args[0] ?? null as the payload, so a null payload published beside an id delivers the id itself as the payload",
        source: "node_modules/.pnpm/@graphql-yoga+subscription@5.0.5/node_modules/@graphql-yoga/subscription/cjs/create-pub-sub.js:13-14",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of subscriptions, and then a null payload is published beside its id",
        ),
    },
    Quirk {
        id: "Q77_FEDERATION_INTROSPECTION_HEADERS",
        what: "the introspection request of a federation carries the headers its headers function answers for no context, so a function that forwards a header of the caller sends that header empty",
        source: "packages/pylon/src/core/gateway.ts:907-914 with node_modules/.pnpm/@graphql-tools+wrap@11.1.9_graphql@16.12.0/node_modules/@graphql-tools/wrap/dist/index.cjs:2342-2362",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of the outbound requests of a federation, and then the introspection request carries no field the headers function answers for no context",
        ),
    },
    Quirk {
        id: "Q78_FEDERATION_REMOTE_ERROR_NO_LOCATIONS",
        what: "a remote error reaches the client of the front with its message, its extensions and the front path and without locations, because the HTTP executor rebuilds it through createGraphQLError, and the GraphQLError of graphql 16 takes no locations from its options",
        source: "node_modules/.pnpm/@graphql-tools+executor-http@3.1.0_@types+node@20.19.33_graphql@16.12.0/node_modules/@graphql-tools/executor-http/dist/index.cjs:751-760 with node_modules/.pnpm/@graphql-tools+utils@11.0.0_graphql@16.12.0/node_modules/@graphql-tools/utils/cjs/errors.js:27-36 and node_modules/.pnpm/graphql@16.12.0/node_modules/graphql/error/GraphQLError.js:85-89 and :122-129",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of the answers of a federation, and then a remote error at the front carries the locations of the front's field, as a local error does",
        ),
    },
    Quirk {
        id: "Q79_FEDERATION_FORCE_BEFORE_NEEDS",
        what: "the force transform runs before the needs transform, so an argument that needs.__args writes on a forced field reaches the remote, contrary to the comment of the reference beside the list",
        source: "packages/pylon/src/core/gateway.ts:916-932 with node_modules/.pnpm/@graphql-tools+delegate@12.0.9_graphql@16.12.0/node_modules/@graphql-tools/delegate/dist/index.cjs:1704-1740",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of the outbound requests of a federation, and then the force transform runs last among the request transforms, as the comment of the reference says, and a forced argument overrides one that needs wrote",
        ),
    },
    Quirk {
        id: "Q80_FEDERATION_STRICT_ROOT_TYPES",
        what: "a strict federation refuses every delegation unless it patches the root types, because the patch transform appends __typename to the root selection set as well and strict then meets the root type",
        source: "packages/pylon/src/core/gateway.ts:695-718 and :755-760",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of the answers of a federation, and then strict looks up a patch for the types below the root alone",
        ),
    },
    Quirk {
        id: "Q83_FEDERATION_DATE_ARGUMENT_EMPTY_OBJECT",
        what: "a Date argument of a delegation reaches the remote as the empty object {}, the value of its variable and an item or a member {} inside a list or an input object, because pylon pull types a remote Date argument as the global Date and astFromJSValue writes an object through Object.entries, which a Date answers with no entry, so a Pylon remote refuses the variable and the front answers that refusal at the delegated field",
        source: "packages/pylon/src/cli/pull/index.ts:29-35 and :55-68 with packages/pylon/src/core/gateway.ts:244-256 and :356-397",
        evidence: Evidence::Proven,
        drops: Some(
            "on the owner's word that byte equality with Pylon is no longer the acceptance rule of the outbound requests of a federation, and then a Date argument goes out as the text the bound Date scalar writes, its toISOString, and the remote answers data",
        ),
    },
    Quirk {
        id: "Q84_FEDERATION_REDIRECT_CARRIES_HEADERS",
        what: "a delegation that meets a redirect of the status 301, 302, 303, 307 or 308 sends its method, its body and every header of the request again to the location, the authorization a headers function forwards among them, to another origin as well and from https to http as well, and a 303 keeps the method POST, because the fetch of @whatwg-node/node-fetch 0.8.5 builds the next request from the old one",
        source: "node_modules/.pnpm/@whatwg-node+node-fetch@0.8.5/node_modules/@whatwg-node/node-fetch/cjs/fetchNodeHttp.js:84-100 with cjs/utils.js:52-54",
        evidence: Evidence::Proven,
        drops: Some(
            "once the project decides which credentials a redirect carries to another origin, or when the fetch of the reference stops sending the headers across origins, and then a hop to another origin drops authorization as the Fetch standard drops it",
        ),
    },
    // Lives in `zitadel_principal` of
    // `plugins/aosabi-auth-zitadel/src/zitadel_auth.rs` over the tenant of the
    // principal of `aosabi-auth-contract`, an `Option<PrincipalId>` whose
    // `None` stands for a tenant `null` and for no tenant alike. Pinned by step
    // 2 of 63_zitadel_auth, which counts the lines of `principal` it reads
    // alike under this id.
    Quirk {
        id: "Q85_ZITADEL_TENANT_NULL_AS_ABSENT",
        what: "the provider sets the tenant of the principal to the answer of the tenant mapper as it is, so a mapper that answers null, a value outside the type string | number | undefined its option declares, gives the principal the tenant null beside the tenant undefined of a mapper that answers nothing, which the readers of the tenant in Pylon read alike and a reader of an application can tell apart",
        source: "packages/pylon/src/auth/zitadel.ts:39-40 and :128",
        evidence: Evidence::Proven,
        drops: Some(
            "once the project decides how a tenant null is read, and then the tenant of the principal takes a state for null beside a tenant and no tenant, so a mapper that answers null gives the principal the tenant null as the reference does",
        ),
    },
];

/// They are recorded so that nothing reuses them and so that a reader who
/// meets an id in another document finds out where it went.
pub const RETIRED: &[(&str, &str)] = &[(
    "Q02_VIEWER_STRAY_BRACE",
    "the unmatched closing brace inside the Voyager stylesheet. Retired with docs/decisions/ADR-0009-aoviewer-owns-viewer.md, because Aosabi carries no Voyager text",
)];

/// One piece of reference behaviour Aosabi does not port, with its reason.
#[derive(Debug, Clone, Copy)]
pub struct NotPorted {
    /// What exists in the reference.
    pub what: &'static str,
    /// Why Aosabi does not carry it.
    pub reason: &'static str,
}

/// Everything that exists in the reference and is deliberately absent here.
pub const NOT_PORTED: &[NotPorted] = &[
    NotPorted {
        what: "the class implements map that is declared and never written",
        reason: "the whole branch that consumes it is unreachable",
    },
    NotPorted {
        what: "the boolean predicate that is exported and imported nowhere",
        reason: "dead on the reference side",
    },
    NotPorted {
        what: "the dry run flag",
        reason: "never passed as true anywhere in the reference",
    },
    NotPorted {
        what: "the configuration file the build never writes but a code path still tries to read",
        reason: "dead on the reference side",
    },
    NotPorted {
        what: "the analytics client and its gate",
        reason: "Aosabi has no telemetry of any kind, on any command and on any target",
    },
    NotPorted {
        what: "the benchmarking command the reference ships",
        reason: "outside the API contract",
    },
    NotPorted {
        what: "the plugin instrumentation member",
        reason: "an observability wrapper around the phases that changes no byte on the wire",
    },
    NotPorted {
        what: "the GraphQL Voyager page served on /viewer",
        reason: "aoviewer owns that route and Aosabi carries no Voyager text, see docs/decisions/ADR-0009-aoviewer-owns-viewer.md",
    },
    NotPorted {
        what: "the development hot swap seam that rebuilds the schema and the Yoga server in a running process, vendor/pylon/packages/pylon/src/app/pylon-handler.ts:353-361",
        reason: "the development loop of Aosabi restarts the served binary, so no running process swaps its schema",
    },
    NotPorted {
        what: "the Yoga options logging, context, fetchAPI, allowedHeaders, extraParamNames, renderGraphiQL, parserAndValidationCache given as an object, and disposeOnProcessTerminate",
        reason: "no fixture and no port sets them, and a key a configuration never sets changes no byte on the wire",
    },
    NotPorted {
        what: "the error of a text that failed to parse, which useParserAndValidationCache of graphql-yoga 5.18.0 keeps and throws from inside the loop of the onParse hooks of the next request with that text, so the later onParse hooks and the parse done hooks of every app plugin before the cache are skipped for a repeated failing text, vendor/pylon/node_modules/.pnpm/graphql-yoga@5.18.0_graphql@16.12.0/node_modules/graphql-yoga/cjs/plugins/use-parser-and-validation-cache.js:15-18 with vendor/pylon/node_modules/.pnpm/@envelop+core@5.5.1/node_modules/@envelop/core/cjs/orchestrator.js:98-136",
        reason: "the request cache of graphql-over-http keeps no failure and parses a failing text again, as the module documentation of crates/graphql-over-http/src/plugins/use_parser_and_validation_cache.rs states, so every request of a repeated failing text runs the parse done hooks of the app plugins again, where the reference runs them for the first request until its cache of 1,024 texts and one hour lets the error go, which no answer of a fixture shows, since no fixture and no plugin of Pylon has a parse done hook, and the entry drops once the project decides whether a failed parse is kept as Yoga keeps it",
    },
    NotPorted {
        what: "the signed cookie helpers getSignedCookie and setSignedCookie the core re-exports from hono, vendor/pylon/packages/pylon/src/core/index.ts:27",
        reason: "they need a keyed MAC the facade does not link by default, so they wait for the first port that reads a signed cookie, while getCookie, setCookie and deleteCookie are ported",
    },
    NotPorted {
        what: "the events clientError, checkContinue, checkExpectation, connect and upgrade of the HTTP server of Node",
        reason: "Pylon registers no listener for any of them, vendor/pylon/node_modules/.pnpm/@hono+node-server@1.19.9_hono@4.11.9/node_modules/@hono/node-server/dist/server.js:584-593, so each keeps the default answer of Node",
    },
    NotPorted {
        what: "the coding gzip of compress(), the first entry of its list of codings, vendor/pylon/node_modules/.pnpm/hono@4.11.9/node_modules/hono/dist/middleware/compress/index.js:3, which Pylon answers wherever Accept-Encoding holds that name",
        reason: "Aosabi codes no gzip by the owner's word of 2026-09-27, so compress() takes the list deflate alone and answers deflate or identity where Pylon answers gzip, with the body equal after decoding, the divergence D43 of docs/byte-equality.md and docs/decisions/ADR-0027-graphtp.md, section Amendment of 2026-09-27, no gzip",
    },
    NotPorted {
        what: "the option eventTarget of createPubSub, vendor/pylon/node_modules/.pnpm/@graphql-yoga+subscription@5.0.5/node_modules/@graphql-yoga/subscription/cjs/create-pub-sub.js:10",
        reason: "experimental_create_pub_sub takes no option, the fixtures call experimentalCreatePubSub without one, and the event target of the port is the hub of the kernel's publish primitive, which every protocol of a build reads",
    },
    NotPorted {
        what: "the operators filter, map and pipe of @graphql-yoga/subscription, vendor/pylon/node_modules/.pnpm/@graphql-yoga+subscription@5.0.5/node_modules/@graphql-yoga/subscription/cjs/index.js",
        reason: "Pylon re-exports createPubSub alone, as experimentalCreatePubSub, and graphql-yoga re-exports the operators, so a later port may lift them",
    },
    NotPorted {
        what: "the three buffers FixedBuffer, SlidingBuffer and DroppingBuffer of @repeaterjs/repeater 3.0.6, vendor/pylon/node_modules/.pnpm/@repeaterjs+repeater@3.0.6/node_modules/@repeaterjs/repeater/cjs/repeater.js:123-235",
        reason: "@graphql-yoga/subscription re-exports the class and none of its buffers, and no Pylon construct passes one, so the buffer of every repeater is undefined",
    },
    NotPorted {
        what: "the four statics race, merge, zip and latest of @repeaterjs/repeater 3.0.6, vendor/pylon/node_modules/.pnpm/@repeaterjs+repeater@3.0.6/node_modules/@repeaterjs/repeater/cjs/repeater.js:542-916",
        reason: "no Pylon construct reaches them, and a later port may lift them with the operators",
    },
    NotPorted {
        what: "the members [Symbol.asyncDispose] of finalAsyncIterator and errorAsyncIterator of @envelop/core 5.5.1, vendor/pylon/node_modules/.pnpm/@envelop+core@5.5.1/node_modules/@envelop/core/cjs/utils.js:124 and :164",
        reason: "nothing Yoga runs calls them, so they change no byte on the wire",
    },
    NotPorted {
        what: "the hook onSubscribeError of Yoga that rethrows an AbortError past the mask of the errors, vendor/pylon/node_modules/.pnpm/graphql-yoga@5.18.0_graphql@16.12.0/node_modules/graphql-yoga/cjs/server.js:187-197",
        reason: "Pylon passes no abort signal, so no error of a subscription is an abort error, and the function of graphql-over-http holds no step for the hook",
    },
    NotPorted {
        what: "the end of the node process on the 1025th push a subscriber that stops pulling leaves pending, which the listener of createPubSub throws inside dispatchEvent, vendor/pylon/node_modules/.pnpm/@graphql-yoga+subscription@5.0.5/node_modules/@graphql-yoga/subscription/cjs/create-pub-sub.js:27-29",
        reason: "the backlog of a subscriber waits in the hub of the kernel, in order up to the depth of its carrier and coalesced past it, and the stream stays up, the one deliberate divergence of the subscriptions, which capture row C60 records from the reference",
    },
    NotPorted {
        what: "the performance spans of useSentry, its transactions, the span of each execution with its attributes and its traceparent data, vendor/pylon/packages/pylon/src/plugins/use-sentry.ts:185-268",
        reason: "the reference sends no span without tracing, which Pylon never turns on, and no fixture measures one, so the span of the port records nothing",
    },
    NotPorted {
        what: "the options of Toucan beyond dsn, environment, release and sampleRate, its integrations beyond the request data among them, vendor/pylon/node_modules/.pnpm/toucan-js@4.1.1/node_modules/toucan-js/dist/index.cjs.js:768-800",
        reason: "none of them changes a byte of the answers of a service, and an option the port does not carry fails to compile at the token",
    },
    NotPorted {
        what: "TLS 1.2 in the outbound client of the network capability",
        reason: "the client speaks TLS 1.3 alone until a port meets a server without it",
    },
    NotPorted {
        what: "the lookup of the host of an outbound call for every connection the fetch of undici 7.22.0 opens, through the host of tls.connect and net.connect of Node, vendor/pylon/node_modules/.pnpm/undici@7.22.0/node_modules/undici/lib/core/connect.js:70-79 and :92-98, so the next connection reads a changed address or a resolver that fails",
        reason: "aosabi-fetch resolves a name by blocking its one runtime thread and keeps the last good answer of a name for 60 seconds and never a failure, so within those seconds a call answers from the kept address where the next lookup of the reference fails or reads another address, and after them a lookup that fails fails the call as the reference's does, and the entry drops once the project decides whether the client resolves for every connection or keeps an answer",
    },
    NotPorted {
        what: "the wait of the token request, the introspection and the user info request that the two halves of zitadel.ts send through openid-client 6.8.2 with no signal, node_modules/.pnpm/openid-client@6.8.2/node_modules/openid-client/build/index.js:1180-1182, so the fetch of Node waits for the 300 seconds undici gives the head and each pause of the body, vendor/pylon/node_modules/.pnpm/undici@7.22.0/node_modules/undici/lib/dispatcher/client.js:253-254, the same code in the undici 7.18.2 that node v24.13.0 bundles for its fetch, while the discovery carries the signal of 30 seconds of openid-client at build/index.js:264-265",
        reason: "aosabi-fetch ends every call after the 30 seconds of TIMEOUT_MS, so an issuer that answers one of the three requests after 30 seconds and before 300 lets the reference log the user in or answer the principal where the twin answers 500 with fetch failed, and the entry drops once the project settles the bounds of the fetch",
    },
    NotPorted {
        what: "the end of the process with the status 1 after a failed send of the Sentry middleware, whose promise waitUntil awaits rejects with nothing to catch it, vendor/pylon/node_modules/.pnpm/@hono+sentry@1.2.2_hono@4.11.9/node_modules/@hono/sentry/dist/index.js:3-11",
        reason: "a server keeps serving after a failed send of its telemetry, which no byte of an answer shows",
    },
    NotPorted {
        what: "the default integrations of Sentry.init of @sentry/node but the console integration, the request isolation of the Http integration and RequestData, vendor/pylon/node_modules/.pnpm/@sentry+node@8.55.0/node_modules/@sentry/node/build/cjs/sdk/index.js:32-53",
        reason: "they write the fields of the SDK, the contexts of the machine, the frames and the breadcrumbs of child processes and threads, where Aosabi writes fields of its own",
    },
    NotPorted {
        what: "the transport of @sentry/node 8.55.0 through which the GraphQL half of the Sentry plugin sends its events once Sentry.init ran, httpModule.request of vendor/pylon/node_modules/.pnpm/@sentry+node@8.55.0/node_modules/@sentry/node/build/cjs/transports/http.js:117, reaching a DSN whose port is one of the bad ports of undici 7.22.0, vendor/pylon/node_modules/.pnpm/undici@7.22.0/node_modules/undici/lib/web/fetch/constants.js:14-21",
        reason: "aosabi-fetch refuses such a port for every caller of the network capability as the fetch of undici refuses it, so the twin refuses those events where the http module of node sends them, while the events of the middleware, which toucan-js sends through that fetch, are refused on both sides, and the entry drops once the project decides whether the transport of the plugin reaches such a port",
    },
    NotPorted {
        what: "the JWT secured authorization response mode JARM of openid-client 6.8.2, useJwtResponseMode, node_modules/.pnpm/openid-client@6.8.2/node_modules/openid-client/build/index.js:793-800",
        reason: "zitadelLogin configures no response mode, so its callback reads a plain query",
    },
    NotPorted {
        what: "the hybrid response type code id_token of openid-client 6.8.2, useCodeIdTokenResponseType, node_modules/.pnpm/openid-client@6.8.2/node_modules/openid-client/build/index.js:873-880",
        reason: "zitadelLogin asks for the response type code alone",
    },
    NotPorted {
        what: "the implicit flow of openid-client 6.8.2, implicitAuthentication and useIdTokenResponseType, node_modules/.pnpm/openid-client@6.8.2/node_modules/openid-client/build/index.js:807-872 and :881-888",
        reason: "zitadelLogin runs the code grant with PKCE alone",
    },
    NotPorted {
        what: "the DPoP sender constraint of openid-client 6.8.2 and oauth4webapi 3.8.4, getDPoPHandle and the option DPoP of every request, node_modules/.pnpm/openid-client@6.8.2/node_modules/openid-client/build/index.js:566-569",
        reason: "Pylon passes no DPoP handle, so no request of the two halves carries a DPoP proof",
    },
    NotPorted {
        what: "the decryption of JWE answers of openid-client 6.8.2, enableDecryptingResponses, node_modules/.pnpm/openid-client@6.8.2/node_modules/openid-client/build/index.js:330-399",
        reason: "Pylon configures no decryption key, so an encrypted answer fails as the reference fails it on a configuration without one",
    },
    NotPorted {
        what: "the signature of a client assertion with an RSA key of 2048 bits or more that ring 0.17.14 does not sign, a key of two primes whose modulus is not 2048, 3072 or 4096 bits long, a key of more than two primes and a key whose public exponent lies outside 65537 to 2^33 - 1, which crypto.subtle.sign of Node 24.13.0 signs for the key the importPrivateKey of vendor/pylon/packages/pylon/src/auth/zitadel.ts:74-87 imports",
        reason: "the signer of the port is ring 0.17.14, which signs two primes of 2048, 3072 and 4096 bits alone, so the import keeps such a key and the signature refuses it with the UnsupportedOperationError that oauth4webapi gives a key under 2048 bits, and the entry drops with a release of ring that signs those keys",
    },
    NotPorted {
        what: "the command eval, the harness that runs an agent on tasks with and without the MCP server, vendor/pylon/packages/pylon/src/cli/index.ts:306-329 and vendor/pylon/packages/pylon/src/cli/eval/",
        reason: "a harness of the reference's own development that changes no byte of a service, and its runner drives a hosted model, which no command of Aosabi calls",
    },
    NotPorted {
        what: "the flag --standalone of build, which traces the runtime file graph of Node into .pylon/standalone/, vendor/pylon/packages/pylon/src/cli/index.ts:144-147 and vendor/pylon/packages/pylon/src/cli/builder/standalone.ts",
        reason: "a Rust service is one binary that links its application, so there is no module graph to trace, and --include copies the runtime data next to the binary into dist/ and into the image of the docker card",
    },
    NotPorted {
        what: "the flags --inspect and --inspect-brk of dev, which open the inspector of Node on the development process, vendor/pylon/packages/pylon/src/cli/index.ts:971-975 and vendor/pylon/packages/pylon/src/cli/dev/inspector.ts",
        reason: "the inspector protocol of V8 has no counterpart in a Rust binary, which a native debugger attaches to",
    },
    NotPorted {
        what: "the analytics client of the command line and its read of .pylon/config.js, vendor/pylon/packages/pylon/src/cli/analytics.ts",
        reason: "Aosabi has no telemetry of any kind, and a Rust service has no .pylon/config.js, because its configuration is code compiled into its binary",
    },
    NotPorted {
        what: "the scheme https of the fetch through which generatePylonTypes of pylon pull posts the introspection query, vendor/pylon/packages/pylon/src/cli/pull/index.ts:39-45, so the pull of the reference reads a remote over TLS",
        reason: "aosabi pull posts through the http:// only client of aosabi-cli, which refuses an https:// URL with AO-C0501, and the entry drops when pull moves onto NativeFetch of aosabi-fetch, which speaks TLS 1.3",
    },
    NotPorted {
        what: "the redirects of the status 307 and 308 that the fetch of pylon pull follows with the post of its body, up to 20 of them, vendor/pylon/node_modules/.pnpm/undici@7.22.0/node_modules/undici/lib/web/fetch/request.js:908 and lib/web/fetch/index.js:1244, so the reference pulls a remote behind such a redirect",
        reason: "the http:// only client of aosabi-cli follows no redirect and hands pull the answer of the redirect, whose body is no JSON, so aosabi pull fails where the reference pulls, as NativeFetch of aosabi-fetch, which follows no redirect either, would, and the entry drops once pull follows a redirect as the fetch of Node does",
    },
    NotPorted {
        what: "the reading of the answer of the introspection by its member data alone, vendor/pylon/packages/pylon/src/cli/pull/index.ts:46-47, so the reference builds the schema of a usable data whatever the member errors beside it holds, null, an empty list or errors",
        reason: "aosabi pull refuses an answer that carries the member errors, so an answer with a usable data and errors beside it pulls on the reference and fails on aosabi, while an answer without a usable data fails on both sides, in buildClientSchema on the reference, and the entry drops on the owner's word that pull reads data alone as the reference reads it",
    },
    NotPorted {
        what: "the bounds the fetch of Node puts on the post of pylon pull, 10 seconds to connect, 300 seconds for the head and for each pause of the body, a head of at most the 16,384 bytes of http.maxHeaderSize and no bound on the body, vendor/pylon/node_modules/.pnpm/undici@7.22.0/node_modules/undici/lib/core/connect.js:53 and lib/dispatcher/client.js:64-70 and :253-254, the same code in the undici 7.18.2 that node v24.13.0 bundles for its fetch",
        reason: "the http:// only client of aosabi-cli bounds the connection at 5 seconds, the whole answer at 30 seconds, its head at 64 KiB and its body at 16 MiB, so a slow or a large answer the reference reads fails on aosabi and a head between 16 KiB and 64 KiB the reference refuses passes, and the entry drops once the client of pull bounds a call as undici does",
    },
    NotPorted {
        what: "the content codings of the fetch of pylon pull, which asks for gzip and deflate in accept-encoding and decodes an answer coded with gzip, deflate, br or zstd, vendor/pylon/node_modules/.pnpm/undici@7.22.0/node_modules/undici/lib/web/fetch/index.js:1514-1518 and :2211-2234, the same code in the undici 7.18.2 that node v24.13.0 bundles for its fetch",
        reason: "the http:// only client of aosabi-cli sends no accept-encoding and refuses an answer with a coding other than identity, so a remote that codes its answer all the same pulls on the reference and fails on aosabi, and the entry drops on the owner's word that the client of pull asks for and decodes the codings of the fetch of Node",
    },
    NotPorted {
        what: "the prompts of create-pylon and its flags -i, --no-install, -y and -pm, vendor/pylon/packages/create-pylon/src/index.ts:27-33 and :44-46, and its prompts at :66, :86, :112, :137, :156 and :181",
        reason: "no command of Aosabi prompts, and cargo resolves and fetches the dependencies of a service on its first build, so new has no install step to confirm and no package manager to choose",
    },
    NotPorted {
        what: "the runtimes bun and deno of create-pylon, vendor/pylon/packages/create-pylon/src/create-directory/index.ts:5-29",
        reason: "a service of Aosabi is a Rust binary, so new offers native in the place of node and workers in the place of cf-workers alone",
    },
    NotPorted {
        what: "the operation compiler, the argument parser and the persisted document id of the query package, vendor/pylon/packages/pylon/src/query/build/compile.ts, parse-args.ts and persisted.ts",
        reason: "the bundler of a frontend runs them over the documents of that frontend, which keeps importing @getcronit/pylon/query, and the client step of the build calls none of them",
    },
    NotPorted {
        what: "the runtime and the React bindings of the query package, vendor/pylon/packages/pylon/src/query/runtime/ and vendor/pylon/packages/pylon/src/query/react/",
        reason: "they run in the browser, and Aosabi ships no client, see docs/decisions/ADR-0016-server-only.md",
    },
    NotPorted {
        what: "the four configuration file names pylon.config.ts, pylon.config.mts, pylon.config.js and pylon.config.mjs the build looks for, vendor/pylon/packages/pylon/src/cli/builder/bundler/build-config.ts:4-9",
        reason: "the configuration of a service is the one Rust file aosabi.config.rs, which its server binary includes as a module",
    },
    NotPorted {
        what: "the hot server of dev over Vite, which runs src/index.ts through the backend runner of Vite, swaps the app in place and serves the pages with Fast Refresh, vendor/pylon/packages/pylon/src/cli/dev/dev-server.ts and vite-hot-server.ts",
        reason: "the development loop of Aosabi rebuilds and restarts the served binary, and Aosabi renders no page, since usePages of the reference is server side rendering, which is no goal of Aosabi",
    },
    NotPorted {
        what: "the tree kill of the worker process of dev, vendor/pylon/packages/pylon/src/cli/tree-kill.ts and its call at vendor/pylon/packages/pylon/src/cli/index.ts:1109",
        reason: "the served child of the development loop shares the process group of the supervisor and starts no process of its own, so ending the child ends the tree",
    },
    NotPorted {
        what: "the glue files .pylon/schema.mjs, .pylon/server.mjs and .pylon/worker.mjs the build writes, vendor/pylon/packages/pylon/src/cli/builder/bundler/emit-server-glue.ts",
        reason: "a Rust server links its application and imports nothing at run time, so the server package the command line generates takes their place",
    },
    NotPorted {
        what: "the refusals of a malformed delegate key, Invalid delegate key format and Unsupported operation type, vendor/pylon/packages/pylon/src/core/gateway.ts:884-905",
        reason: "the key of a delegation is a constant the pulled registry generates, so a key without both halves or with a root other than Query, Mutation and Subscription cannot be written",
    },
    NotPorted {
        what: "the delegation of a subscription through the entry Subscription of the operation map of a federation, vendor/pylon/packages/pylon/src/core/gateway.ts:892-896",
        reason: "the registry pylon pull writes lists delegate keys for Query and Mutation alone, so no typed call delegates a subscription",
    },
    NotPorted {
        what: "the GET, persisted query, retry, event stream and incremental multipart branches of buildHTTPExecutor of @graphql-tools/executor-http 3.1.0, vendor/pylon/node_modules/.pnpm/@graphql-tools+executor-http@3.1.0_@types+node@20.19.33_graphql@16.12.0/node_modules/@graphql-tools/executor-http/dist/index.cjs:559-563, :637, :714-726 and :878-922",
        reason: "Pylon's federation builds the executor with endpoint and headers alone, so it sends a POST with no persisted query and no retry, and a Pylon remote answers a query and a mutation with one JSON body, so no fixture and no port reaches a branch of them",
    },
    NotPorted {
        what: "the batching and the stitching of @graphql-tools/delegate 12.0.9, the batch executor and the merged types of a subschema configuration",
        reason: "Pylon's federation hands delegateToSchema a plain schema, so the delegation finds no subschema configuration and executes over the wrapped schema, and the proxying resolvers carry no stitching information",
    },
    NotPorted {
        what: "the one executor and headers function every federation of a URL sends through, the first that introspected it, since schemaCache of getRemoteSchema is keyed by the URL alone and keeps the executor that the federation which missed the cache, the first that introspected the URL, built from its own headers, vendor/pylon/packages/pylon/src/core/gateway.ts:126-164 and :907-914",
        reason: "a second federation of a cached URL sends with its own executor and headers and never with the credentials of the first, so the cache of remote schemas keys a slot by the URL and the headers function, and a federation with another headers function introspects the URL once more",
    },
    NotPorted {
        what: "the list of the inline fragments of the members that @graphql-tools/delegate 12.0.9 stores in place of a field of a union whose need selects member fields, and the TypeError Cannot read properties of undefined (reading 'value') it answers at the path of that field with no data request, dist/index.cjs:926-1006 and :1378",
        reason: "the port splices the inline fragments under the field and answers the data, since a selection set of aosabi-lang holds no list",
    },
];

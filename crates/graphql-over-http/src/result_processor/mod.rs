// SPDX-License-Identifier: MIT
/*
 * graphql-over-http: the result processors and the choice of one.
 *
 * Each file of the directory names the file of Yoga it translates.
 *
 * Derived from graphql-yoga 5.18.0, cjs/plugins/use-result-processor.js, MIT.
 */

//! The result processors.
//!
//! [`Processors::choose`] tries the processors in the order stream, regular,
//! and within each the media types the request accepts in the order of the
//! negotiation, so the last match wins. Every media type a processor offers
//! is appended to the acceptable list once per media type of the request,
//! which is the list a 406 answers in its `accept` header. The choice runs
//! before the hooks `onResultProcess` of the app, which may replace it. A
//! stream skips every processor that takes no async iterable, so a
//! subscription that answers a stream is written by the processor of an event
//! stream or answered with 406, and one that answers a single result, an
//! early error, may be written as JSON.
//!
//! Yoga offers the processor of `multipart/mixed` between the two, to every
//! request but the request of a subscription, which it marks in the hook
//! `onSubscribe`. The function holds no such processor, the named divergence
//! D59 of `docs/byte-equality.md` section 3, so every request is offered the
//! list of a subscription, a client that accepts `multipart/mixed` alone gets
//! the 406 of a type the function does not serve, and the `accept` header of
//! every 406 lists no `multipart/mixed`.

use std::{
    borrow::Cow,
    rc::Rc, //
};

use aosabi_http::Request;

use crate::plugin::{
    HttpResult,
    ResultProcessor, //
};

pub mod accept;
pub mod regular;
pub mod sse;
pub mod stringify;

/// A processor with the media types it answers.
struct ProcessorConfig {
    /// The media types, `mediaTypes`.
    media_types: &'static [&'static str],
    /// Whether it answers a stream, `asyncIterables`.
    async_iterables: bool,
    /// The processor, `processResult`.
    process_result: ResultProcessor,
}

/// The result processors of the function.
pub struct Processors {
    /// The processor of an event stream.
    sse: ProcessorConfig,
    /// The processor of a JSON answer.
    regular: ProcessorConfig,
}

impl Processors {
    /// Returns the processors of an event stream and of JSON, `useResultProcessors()` with no multipart answer.
    #[must_use]
    pub fn new() -> Self {
        Self {
            sse: ProcessorConfig {
                media_types: &["text/event-stream"],
                async_iterables: true,
                process_result: Rc::new(sse::process_sse_result),
            },
            regular: ProcessorConfig {
                media_types: &["application/graphql-response+json", "application/json"],
                async_iterables: false,
                process_result: Rc::new(regular::process_regular_result),
            },
        }
    }

    /// Chooses the processor of `result` for `request`, the hook `onResultProcess` of `useResultProcessors()`.
    ///
    /// Every media type a processor offers is appended to `acceptable`, and
    /// the last match is written to `chosen` with its media type.
    pub fn choose(
        &self,
        request: &Request,
        result: &HttpResult,
        acceptable: &mut Vec<Cow<'static, str>>,
        chosen: &mut Option<(ResultProcessor, Cow<'static, str>)>,
    ) {
        let configs = [&self.sse, &self.regular];
        let is_async_iterable_result = match result {
            HttpResult::Single(outcome) => outcome.single().is_none(),
            HttpResult::Batch(_) => false,
        };
        accept::with_media_types_in_order(request, |request_media_types| {
            for config in configs {
                for request_media_type in request_media_types.clone() {
                    if is_async_iterable_result && !config.async_iterables {
                        continue;
                    }
                    for processor_media_type in config.media_types {
                        acceptable.push(Cow::Borrowed(*processor_media_type));
                        if accept::is_matching_media_type(processor_media_type, request_media_type)
                        {
                            *chosen = Some((
                                Rc::clone(&config.process_result),
                                Cow::Borrowed(*processor_media_type),
                            ));
                        }
                    }
                }
            }
        });
    }
}

impl Default for Processors {
    fn default() -> Self {
        Self::new()
    }
}

// SPDX-License-Identifier: MIT
/*
 * aosabi-serve: the answer of the Node adapter as the neutral answer of
 * aosabi-http.
 *
 * Derived from hono 1.19.9, dist/listener.js, MIT.
 *
 * It carries the quirks Q49 to Q52, Q58 and Q59 of docs/byte-equality.md
 * section 13.1, the adapter's halves of them.
 */

//! The answer of the Node adapter.
//!
//! The listener writes an answer on one of two paths (A2 to A4 of the plan's
//! version 2 draft). A response whose adapter cache holds takes the cached
//! path: its headers become the record through the Fetch iteration when they
//! are a `Headers` and as given otherwise, a string, bytes or a blob append
//! `Content-Length` with capitals, and a stream writes the head at once, quirk
//! `Q52_NODE_ADAPTER_CACHED_LENGTH`. Every other response takes the object
//! path: the record through the Fetch iteration with the default
//! `content-type` appended, quirk `Q49_NODE_ADAPTER_RECORD`, and the body read
//! without blocking, a length appended in lower case when it ended within
//! those reads, quirk `Q50_NODE_ADAPTER_FIRST_READS`, and a null body written
//! with `end()`, quirk `Q51_NODE_ADAPTER_NULL_BODY`. The path fixes the first
//! write, which fixes the bytes of a value, quirk
//! `Q58_NODE_ENCODING_FIRST_WRITE`, and Node's check of the record answers
//! the 500 of `handleResponseError` where it refuses a line, quirk
//! `Q59_NODE_RECORD_CHECK_500`.

use core::{
    future::Future,
    pin::Pin,
    task::{
        Context,
        Poll,
        Waker, //
    }, //
};
use std::{
    collections::VecDeque,
    rc::Rc, //
};

use aosabi_http::{
    context::Env,
    fetch::{
        body::{
            Body,
            BodyStream,
            LocalBoxFuture, //
        },
        headers::Headers,
        request::Request,
        response::{
            CachedHeaders,
            Response, //
        }, //
    },
    message::{
        ChunkError,
        ChunkSource,
        FirstWrite,
        Line,
        NodeBody,
        NodeIncoming,
        NodeOutgoing, //
    },
    node::{
        append_cached_length,
        append_read_length,
        build_outgoing_http_headers,
        check_record, //
    },
    web_app::RouteError, //
};

use crate::node_adapter::{
    handle_error::{
        handle_fetch_error,
        handle_request_error,
        handle_response_error, //
    },
    request::new_request, //
};

/// The pair `{incoming, outgoing}` the adapter hands `fetch` as its environment.
///
/// It holds no variable, so a resolver that reads the environment of the
/// adapter pair finds none.
#[derive(Clone, Copy, Debug, Default)]
pub struct NodeBindings;

/// Answers `incoming` through `fetch`, the request listener of the adapter.
///
/// The request is built, `fetch` answers it with the adapter pair as its
/// environment, a request that cannot be built is answered with 400 and a
/// fetch that rejects with 500, and the answer is written on the cached path
/// or the object path. A request whose method is neither `GET` nor `HEAD`
/// marks the answer for the cleanup of its unread body, the adapter's
/// `autoCleanupIncoming`, which the app never turns off (A10 of the plan's
/// version 2 draft). The future answers a request that cannot be built at its
/// first poll, as the listener writes that answer before its first `await`,
/// and waits once before the answer of `fetch`, the `await` of the promise of
/// the app, so a connection that polls it once where Node runs the listener
/// writes only the first inside the read.
pub fn serve<F, Fut, E>(incoming: NodeIncoming, fetch: F) -> LocalBoxFuture<'static, NodeOutgoing>
where
    F: FnOnce(Rc<Request>, Env) -> Fut + 'static,
    Fut: Future<Output = Result<Response, E>> + 'static,
    E: RouteError,
{
    let cleanup = !matches!(incoming.method, "GET" | "HEAD");
    Box::pin(async move {
        let (res, cleanup_incoming) = match new_request(incoming) {
            Err(_) => (handle_request_error(), false),
            Ok(request) => {
                let fetched = fetch(request, Env::Bindings(Rc::new(NodeBindings)));
                // A TypeScript app answers with a promise, since its first
                // middleware `compress()` is async, and the listener awaits
                // it at `listener.js:433` before it writes the answer, so no
                // answer of the app goes out where the listener runs.
                yield_now().await;
                match fetched.await {
                    Ok(res) => (res, cleanup),
                    Err(error) => (handle_fetch_error(&error.name()), cleanup),
                }
            }
        };
        let mut answer = outgoing(res).await;
        answer.cleanup_incoming = cleanup_incoming;
        answer
    })
}

/// Returns the answer the listener writes for `res`.
pub async fn outgoing(res: Response) -> NodeOutgoing {
    if let Some((status, body, headers)) = res.node_cache() {
        return via_cache(status, body, headers);
    }
    via_object(res).await
}

/// Returns the answer of the cached path.
fn via_cache(status: u16, body: Body, headers: CachedHeaders) -> NodeOutgoing {
    let mut record = match headers {
        CachedHeaders::Fetch(headers) => record_of(&headers),
        CachedHeaders::Plain(entries) => plain_record(&entries),
    };
    let (body, first_write) = match body {
        Body::Text(text) => {
            append_cached_length(&mut record, text.len() as u64);
            if text.is_empty() {
                (NodeBody::Empty, FirstWrite::End)
            } else {
                (NodeBody::Full(text.as_bytes().to_vec()), FirstWrite::Text)
            }
        }
        Body::Full(bytes) => {
            append_cached_length(&mut record, bytes.len() as u64);
            (NodeBody::Full(bytes.to_vec()), FirstWrite::Bytes)
        }
        Body::Stream(stream) => (
            NodeBody::Stream(Box::new(Source::new(Vec::new(), Some(stream)))),
            FirstWrite::Flush,
        ),
        Body::Empty | Body::Html(_) => (NodeBody::Empty, FirstWrite::End),
    };
    answer(status, record, body, first_write)
}

/// Returns the answer of the object path.
///
/// The body is polled without a waker, as `readWithoutBlocking` races a read
/// against two ticks: a first poll that is not ready ends the polls with no
/// chunk, a first poll that gave a chunk is followed by a second, and only a
/// second poll that is not ready is followed by one turn of the executor and a
/// third poll. A poll that fails ends the polls as the end does, since the
/// adapter logs the error and sets `done`. A body that ended within the polls
/// gets `content-length` over the chunks polled when the record has none,
/// quirk `Q50_NODE_ADAPTER_FIRST_READS`, unless the record names
/// `transfer-encoding: chunked`, which skips the polls. A body that ended in
/// one chunk is that chunk, so the bytes of an answer reach the connection
/// with no copy here.
async fn via_object(res: Response) -> NodeOutgoing {
    let status = res.status();
    let mut record = record_of(&res.headers());
    let body = res.body();
    let Some(stream) = body.to_stream() else {
        return answer(status, record, NodeBody::Empty, FirstWrite::End);
    };
    let mut values: Vec<Vec<u8>> = Vec::new();
    let mut done = false;
    let chunked = record
        .iter()
        .any(|line| line.name == "transfer-encoding" && line.value == b"chunked");
    if !chunked {
        let mut max_read_count = 2;
        let mut failed: Option<String> = None;
        let mut i = 0;
        while i < max_read_count {
            let polled = if let Some(error) = &failed {
                Poll::Ready(Some(Err(error.clone())))
            } else {
                let mut cx = Context::from_waker(Waker::noop());
                stream
                    .poll_chunk(&mut cx)
                    .map(|item| item.map(|chunk| chunk.map_err(|error| error.to_string())))
            };
            let chunk = match polled {
                Poll::Pending => None,
                Poll::Ready(Some(Err(error))) => {
                    eprintln!("{error}");
                    done = true;
                    failed = Some(error);
                    None
                }
                Poll::Ready(chunk) => Some(chunk),
            };
            match chunk {
                None if i == 1 => {
                    yield_now().await;
                    max_read_count = 3;
                    i += 1;
                    continue;
                }
                None => break,
                Some(Some(Ok(value))) => values.push(value),
                Some(_) => {
                    done = true;
                    break;
                }
            }
            i += 1;
        }
        if done {
            let length: usize = values.iter().map(Vec::len).sum();
            append_read_length(&mut record, length as u64);
        }
    }
    let first_write = match (values.is_empty(), done) {
        (false, _) => FirstWrite::Bytes,
        (true, true) => FirstWrite::End,
        (true, false) => FirstWrite::Flush,
    };
    let body = if done {
        match values.len() {
            0 => NodeBody::Empty,
            1 => NodeBody::Full(values.swap_remove(0)),
            _ => NodeBody::Full(values.concat()),
        }
    } else {
        NodeBody::Stream(Box::new(Source::new(values, Some(stream))))
    };
    answer(status, record, body, first_write)
}

/// Returns the answer of `status` with `record`, `body` and `first_write`, checked as Node's `writeHead` checks it.
///
/// A record Node refuses is answered as `handleResponseError` answers it,
/// quirk `Q59_NODE_RECORD_CHECK_500`.
fn answer(status: u16, record: Vec<Line>, body: NodeBody, first_write: FirstWrite) -> NodeOutgoing {
    if let Err(error) = check_record(&record) {
        return handle_response_error(&error.message(&record), status);
    }
    NodeOutgoing {
        status,
        reason: None,
        record,
        body,
        first_write,
        cleanup_incoming: false,
        destroy: false,
    }
}

/// Returns the record of `headers` through the Fetch iteration, quirk `Q49_NODE_ADAPTER_RECORD`.
fn record_of(headers: &Headers) -> Vec<Line> {
    let entries: Vec<(String, String)> = headers
        .entries()
        .into_iter()
        .filter(|(name, _)| name != "set-cookie")
        .collect();
    build_outgoing_http_headers(&entries, &headers.get_set_cookie())
}

/// Returns the record of a plain object as given, its keys in the order of `Object.keys`.
///
/// A key given twice keeps its first place and its last value, and the keys
/// that are array indices come first in ascending order, the rule of
/// `OrdinaryOwnPropertyKeys`.
fn plain_record(entries: &[(String, String)]) -> Vec<Line> {
    let mut record: Vec<Line> = Vec::with_capacity(entries.len());
    for (name, value) in entries {
        let value: Vec<u8> = value
            .chars()
            .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
            .collect();
        match record.iter_mut().find(|line| line.name == name.as_str()) {
            Some(line) => line.value = value,
            None => record.push(Line::new(name.clone(), value)),
        }
    }
    record.sort_by_key(|line| match array_index(&line.name) {
        Some(index) => (0, index),
        None => (1, 0),
    });
    record
}

/// Returns the array index `name` is, when it is one.
fn array_index(name: &str) -> Option<u32> {
    let canonical = !name.is_empty()
        && name.bytes().all(|byte| byte.is_ascii_digit())
        && (name == "0" || !name.starts_with('0'));
    if !canonical {
        return None;
    }
    name.parse::<u32>().ok().filter(|&index| index != u32::MAX)
}

/// The body of an answer written chunk by chunk, the chunks polled on the object path first.
struct Source {
    /// The chunks polled before the head.
    values: VecDeque<Vec<u8>>,
    /// The rest of the body.
    stream: Option<BodyStream>,
}

impl Source {
    /// Creates the source of `values` followed by `stream`.
    fn new(values: Vec<Vec<u8>>, stream: Option<BodyStream>) -> Self {
        Self {
            values: values.into(),
            stream,
        }
    }
}

impl ChunkSource for Source {
    fn poll_chunk(&mut self, cx: &mut Context<'_>) -> Poll<Option<Result<Vec<u8>, ChunkError>>> {
        if let Some(value) = self.values.pop_front() {
            return Poll::Ready(Some(Ok(value)));
        }
        let Some(stream) = &self.stream else {
            return Poll::Ready(None);
        };
        match stream.poll_chunk(cx) {
            Poll::Ready(Some(Ok(value))) => Poll::Ready(Some(Ok(value))),
            Poll::Ready(Some(Err(_))) => {
                self.stream = None;
                Poll::Ready(Some(Err(ChunkError)))
            }
            Poll::Ready(None) => {
                self.stream = None;
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn take_driver(&mut self) -> Option<Pin<Box<dyn Future<Output = ()>>>> {
        self.stream.as_ref().and_then(BodyStream::take_driver)
    }
}

/// Yields once to the executor, the macrotask `await new Promise(r => setTimeout(r))` waits.
fn yield_now() -> YieldNow {
    YieldNow(false)
}

/// The future of [`yield_now`].
struct YieldNow(bool);

impl Future for YieldNow {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.0 {
            return Poll::Ready(());
        }
        self.0 = true;
        cx.waker().wake_by_ref();
        Poll::Pending
    }
}

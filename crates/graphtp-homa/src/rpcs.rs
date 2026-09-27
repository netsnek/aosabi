// SPDX-License-Identifier: MIT
/*
 * graphtp-homa rpcs: the one adapter that calls Homa, the loops of a server
 * over homa::Rpcs and over homa::Endpoint and the loop of a client over
 * homa::Rpcs.
 *
 * No analogue in the reference. The calls of Homa are those the crate homa
 * hands GraphTP.
 */

//! The one adapter that calls Homa.
//!
//! A server runs `serve` over [`homa::Rpcs`], which [`homa::Endpoint`]
//! implements where the build has both roles of `homa`, and `serve_endpoint`
//! over the inherent calls of [`homa::Endpoint`] where the build has the role
//! `server` alone. A client runs `send` and `drive` over [`homa::Rpcs`]. A
//! build holds the loops of its roles alone, so this doc names the loops
//! without a link. Each loop takes one role per engine: a caller that runs a
//! server and a client over one engine dispatches its events itself with the
//! sans I/O types, and a caller that drives its engine itself leaves this
//! module unused.
//!
//! Each loop takes every event the engine has, hands a whole request or answer
//! to the sans I/O type, sends what the type yields, and returns. The engine's
//! own turn, its packets and its timers, stays with the caller.

#[cfg(feature = "client")]
use alloc::vec::Vec;
#[cfg(feature = "server")]
use core::hash::Hash;

#[cfg(feature = "server")]
use graphtp::{
    head::AnswerHead,
    status::Code, //
};
#[cfg(feature = "client")]
use homa::Gone;
#[cfg(feature = "server")]
use homa::{
    Endpoint,
    PeerPort,
    SendError,
    Store, //
    wire::Form,
};
#[cfg(any(feature = "client", feature = "server"))]
use homa::{
    Event,
    Read,
    RpcId,
    Rpcs, //
};

#[cfg(feature = "client")]
use crate::client::{
    Client,
    Incoming,
    Outgoing, //
};
#[cfg(feature = "server")]
use crate::{
    handler::Handler,
    server::{
        Message,
        Server,
        Step, //
    },
};

/// What one turn of a server loop did.
#[cfg(feature = "server")]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Served {
    /// The whole requests taken.
    pub requests: usize,
    /// The answers the engine queued.
    pub replies: usize,
    /// The answers the engine refused, whose RPC was gone or could take no
    /// answer and no refusal.
    pub refused: usize,
}

/// Takes every event of `rpcs` for the server `server`, hands each OPERATION
/// to `handler` and sends every answer the server yields.
///
/// A request is freed once the server holds what it needs of it, and a peer
/// that is gone ends in the server. An answer the engine refuses is sent again
/// as the refusal 500.1, since a message longer than the peer accepts is the
/// one refusal of an answer the server can name, and counted in
/// [`Served::refused`] when that fails as well.
///
/// # Errors
///
/// Returns the error of `rpcs` when it cannot read its events.
#[cfg(feature = "server")]
pub fn serve<R, H>(
    rpcs: &mut R,
    server: &mut Server<R::Peer>,
    handler: &mut H,
) -> Result<Served, R::Error>
where
    R: Rpcs,
    H: Handler<R::Peer>,
{
    let mut tally = Served::default();
    flush_rpcs(rpcs, server, &mut tally);
    while let Some(event) = rpcs.next_event()? {
        match event {
            Event::Request { peer, id } => {
                let Some(step) = rpcs
                    .message(id)
                    .map(|message| take(server, peer, id, &message))
                else {
                    continue;
                };
                tally.requests += 1;
                // The step owns the request's bytes, so its pages go back to
                // the engine before the handler runs, and a refusal of the
                // release changes nothing the server answers.
                let _ = rpcs.release(id);
                match step {
                    Step::Execute(exchange, operation) => handler.operation(
                        exchange,
                        operation.credential.as_deref(),
                        &operation.content,
                        server,
                    ),
                    Step::Reply(id, message) => {
                        flush_rpcs(rpcs, server, &mut tally);
                        reply_rpcs(rpcs, id, &message, &mut tally);
                    }
                    Step::Parked => {}
                }
            }
            Event::PeerGone { peer, .. } => server.link_down(peer),
            // A response belongs to a client, which a server loop does not
            // run, so its pages go back to the engine.
            Event::Response { id } => {
                let _ = rpcs.release(id);
            }
            _ => {}
        }
        flush_rpcs(rpcs, server, &mut tally);
    }
    Ok(tally)
}

/// Takes every event of `endpoint`, whose build may have the role `server`
/// alone, for the server `server`, hands each OPERATION to `handler` and sends
/// every answer the server yields.
///
/// The request stays in the engine's pages until its answer is queued, as the
/// inherent calls of [`Endpoint`] keep it. An answer longer than the peer
/// accepts is answered 500.1, one the pages cannot hold 503.1, and an answer
/// whose RPC is gone is counted in [`Served::refused`].
#[cfg(feature = "server")]
pub fn serve_endpoint<P, S, W, H>(
    endpoint: &mut Endpoint<P, S, W>,
    server: &mut Server<P>,
    handler: &mut H,
) -> Served
where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
    H: Handler<P>,
{
    let mut tally = Served::default();
    flush_endpoint(endpoint, server, &mut tally);
    while let Some(event) = endpoint.poll_event() {
        match event {
            Event::Request { peer, id } => {
                let Some(step) = endpoint
                    .request_of(id)
                    .map(|message| take(server, peer, id, &message))
                else {
                    continue;
                };
                tally.requests += 1;
                match step {
                    Step::Execute(exchange, operation) => handler.operation(
                        exchange,
                        operation.credential.as_deref(),
                        &operation.content,
                        server,
                    ),
                    Step::Reply(id, message) => {
                        flush_endpoint(endpoint, server, &mut tally);
                        reply_endpoint(endpoint, id, &message, &mut tally);
                    }
                    Step::Parked => {}
                }
            }
            Event::PeerGone { peer, .. } => server.link_down(peer),
            _ => {}
        }
        flush_endpoint(endpoint, server, &mut tally);
    }
    tally
}

/// Sends the request `outgoing` of `client` to `peer` over `rpcs` and hands
/// the RPC back to the client.
///
/// # Errors
///
/// Returns the error of `rpcs` when it cannot queue the request, and the
/// client then holds nothing of it.
#[cfg(feature = "client")]
pub fn send<R: Rpcs>(
    rpcs: &mut R,
    peer: R::Peer,
    client: &mut Client,
    outgoing: Outgoing,
) -> Result<RpcId, R::Error> {
    let id = rpcs.request(peer, &outgoing.message, outgoing.options)?;
    client.sent(id, outgoing);
    Ok(id)
}

/// Sends what `client` needs sent to `peer`, takes every event of `rpcs` for
/// it and appends what each answer and failure gives to `out`, with its RPC.
///
/// The CANCEL of handle 0 of an unknown count and the PULL kept parked while a
/// stream is open leave from here, and a request to repeat goes to `out` for
/// the caller to send with [`send`]. A link down of `peer` appends the streams
/// it ends as one [`Incoming::Stream`] with no RPC.
///
/// # Errors
///
/// Returns the error of `rpcs` when it cannot read its events or queue a
/// request the client needs.
#[cfg(feature = "client")]
pub fn drive<R: Rpcs>(
    rpcs: &mut R,
    peer: R::Peer,
    client: &mut Client,
    out: &mut Vec<(Option<RpcId>, Incoming)>,
) -> Result<(), R::Error> {
    loop {
        while let Some(outgoing) = client.poll() {
            send(rpcs, peer, client, outgoing)?;
        }
        let Some(event) = rpcs.next_event()? else {
            return Ok(());
        };
        match event {
            Event::Response { id } => {
                let Some(incoming) = rpcs.message(id).map(|message| read(client, id, &message))
                else {
                    continue;
                };
                // The client holds what it needs of the answer, so its pages
                // and the RPC go back to the engine.
                let _ = rpcs.release(id);
                out.push((Some(id), incoming));
            }
            Event::Failed { id, why } => out.push((Some(id), client.failure(id, why))),
            Event::PeerGone {
                peer: gone,
                why: Gone::LinkDown,
            } if gone == peer => {
                let lost = client.link_down();
                if !lost.is_empty() {
                    out.push((None, Incoming::Stream(lost)));
                }
            }
            _ => {}
        }
    }
}

/// Hands the whole request `message` to the server, from one page or copied
/// from several.
#[cfg(feature = "server")]
fn take<P: Copy + Eq, M: Read>(server: &mut Server<P>, peer: P, id: RpcId, message: &M) -> Step<P> {
    if let Some(bytes) = message.as_slice() {
        return server.request(peer, id, bytes);
    }
    server.request(peer, id, &whole(message))
}

/// Hands the whole answer `message` to the client, from one page or copied
/// from several.
#[cfg(feature = "client")]
fn read<M: Read>(client: &mut Client, id: RpcId, message: &M) -> Incoming {
    if let Some(bytes) = message.as_slice() {
        return client.answer(id, bytes);
    }
    client.answer(id, &whole(message))
}

/// Copies a message that spans several pages into one buffer.
#[cfg(any(feature = "client", feature = "server"))]
fn whole<M: Read>(message: &M) -> alloc::vec::Vec<u8> {
    let len = usize::try_from(message.len()).unwrap_or(0);
    let mut bytes = alloc::vec![0; len];
    let copied = message.read_at(0, &mut bytes);
    bytes.truncate(copied);
    bytes
}

/// Sends every answer the server yields over `rpcs`.
#[cfg(feature = "server")]
fn flush_rpcs<R: Rpcs>(rpcs: &mut R, server: &mut Server<R::Peer>, tally: &mut Served) {
    while let Some((id, message)) = server.poll() {
        reply_rpcs(rpcs, id, &message, tally);
    }
}

/// Sends one answer over `rpcs`, or the refusal 500.1 in its place.
#[cfg(feature = "server")]
fn reply_rpcs<R: Rpcs>(rpcs: &mut R, id: RpcId, message: &Message, tally: &mut Served) {
    if rpcs.reply(id, message).is_ok() || rpcs.reply(id, &refusal(Code::ANSWER_TOO_LARGE)).is_ok() {
        tally.replies += 1;
    } else {
        tally.refused += 1;
    }
}

/// Sends every answer the server yields over `endpoint`.
#[cfg(feature = "server")]
fn flush_endpoint<P, S, W>(
    endpoint: &mut Endpoint<P, S, W>,
    server: &mut Server<P>,
    tally: &mut Served,
) where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
{
    while let Some((id, message)) = server.poll() {
        reply_endpoint(endpoint, id, &message, tally);
    }
}

/// Sends one answer over `endpoint`, or the refusal its failure names.
#[cfg(feature = "server")]
fn reply_endpoint<P, S, W>(
    endpoint: &mut Endpoint<P, S, W>,
    id: RpcId,
    message: &Message,
    tally: &mut Served,
) where
    P: Copy + Eq + Hash + PeerPort,
    S: Store<P>,
    W: Form,
{
    let sent = match endpoint.reply(id, message) {
        Ok(()) => true,
        Err(SendError::TooLong) => endpoint.reply(id, &refusal(Code::ANSWER_TOO_LARGE)).is_ok(),
        Err(SendError::NoSlot) => endpoint
            .reply(id, &refusal(Code::INSUFFICIENT_MEMORY))
            .is_ok(),
        Err(_) => false,
    };
    if sent {
        tally.replies += 1;
    } else {
        tally.refused += 1;
    }
}

/// Returns the refusal `code`, its 2 bytes.
#[cfg(feature = "server")]
fn refusal(code: Code) -> [u8; 2] {
    AnswerHead { code }.to_bytes()
}

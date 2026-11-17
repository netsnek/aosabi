// SPDX-License-Identifier: MIT
/*
 * aosabi-core carrier: the policy of the carrier a request came over, kept on
 * its context.
 *
 * No analogue in the reference. The policy rides on the request context under
 * one key, so the allowlist of the kernel's publish module reaches the
 * subscription binding over every carrier, and the revisions of a hub fed
 * stream ride beside it.
 */

//! The carrier policy of a request.
//!
//! A carrier crate builds the [`CarrierPolicy`] of its card and sets it on
//! the context of every request it hands over, and the subscription binding
//! of the core reads it before a subscription runs, so a refusal never
//! reaches the executor. A request no carrier set a policy for runs under
//! [`CarrierPolicy::NETWORK`], which allows every field with the queue bound
//! of the reference repeater, so no byte of HTTP moves.
//!
//! The context also keeps the revisions the pump of a hub fed stream notes,
//! one per event it pushes. The revision of an event leaves the queue when
//! the execution of that event starts and stays with that execution, and the
//! stream of the service takes it with the result the execution yields. A
//! revision reaches no frame.

use alloc::collections::VecDeque;
use core::cell::{
    Cell,
    RefCell, //
};
use std::rc::Rc;

use aosabi_execute::promise::Agent;
use aosabi_http::Key;
use aosabi_kernel::publish::write_refusal;
pub use aosabi_kernel::publish::{
    Allow,
    CarrierId,
    CarrierPolicy,
    REFUSED_ON_CARRIER,
    Revision, //
};
use aosabi_lang::{
    GraphQLError,
    GraphQLErrorOptions, //
};

use crate::{
    context::Context,
    glue::Thrown, //
};

/// The key the policy is kept under in the context's variables.
pub const CARRIER_POLICY_KEY: &str = "aosabi.carrierPolicy";

/// The variable of the policy, [`CARRIER_POLICY_KEY`] typed.
const CARRIER_POLICY: Key<CarrierPolicy> = Key::new(CARRIER_POLICY_KEY);

/// The revisions of the events of the stream of a context.
#[derive(Default)]
struct Revisions {
    /// The revisions a pump noted whose execution has not started yet, oldest first.
    // An `unsafe` cell would skip the borrow flag.
    noted: RefCell<VecDeque<Revision>>,
    /// The revision of the event whose execution started last, until the stream takes it.
    started: Cell<Option<Revision>>,
}

/// Returns the revisions of `context`, made at the first use.
fn revisions(context: &Context) -> Rc<Revisions> {
    context.extension::<Revisions>().unwrap_or_else(|| {
        let revisions = Rc::new(Revisions::default());
        context.set_extension(Rc::clone(&revisions));
        revisions
    })
}

/// Sets the policy of the carrier the request of `context` came over.
pub fn set_carrier_policy(context: &Context, policy: CarrierPolicy) {
    context.set(CARRIER_POLICY, policy);
}

/// Returns the policy of the request, [`CarrierPolicy::NETWORK`] when none was set.
#[must_use]
pub fn carrier_policy(context: &Context) -> CarrierPolicy {
    context
        .get(CARRIER_POLICY)
        .unwrap_or(CarrierPolicy::NETWORK)
}

/// Returns the refusal of `field` on the carrier `carrier`.
///
/// The refusal is a thrown `GraphQLError` with the `extensions`
/// `{"code": "REFUSED_ON_CARRIER"}` and the message of [`write_refusal`],
/// `Subscription field "<field>" is refused on <carrier>.`, an original
/// error, so the masking of Yoga passes it as it is. It never reaches a
/// client: [`CarrierPolicy::NETWORK`] refuses no field over HTTP, and on
/// GraphTP(Homa) the listener answers 403.1 Stream Not Allowed with an empty
/// body and logs the message.
#[must_use]
pub fn refusal(field: &str, carrier: CarrierId) -> Thrown {
    let mut message = String::new();
    // A write into a `String` never fails.
    let _ = write_refusal(field, carrier, &mut message);
    let mut error = GraphQLError::new(message, GraphQLErrorOptions::default());
    error.extensions.insert("code", REFUSED_ON_CARRIER);
    Thrown::from(error)
}

/// Notes the revision of the publish whose payload the pump pushes next.
pub fn note_revision(context: &Context, revision: Revision) {
    revisions(context).noted.borrow_mut().push_back(revision);
}

/// Sets the hook on `agent` that hands the oldest noted revision of `context`
/// to the execution that starts.
///
/// The executor runs the hook at the start of the execution of each event of
/// a subscription on `agent`, and the pump of a hub fed stream sets it before
/// it takes its first event. Every event the pump delivers reaches exactly
/// one execution, in the order the pump noted their revisions, since an app
/// can neither filter nor merge a topic. So the revision of each event leaves
/// the queue when the execution of that event starts, on every road a
/// subscription takes, and the queue keeps at most the revision of the one
/// push the pump holds. The hook reads no ambient context, since the readable
/// stream of server sent events runs its pulls with none in place, and it
/// keeps the revisions alone, so the agent holds no handle of the context.
pub(crate) fn follow_executions(agent: &Agent, context: &Context) {
    let revisions = revisions(context);
    agent.set_event_hook(Rc::new(move || {
        let started = revisions.noted.borrow_mut().pop_front();
        revisions.started.set(started);
    }));
}

/// Takes the revision of the event whose execution started last, once per result the stream yields.
///
/// A stream whose events no hub fed, a repeater that pushes on its own,
/// noted none, and the answer is `None`, as it is for a second take after
/// one execution.
#[must_use]
pub fn take_revision(context: &Context) -> Option<Revision> {
    context.extension::<Revisions>()?.started.take()
}

/// Returns how many revisions `context` keeps whose execution has not started.
///
/// The operation `revisions` of the devtool answers it after each result of a
/// stream, so a notebook can read that the queue keeps at most one revision
/// on each road a subscription takes.
#[must_use]
pub fn noted_revisions(context: &Context) -> usize {
    context
        .extension::<Revisions>()
        .map_or(0, |revisions| revisions.noted.borrow().len())
}

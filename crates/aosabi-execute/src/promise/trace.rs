// SPDX-License-Identifier: MIT
/*
 * aosabi-execute promise trace: the events a request records for the harness.
 *
 * No analogue in the reference. The shape is the trace of the behaviour tables.
 */

//! The trace of a request.
//!
//! The behaviour interpreter and the request stream of a twin record when a
//! resolver is called, when it throws and when a promise of theirs settles,
//! so that a notebook compares the schedule of both sides and not only the
//! result. The executor records nothing.

use aosabi_json::{
    JsObject,
    JsValue, //
};

use crate::promise::Agent;

/// One recorded event.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEvent {
    /// The virtual time of the event in timer units.
    pub time: u64,
    /// The count of jobs started when the event happened.
    pub tick: u64,
    /// The label of what the event is about.
    pub label: String,
    /// What happened, one of `called`, `thrown`, `fulfilled` and `rejected`.
    pub state: &'static str,
}

impl TraceEvent {
    /// Returns the event as the object the harness compares.
    ///
    /// The keys are `time`, `label` and `state`, as the node side writes
    /// them, and `tick` last, which only this side has.
    #[must_use]
    pub fn to_js_value(&self) -> JsValue {
        let mut event = JsObject::new();
        event.insert("time", self.time);
        event.insert("label", self.label.as_str());
        event.insert("state", self.state);
        event.insert("tick", self.tick);
        JsValue::Object(event)
    }
}

impl Agent {
    /// Records the event `state` of `label` at the current time and tick.
    ///
    /// This is a no operation unless the agent was made with
    /// `AgentOptions::trace`.
    pub fn record(&self, label: &str, state: &'static str) {
        if !self.0.options.trace {
            return;
        }
        self.0.trace.borrow_mut().push(TraceEvent {
            time: self.now(),
            tick: self.jobs_run(),
            label: String::from(label),
            state,
        });
    }

    /// Returns the recorded events in the order they happened and clears the record.
    #[must_use]
    pub fn take_trace(&self) -> Vec<TraceEvent> {
        self.0.trace.take()
    }
}

// SPDX-License-Identifier: MIT
/*
 * hyper::proto::h1::node::timers: the timers Node runs on a connection.
 *
 * Translation of the head and request timers of Node 24.13.0,
 * src/node_http_parser.cc:269-287, :496-507, :684-692 and :1124-1181 with
 * lib/_http_server.js:468-510, and of its idle close,
 * lib/_http_server.js:1047-1058 and :881-889. It implements quirk Q43 of
 * docs/byte-equality.md section 13.1.
 */
//! The head, request and idle deadlines of Node over the timer of the connection, quirk Q43.

use core::time::Duration;

use super::NodeConfig;

/// What a deadline of [`Timers`] ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Expired {
    /// A head or a request past its timer, answered with Node's 408.
    Request,
    /// An idle connection, closed with no byte.
    Idle,
}

/// The timers of one Node connection, each a time since the epoch of its
/// timer.
#[derive(Debug)]
pub(crate) struct Timers {
    /// When the message in progress began and whether its head is complete.
    message: Option<(Duration, bool)>,
    /// When an idle connection closes.
    idle: Option<Duration>,
}

impl Timers {
    /// Returns the timers of a connection accepted at `now`, whose first head
    /// runs from the accept.
    pub(crate) fn new(now: Duration) -> Self {
        Timers {
            message: Some((now, false)),
            idle: None,
        }
    }

    /// Starts the timers of a message at its first byte llhttp does not skip,
    /// never at a skipped one.
    pub(crate) fn begun(&mut self, now: Duration) {
        if self.message.is_none() {
            self.message = Some((now, false));
        }
    }

    /// Stops the head timer at a complete head, which also clears the idle
    /// close.
    pub(crate) fn head_complete(&mut self) {
        if let Some(message) = &mut self.message {
            message.1 = true;
        }
        self.idle = None;
    }

    /// Stops the request timer at the end of a message.
    pub(crate) fn message_complete(&mut self) {
        self.message = None;
    }

    /// Pushes the idle close after a read, the bytes llhttp skips included.
    pub(crate) fn read(&mut self, now: Duration, config: &NodeConfig) {
        if self.idle.is_some() {
            self.idle(now, config);
        }
    }

    /// Sets the idle close once the last answer in flight has ended.
    pub(crate) fn idle(&mut self, now: Duration, config: &NodeConfig) {
        let after = config.keep_alive_timeout_ms + config.keep_alive_buffer_ms;
        self.idle = Some(now + Duration::from_millis(u64::from(after)));
    }

    /// Returns the earliest deadline, a check of the head and request timers
    /// at the next multiple of `check_interval_ms` after a timeout or the idle
    /// close.
    pub(crate) fn deadline(&self, config: &NodeConfig) -> Option<Duration> {
        let check = self.message.map(|(start, head)| {
            let timeout = if head {
                config.request_timeout_ms
            } else {
                config.headers_timeout_ms.min(config.request_timeout_ms)
            };
            tick_after(start + Duration::from_millis(u64::from(timeout)), config)
        });
        match (check, self.idle) {
            (Some(check), Some(idle)) => Some(check.min(idle)),
            (check, idle) => check.or(idle),
        }
    }

    /// Returns what the deadlines end at `now`.
    pub(crate) fn expired(&self, now: Duration, config: &NodeConfig) -> Option<Expired> {
        let check = self.message.map(|(start, head)| {
            let timeout = if head {
                config.request_timeout_ms
            } else {
                config.headers_timeout_ms.min(config.request_timeout_ms)
            };
            tick_after(start + Duration::from_millis(u64::from(timeout)), config)
        });
        if check.is_some_and(|check| now >= check) {
            return Some(Expired::Request);
        }
        self.idle.filter(|idle| now >= *idle).map(|_| Expired::Idle)
    }
}

/// Returns the first check of Node's interval strictly after `time`, since a
/// check refuses a message older than its timeout and not one as old as it.
fn tick_after(time: Duration, config: &NodeConfig) -> Duration {
    let interval = u128::from(config.check_interval_ms.max(1));
    let ticks = time.as_millis() / interval + 1;
    Duration::from_millis(u64::try_from(ticks * interval).unwrap_or(u64::MAX))
}

//! Zero-dep autostart stopwatch with named checkpoints / milestones. Lives
//! alongside the bench harness so callers can drop one in mid-loop
//! without adding a dep.
//!
//! ```
//! use subms::SubMsTimer;
//!
//! let mut t = SubMsTimer::new("parse-request");
//! t.mark("headers-read");
//! // do work
//! t.mark("body-decoded");
//! // do more work
//! t.stop("served");
//! t.print(&mut std::io::stdout()).unwrap();
//!
//! // Or grab structured data and ship it to your own pipeline:
//! for cp in t.checkpoints() {
//!     // metrics.record(cp.label, cp.since_start_ns);
//!     let _ = cp;
//! }
//! ```
//!
//! For production span/event work prefer the [`tracing`](https://docs.rs/tracing)
//! crate's spans + events, or OpenTelemetry's Rust SDK. `SubMsTimer` is the
//! right pick when you need sub-microsecond overhead in a hot loop and the
//! consumer is the same process.
//!
//! The Java sibling ships an identical surface as
//! `com.submillisecond.perf.SubMsTimer`.

use std::io::{self, Write};
use std::time::Instant;

use crate::bench::format_ns;

/// A single named milestone captured by [`SubMsTimer::mark`] /
/// [`SubMsTimer::lap`] / [`SubMsTimer::stop`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubMsTimerCheckpoint {
    /// Label passed to `mark` / `lap` / `stop`.
    pub label: String,
    /// Nanoseconds between this checkpoint and the previous (or `start`).
    pub since_last_ns: u64,
    /// Nanoseconds between this checkpoint and `start`.
    pub since_start_ns: u64,
    /// `true` if this checkpoint stopped the timer.
    pub is_stop: bool,
}

/// Autostart stopwatch with named checkpoints. Not thread-safe; use one per
/// task or synchronise externally.
pub struct SubMsTimer {
    name: String,
    started_at: Instant,
    last_at: Instant,
    stopped_at: Option<Instant>,
    checkpoints: Vec<SubMsTimerCheckpoint>,
}

impl SubMsTimer {
    /// Autostart unnamed timer.
    pub fn unnamed() -> Self {
        Self::new("")
    }

    /// Autostart timer with a display name (printed in the header).
    pub fn new(name: &str) -> Self {
        let now = Instant::now();
        Self {
            name: name.to_string(),
            started_at: now,
            last_at: now,
            stopped_at: None,
            checkpoints: Vec::new(),
        }
    }

    /// Reset to t=0 and clear all checkpoints.
    pub fn start(&mut self) -> &mut Self {
        let now = Instant::now();
        self.started_at = now;
        self.last_at = now;
        self.stopped_at = None;
        self.checkpoints.clear();
        self
    }

    /// Alias of [`start`](Self::start).
    pub fn reset(&mut self) -> &mut Self {
        self.start()
    }

    /// Record a checkpoint with the given label. Returns the elapsed-since-start ns.
    pub fn mark(&mut self, label: &str) -> u64 {
        let now = Instant::now();
        let since_start = now.duration_since(self.started_at).as_nanos() as u64;
        let since_last = now.duration_since(self.last_at).as_nanos() as u64;
        self.last_at = now;
        self.checkpoints.push(SubMsTimerCheckpoint {
            label: label.to_string(),
            since_last_ns: since_last,
            since_start_ns: since_start,
            is_stop: false,
        });
        since_start
    }

    /// Alias of [`mark`](Self::mark) - emphasises "split / next leg" semantics.
    pub fn lap(&mut self, label: &str) -> u64 {
        self.mark(label)
    }

    /// Final checkpoint; the timer stops accumulating after this.
    pub fn stop(&mut self, label: &str) -> u64 {
        let now = Instant::now();
        let since_start = now.duration_since(self.started_at).as_nanos() as u64;
        let since_last = now.duration_since(self.last_at).as_nanos() as u64;
        self.last_at = now;
        self.stopped_at = Some(now);
        self.checkpoints.push(SubMsTimerCheckpoint {
            label: label.to_string(),
            since_last_ns: since_last,
            since_start_ns: since_start,
            is_stop: true,
        });
        since_start
    }

    /// `true` after [`stop`](Self::stop) is called.
    pub fn is_stopped(&self) -> bool {
        self.stopped_at.is_some()
    }

    /// Elapsed since start. Frozen at stop-time if stopped.
    pub fn elapsed_ns(&self) -> u64 {
        let end = self.stopped_at.unwrap_or_else(Instant::now);
        end.duration_since(self.started_at).as_nanos() as u64
    }

    pub fn name(&self) -> &str { &self.name }

    pub fn checkpoints(&self) -> &[SubMsTimerCheckpoint] { &self.checkpoints }

    /// Print a fixed-width timeline. Byte-equivalent to Java's
    /// `SubMsTimer.print`.
    ///
    /// Layout:
    /// ```text
    /// timer "parse-request"  total=3.2us
    ///   headers-read         +1.1us       1.1us
    ///   body-decoded         +800ns       1.9us
    ///   served *             +1.3us       3.2us
    /// ```
    pub fn print<W: Write>(&self, out: &mut W) -> io::Result<()> {
        writeln!(
            out,
            "timer \"{}\"  total={}",
            self.name,
            format_ns(self.elapsed_ns())
        )?;
        for cp in &self.checkpoints {
            let label = if cp.is_stop {
                format!("{} *", cp.label)
            } else {
                cp.label.clone()
            };
            writeln!(
                out,
                "  {:<18}  +{:>8}   {:>8}",
                label,
                format_ns(cp.since_last_ns),
                format_ns(cp.since_start_ns)
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn autostart_and_mark_captures_increasing_since_start() {
        let mut t = SubMsTimer::new("x");
        thread::sleep(Duration::from_millis(1));
        let a = t.mark("a");
        thread::sleep(Duration::from_millis(1));
        let b = t.mark("b");
        assert!(a > 0);
        assert!(b > a);
        assert_eq!(t.checkpoints().len(), 2);
        assert_eq!(t.checkpoints()[0].label, "a");
        assert_eq!(t.checkpoints()[1].label, "b");
        assert!(!t.checkpoints()[0].is_stop);
    }

    #[test]
    fn stop_marks_is_stop_and_freezes_elapsed() {
        let mut t = SubMsTimer::new("x");
        thread::sleep(Duration::from_millis(1));
        t.stop("done");
        assert!(t.is_stopped());
        let e1 = t.elapsed_ns();
        thread::sleep(Duration::from_millis(2));
        let e2 = t.elapsed_ns();
        assert_eq!(e1, e2, "elapsed should freeze after stop");
        assert!(t.checkpoints().last().unwrap().is_stop);
    }

    #[test]
    fn reset_clears_checkpoints() {
        let mut t = SubMsTimer::new("x");
        t.mark("a");
        t.mark("b");
        t.reset();
        assert!(t.checkpoints().is_empty());
        assert!(!t.is_stopped());
    }

    #[test]
    fn lap_is_alias_of_mark() {
        let mut t = SubMsTimer::new("x");
        t.lap("a");
        assert_eq!(t.checkpoints().len(), 1);
        assert_eq!(t.checkpoints()[0].label, "a");
    }

    #[test]
    fn print_emits_header_and_checkpoints() {
        let mut t = SubMsTimer::new("parse");
        t.mark("a");
        t.stop("done");
        let mut buf = Vec::new();
        t.print(&mut buf).unwrap();
        let out = String::from_utf8(buf).unwrap();
        assert!(out.contains("timer \"parse\""));
        assert!(out.contains("a"));
        assert!(out.contains("done *"));
    }
}

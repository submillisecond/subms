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
use std::sync::OnceLock;
use std::time::Instant;

use crate::bench::format_ns;

/// Opaque starting-tick handle. Returned by [`SubMsTimer::tick`]; the only
/// useful operation is [`SubMsTick::elapsed_ns`] later. Zero-cost wrapper
/// around `Instant` (`repr(transparent)`).
///
/// Use this everywhere a recipe or harness wants the
/// "snapshot now / do work / read delta" pattern - it routes every
/// platform-clock read through `SubMsTimer` so we can swap to a faster
/// path (rdtsc, cntvct_el0, vDSO) in one place.
#[derive(Copy, Clone, Debug)]
#[repr(transparent)]
pub struct SubMsTick(Instant);

impl SubMsTick {
    /// Nanoseconds since this tick was taken.
    #[inline]
    pub fn elapsed_ns(self) -> u64 {
        self.0.elapsed().as_nanos() as u64
    }
}

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
    /// Single source of truth for monotonic-clock reads across the
    /// subms harness. Returns nanoseconds since a process-local epoch
    /// (snapshotted on first call); meaningful only as a delta against
    /// another `nanos_now()` reading.
    ///
    /// Prefer [`SubMsTimer::tick`] + [`SubMsTick::elapsed_ns`] in hot
    /// loops - it's one fewer atomic load + one fewer arithmetic op
    /// because we don't normalise against the process epoch on each
    /// call.
    #[inline]
    pub fn nanos_now() -> u64 {
        static EPOCH: OnceLock<Instant> = OnceLock::new();
        let epoch = EPOCH.get_or_init(Instant::now);
        epoch.elapsed().as_nanos() as u64
    }

    /// Snapshot the monotonic clock for the "start tick / do work /
    /// read delta" pattern. Recipes use this inside their bench loops
    /// instead of `std::time::Instant::now()`. Cost is identical to a
    /// raw `Instant::now()` (the return is a `#[repr(transparent)]`
    /// wrapper).
    #[inline]
    pub fn tick() -> SubMsTick {
        SubMsTick(Instant::now())
    }

    /// Time a closure. Returns the closure result and elapsed
    /// nanoseconds. Convenient when the call is on the hot path and
    /// the caller doesn't care about retaining a tick handle.
    #[inline]
    pub fn measure_ns<F, T>(f: F) -> (T, u64)
    where
        F: FnOnce() -> T,
    {
        let t0 = Instant::now();
        let r = f();
        let elapsed = t0.elapsed().as_nanos() as u64;
        (r, elapsed)
    }

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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn checkpoints(&self) -> &[SubMsTimerCheckpoint] {
        &self.checkpoints
    }

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
#[path = "timer_tests.rs"]
mod tests;

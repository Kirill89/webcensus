use std::collections::BTreeSet;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::task::JoinHandle;

use skim::parse_dns_line;

/// Counters and position tracking shared between the feeder, the probe
/// tasks, and the periodic logger. Wrapped in `Arc` because all three need
/// shared ownership.
pub struct Progress {
    total: AtomicU64,
    errors: AtomicU64,
    skipped: AtomicU64,
    /// Lines currently being probed (out-of-order completion is normal). The
    /// minimum here is the line below which we know everything is done.
    inflight: StdMutex<BTreeSet<u64>>,
    /// Highest input line the feeder has consumed (probed *or* skipped).
    feeder_progress: AtomicU64,
    /// Total probeable records (from the precount pass), if known.
    total_probeable: Option<u64>,
    started: Instant,
    /// Last (total, instant) sample, used by [`Progress::recent_rpm`] to
    /// produce a windowed rate based on the previous logger tick.
    rate_sample: StdMutex<Option<(u64, Instant)>>,
}

impl Progress {
    pub fn new(start_line: u64, total_probeable: Option<u64>) -> Arc<Self> {
        Arc::new(Self {
            total: AtomicU64::new(0),
            errors: AtomicU64::new(0),
            skipped: AtomicU64::new(0),
            inflight: StdMutex::new(BTreeSet::new()),
            feeder_progress: AtomicU64::new(start_line.saturating_sub(1)),
            total_probeable,
            started: Instant::now(),
            rate_sample: StdMutex::new(None),
        })
    }

    /// Mark a line as having entered the probe pipeline.
    pub fn start_probe(&self, line: u64) {
        self.inflight.lock().unwrap().insert(line);
        self.feeder_progress.store(line, Ordering::Relaxed);
    }

    /// Mark a probe as completed; `ok` distinguishes success from any failure.
    pub fn finish_probe(&self, line: u64, ok: bool) {
        self.total.fetch_add(1, Ordering::Relaxed);
        if !ok {
            self.errors.fetch_add(1, Ordering::Relaxed);
        }
        self.inflight.lock().unwrap().remove(&line);
    }

    /// Record an input line skipped by the feeder (no usable A record).
    pub fn record_skip(&self, line: u64) {
        self.skipped.fetch_add(1, Ordering::Relaxed);
        self.feeder_progress.store(line, Ordering::Relaxed);
    }

    /// Record that the feeder consumed a line without acting on it (used for
    /// pre-`--start-line` lines).
    pub fn advance_feeder(&self, line: u64) {
        self.feeder_progress.store(line, Ordering::Relaxed);
    }

    /// The highest input line number we can guarantee is fully processed.
    /// Resume by passing `--start-line position+1`.
    pub fn position(&self) -> u64 {
        match self.inflight.lock().unwrap().iter().next() {
            Some(&min) => min.saturating_sub(1),
            None => self.feeder_progress.load(Ordering::Relaxed),
        }
    }

    /// Spawn a task that prints a stats line every `every`. Caller must
    /// `.abort()` the returned handle to stop it.
    pub fn spawn_periodic_logger(self: &Arc<Self>, every: Duration) -> JoinHandle<()> {
        let me = Arc::clone(self);
        tokio::spawn(async move {
            let mut tick = tokio::time::interval(every);
            tick.tick().await;
            loop {
                tick.tick().await;
                me.log_periodic();
            }
        })
    }

    fn log_periodic(&self) {
        let snap = self.snapshot();
        let rpm = self.recent_rpm(&snap);
        let pct = match self.total_probeable {
            Some(total) if total > 0 => format!(" pct={:.1}%", percent(snap.total, total)),
            _ => String::new(),
        };
        let eta = match self.total_probeable {
            Some(total) if rpm > 0 => {
                let remaining = total.saturating_sub(snap.total);
                format!(" eta={}", fmt_duration(remaining * 60 / rpm))
            }
            _ => String::new(),
        };
        eprintln!(
            "[{:>5}s] done={}{pct} errors={} skipped={} rpm={rpm} pos={}{eta}",
            snap.elapsed.as_secs(),
            snap.total,
            snap.errors,
            snap.skipped,
            snap.position,
        );
    }

    pub fn final_summary(&self) {
        let snap = self.snapshot();
        let rpm = cumulative_rpm(snap.total, snap.elapsed);
        eprintln!(
            "[done] total={} errors={} skipped={} rpm={rpm} pos={} elapsed={}",
            snap.total,
            snap.errors,
            snap.skipped,
            snap.position,
            fmt_duration(snap.elapsed.as_secs()),
        );
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            total: self.total.load(Ordering::Relaxed),
            errors: self.errors.load(Ordering::Relaxed),
            skipped: self.skipped.load(Ordering::Relaxed),
            position: self.position(),
            elapsed: self.started.elapsed(),
        }
    }

    /// Rate over the interval since the last `recent_rpm` call (i.e. one
    /// logger tick). Falls back to cumulative on the first call. Has the
    /// side effect of advancing the rate sample.
    fn recent_rpm(&self, snap: &Snapshot) -> u64 {
        let now = Instant::now();
        let mut sample = self.rate_sample.lock().unwrap();
        let rpm = match *sample {
            Some((prev_total, prev_at)) => {
                let dt = now.duration_since(prev_at).as_secs_f64();
                if dt > 0.001 && snap.total >= prev_total {
                    rate_per_minute(snap.total - prev_total, dt)
                } else {
                    cumulative_rpm(snap.total, snap.elapsed)
                }
            }
            None => cumulative_rpm(snap.total, snap.elapsed),
        };
        *sample = Some((snap.total, now));
        rpm
    }
}

struct Snapshot {
    total: u64,
    errors: u64,
    skipped: u64,
    position: u64,
    elapsed: Duration,
}

fn cumulative_rpm(total: u64, elapsed: Duration) -> u64 {
    rate_per_minute(total, elapsed.as_secs_f64().max(0.001))
}

#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
fn rate_per_minute(count: u64, dt_seconds: f64) -> u64 {
    (count as f64 * 60.0 / dt_seconds) as u64
}

#[allow(clippy::cast_precision_loss)]
fn percent(numerator: u64, denominator: u64) -> f64 {
    numerator as f64 * 100.0 / denominator as f64
}

pub fn fmt_duration(secs: u64) -> String {
    if secs >= 3600 {
        format!("{}h{}m", secs / 3600, (secs / 60) % 60)
    } else if secs >= 60 {
        format!("{}m{}s", secs / 60, secs % 60)
    } else {
        format!("{secs}s")
    }
}

/// Stream the input file once and count probeable records (those where
/// [`parse_dns_line`] returns `Some`), starting from `start_line`. Used to
/// give the periodic logger an ETA estimate.
pub async fn precount(input: &Path, start_line: u64) -> Result<u64> {
    let f = tokio::fs::File::open(input)
        .await
        .with_context(|| format!("opening input {} for precount", input.display()))?;
    let mut lines = BufReader::new(f).lines();
    let mut current = 0u64;
    let mut probeable = 0u64;
    let mut last_print = Instant::now();
    let started = Instant::now();
    while let Some(line) = lines.next_line().await? {
        current += 1;
        if current < start_line {
            continue;
        }
        if parse_dns_line(&line).is_some() {
            probeable += 1;
        }
        if last_print.elapsed() >= Duration::from_secs(5) {
            eprintln!("[precount] read {current} lines, {probeable} probeable so far");
            last_print = Instant::now();
        }
    }
    eprintln!(
        "[precount] {current} lines scanned, {probeable} probeable in {}",
        fmt_duration(started.elapsed().as_secs())
    );
    Ok(probeable)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{cumulative_rpm, fmt_duration, percent, rate_per_minute};

    #[test]
    fn formats_durations() {
        assert_eq!(fmt_duration(0), "0s");
        assert_eq!(fmt_duration(5), "5s");
        assert_eq!(fmt_duration(65), "1m5s");
        assert_eq!(fmt_duration(3725), "1h2m");
    }

    #[test]
    fn rate_math_matches_units() {
        // 600 events in 60 seconds = 600 per minute.
        assert_eq!(rate_per_minute(600, 60.0), 600);
        // 100 events in 5 seconds = 1200 per minute.
        assert_eq!(rate_per_minute(100, 5.0), 1200);
        // Cumulative wraps the same math.
        assert_eq!(cumulative_rpm(600, Duration::from_secs(60)), 600);
    }

    #[test]
    fn percent_computes_correctly() {
        assert!((percent(0, 100) - 0.0).abs() < 1e-9);
        assert!((percent(25, 100) - 25.0).abs() < 1e-9);
        assert!((percent(33, 100) - 33.0).abs() < 1e-9);
    }
}

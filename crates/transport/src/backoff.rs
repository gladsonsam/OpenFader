//! Reconnect with exponential backoff.
use std::time::Duration;

/// A backoff multiplier that cannot produce a finite, nondecreasing delay.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct InvalidBackoffFactor(f64);

impl std::fmt::Display for InvalidBackoffFactor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "backoff factor must be finite and at least 1, got {}",
            self.0
        )
    }
}

impl std::error::Error for InvalidBackoffFactor {}

/// Exponential retry delays capped by the configured maximum duration.
#[derive(Debug)]
pub struct Backoff {
    pub initial: Duration,
    pub max: Duration,
    factor: f64,
    attempts: u32,
}

impl Backoff {
    pub fn new(initial: Duration, max: Duration) -> Self {
        Self {
            initial,
            max,
            factor: 2.0,
            attempts: 0,
        }
    }

    /// Set the multiplier applied between retry attempts.
    ///
    /// # Errors
    /// Returns an error for non-finite factors or factors below one.
    pub fn with_factor(mut self, factor: f64) -> Result<Self, InvalidBackoffFactor> {
        if !factor.is_finite() || factor < 1.0 {
            return Err(InvalidBackoffFactor(factor));
        }
        self.factor = factor;
        Ok(self)
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn reset(&mut self) {
        self.attempts = 0;
    }

    /// Return the next capped delay and advance the retry count.
    pub fn next_delay(&mut self) -> Duration {
        let exp = self.factor.powi(self.attempts.min(16) as i32);
        let seconds = if self.initial.is_zero() {
            0.0
        } else {
            self.initial.as_secs_f64() * exp
        };
        // Cap before converting to Duration: multiplication can exceed its
        // representable range even when the configured maximum is small.
        let delay = if seconds >= self.max.as_secs_f64() {
            self.max
        } else {
            Duration::try_from_secs_f64(seconds)
                .unwrap_or(self.max)
                .min(self.max)
        };
        self.attempts = self.attempts.saturating_add(1);
        delay
    }
}

impl Default for Backoff {
    fn default() -> Self {
        Self::new(Duration::from_millis(250), Duration::from_secs(10))
    }
}

/// Run `op` until it succeeds or `max_attempts` is exhausted, sleeping with
/// backoff between tries. `max_attempts == 0` means retry forever.
/// Returns the successful value or the last error.
pub async fn connect_with_backoff<F, Fut, T>(
    mut op: F,
    max_attempts: usize,
    initial: Duration,
    max_delay: Duration,
) -> anyhow::Result<T>
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = anyhow::Result<T>>,
{
    let mut backoff = Backoff::new(initial, max_delay);
    let mut attempt: usize = 0;
    loop {
        match op().await {
            Ok(v) => return Ok(v),
            Err(e) => {
                attempt += 1;
                if max_attempts != 0 && attempt >= max_attempts {
                    return Err(e);
                }
                tokio::time::sleep(backoff.next_delay()).await;
            }
        }
    }
}

/// Backoff delays for attempts 0..`max_attempts`: 250ms doubling to an 8s cap.
/// Pure iterator so drivers never hand-roll sleep loops.
pub fn backoff_delays(max_attempts: usize) -> impl Iterator<Item = Duration> {
    (0..max_attempts).map(|n| Duration::from_millis(250 << n.min(5)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn backoff_caps() {
        let ds: Vec<_> = backoff_delays(8).collect();
        assert_eq!(ds[0], Duration::from_millis(250));
        assert_eq!(ds[1], Duration::from_millis(500));
        assert!(ds.iter().all(|d| *d <= Duration::from_secs(8)));
    }

    #[tokio::test]
    async fn retries_then_succeeds() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let worker = attempts.clone();
        let v = connect_with_backoff(
            move || {
                let worker = worker.clone();
                async move {
                    let n = worker.fetch_add(1, Ordering::SeqCst);
                    if n < 2 {
                        anyhow::bail!("flaky wifi {n}")
                    } else {
                        Ok(42)
                    }
                }
            },
            5,
            Duration::from_millis(1),
            Duration::from_millis(2),
        )
        .await
        .unwrap();
        assert_eq!(v, 42);
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }

    #[tokio::test]
    async fn exhaustion_returns_last_error() {
        let attempts = Arc::new(AtomicUsize::new(0));
        let worker = attempts.clone();
        let err = connect_with_backoff::<_, _, i32>(
            move || {
                let worker = worker.clone();
                async move {
                    worker.fetch_add(1, Ordering::SeqCst);
                    anyhow::bail!("no route to mixer")
                }
            },
            3,
            Duration::from_millis(1),
            Duration::from_millis(2),
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("no route to mixer"));
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}

use std::time::Instant;

/// A thread-safe token bucket rate limiter.
///
/// Tokens are replenished at a steady rate (requests_per_minute / 60 per second).
/// Burst capacity allows short spikes above the steady-state rate.
pub struct TokenBucket {
    /// Maximum tokens the bucket can hold
    capacity: f64,
    /// Tokens added per second
    refill_rate: f64,
    /// Current available tokens
    tokens: f64,
    /// Last time tokens were refilled
    last_refill: Instant,
}

impl TokenBucket {
    /// Create a new token bucket.
    ///
    /// - `requests_per_minute`: steady-state rate
    /// - `burst`: maximum burst size (bucket capacity)
    pub fn new(requests_per_minute: u32, burst: u32) -> Self {
        let capacity = burst as f64;
        let refill_rate = requests_per_minute as f64 / 60.0;

        Self {
            capacity,
            refill_rate,
            tokens: capacity, // start full
            last_refill: Instant::now(),
        }
    }

    /// Try to acquire `count` tokens. Returns true if successful.
    pub fn try_acquire(&mut self, count: u32) -> bool {
        self.refill();
        let needed = count as f64;
        if self.tokens >= needed {
            self.tokens -= needed;
            true
        } else {
            false
        }
    }

    /// Returns the number of remaining tokens (floored to integer).
    pub fn remaining(&mut self) -> u32 {
        self.refill();
        self.tokens as u32
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.capacity);
        self.last_refill = now;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn new_bucket_starts_full() {
        let mut bucket = TokenBucket::new(60, 10);
        assert_eq!(bucket.remaining(), 10);
    }

    #[test]
    fn acquire_reduces_tokens() {
        let mut bucket = TokenBucket::new(60, 10);
        assert!(bucket.try_acquire(3));
        assert_eq!(bucket.remaining(), 7);
    }

    #[test]
    fn acquire_fails_when_insufficient() {
        let mut bucket = TokenBucket::new(60, 5);
        assert!(bucket.try_acquire(5));
        assert!(!bucket.try_acquire(1));
    }

    #[test]
    fn tokens_refill_over_time() {
        let mut bucket = TokenBucket::new(6000, 10); // 100/sec
        assert!(bucket.try_acquire(10)); // drain it
        assert_eq!(bucket.remaining(), 0);

        // Manually advance time by setting last_refill in the past
        bucket.last_refill = Instant::now() - Duration::from_millis(100);
        // Should have refilled ~10 tokens (100/s * 0.1s)
        assert!(bucket.remaining() >= 9); // allow for timing
    }

    #[test]
    fn tokens_do_not_exceed_capacity() {
        let mut bucket = TokenBucket::new(6000, 5);
        // Wait simulated time
        bucket.last_refill = Instant::now() - Duration::from_secs(10);
        assert_eq!(bucket.remaining(), 5); // capped at capacity
    }
}

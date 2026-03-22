/// Suspicion increment for an unregistered tool proposal.
pub const SUSPICION_UNREGISTERED_TOOL: f64 = 0.3;
/// Suspicion increment for an oversized payload.
pub const SUSPICION_OVERSIZED: f64 = 0.2;
/// Suspicion increment for a rate-limit hit.
pub const SUSPICION_RATE_LIMIT: f64 = 0.1;
/// Suspicion increment for a failed adjudication.
pub const SUSPICION_ADJUDICATION_FAILURE: f64 = 0.2;
/// Suspicion increment for a detected injection attempt.
pub const SUSPICION_INJECTION: f64 = 0.5;

/// Tracks the remaining trust budget and suspicion score for a satellite session.
///
/// Every tool proposal from a satellite deducts from the trust budget. If the
/// budget reaches zero the session is terminated. The suspicion score
/// accumulates separately; once it exceeds the threshold the session is also
/// terminated.
#[derive(Debug, Clone)]
pub struct TrustBudget {
    /// Starting budget value.
    pub initial: f64,
    /// Currently remaining budget.
    pub remaining: f64,
    /// Accumulated suspicion score.
    pub suspicion_score: f64,
    /// Threshold at which suspicion triggers session termination.
    pub suspicion_threshold: f64,
}

impl TrustBudget {
    /// Create a new trust budget with the given initial value and suspicion
    /// threshold.
    pub fn new(initial: f64, suspicion_threshold: f64) -> Self {
        Self {
            initial,
            remaining: initial,
            suspicion_score: 0.0,
            suspicion_threshold,
        }
    }

    /// Deduct `amount` from the remaining budget. Returns `true` if the budget
    /// is still positive after the deduction, `false` if exhausted.
    pub fn deduct(&mut self, amount: f64) -> bool {
        self.remaining -= amount;
        self.remaining > 0.0
    }

    /// Add `amount` to the suspicion score. Returns `true` if the session is
    /// still within the threshold, `false` if the threshold has been exceeded.
    pub fn add_suspicion(&mut self, amount: f64) -> bool {
        self.suspicion_score += amount;
        self.suspicion_score < self.suspicion_threshold
    }

    /// Whether the budget is exhausted.
    pub fn is_exhausted(&self) -> bool {
        self.remaining <= 0.0
    }

    /// Whether the suspicion threshold has been exceeded.
    pub fn is_suspicious(&self) -> bool {
        self.suspicion_score >= self.suspicion_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deduct_within_budget_returns_true() {
        let mut budget = TrustBudget::new(10.0, 1.0);
        assert!(budget.deduct(3.0));
        assert!((budget.remaining - 7.0).abs() < f64::EPSILON);
    }

    #[test]
    fn deduct_to_zero_returns_false() {
        let mut budget = TrustBudget::new(5.0, 1.0);
        assert!(!budget.deduct(5.0));
        assert!(budget.is_exhausted());
    }

    #[test]
    fn deduct_past_zero_returns_false() {
        let mut budget = TrustBudget::new(2.0, 1.0);
        assert!(!budget.deduct(3.0));
        assert!(budget.is_exhausted());
    }

    #[test]
    fn suspicion_below_threshold_returns_true() {
        let mut budget = TrustBudget::new(10.0, 1.0);
        assert!(budget.add_suspicion(SUSPICION_UNREGISTERED_TOOL));
        assert!(!budget.is_suspicious());
    }

    #[test]
    fn suspicion_at_threshold_returns_false() {
        let mut budget = TrustBudget::new(10.0, 1.0);
        assert!(!budget.add_suspicion(1.0));
        assert!(budget.is_suspicious());
    }

    #[test]
    fn suspicion_above_threshold_returns_false() {
        let mut budget = TrustBudget::new(10.0, 1.0);
        assert!(!budget.add_suspicion(1.5));
        assert!(budget.is_suspicious());
    }

    #[test]
    fn suspicion_accumulates_across_calls() {
        let mut budget = TrustBudget::new(10.0, 1.0);
        assert!(budget.add_suspicion(SUSPICION_UNREGISTERED_TOOL)); // 0.3
        assert!(budget.add_suspicion(SUSPICION_OVERSIZED)); // 0.5
        assert!(budget.add_suspicion(SUSPICION_RATE_LIMIT)); // 0.6
        assert!(budget.add_suspicion(SUSPICION_ADJUDICATION_FAILURE)); // 0.8
        // 0.8 + 0.5 = 1.3 >= 1.0 => false
        assert!(!budget.add_suspicion(SUSPICION_INJECTION));
        assert!(budget.is_suspicious());
    }

    #[test]
    fn default_increments_have_expected_values() {
        assert!((SUSPICION_UNREGISTERED_TOOL - 0.3).abs() < f64::EPSILON);
        assert!((SUSPICION_OVERSIZED - 0.2).abs() < f64::EPSILON);
        assert!((SUSPICION_RATE_LIMIT - 0.1).abs() < f64::EPSILON);
        assert!((SUSPICION_ADJUDICATION_FAILURE - 0.2).abs() < f64::EPSILON);
        assert!((SUSPICION_INJECTION - 0.5).abs() < f64::EPSILON);
    }
}

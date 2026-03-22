use std::collections::HashMap;
use std::sync::Mutex;

use crate::config::{RateLimitConfig, RateLimitRule};

use super::token_bucket::TokenBucket;

/// Result of a rate limit check, including remaining quota info for headers.
#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub remaining: u32,
    pub limit: u32,
}

/// Multi-tier rate limiter: global, per-identity, and per-tool.
pub struct RateLimiter {
    global: Mutex<TokenBucket>,
    global_rule: RateLimitRule,
    per_identity: Mutex<HashMap<String, TokenBucket>>,
    per_identity_rule: RateLimitRule,
    per_tool: Mutex<HashMap<String, TokenBucket>>,
    per_tool_rules: HashMap<String, RateLimitRule>,
}

impl RateLimiter {
    pub fn from_config(config: &RateLimitConfig) -> Self {
        Self {
            global: Mutex::new(TokenBucket::new(
                config.global.requests_per_minute,
                config.global.burst,
            )),
            global_rule: config.global.clone(),
            per_identity: Mutex::new(HashMap::new()),
            per_identity_rule: config.per_identity.clone(),
            per_tool: Mutex::new(HashMap::new()),
            per_tool_rules: config.per_tool.clone(),
        }
    }

    /// Check rate limits for a request. Checks global, then per-identity.
    /// Returns the most restrictive result.
    pub fn check_request(&self, identity_sub: &str) -> RateLimitResult {
        // Check global limit
        let global_result = {
            let mut global = self.global.lock().unwrap();
            let allowed = global.try_acquire(1);
            RateLimitResult {
                allowed,
                remaining: global.remaining(),
                limit: self.global_rule.requests_per_minute,
            }
        };

        if !global_result.allowed {
            return global_result;
        }

        // Check per-identity limit
        let mut per_id = self.per_identity.lock().unwrap();
        let bucket = per_id.entry(identity_sub.to_string()).or_insert_with(|| {
            TokenBucket::new(
                self.per_identity_rule.requests_per_minute,
                self.per_identity_rule.burst,
            )
        });

        let allowed = bucket.try_acquire(1);
        let remaining = bucket.remaining();

        RateLimitResult {
            allowed,
            remaining,
            limit: self.per_identity_rule.requests_per_minute,
        }
    }

    /// Check per-tool rate limit. Returns None if no rule is configured for
    /// the given tool, meaning "allowed with no limit".
    pub fn check_tool(&self, tool_name: &str) -> Option<RateLimitResult> {
        let rule = self.per_tool_rules.get(tool_name)?;

        let mut per_tool = self.per_tool.lock().unwrap();
        let bucket = per_tool
            .entry(tool_name.to_string())
            .or_insert_with(|| TokenBucket::new(rule.requests_per_minute, rule.burst));

        let allowed = bucket.try_acquire(1);
        let remaining = bucket.remaining();

        Some(RateLimitResult {
            allowed,
            remaining,
            limit: rule.requests_per_minute,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> RateLimitConfig {
        RateLimitConfig {
            enabled: true,
            global: RateLimitRule {
                requests_per_minute: 100,
                burst: 10,
            },
            per_identity: RateLimitRule {
                requests_per_minute: 60,
                burst: 5,
            },
            per_tool: {
                let mut m = HashMap::new();
                m.insert(
                    "expensive_tool".to_string(),
                    RateLimitRule {
                        requests_per_minute: 10,
                        burst: 2,
                    },
                );
                m
            },
        }
    }

    #[test]
    fn allows_requests_within_limits() {
        let limiter = RateLimiter::from_config(&test_config());
        let result = limiter.check_request("user-1");
        assert!(result.allowed);
    }

    #[test]
    fn per_identity_limit_enforced() {
        let limiter = RateLimiter::from_config(&test_config());

        // Per-identity burst is 5
        for _ in 0..5 {
            let result = limiter.check_request("user-1");
            assert!(result.allowed);
        }

        // 6th should be rejected
        let result = limiter.check_request("user-1");
        assert!(!result.allowed);
    }

    #[test]
    fn different_identities_have_separate_buckets() {
        let limiter = RateLimiter::from_config(&test_config());

        for _ in 0..5 {
            limiter.check_request("user-1");
        }

        // user-2 should still have full quota
        let result = limiter.check_request("user-2");
        assert!(result.allowed);
    }

    #[test]
    fn per_tool_limit_enforced() {
        let limiter = RateLimiter::from_config(&test_config());

        // expensive_tool has burst of 2
        let r1 = limiter.check_tool("expensive_tool");
        assert!(r1.unwrap().allowed);

        let r2 = limiter.check_tool("expensive_tool");
        assert!(r2.unwrap().allowed);

        let r3 = limiter.check_tool("expensive_tool");
        assert!(!r3.unwrap().allowed);
    }

    #[test]
    fn unconfigured_tool_returns_none() {
        let limiter = RateLimiter::from_config(&test_config());
        assert!(limiter.check_tool("unknown_tool").is_none());
    }
}

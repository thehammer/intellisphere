use super::identity::Identity;

/// Error type for authentication failures.
#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("invalid token: {0}")]
    InvalidToken(String),

    #[error("expired token")]
    ExpiredToken,

    #[error("missing token")]
    MissingToken,

    #[error("unknown API key")]
    UnknownApiKey,
}

/// Trait for pluggable authentication strategies.
pub trait AuthStrategy: Send + Sync {
    /// Human-readable name for this strategy (used in logs).
    fn name(&self) -> &str;

    /// Attempt to authenticate the given token/key string.
    /// Returns an Identity on success or an AuthError on failure.
    fn authenticate(&self, token: &str) -> Result<Identity, AuthError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AlwaysOk;

    impl AuthStrategy for AlwaysOk {
        fn name(&self) -> &str {
            "always_ok"
        }

        fn authenticate(&self, _token: &str) -> Result<Identity, AuthError> {
            Ok(Identity::anonymous())
        }
    }

    #[test]
    fn trait_is_object_safe() {
        let strategy: Box<dyn AuthStrategy> = Box::new(AlwaysOk);
        assert_eq!(strategy.name(), "always_ok");
        assert!(strategy.authenticate("anything").is_ok());
    }
}

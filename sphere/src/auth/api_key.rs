use std::collections::HashMap;

use super::identity::Identity;
use super::strategy::{AuthError, AuthStrategy};
use crate::config::ApiKeyEntry;

/// API key authentication strategy.
/// Validates static API keys from configuration and returns
/// a pre-configured Identity for each key.
pub struct ApiKeyAuthStrategy {
    /// Map from API key string to the identity it grants
    keys: HashMap<String, Identity>,
}

impl ApiKeyAuthStrategy {
    pub fn new(entries: &[ApiKeyEntry]) -> Self {
        let mut keys = HashMap::new();
        for entry in entries {
            keys.insert(
                entry.key.clone(),
                Identity {
                    sub: entry.identity_sub.clone(),
                    roles: entry.roles.clone(),
                    scopes: entry.scopes.clone(),
                    metadata: HashMap::new(),
                },
            );
        }
        Self { keys }
    }
}

impl AuthStrategy for ApiKeyAuthStrategy {
    fn name(&self) -> &str {
        "api_key"
    }

    fn authenticate(&self, token: &str) -> Result<Identity, AuthError> {
        self.keys
            .get(token)
            .cloned()
            .ok_or(AuthError::UnknownApiKey)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_key_returns_identity() {
        let entries = vec![ApiKeyEntry {
            key: "sk-test-123".to_string(),
            identity_sub: "service-a".to_string(),
            roles: vec!["reader".to_string()],
            scopes: vec!["chat".to_string()],
        }];

        let strategy = ApiKeyAuthStrategy::new(&entries);
        let identity = strategy.authenticate("sk-test-123").unwrap();

        assert_eq!(identity.sub, "service-a");
        assert_eq!(identity.roles, vec!["reader"]);
        assert_eq!(identity.scopes, vec!["chat"]);
    }

    #[test]
    fn invalid_key_returns_error() {
        let entries = vec![ApiKeyEntry {
            key: "sk-test-123".to_string(),
            identity_sub: "service-a".to_string(),
            roles: vec![],
            scopes: vec![],
        }];

        let strategy = ApiKeyAuthStrategy::new(&entries);
        let err = strategy.authenticate("sk-wrong-key").unwrap_err();
        assert!(matches!(err, AuthError::UnknownApiKey));
    }

    #[test]
    fn empty_keys_always_rejects() {
        let strategy = ApiKeyAuthStrategy::new(&[]);
        assert!(strategy.authenticate("anything").is_err());
    }
}

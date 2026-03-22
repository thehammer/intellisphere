use crate::auth::identity::Identity;
use crate::auth::strategy::{AuthError, AuthStrategy};
use crate::config::AuthConfig;
use crate::errors::SphereError;

/// Ingress gate: authenticates incoming requests using configured strategies.
pub struct IngressGate {
    enabled: bool,
    strategies: Vec<Box<dyn AuthStrategy>>,
}

impl IngressGate {
    /// Build an IngressGate from auth configuration.
    pub fn from_config(config: &AuthConfig) -> Self {
        use crate::auth::api_key::ApiKeyAuthStrategy;
        use crate::auth::jwt::JwtAuthStrategy;
        use crate::config::AuthStrategyConfig;

        let mut strategies: Vec<Box<dyn AuthStrategy>> = Vec::new();

        for strategy_config in &config.strategies {
            match strategy_config {
                AuthStrategyConfig::Jwt {
                    secret,
                    issuer,
                    audience,
                } => {
                    strategies.push(Box::new(JwtAuthStrategy::new_hs256(
                        secret,
                        issuer.as_deref(),
                        audience.as_deref(),
                    )));
                }
                AuthStrategyConfig::ApiKey { keys } => {
                    strategies.push(Box::new(ApiKeyAuthStrategy::new(keys)));
                }
            }
        }

        Self {
            enabled: config.enabled,
            strategies,
        }
    }

    /// Authenticate a request using the Authorization header value or API key.
    ///
    /// - If auth is disabled, returns an anonymous Identity.
    /// - Tries `Authorization: Bearer <token>` first, then `X-API-Key` header.
    /// - Tries each configured strategy in order until one succeeds.
    pub fn authenticate(
        &self,
        authorization_header: Option<&str>,
        api_key_header: Option<&str>,
    ) -> Result<Identity, SphereError> {
        if !self.enabled {
            return Ok(Identity::anonymous());
        }

        // Extract the token to try
        let token = if let Some(auth_value) = authorization_header {
            if let Some(bearer_token) = auth_value.strip_prefix("Bearer ") {
                Some(bearer_token.to_string())
            } else {
                Some(auth_value.to_string())
            }
        } else {
            api_key_header.map(|k| k.to_string())
        };

        let token = token.ok_or_else(|| {
            SphereError::AuthError("missing Authorization or X-API-Key header".to_string())
        })?;

        // Try each strategy in order
        let mut last_error = AuthError::MissingToken;
        for strategy in &self.strategies {
            match strategy.authenticate(&token) {
                Ok(identity) => {
                    tracing::debug!(
                        strategy = strategy.name(),
                        sub = %identity.sub,
                        "Authentication succeeded"
                    );
                    return Ok(identity);
                }
                Err(e) => {
                    tracing::trace!(
                        strategy = strategy.name(),
                        error = %e,
                        "Strategy did not match"
                    );
                    last_error = e;
                }
            }
        }

        Err(SphereError::AuthError(format!(
            "all strategies failed: {}",
            last_error
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{ApiKeyEntry, AuthStrategyConfig};

    fn config_with_api_key() -> AuthConfig {
        AuthConfig {
            enabled: true,
            strategies: vec![AuthStrategyConfig::ApiKey {
                keys: vec![ApiKeyEntry {
                    key: "test-key-123".to_string(),
                    identity_sub: "svc-test".to_string(),
                    roles: vec!["admin".to_string()],
                    scopes: vec!["chat".to_string()],
                }],
            }],
        }
    }

    #[test]
    fn disabled_auth_returns_anonymous() {
        let gate = IngressGate::from_config(&AuthConfig::default());
        let identity = gate.authenticate(None, None).unwrap();
        assert_eq!(identity.sub, "anonymous");
    }

    #[test]
    fn api_key_via_header() {
        let gate = IngressGate::from_config(&config_with_api_key());
        let identity = gate.authenticate(None, Some("test-key-123")).unwrap();
        assert_eq!(identity.sub, "svc-test");
        assert_eq!(identity.roles, vec!["admin"]);
    }

    #[test]
    fn bearer_token_fallback_to_api_key() {
        // When Bearer token is used but only API key strategy is configured,
        // the API key strategy will try to match the bearer token as a key
        let gate = IngressGate::from_config(&config_with_api_key());
        let identity = gate
            .authenticate(Some("Bearer test-key-123"), None)
            .unwrap();
        assert_eq!(identity.sub, "svc-test");
    }

    #[test]
    fn missing_credentials_returns_error() {
        let gate = IngressGate::from_config(&config_with_api_key());
        assert!(gate.authenticate(None, None).is_err());
    }

    #[test]
    fn wrong_key_returns_error() {
        let gate = IngressGate::from_config(&config_with_api_key());
        assert!(gate.authenticate(None, Some("wrong-key")).is_err());
    }
}

use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use std::collections::HashMap;

use super::identity::Identity;
use super::strategy::{AuthError, AuthStrategy};

/// JWT claims we extract from tokens.
#[derive(Debug, Deserialize)]
struct Claims {
    sub: Option<String>,
    #[serde(default)]
    roles: Vec<String>,
    #[serde(default)]
    scopes: Vec<String>,
    /// Alternative: space-delimited scope string (OAuth2 convention)
    scope: Option<String>,
}

/// JWT authentication strategy supporting HS256 with a shared secret.
pub struct JwtAuthStrategy {
    decoding_key: DecodingKey,
    validation: Validation,
}

impl JwtAuthStrategy {
    /// Create a new HS256 JWT strategy with the given shared secret.
    pub fn new_hs256(
        secret: &str,
        issuer: Option<&str>,
        audience: Option<&str>,
    ) -> Self {
        let decoding_key = DecodingKey::from_secret(secret.as_bytes());
        let mut validation = Validation::new(Algorithm::HS256);

        // When no issuer/audience is configured, don't require spec claims
        // beyond exp (which jsonwebtoken checks by default).
        let mut required_claims: Vec<&str> = vec!["exp"];

        if let Some(iss) = issuer {
            validation.set_issuer(&[iss]);
            required_claims.push("iss");
        }

        if let Some(aud) = audience {
            validation.set_audience(&[aud]);
            required_claims.push("aud");
        }

        validation.set_required_spec_claims(&required_claims);

        Self {
            decoding_key,
            validation,
        }
    }
}

impl AuthStrategy for JwtAuthStrategy {
    fn name(&self) -> &str {
        "jwt"
    }

    fn authenticate(&self, token: &str) -> Result<Identity, AuthError> {
        let token_data = decode::<Claims>(token, &self.decoding_key, &self.validation)
            .map_err(|e| {
                let msg = e.to_string();
                if msg.contains("ExpiredSignature") {
                    AuthError::ExpiredToken
                } else {
                    AuthError::InvalidToken(msg)
                }
            })?;

        let claims = token_data.claims;
        let sub = claims
            .sub
            .ok_or_else(|| AuthError::InvalidToken("missing sub claim".to_string()))?;

        // Merge scopes from both array and space-delimited string
        let mut scopes = claims.scopes;
        if let Some(scope_str) = claims.scope {
            for s in scope_str.split_whitespace() {
                if !scopes.contains(&s.to_string()) {
                    scopes.push(s.to_string());
                }
            }
        }

        Ok(Identity {
            sub,
            roles: claims.roles,
            scopes,
            metadata: HashMap::new(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use jsonwebtoken::{encode, EncodingKey, Header};
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestClaims {
        sub: String,
        roles: Vec<String>,
        scopes: Vec<String>,
        exp: u64,
    }

    // HS256 requires at least 32 bytes for the secret in jsonwebtoken v10
    const TEST_SECRET: &str = "test-secret-key-minimum-32-bytes!";
    const WRONG_SECRET: &str = "wrong-secret-key-minimum-32bytes!";

    fn make_token(claims: &TestClaims, secret: &str) -> String {
        encode(
            &Header::default(),
            claims,
            &EncodingKey::from_secret(secret.as_bytes()),
        )
        .unwrap()
    }

    #[test]
    fn valid_jwt_returns_identity() {
        let strategy = JwtAuthStrategy::new_hs256(TEST_SECRET, None, None);

        let claims = TestClaims {
            sub: "user-123".to_string(),
            roles: vec!["admin".to_string()],
            scopes: vec!["read".to_string(), "write".to_string()],
            exp: chrono::Utc::now().timestamp() as u64 + 3600,
        };

        let token = make_token(&claims, TEST_SECRET);
        let identity = strategy.authenticate(&token).unwrap();

        assert_eq!(identity.sub, "user-123");
        assert_eq!(identity.roles, vec!["admin"]);
        assert_eq!(identity.scopes, vec!["read", "write"]);
    }

    #[test]
    fn invalid_secret_returns_error() {
        let strategy = JwtAuthStrategy::new_hs256(TEST_SECRET, None, None);

        let claims = TestClaims {
            sub: "user-123".to_string(),
            roles: vec![],
            scopes: vec![],
            exp: chrono::Utc::now().timestamp() as u64 + 3600,
        };

        let token = make_token(&claims, WRONG_SECRET);
        assert!(strategy.authenticate(&token).is_err());
    }

    #[test]
    fn expired_token_returns_expired_error() {
        let strategy = JwtAuthStrategy::new_hs256(TEST_SECRET, None, None);

        let claims = TestClaims {
            sub: "user-123".to_string(),
            roles: vec![],
            scopes: vec![],
            exp: 1000, // long expired
        };

        let token = make_token(&claims, TEST_SECRET);
        let err = strategy.authenticate(&token).unwrap_err();
        assert!(matches!(err, AuthError::ExpiredToken));
    }

    #[test]
    fn issuer_validation() {
        let strategy = JwtAuthStrategy::new_hs256(TEST_SECRET, Some("my-issuer"), None);

        // Token without issuer should fail
        let claims = TestClaims {
            sub: "user-123".to_string(),
            roles: vec![],
            scopes: vec![],
            exp: chrono::Utc::now().timestamp() as u64 + 3600,
        };

        let token = make_token(&claims, TEST_SECRET);
        assert!(strategy.authenticate(&token).is_err());
    }
}

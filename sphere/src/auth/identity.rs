use std::collections::HashMap;

/// Represents an authenticated caller's identity.
#[derive(Debug, Clone)]
pub struct Identity {
    /// Subject identifier (e.g., user ID, service account name)
    pub sub: String,

    /// Roles assigned to this identity
    pub roles: Vec<String>,

    /// OAuth2-style scopes granted to this identity
    pub scopes: Vec<String>,

    /// Arbitrary metadata extracted from the auth token
    pub metadata: HashMap<String, String>,
}

impl Identity {
    /// Create an anonymous identity (used when auth is disabled).
    pub fn anonymous() -> Self {
        Self {
            sub: "anonymous".to_string(),
            roles: vec![],
            scopes: vec![],
            metadata: HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn anonymous_identity_has_expected_defaults() {
        let id = Identity::anonymous();
        assert_eq!(id.sub, "anonymous");
        assert!(id.roles.is_empty());
        assert!(id.scopes.is_empty());
        assert!(id.metadata.is_empty());
    }
}

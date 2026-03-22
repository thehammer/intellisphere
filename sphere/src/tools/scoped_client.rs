use std::collections::HashSet;
use std::time::Duration;

use url::Url;

/// HTTP client that restricts requests to an allowlist of domains.
/// Enforces TLS, size limits, and timeouts.
#[derive(Clone)]
pub struct ScopedHttpClient {
    client: reqwest::Client,
    allowed_domains: HashSet<String>,
    max_request_body_bytes: usize,
    max_response_body_bytes: usize,
}

#[derive(Debug, thiserror::Error)]
pub enum ScopedClientError {
    #[error("Domain '{0}' is not in the allowlist")]
    DomainNotAllowed(String),

    #[error("Only HTTPS URLs are allowed, got: {0}")]
    InsecureUrl(String),

    #[error("Response body exceeds maximum size of {0} bytes")]
    ResponseTooLarge(usize),

    #[error("HTTP request failed: {0}")]
    RequestFailed(#[from] reqwest::Error),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

impl ScopedHttpClient {
    pub fn new(
        allowed_domains: HashSet<String>,
        timeout: Duration,
        max_request_body_bytes: usize,
        max_response_body_bytes: usize,
    ) -> Self {
        let client = reqwest::Client::builder()
            .timeout(timeout)
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .expect("Failed to build HTTP client");

        Self {
            client,
            allowed_domains,
            max_request_body_bytes,
            max_response_body_bytes,
        }
    }

    /// Validate that a URL is allowed.
    fn validate_url(&self, url_str: &str) -> Result<Url, ScopedClientError> {
        let url = Url::parse(url_str).map_err(|e| ScopedClientError::InvalidUrl(e.to_string()))?;

        // Enforce HTTPS
        if url.scheme() != "https" {
            return Err(ScopedClientError::InsecureUrl(url_str.to_string()));
        }

        // Check domain allowlist
        let domain = url
            .host_str()
            .ok_or_else(|| ScopedClientError::InvalidUrl("No host in URL".to_string()))?;

        if !self.allowed_domains.contains(domain) {
            return Err(ScopedClientError::DomainNotAllowed(domain.to_string()));
        }

        Ok(url)
    }

    /// Send a GET request to an allowed URL.
    pub async fn get(&self, url: &str) -> Result<String, ScopedClientError> {
        let validated_url = self.validate_url(url)?;

        let response = self.client.get(validated_url).send().await?;

        let body = response.text().await?;
        if body.len() > self.max_response_body_bytes {
            return Err(ScopedClientError::ResponseTooLarge(
                self.max_response_body_bytes,
            ));
        }

        Ok(body)
    }

    /// Send a POST request with a JSON body to an allowed URL.
    pub async fn post(&self, url: &str, body: &str) -> Result<String, ScopedClientError> {
        let validated_url = self.validate_url(url)?;

        if body.len() > self.max_request_body_bytes {
            return Err(ScopedClientError::ResponseTooLarge(
                self.max_request_body_bytes,
            ));
        }

        let response = self
            .client
            .post(validated_url)
            .header("Content-Type", "application/json")
            .body(body.to_string())
            .send()
            .await?;

        let response_body = response.text().await?;
        if response_body.len() > self.max_response_body_bytes {
            return Err(ScopedClientError::ResponseTooLarge(
                self.max_response_body_bytes,
            ));
        }

        Ok(response_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_client() -> ScopedHttpClient {
        let mut domains = HashSet::new();
        domains.insert("api.example.com".to_string());
        ScopedHttpClient::new(domains, Duration::from_secs(5), 1024 * 1024, 1024 * 1024)
    }

    #[test]
    fn test_rejects_non_https() {
        let client = test_client();
        let result = client.validate_url("http://api.example.com/data");
        assert!(matches!(result, Err(ScopedClientError::InsecureUrl(_))));
    }

    #[test]
    fn test_rejects_unknown_domain() {
        let client = test_client();
        let result = client.validate_url("https://evil.com/data");
        assert!(matches!(
            result,
            Err(ScopedClientError::DomainNotAllowed(_))
        ));
    }

    #[test]
    fn test_accepts_allowed_domain() {
        let client = test_client();
        let result = client.validate_url("https://api.example.com/data");
        assert!(result.is_ok());
    }

    #[test]
    fn test_rejects_invalid_url() {
        let client = test_client();
        let result = client.validate_url("not a url");
        assert!(matches!(result, Err(ScopedClientError::InvalidUrl(_))));
    }
}

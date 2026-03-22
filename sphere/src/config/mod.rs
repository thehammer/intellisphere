mod loader;

pub use loader::*;

use std::collections::HashMap;

use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct SphereConfig {
    /// Address to listen on (e.g., "0.0.0.0:8080")
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,

    /// gRPC URL for the Core service
    #[serde(default = "default_core_grpc_url")]
    pub core_grpc_url: String,

    /// Pipeline configuration
    #[serde(default)]
    pub pipeline: PipelineConfig,

    /// Tool configuration
    #[serde(default)]
    pub tools: ToolsConfig,

    /// Authentication configuration
    #[serde(default)]
    pub auth: AuthConfig,

    /// Rate limiting configuration
    #[serde(default)]
    pub rate_limit: RateLimitConfig,

    /// Policy engine configuration
    #[serde(default)]
    pub policies: Vec<PolicyConfig>,
}

// ── Authentication config ──────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    /// Whether authentication is enabled (default false for dev)
    #[serde(default)]
    pub enabled: bool,

    /// Ordered list of authentication strategies to try
    #[serde(default)]
    pub strategies: Vec<AuthStrategyConfig>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            strategies: vec![],
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AuthStrategyConfig {
    Jwt {
        secret: String,
        issuer: Option<String>,
        audience: Option<String>,
    },
    ApiKey {
        keys: Vec<ApiKeyEntry>,
    },
}

#[derive(Debug, Clone, Deserialize)]
pub struct ApiKeyEntry {
    pub key: String,
    pub identity_sub: String,
    #[serde(default)]
    pub roles: Vec<String>,
    #[serde(default)]
    pub scopes: Vec<String>,
}

// ── Rate limiting config ───────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    /// Whether rate limiting is enabled (default false for dev)
    #[serde(default)]
    pub enabled: bool,

    /// Global rate limit (all requests combined)
    #[serde(default = "default_global_rate_limit")]
    pub global: RateLimitRule,

    /// Per-identity rate limit
    #[serde(default = "default_per_identity_rate_limit")]
    pub per_identity: RateLimitRule,

    /// Per-tool rate limits (keyed by tool name)
    #[serde(default)]
    pub per_tool: HashMap<String, RateLimitRule>,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            global: default_global_rate_limit(),
            per_identity: default_per_identity_rate_limit(),
            per_tool: HashMap::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitRule {
    #[serde(default = "default_rpm")]
    pub requests_per_minute: u32,
    #[serde(default = "default_burst")]
    pub burst: u32,
}

fn default_global_rate_limit() -> RateLimitRule {
    RateLimitRule {
        requests_per_minute: 600,
        burst: 50,
    }
}

fn default_per_identity_rate_limit() -> RateLimitRule {
    RateLimitRule {
        requests_per_minute: 60,
        burst: 10,
    }
}

fn default_rpm() -> u32 {
    60
}

fn default_burst() -> u32 {
    10
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct PipelineConfig {
    /// Inbound filter chain configuration
    #[serde(default)]
    pub inbound: InboundConfig,

    /// Outbound filter chain configuration
    #[serde(default)]
    pub outbound: OutboundConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct InboundConfig {
    /// Input sanitization filter settings
    #[serde(default)]
    pub input_sanitization: InputSanitizationConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct InputSanitizationConfig {
    /// Whether this filter is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,

    /// Whether to strip control characters (U+0000–U+001F except \n \r \t)
    #[serde(default = "default_true")]
    pub strip_control_chars: bool,

    /// Whether to normalize unicode to NFC
    #[serde(default = "default_true")]
    pub normalize_unicode: bool,

    /// Whether to reject null bytes
    #[serde(default = "default_true")]
    pub reject_null_bytes: bool,
}

impl Default for InputSanitizationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            strip_control_chars: true,
            normalize_unicode: true,
            reject_null_bytes: true,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutboundConfig {
    /// PII leak detection in outbound responses
    #[serde(default)]
    pub pii_leak_detection: OutboundFilterToggle,

    /// Injection echo detection
    #[serde(default)]
    pub injection_echo: OutboundFilterToggle,

    /// Result size enforcement
    #[serde(default)]
    pub result_size: ResultSizeConfig,

    /// Response content classifier
    #[serde(default)]
    pub response_classifier: OutboundFilterToggle,

    /// Hallucination URL flag
    #[serde(default)]
    pub hallucination_flag: OutboundFilterToggle,
}

impl Default for OutboundConfig {
    fn default() -> Self {
        Self {
            pii_leak_detection: OutboundFilterToggle::default(),
            injection_echo: OutboundFilterToggle::default(),
            result_size: ResultSizeConfig::default(),
            response_classifier: OutboundFilterToggle::default(),
            hallucination_flag: OutboundFilterToggle::default(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct OutboundFilterToggle {
    #[serde(default)]
    pub enabled: bool,
}

impl Default for OutboundFilterToggle {
    fn default() -> Self {
        Self { enabled: false }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct ResultSizeConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_max_response_chars")]
    pub max_chars: usize,
}

impl Default for ResultSizeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_chars: default_max_response_chars(),
        }
    }
}

fn default_max_response_chars() -> usize {
    100_000
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ToolsConfig {
    /// Directory containing tool manifests
    pub manifest_dir: Option<String>,

    /// Maximum tool loop iterations
    #[serde(default = "default_max_tool_iterations")]
    pub max_tool_iterations: usize,
}

fn default_listen_addr() -> String {
    "0.0.0.0:8080".to_string()
}

fn default_core_grpc_url() -> String {
    "http://core:50051".to_string()
}

fn default_true() -> bool {
    true
}

fn default_max_tool_iterations() -> usize {
    10
}

// ── Policy config ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct PolicyConfig {
    /// Human-readable policy name
    pub name: String,

    /// Scope the policy applies to
    pub scope: crate::policy::PolicyScope,

    /// Rules that make up the policy
    pub rules: Vec<crate::policy::PolicyRule>,

    /// Action to take on violation
    pub action: crate::policy::PolicyAction,
}

use serde::Deserialize;

/// A policy that governs behaviour within the Sphere.
#[derive(Debug, Clone)]
pub struct Policy {
    pub name: String,
    pub scope: PolicyScope,
    pub rules: Vec<PolicyRule>,
    pub action: PolicyAction,
}

/// The scope a policy applies to.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyScope {
    /// Applies per-session
    Session,
    /// Applies per-identity
    Identity,
    /// Applies to a specific tool
    Tool { name: String },
    /// Applies to all requests
    Global,
}

/// Individual rules that make up a policy.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum PolicyRule {
    MaxToolCallsPerSession { limit: u32 },
    MaxTokensPerIdentityPerHour { limit: u64 },
    RequireMfa,
    RequireRoles { roles: Vec<String> },
    MaxResultSize { limit: usize },
}

/// Action to take when a policy is violated.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum PolicyAction {
    TerminateSession,
    Throttle,
    Block,
    Flag,
    Log,
}

/// Events that trigger policy evaluation.
#[derive(Debug, Clone)]
pub enum PolicyEvent {
    /// A new request has been received
    RequestReceived,
    /// A tool call has been requested
    ToolCallRequested { tool_name: String },
    /// Session state has been updated (e.g., token counts changed)
    SessionUpdate,
}

/// A violation produced when a policy rule is not satisfied.
#[derive(Debug, Clone, PartialEq)]
pub struct PolicyViolation {
    /// Name of the policy that was violated
    pub policy_name: String,
    /// Action that should be taken
    pub action: PolicyAction,
    /// Human-readable reason for the violation
    pub reason: String,
}

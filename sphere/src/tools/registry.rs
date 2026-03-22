use std::collections::HashMap;

use serde::Deserialize;

/// Registration entry for a tool in the registry.
#[derive(Debug, Clone, Deserialize)]
pub struct ToolRegistration {
    /// Unique tool name
    pub name: String,

    /// Human-readable description
    pub description: String,

    /// JSON Schema for input parameters
    pub input_schema_json: String,

    /// Where this tool executes
    pub zone: ToolZone,

    /// Required scopes for authorization
    #[serde(default)]
    pub required_scopes: Vec<String>,

    /// Required roles for authorization
    #[serde(default)]
    pub required_roles: Vec<String>,

    /// Handler endpoint (for Sphere-zone HTTP tools)
    pub handler_url: Option<String>,

    /// Timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum ToolZone {
    Sphere,
    Satellite,
    Wasm,
}

fn default_timeout_ms() -> u64 {
    5000
}

/// In-memory tool registry. Tools are loaded from JSON manifests at startup.
pub struct ToolRegistry {
    tools: HashMap<String, ToolRegistration>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        Self {
            tools: HashMap::new(),
        }
    }

    /// Register a tool. Returns error if name already exists.
    pub fn register(&mut self, tool: ToolRegistration) -> Result<(), String> {
        if self.tools.contains_key(&tool.name) {
            return Err(format!("Tool '{}' is already registered", tool.name));
        }
        self.tools.insert(tool.name.clone(), tool);
        Ok(())
    }

    /// Look up a tool by name.
    pub fn get(&self, name: &str) -> Option<&ToolRegistration> {
        self.tools.get(name)
    }

    /// Get all registered tools.
    pub fn all(&self) -> impl Iterator<Item = &ToolRegistration> {
        self.tools.values()
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.len()
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }
}

//! Schema visibility control for federated queries
//!
//! This module manages which schemas and tables are visible for federated query operations.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for schema and table visibility in federated queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationVisibilityConfig {
    /// Connection ID this config applies to
    pub connection_id: String,
    /// Default schema to use when none specified
    #[serde(default = "default_public_schema")]
    pub default_schema: String,
    /// If true, allow all schemas except excluded ones
    #[serde(default)]
    pub allow_all_schemas: bool,
    /// List of allowed schemas (only used if allow_all_schemas is false)
    #[serde(default)]
    pub allowed_schemas: Vec<String>,
    /// List of excluded schemas (used when allow_all_schemas is true)
    #[serde(default)]
    pub excluded_schemas: Vec<String>,
    /// List of fully qualified tables to exclude (e.g., "schema.table")
    #[serde(default)]
    pub excluded_tables: Vec<String>,
    /// Whether to show system/internal schemas
    #[serde(default)]
    pub include_system_schemas: bool,
}

fn default_public_schema() -> String {
    "public".to_string()
}

impl Default for FederationVisibilityConfig {
    fn default() -> Self {
        Self {
            connection_id: String::new(),
            default_schema: default_public_schema(),
            allow_all_schemas: true,
            allowed_schemas: vec![],
            excluded_schemas: vec![],
            excluded_tables: vec![],
            include_system_schemas: false,
        }
    }
}

impl FederationVisibilityConfig {
    /// Create a new config for a specific connection
    pub fn new(connection_id: &str) -> Self {
        Self { connection_id: connection_id.to_string(), ..Self::default() }
    }

    /// Check if a schema is accessible
    pub fn is_schema_accessible(&self, schema: &str) -> bool {
        // Always allow the default schema
        if schema == self.default_schema {
            return true;
        }

        // Handle system schemas
        if !self.include_system_schemas && is_system_schema(schema) {
            return false;
        }

        // Check exclusions first
        if self.excluded_schemas.contains(&schema.to_string()) {
            return false;
        }

        // If allow_all, check no exclusions
        if self.allow_all_schemas {
            return true;
        }

        // Otherwise check inclusion list
        self.allowed_schemas.contains(&schema.to_string())
    }

    /// Check if a table is accessible
    pub fn is_table_accessible(&self, schema: &str, table: &str) -> bool {
        let qualified_name = format!("{}.{}", schema, table);

        // First check schema accessibility
        if !self.is_schema_accessible(schema) {
            return false;
        }

        // Check table exclusions
        !self.excluded_tables.contains(&qualified_name)
    }

    /// Get all accessible schemas (for UI display)
    pub fn get_accessible_schemas(&self, available_schemas: &[&str]) -> Vec<String> {
        available_schemas.iter().filter(|s| self.is_schema_accessible(s)).map(|s| s.to_string()).collect()
    }

    /// Add an excluded schema
    pub fn add_excluded_schema(&mut self, schema: &str) {
        if !self.excluded_schemas.contains(&schema.to_string()) {
            self.excluded_schemas.push(schema.to_string());
        }
    }

    /// Remove an excluded schema
    pub fn remove_excluded_schema(&mut self, schema: &str) {
        self.excluded_schemas.retain(|s| s != schema);
    }

    /// Add an allowed schema
    pub fn add_allowed_schema(&mut self, schema: &str) {
        if !self.allowed_schemas.contains(&schema.to_string()) {
            self.allowed_schemas.push(schema.to_string());
        }
    }

    /// Remove an allowed schema
    pub fn remove_allowed_schema(&mut self, schema: &str) {
        self.allowed_schemas.retain(|s| s != schema);
    }

    /// Add an excluded table
    pub fn add_excluded_table(&mut self, schema: &str, table: &str) {
        let qualified = format!("{}.{}", schema, table);
        if !self.excluded_tables.contains(&qualified) {
            self.excluded_tables.push(qualified);
        }
    }

    /// Remove an excluded table
    pub fn remove_excluded_table(&mut self, schema: &str, table: &str) {
        let qualified = format!("{}.{}", schema, table);
        self.excluded_tables.retain(|t| t != &qualified);
    }
}

/// Helper to identify system/internal schemas
fn is_system_schema(schema: &str) -> bool {
    // Note: schema is lowercased before matching, so only lowercase variants are valid.
    matches!(
        schema.to_lowercase().as_str(),
        "information_schema" | "pg_catalog" | "sys" | "mysql" | "performance_schema"
    )
}

/// Schema visibility manager - maintains configs for all connections
#[derive(Debug, Default)]
pub struct FederationVisibilityManager {
    configs: HashMap<String, FederationVisibilityConfig>,
}

impl FederationVisibilityManager {
    /// Create a new visibility manager
    pub fn new() -> Self {
        Self { configs: HashMap::new() }
    }

    /// Get or create a config for a connection
    pub fn get_or_create_config(&mut self, connection_id: &str) -> &mut FederationVisibilityConfig {
        self.configs.entry(connection_id.to_string()).or_insert_with(|| FederationVisibilityConfig::new(connection_id))
    }

    /// Get config for a connection (if exists)
    pub fn get_config(&self, connection_id: &str) -> Option<&FederationVisibilityConfig> {
        self.configs.get(connection_id)
    }

    /// Check if a connection has visibility configured
    pub fn has_config(&self, connection_id: &str) -> bool {
        self.configs.contains_key(connection_id)
    }

    /// Remove config for a connection
    pub fn remove_config(&mut self, connection_id: &str) -> Option<FederationVisibilityConfig> {
        self.configs.remove(connection_id)
    }

    /// Get all connection IDs with configured visibility
    pub fn connection_ids(&self) -> Vec<String> {
        self.configs.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config_allows_public_schema() {
        let config = FederationVisibilityConfig::default();
        assert!(config.is_schema_accessible("public"));
    }

    #[test]
    fn test_config_with_allow_all() {
        let config = FederationVisibilityConfig::new("conn1");

        // Default should allow all except system schemas
        assert!(config.is_schema_accessible("analytics"));
        assert!(config.is_schema_accessible("reports"));

        // Should block system schemas by default
        assert!(!config.is_schema_accessible("information_schema"));
        assert!(!config.is_schema_accessible("pg_catalog"));
    }

    #[test]
    fn test_config_with_exclusions() {
        let mut config = FederationVisibilityConfig::new("conn1");
        config.add_excluded_schema("sensitive_data");
        config.add_excluded_schema("internal");

        assert!(config.is_schema_accessible("public"));
        assert!(config.is_schema_accessible("analytics"));
        assert!(!config.is_schema_accessible("sensitive_data"));
        assert!(!config.is_schema_accessible("internal"));
    }

    #[test]
    fn test_config_with_allowed_list() {
        let mut config = FederationVisibilityConfig::new("conn1");
        config.allow_all_schemas = false;
        config.add_allowed_schema("public");
        config.add_allowed_schema("analytics");

        assert!(config.is_schema_accessible("public"));
        assert!(config.is_schema_accessible("analytics"));
        assert!(!config.is_schema_accessible("other"));
    }

    #[test]
    fn test_table_exclusion() {
        let mut config = FederationVisibilityConfig::new("conn1");
        config.add_excluded_table("public", "passwords");
        config.add_excluded_table("analytics", "employees");

        assert!(config.is_table_accessible("public", "users"));
        assert!(!config.is_table_accessible("public", "passwords"));
        assert!(!config.is_table_accessible("analytics", "employees"));
        assert!(config.is_table_accessible("analytics", "metrics"));
    }

    #[test]
    fn test_manager_operations() {
        let mut manager = FederationVisibilityManager::new();

        // Get or create
        {
            let config1 = manager.get_or_create_config("conn1");
            assert_eq!(config1.connection_id, "conn1");
        }
        {
            let config2 = manager.get_or_create_config("conn2");
            assert_eq!(config2.connection_id, "conn2");
        }

        // Has config
        assert!(manager.has_config("conn1"));
        assert!(!manager.has_config("nonexistent"));

        // Get config
        assert!(manager.get_config("conn1").is_some());
        assert!(manager.get_config("nonexistent").is_none());

        // Connection IDs
        let ids = manager.connection_ids();
        assert_eq!(ids.len(), 2);
        assert!(ids.contains(&"conn1".to_string()));
        assert!(ids.contains(&"conn2".to_string()));

        // Remove config
        let removed = manager.remove_config("conn1");
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().connection_id, "conn1");
        assert!(!manager.has_config("conn1"));
    }

    #[test]
    fn test_serialization() {
        let config = FederationVisibilityConfig {
            connection_id: "conn1".to_string(),
            default_schema: "public".to_string(),
            allow_all_schemas: false,
            allowed_schemas: vec!["analytics".to_string()],
            excluded_schemas: vec!["sensitive".to_string()],
            excluded_tables: vec!["public.secret".to_string()],
            include_system_schemas: false,
        };

        let json = serde_json::to_string(&config).expect("Should serialize");
        assert!(json.contains("conn1"));
        assert!(json.contains("analytics"));
        assert!(json.contains("sensitive"));

        let deserialized: FederationVisibilityConfig = serde_json::from_str(&json).expect("Should deserialize");
        assert_eq!(deserialized.connection_id, "conn1");
        assert_eq!(deserialized.allowed_schemas, vec!["analytics"]);
    }
}
// TODO: FederationVisibilityManager is currently unused - see PR #XXXX

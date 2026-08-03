//! gRPC protocol for Calcite Agent communication
//! 
//! This module defines the gRPC service interface for communicating with
//! the Java-based Apache Calcite Agent.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use tokio_util::sync::CancellationToken;

/// Request to register a data source with the Calcite Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSourceRequest {
    /// Unique connection ID from dbx
    pub connection_id: String,
    /// JDBC URL for the data source
    pub jdbc_url: String,
    /// Optional username (may be encrypted)
    pub username: Option<String>,
    /// Driver class name
    pub driver_class: String,
    /// Schema visibility configuration
    pub schema_visibility: SchemaVisibilityConfig,
    /// Additional properties
    #[serde(default)]
    pub properties: HashMap<String, String>,
}

/// Response after registering a data source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterSourceResponse {
    /// Connection ID
    pub connection_id: String,
    /// Database product name (e.g., "PostgreSQL")
    pub database_product: Option<String>,
    /// Database version
    pub database_version: Option<String>,
    /// Success status
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
}

/// Configuration for schema/table visibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SchemaVisibilityConfig {
    /// Default schema name
    #[serde(default = "default_schema")]
    pub default_schema: String,
    /// List of allowed schemas (empty means use allow_all)
    #[serde(default)]
    pub allowed_schemas: Vec<String>,
    /// List of excluded schemas
    #[serde(default)]
    pub excluded_schemas: Vec<String>,
    /// List of excluded tables (by fully qualified name)
    #[serde(default)]
    pub excluded_tables: Vec<String>,
    /// Whether to allow all schemas by default
    #[serde(default)]
    pub allow_all_schemas: bool,
}

impl Default for SchemaVisibilityConfig {
    fn default() -> Self {
        Self {
            default_schema: default_schema(),
            allowed_schemas: vec![],
            excluded_schemas: vec![],
            excluded_tables: vec![],
            allow_all_schemas: true,
        }
    }
}

fn default_schema() -> String {
    "public".to_string()
}

/// Request to execute a federated query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteFederatedQueryRequest {
    /// Unique query ID for tracking and cancellation
    pub query_id: String,
    /// SQL query to execute
    pub sql: String,
    /// Maximum rows to return
    #[serde(default = "default_max_rows")]
    pub max_rows: i32,
    /// Timeout in milliseconds
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: i64,
    /// Query execution options
    #[serde(default)]
    pub options: QueryExecutionOptions,
}

/// Query execution options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueryExecutionOptions {
    /// Enable query plan caching
    #[serde(default)]
    pub enable_caching: bool,
    /// Cache TTL in seconds
    #[serde(default = "default_cache_ttl")]
    pub cache_ttl_seconds: i64,

    /// Query priority (higher number = higher priority)
    #[serde(default = "default_priority")]
    pub priority: i32,
}

impl Default for QueryExecutionOptions {
    fn default() -> Self {
        Self {
            enable_caching: false,
            cache_ttl_seconds: 3600,
            priority: 0,
        }
    }
}

fn default_max_rows() -> i32 {
    1000
}

fn default_timeout_ms() -> i64 {
    30000
}

fn default_cache_ttl() -> i64 {
    300
}

fn default_priority() -> i32 {
    0
}

/// Response for federated query execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteFederatedQueryResponse {
    /// Query ID
    pub query_id: String,
    /// Column names
    pub columns: Vec<String>,
    /// Row data as list of maps
    pub rows: Vec<HashMap<String, serde_json::Value>>,
    /// Number of rows returned
    pub row_count: i32,
    /// Execution duration in milliseconds
    pub duration_ms: i64,
    /// True if successful
    pub success: bool,
    /// Error message if failed
    pub error: Option<String>,
    /// Query plan (if explain mode)
    pub plan: Option<String>,
}

/// Request to explain a federated query
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainFederatedQueryRequest {
    /// Query ID
    pub query_id: String,
    /// SQL query to explain
    pub sql: String,
}

/// Response for query explanation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExplainFederatedQueryResponse {
    /// Query ID
    pub query_id: String,
    /// Query plan
    pub plan: String,
    /// Estimated cost
    pub estimated_cost: Option<f64>,
    /// Success status
    pub success: bool,
}

/// Request to unregister a data source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnregisterSourceRequest {
    /// Connection ID to unregister
    pub connection_id: String,
}

/// Response for unregistering a source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnregisterSourceResponse {
    pub success: bool,
    pub error: Option<String>,
}

/// Request to get data source metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetDataSourceMetadataRequest {
    /// Connection ID
    pub connection_id: String,
}

/// Metadata about a registered data source
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceMetadata {
    /// Connection ID
    pub connection_id: String,
    /// Database product name
    pub database_product: String,
    /// Database version
    pub database_version: String,
    /// JDBC driver name
    pub driver_name: String,
    /// JDBC URL
    pub url: String,
    /// Available schemas
    pub available_schemas: Vec<String>,
    /// Table count per schema
    pub table_counts: HashMap<String, usize>,
}

/// Request for updating schema visibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSchemaVisibilityRequest {
    /// Connection ID
    pub connection_id: String,
    /// New visibility configuration
    pub config: SchemaVisibilityConfig,
}

/// Response for updating schema visibility
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateSchemaVisibilityResponse {
    pub success: bool,
    pub error: Option<String>,
}

/// Enum representing all possible request types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum FederationRequest {
    RegisterSource(RegisterSourceRequest),
    UnregisterSource(UnregisterSourceRequest),
    ExecuteFederatedQuery(ExecuteFederatedQueryRequest),
    ExplainFederatedQuery(ExplainFederatedQueryRequest),
    GetDataSourceMetadata(GetDataSourceMetadataRequest),
    UpdateSchemaVisibility(UpdateSchemaVisibilityRequest),
}

/// Enum representing all possible response types
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "method", rename_all = "camelCase")]
pub enum FederationResponse {
    RegisterSource(RegisterSourceResponse),
    UnregisterSource(UnregisterSourceResponse),
    ExecuteFederatedQuery(ExecuteFederatedQueryResponse),
    ExplainFederatedQuery(ExplainFederatedQueryResponse),
    GetDataSourceMetadata(DataSourceMetadata),
    UpdateSchemaVisibility(UpdateSchemaVisibilityResponse),
}

/// gRPC-style client interface for Calcite Agent
pub trait CalciteAgentClient: Send + Sync {
    /// Register a source with the agent
    async fn register_source(&self, request: RegisterSourceRequest) -> Result<RegisterSourceResponse, String>;
    
    /// Unregister a source
    async fn unregister_source(&self, request: UnregisterSourceRequest) -> Result<UnregisterSourceResponse, String>;
    
    /// Execute a federated query
    async fn execute_federated_query(
        &self,
        request: ExecuteFederatedQueryRequest,
        cancel_token: Option<CancellationToken>,
    ) -> Result<ExecuteFederatedQueryResponse, String>;
    
    /// Explain a federated query
    async fn explain_federated_query(
        &self,
        request: ExplainFederatedQueryRequest,
    ) -> Result<ExplainFederatedQueryResponse, String>;
    
    /// Get metadata for a data source
    async fn get_data_source_metadata(
        &self,
        request: GetDataSourceMetadataRequest,
    ) -> Result<DataSourceMetadata, String>;
    
    /// Update schema visibility
    async fn update_schema_visibility(
        &self,
        request: UpdateSchemaVisibilityRequest,
    ) -> Result<UpdateSchemaVisibilityResponse, String>;
    
    /// Check if agent is running
    async fn ping(&self) -> Result<bool, String>;
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_default_schema_visibility() {
        let config = SchemaVisibilityConfig::default();
        assert_eq!(config.default_schema, "public");
        assert!(config.allow_all_schemas);
        assert!(config.allowed_schemas.is_empty());
        assert!(config.excluded_schemas.is_empty());
    }
    
    #[test]
    fn test_schema_visibility_config() {
        let config = SchemaVisibilityConfig {
            default_schema: "analytics".to_string(),
            allowed_schemas: vec!["public".to_string(), "analytics".to_string()],
            excluded_schemas: vec!["sensitive".to_string()],
            excluded_tables: vec!["secret_data".to_string()],
            allow_all_schemas: false,
        };
        
        assert_eq!(config.default_schema, "analytics");
        assert_eq!(config.allowed_schemas.len(), 2);
        assert!(config.allow_all_schemas == false);
    }
    
    #[test]
    fn test_query_execution_options_defaults() {
        let options = QueryExecutionOptions::default();
        assert!(!options.enable_caching);
        assert_eq!(options.priority, 0);
        assert_eq!(options.cache_ttl_seconds, 3600);
    }
}

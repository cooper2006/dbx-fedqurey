//! Federated query resolution: parse SQL AST to map table references to database connections.
//!
//! This module implements the core federation logic:
//! 1. Parse SQL using sqlparser to extract table references
//! 2. Detect 3-part naming convention: `connection_name.schema.table`
//! 3. Map each table reference to its owning connection
//! 4. Determine if single-connection fast path or multi-connection Calcite path is needed

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use sqlparser::ast::{
    visit_relations, visit_relations_mut, Ident, ObjectName, ObjectNamePart, Statement,
};
use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser::Parser;

use crate::models::connection::ConnectionConfig;

/// A single table reference with its connection mapping
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FederatedTableRef {
    /// The original table name as written in SQL (e.g., "my_pg.public.users")
    pub original_name: String,
    /// The connection name (e.g., "my_pg")
    pub connection_name: String,
    /// The database name (for 4-part names like connection.database.schema.table)
    pub database_name: Option<String>,
    /// The schema name (e.g., "public")
    pub schema_name: Option<String>,
    /// The table name (e.g., "users")
    pub table_name: String,
    /// Optional alias used in the query
    pub alias: Option<String>,
}

/// Result of federated query analysis
#[derive(Debug, Clone)]
pub struct FederatedAnalysis {
    /// All unique table references found in the query
    pub table_refs: Vec<FederatedTableRef>,
    /// Connections used by this query (in order of first appearance)
    pub connections: Vec<String>,
    /// Whether this is a single-connection query (fast path available)
    pub is_single_connection: bool,
    /// The single connection name if applicable
    pub single_connection: Option<String>,
    /// Whether the query uses explicit federation syntax (connection.schema.table)
    pub uses_federation_syntax: bool,
}

/// Parse SQL and detect federation patterns
pub fn analyze_federation(sql: &str, connections: &[ConnectionConfig]) -> FederatedAnalysis {
    let mut table_refs = Vec::new();
    let mut connections_seen: Vec<String> = Vec::new();
    let mut uses_federation_syntax = false;

    // Parse the SQL
    let dialect = PostgreSqlDialect {};
    let statements = match Parser::parse_sql(&dialect, sql) {
        Ok(stmts) => stmts,
        Err(_) => {
            // If parsing fails, return empty analysis
            return FederatedAnalysis {
                table_refs,
                connections: connections_seen,
                is_single_connection: true,
                single_connection: None,
                uses_federation_syntax: false,
            };
        }
    };

    // Build a map from connection name to config for quick lookup
    let conn_map: HashMap<&str, &ConnectionConfig> = connections
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();

    // Walk through all statements and extract table references
    for stmt in &statements {
        extract_table_refs(stmt, &conn_map, &mut table_refs, &mut uses_federation_syntax);
    }

    // Deduplicate and order connections (exclude empty connection names from non-federated refs)
    let mut seen_conns = HashSet::new();
    for ref_ in &table_refs {
        if !ref_.connection_name.is_empty() && seen_conns.insert(ref_.connection_name.clone()) {
            connections_seen.push(ref_.connection_name.clone());
        }
    }

    let is_single_connection = connections_seen.len() <= 1;
    let single_connection = if is_single_connection && !connections_seen.is_empty() {
        Some(connections_seen[0].clone())
    } else {
        None
    };

    FederatedAnalysis {
        table_refs,
        connections: connections_seen,
        is_single_connection,
        single_connection,
        uses_federation_syntax,
    }
}

/// Extract table references from a statement, detecting federation patterns
fn extract_table_refs(
    stmt: &Statement,
    conn_map: &HashMap<&str, &ConnectionConfig>,
    table_refs: &mut Vec<FederatedTableRef>,
    uses_federation: &mut bool,
) {
    let _ = visit_relations(stmt, |name: &ObjectName| {
        let parts: Vec<&str> = name
            .0
            .iter()
            .filter_map(|p| match p {
                ObjectNamePart::Identifier(Ident { value, .. }) => Some(value.as_str()),
                _ => None,
            })
            .collect();

        // Check if this is a federated reference (has connection prefix)
        if parts.len() >= 3 {
            // 3 parts: connection.schema.table (PostgreSQL) or connection.database.table (MySQL)
            // 4 parts: connection.database.schema.table (full qualified)
            // Try to match the first part as a connection name
            let conn_name = parts[0];
            if conn_map.contains_key(conn_name) {
                *uses_federation = true;
                let (database_name, schema_name, table_name) = if parts.len() >= 4 {
                    // connection.database.schema.table
                    (Some(parts[1].to_string()), Some(parts[2].to_string()), parts[3].to_string())
                } else {
                    // connection.schema.table (3 parts)
                    (None, Some(parts[1].to_string()), parts[2].to_string())
                };

                table_refs.push(FederatedTableRef {
                    original_name: name.to_string(),
                    connection_name: conn_name.to_string(),
                    database_name,
                    schema_name,
                    table_name,
                    alias: None, // Will be filled from alias info
                });
                return ControlFlow::<()>::Continue(());
            }
        }

        // Non-federated reference - add with empty connection
        if parts.len() >= 2 {
            table_refs.push(FederatedTableRef {
                original_name: name.to_string(),
                connection_name: String::new(),
                database_name: None,
                schema_name: Some(parts[0].to_string()),
                table_name: parts[1].to_string(),
                alias: None,
            });
        } else if parts.len() == 1 {
            table_refs.push(FederatedTableRef {
                original_name: name.to_string(),
                connection_name: String::new(),
                database_name: None,
                schema_name: None,
                table_name: parts[0].to_string(),
                alias: None,
            });
        }
        ControlFlow::<()>::Continue(())
    });
}

/// Get the default schema for a database type
#[allow(dead_code)]
fn get_default_schema(db_type: &crate::models::connection::DatabaseType, database: &str) -> String {
    use crate::models::connection::DatabaseType as DT;
    match db_type {
        // PostgreSQL 系 — 默认 schema 为 "public"
        DT::Postgres | DT::Redshift | DT::Kingbase | DT::Highgo | DT::Uxdb
        | DT::Vastbase | DT::Gaussdb | DT::OpenGauss | DT::Kwdb | DT::Oscar => "public".to_string(),
        // MySQL 系 — schema 等同于 database 名
        DT::Mysql | DT::Doris | DT::StarRocks | DT::Goldendb | DT::Gbase
        | DT::ManticoreSearch | DT::Databend | DT::ClickHouse => database.to_string(),
        // SQL Server — 默认 schema 为 "dbo"
        DT::SqlServer => "dbo".to_string(),
        // Oracle 系 — 默认 schema 等同于用户名（此处用 database 代替）
        DT::Oracle | DT::OceanbaseOracle => database.to_string(),
        // DB2 — 默认 schema 等同于用户名
        DT::Db2 => database.to_string(),
        // 达梦 — 默认 schema 为 "SYSDBA"
        DT::Dameng => "SYSDBA".to_string(),
        _ => "public".to_string(),
    }
}

/// Get the list of visible schema names for a specific database in a connection.
/// Returns `None` if all schemas are visible (no restriction configured).
pub fn get_visible_schemas_for_database(config: &ConnectionConfig, database: &str) -> Option<Vec<String>> {
    config.visible_schemas.as_ref().and_then(|map| {
        map.get(database).cloned().or_else(|| {
            // If the specific database isn't listed, check if there's a wildcard entry
            map.get("*").cloned()
        })
    })
}

/// Rewrite SQL to remove connection prefixes for single-connection execution.
///
/// Uses AST-level rewriting via `visit_relations_mut` instead of fragile string
/// replacement. This correctly handles edge cases such as table names appearing
/// in string literals, comments, or as substrings of other identifiers.
pub fn rewrite_federated_sql(sql: &str, analysis: &FederatedAnalysis) -> Option<String> {
    if !analysis.uses_federation_syntax || !analysis.is_single_connection {
        return None;
    }

    let conn_name = analysis.single_connection.as_ref()?;

    // Build a map from original full name -> stripped name (without connection prefix)
    let mut rewrite_map: HashMap<String, Vec<ObjectNamePart>> = HashMap::new();
    for ref_ in &analysis.table_refs {
        if ref_.connection_name == *conn_name {
            let mut new_parts: Vec<ObjectNamePart> = Vec::new();
            if let Some(ref db) = ref_.database_name {
                new_parts.push(ObjectNamePart::Identifier(Ident::new(db)));
            }
            if let Some(ref schema) = ref_.schema_name {
                new_parts.push(ObjectNamePart::Identifier(Ident::new(schema)));
            }
            new_parts.push(ObjectNamePart::Identifier(Ident::new(&ref_.table_name)));
            rewrite_map.insert(ref_.original_name.clone(), new_parts);
        }
    }

    if rewrite_map.is_empty() {
        return None;
    }

    // Parse the SQL and rewrite at AST level
    let dialect = PostgreSqlDialect {};
    let mut statements = match Parser::parse_sql(&dialect, sql) {
        Ok(stmts) => stmts,
        Err(_) => return None,
    };

    let _ = visit_relations_mut(&mut statements, |name: &mut ObjectName| {
        let name_str = name.to_string();
        if let Some(new_parts) = rewrite_map.get(&name_str) {
            name.0 = new_parts.clone();
        }
        ControlFlow::<()>::Continue(())
    });

    let rewritten: Vec<String> = statements.iter().map(|s| s.to_string()).collect();
    Some(rewritten.join("; "))
}

/// Validation error for federated queries
#[derive(Debug, Clone)]
pub enum FederationValidationError {
    /// The connection does not have federation enabled
    FederationNotEnabled(String),
    /// The referenced schema is not in the visible schemas list
    SchemaNotVisible { connection: String, schema: String },
}

impl std::fmt::Display for FederationValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FederationValidationError::FederationNotEnabled(conn) => {
                write!(f, "Connection '{}' does not have federated query enabled. Enable it in the connection settings.", conn)
            }
            FederationValidationError::SchemaNotVisible { connection, schema } => {
                write!(f, "Schema '{}' is not visible for connection '{}'. Check the connection's visible schemas configuration.", schema, connection)
            }
        }
    }
}

/// Validate that federated query references are allowed.
///
/// Checks:
/// 1. Each referenced connection has `federation_enabled` set to true
/// 2. Each referenced schema is in the connection's visible schemas (if configured)
pub fn validate_federation(
    analysis: &FederatedAnalysis,
    connections: &[ConnectionConfig],
) -> Result<(), FederationValidationError> {
    let conn_map: HashMap<&str, &ConnectionConfig> = connections
        .iter()
        .map(|c| (c.name.as_str(), c))
        .collect();

    for ref_ in &analysis.table_refs {
        // Skip non-federated references (no connection name)
        if ref_.connection_name.is_empty() {
            continue;
        }

        let config = match conn_map.get(ref_.connection_name.as_str()) {
            Some(c) => *c,
            None => continue, // Unknown connection - will be caught later during execution
        };

        // Check federation_enabled flag
        if !config.federation_enabled {
            return Err(FederationValidationError::FederationNotEnabled(
                ref_.connection_name.clone(),
            ));
        }

        // Check schema visibility if configured
        if let Some(ref schema_name) = ref_.schema_name {
            let database = config.database.as_deref().unwrap_or("");
            if let Some(visible) = get_visible_schemas_for_database(config, database) {
                if !visible.iter().any(|s| s == schema_name) {
                    return Err(FederationValidationError::SchemaNotVisible {
                        connection: ref_.connection_name.clone(),
                        schema: schema_name.clone(),
                    });
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::connection::DatabaseType;

    fn make_test_connection(name: &str, db_type: DatabaseType, database: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: name.to_string(),
            name: name.to_string(),
            note: String::new(),
            db_type,
            driver_profile: None,
            driver_label: None,
            url_params: None,
            agent_java_options: Vec::new(),
            host: "localhost".to_string(),
            port: 5432,
            username: "test".to_string(),
            password: "test".to_string(),
            database: Some(database.to_string()),
            visible_databases: None,
            visible_schemas: None,
            show_system_schemas: false,
            attached_databases: Vec::new(),
            init_script: None,
            color: None,
            transport_layers: Vec::new(),
            connect_timeout_secs: 30,
            query_timeout_secs: 300,
            idle_timeout_secs: 600,
            keepalive_interval_secs: 30,
            ssl: false,
            ca_cert_path: String::new(),
            client_cert_path: String::new(),
            client_key_path: String::new(),
            sysdba: false,
            oracle_connection_type: None,
            connection_string: None,
            redis_connection_mode: None,
            redis_sentinel_master: String::new(),
            redis_sentinel_nodes: String::new(),
            redis_sentinel_username: String::new(),
            redis_sentinel_password: String::new(),
            redis_sentinel_tls: false,
            redis_cluster_nodes: String::new(),
            redis_key_separator: String::new(),
            redis_scan_page_size: None,
            redis_database_aliases: HashMap::new(),
            etcd_endpoints: String::new(),
            gbase_server: String::new(),
            informix_server: String::new(),
            external_config: None,
            jdbc_driver_class: None,
            jdbc_driver_paths: Vec::new(),
            one_time: false,
            read_only: false,
            is_production: false,
            production_databases: Vec::new(),
            database_info: None,
            federation_enabled: true,
        }
    }

    #[test]
    fn test_single_connection_federation() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM my_pg.public.users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(analysis.uses_federation_syntax);
        assert!(analysis.is_single_connection);
        assert_eq!(analysis.single_connection, Some("my_pg".to_string()));
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].connection_name, "my_pg");
        assert_eq!(analysis.table_refs[0].schema_name, Some("public".to_string()));
        assert_eq!(analysis.table_refs[0].table_name, "users");
    }

    #[test]
    fn test_multi_connection_federation() {
        let pg_conn = make_test_connection("pg_db", DatabaseType::Postgres, "analytics");
        let mysql_conn = make_test_connection("mysql_db", DatabaseType::Mysql, "shop");

        let sql = "SELECT p.name, o.total FROM pg_db.public.products p JOIN mysql_db.shop.orders o ON p.id = o.product_id";

        let analysis = analyze_federation(sql, &[pg_conn.clone(), mysql_conn.clone()]);

        assert!(analysis.uses_federation_syntax);
        assert!(!analysis.is_single_connection);
        assert_eq!(analysis.connections.len(), 2);
        assert_eq!(analysis.table_refs.len(), 2);
    }

    #[test]
    fn test_four_part_name_rewrite() {
        // Test 4-part name: connection.database.schema.table
        let conn = make_test_connection("postgresql", DatabaseType::Postgres, "ihrcore");
        let sql = r#"SELECT "id", "connection_name", "db_type" FROM postgresql.ihrcore."public"."database_connection""#;

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(analysis.uses_federation_syntax);
        assert!(analysis.is_single_connection);
        assert_eq!(analysis.single_connection, Some("postgresql".to_string()));
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].connection_name, "postgresql");
        assert_eq!(analysis.table_refs[0].database_name, Some("ihrcore".to_string()));
        assert_eq!(analysis.table_refs[0].schema_name, Some("public".to_string()));
        assert_eq!(analysis.table_refs[0].table_name, "database_connection");

        // Verify rewrite removes connection prefix but keeps database.schema.table
        if let Some(rewritten) = rewrite_federated_sql(sql, &analysis) {
            assert!(rewritten.contains("ihrcore.public.database_connection"));
            assert!(!rewritten.contains("postgresql."));
        } else {
            panic!("Expected successful rewrite for 4-part name");
        }
    }

    #[test]
    fn test_non_federated_query() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(!analysis.uses_federation_syntax);
        assert!(analysis.is_single_connection);
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].connection_name, "");
    }

    #[test]
    fn test_rewrite_single_connection_sql() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM my_pg.public.users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        let rewritten = rewrite_federated_sql(sql, &analysis);

        assert!(rewritten.is_some());
        assert_eq!(rewritten.unwrap(), "SELECT * FROM public.users WHERE id = 1");
    }
}

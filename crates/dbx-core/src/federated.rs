//! Federated query resolution: parse SQL AST to map table references to database connections.
//!
//! This module implements the core federation logic:
//! 1. Parse SQL using sqlparser to extract table references
//! 2. Detect naming conventions:
//!    - 3-part: `connection.database.table` (database is required, schema auto-detected from dialect)
//!    - 4-part: `connection.database.schema.table` (full qualified)
//! 3. Map each table reference to its owning connection
//! 4. Determine if single-connection fast path or multi-connection Calcite path is needed

use std::collections::{HashMap, HashSet};
use std::ops::ControlFlow;

use regex::Regex;
use sqlparser::ast::{visit_relations, visit_relations_mut, Ident, ObjectName, ObjectNamePart, Statement};
use sqlparser::dialect::GenericDialect;
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
    /// Whether the query uses explicit federation syntax (connection.database.table)
    pub uses_federation_syntax: bool,
}

/// Resolve a connection by name, preferring an exact (case-sensitive) match and
/// falling back to a case-insensitive lookup. Exact priority avoids silently
/// shadowing two connections whose names differ only by case.
fn resolve_connection<'a>(
    exact_map: &HashMap<&str, &'a ConnectionConfig>,
    insensitive_map: &HashMap<String, &'a ConnectionConfig>,
    name: &str,
) -> Option<&'a ConnectionConfig> {
    exact_map.get(name).copied().or_else(|| insensitive_map.get(&name.to_lowercase()).copied())
}

/// Check if a name contains characters that make it invalid as an unquoted
/// SQL identifier (i.e., anything other than letters, digits, and underscores,
/// or starting with a digit).
fn needs_quoting(name: &str) -> bool {
    let first = name.chars().next();
    let valid_start = first.is_some_and(|c| c.is_alphabetic() || c == '_');
    let valid_chars = name.chars().all(|c| c.is_alphanumeric() || c == '_');
    !valid_start || !valid_chars
}

/// Validate a connection name to ensure it can be safely used as an unquoted
/// SQL identifier in federated queries. Returns `Err` with a descriptive
/// message if the name contains characters that would cause SQL parsing errors
/// (e.g., hyphens, spaces, dots) or starts with a digit.
///
/// Empty names are allowed — they are handled by auto-generation logic elsewhere.
/// Call this when creating or editing a connection to provide early feedback
/// and prevent SQL syntax errors at query time.
pub fn validate_connection_name(name: &str) -> Result<(), String> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Ok(());
    }
    if needs_quoting(trimmed) {
        return Err(format!(
            "Connection name '{}' contains characters that are invalid in SQL identifiers. \
             Only letters, digits, and underscores are allowed, and the name must not start with a digit.",
            trimmed
        ));
    }
    Ok(())
}

/// Preprocess SQL to quote connection names that contain special characters
/// (e.g., hyphens) so the SQL parser can correctly identify them as identifier
/// parts in multi-part table references.
///
/// For example, if a connection is named "doris-Local", the user's SQL
/// `FROM doris-Local.freequery.DIM_BM_AD_PS` is rewritten to
/// `FROM "doris-Local".freequery.DIM_BM_AD_PS` before parsing.
///
/// This is necessary because the SQL parser interprets `doris-Local` as the
/// arithmetic expression `doris - Local`, not as a single identifier.
fn preprocess_federated_sql(sql: &str, connection_names: &[&str]) -> String {
    let special_names: Vec<&str> = connection_names.iter().copied().filter(|n| needs_quoting(n)).collect();
    if special_names.is_empty() {
        return sql.to_string();
    }
    let mut result = sql.to_string();
    for name in &special_names {
        let escaped = regex::escape(name);
        // Match the connection name when preceded by a non-identifier character
        // (or start of string) and followed by a dot. Case-insensitive to handle
        // different casing in user SQL. The preceding character is captured so
        // it can be reinserted in the replacement.
        let pattern = format!(r#"(?i)(^|[^A-Za-z0-9_`"])({})\."#, escaped);
        if let Ok(re) = Regex::new(&pattern) {
            result = re.replace_all(&result, r#"$1"$2"."#).to_string();
        }
    }
    result
}

/// Fix malformed table references that start with a leading dot (e.g., ".store_sales" -> "store_sales").
/// This handles cases where the frontend generates SQL with an extra leading dot before the table name.
fn fix_leading_dot_table_refs(sql: &str) -> String {
    // Match a dot followed by an identifier where the dot is preceded by whitespace, comma, FROM, JOIN, ON, etc.
    // Replace ".table_name" with "table_name" (strip the leading dot).
    let pattern = regex::Regex::new(r"(?i)([\s,(])(\.[A-Za-z_]\w*)").unwrap();
    pattern
        .replace_all(sql, |caps: &regex::Captures| {
            let prefix = &caps[1];
            let dotted = &caps[2];
            // Strip the leading dot: ".store_sales" -> "store_sales"
            format!("{}{}", prefix, &dotted[1..])
        })
        .to_string()
}

/// Parse SQL and detect federation patterns
pub fn analyze_federation(sql: &str, connections: &[ConnectionConfig]) -> FederatedAnalysis {
    let mut table_refs = Vec::new();
    let mut connections_seen: Vec<String> = Vec::new();
    let mut uses_federation_syntax = false;

    // Preprocess SQL to quote connection names containing special characters
    // (e.g., hyphens) so the parser can correctly identify them as identifiers.
    let connection_names: Vec<&str> = connections.iter().map(|c| c.name.as_str()).collect();
    let preprocessed_sql = preprocess_federated_sql(sql, &connection_names);
    // Fix malformed table references with leading dots (e.g., ".store_sales" -> "store_sales")
    let fixed_sql = fix_leading_dot_table_refs(&preprocessed_sql);

    log::debug!(
        "[federated] analyze_federation: sql_len={}, connection_count={}, connections=[{}], sql={}",
        sql.len(),
        connections.len(),
        connection_names.join(", "),
        sql
    );

    // Parse the SQL
    let dialect = GenericDialect {};
    let statements = match Parser::parse_sql(&dialect, &fixed_sql) {
        Ok(stmts) => stmts,
        Err(e) => {
            log::debug!("[federated] SQL parse failed: {e}, sql={}", sql);
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

    log::debug!("[federated] Parsed {} statements", statements.len());

    // Build maps for connection lookup. Exact names take priority; a
    // case-insensitive map is the fallback so `postgresql` matches a connection
    // named `PostgreSQL`. Keeping both avoids silently shadowing two connections
    // whose names differ only by case (e.g. "MyDB" vs "mydb").
    let exact_map: HashMap<&str, &ConnectionConfig> = connections.iter().map(|c| (c.name.as_str(), c)).collect();
    let insensitive_map: HashMap<String, &ConnectionConfig> =
        connections.iter().map(|c| (c.name.to_lowercase(), c)).collect();

    // Walk through all statements and extract table references
    for stmt in &statements {
        extract_table_refs(stmt, &exact_map, &insensitive_map, &mut table_refs, &mut uses_federation_syntax);
    }

    log::debug!(
        "[federated] Extracted {} table refs, federation_syntax={}, refs=[{:?}]",
        table_refs.len(),
        uses_federation_syntax,
        table_refs
    );

    // Deduplicate and order connections (exclude empty connection names from non-federated refs)
    let mut seen_conns = HashSet::new();
    for ref_ in &table_refs {
        if !ref_.connection_name.is_empty() && seen_conns.insert(ref_.connection_name.clone()) {
            connections_seen.push(ref_.connection_name.clone());
        }
    }

    let is_single_connection = connections_seen.len() <= 1;
    let single_connection =
        if is_single_connection && !connections_seen.is_empty() { Some(connections_seen[0].clone()) } else { None };

    log::debug!(
        "[federated] Result: is_single_connection={}, single_connection={:?}, connections_seen={:?}",
        is_single_connection,
        single_connection,
        connections_seen
    );

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
    exact_map: &HashMap<&str, &ConnectionConfig>,
    insensitive_map: &HashMap<String, &ConnectionConfig>,
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
            // 3 parts: connection.database.table (database is required)
            // 4 parts: connection.database.schema.table (full qualified)
            // Try to match the first part as a connection name (exact first, then case-insensitive)
            let conn_name = parts[0];
            if let Some(config) = resolve_connection(exact_map, insensitive_map, conn_name) {
                *uses_federation = true;
                let (database_name, schema_name, table_name) = if parts.len() >= 4 {
                    // connection.database.schema.table
                    (Some(parts[1].to_string()), Some(parts[2].to_string()), parts[3].to_string())
                } else {
                    // connection.database.table (3 parts)
                    (Some(parts[1].to_string()), None, parts[2].to_string())
                };

                // Use the canonical connection name from the config so downstream
                // matching (single_connection / rewrite / validation) is consistent.
                table_refs.push(FederatedTableRef {
                    original_name: name.to_string(),
                    connection_name: config.name.clone(),
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
fn get_default_schema(db_type: &crate::models::connection::DatabaseType, database: &str) -> String {
    use crate::models::connection::DatabaseType as DT;
    match db_type {
        // PostgreSQL 系 — 默认 schema 为 "public"
        DT::Postgres
        | DT::Redshift
        | DT::Kingbase
        | DT::Highgo
        | DT::Uxdb
        | DT::Vastbase
        | DT::Gaussdb
        | DT::OpenGauss
        | DT::Kwdb
        | DT::Oscar
        | DT::Vertica
        | DT::Questdb => "public".to_string(),
        // MySQL 系 + ClickHouse/Databend — schema 等同于 database 名
        DT::Mysql
        | DT::Doris
        | DT::StarRocks
        | DT::Goldendb
        | DT::Gbase
        | DT::ManticoreSearch
        | DT::Databend
        | DT::ClickHouse
        | DT::Tdengine
        | DT::InfluxDb
        | DT::VictoriaMetrics => database.to_string(),
        // Snowflake/Teradata/Exasol/Firebird — 有独立 schema，默认 PUBLIC
        DT::Snowflake => "PUBLIC".to_string(),
        DT::Teradata | DT::Exasol | DT::Firebird => database.to_string(),
        // SQL Server — 默认 schema 为 "dbo"
        DT::SqlServer => "dbo".to_string(),
        // Oracle 系 — 默认 schema 为用户名，用 database 占位
        DT::Oracle | DT::OceanbaseOracle => database.to_string(),
        // DB2 — 默认 schema 为用户名（授权 ID）
        DT::Db2 => database.to_string(),
        // 达梦 — 默认 schema 为用户的默认模式
        DT::Dameng => database.to_string(),
        // Hive/Spark/Kyuubi/Impala/Databricks — catalog.schema.table，schema 段保留
        DT::Hive | DT::PrestoSql | DT::Trino | DT::Spark | DT::Kyuubi | DT::Impala | DT::Databricks => {
            database.to_string()
        }
        // Kylin/Sundb — 特殊国产库，默认 schema
        DT::Kylin | DT::Sundb => "PUBLIC".to_string(),
        // SQLite 系 — 默认 schema 为 "main"
        DT::Sqlite | DT::Rqlite | DT::Turso | DT::CloudflareD1 => "main".to_string(),
        // Neo4j/Cassandra/BigQuery — 无 schema 概念
        DT::Neo4j | DT::Cassandra | DT::Bigquery => database.to_string(),
        // H2/Informix/Xugu/Yashandb — PG 兼容，默认 public
        DT::H2 | DT::Informix | DT::Xugu | DT::Yashandb => "public".to_string(),
        // 兜底
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
pub fn rewrite_federated_sql(
    sql: &str,
    analysis: &FederatedAnalysis,
    connections: &[ConnectionConfig],
) -> Option<String> {
    if !analysis.uses_federation_syntax || !analysis.is_single_connection {
        return None;
    }

    let conn_name = analysis.single_connection.as_ref()?;
    // 找到被引用的连接配置，用于判断 database 段是否只是连接的实际数据库名
    let conn_config = connections.iter().find(|c| c.name == *conn_name)?;
    let conn_database = conn_config.database.as_deref().unwrap_or("");
    let db_type = &conn_config.db_type;

    // Build a map from original full name -> stripped name (without connection prefix)
    let mut rewrite_map: HashMap<String, Vec<ObjectNamePart>> = HashMap::new();
    for ref_ in &analysis.table_refs {
        if ref_.connection_name == *conn_name {
            let database_matches_conn =
                ref_.database_name.as_deref().map_or(false, |db| db.eq_ignore_ascii_case(conn_database));
            let mut new_parts: Vec<ObjectNamePart> = Vec::new();

            if database_matches_conn {
                // database 段只是连接的实际数据库名（如 pgLocal.tpcds.item 中的 tpcds），
                // 对 PostgreSQL 系它并非 schema。丢弃该段：
                // - 4 段式 connection.database.schema.table：保留 schema 段
                // - 3 段式 connection.database.table：退化为 default schema，如 public.item
                //
                // 特殊处理：Oracle/DB2 的 database 本身就是默认 schema，
                // 4 段式需要保留 default_schema 作为前缀（如 hr.MY_SCHEMA.users）
                let is_oracle_like = matches!(
                    db_type,
                    crate::models::connection::DatabaseType::Oracle
                        | crate::models::connection::DatabaseType::OceanbaseOracle
                );
                if let Some(ref schema) = ref_.schema_name {
                    if is_oracle_like {
                        // Oracle/DB2: database = default_schema，需保留前缀
                        let default_schema = conn_config
                            .default_schema
                            .clone()
                            .unwrap_or_else(|| get_default_schema(db_type, conn_database));
                        new_parts.push(ObjectNamePart::Identifier(Ident::new(&default_schema)));
                    }
                    new_parts.push(ObjectNamePart::Identifier(Ident::new(schema)));
                } else {
                    let default_schema = conn_config
                        .default_schema
                        .clone()
                        .unwrap_or_else(|| get_default_schema(db_type, conn_database));
                    new_parts.push(ObjectNamePart::Identifier(Ident::new(&default_schema)));
                }
            } else {
                // database 段是真实的 schema/database 名，原样保留
                if let Some(ref db) = ref_.database_name {
                    new_parts.push(ObjectNamePart::Identifier(Ident::new(db)));
                }
                if let Some(ref schema) = ref_.schema_name {
                    new_parts.push(ObjectNamePart::Identifier(Ident::new(schema)));
                }
            }

            new_parts.push(ObjectNamePart::Identifier(Ident::new(&ref_.table_name)));
            rewrite_map.insert(ref_.original_name.clone(), new_parts);
        }
    }

    if rewrite_map.is_empty() {
        return None;
    }

    // Parse the SQL and rewrite at AST level
    // Preprocess SQL to quote connection names with special characters,
    // matching the preprocessing done in analyze_federation.
    let connection_names: Vec<&str> = analysis.connections.iter().map(|s| s.as_str()).collect();
    let preprocessed_sql = preprocess_federated_sql(sql, &connection_names);
    let fixed_sql = fix_leading_dot_table_refs(&preprocessed_sql);
    let dialect = GenericDialect {};
    let mut statements = match Parser::parse_sql(&dialect, &fixed_sql) {
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
                write!(
                    f,
                    "Connection '{}' does not have federated query enabled. Enable it in the connection settings.",
                    conn
                )
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
    // Exact names take priority; case-insensitive lookup is the fallback,
    // matching `analyze_federation`.
    let exact_map: HashMap<&str, &ConnectionConfig> = connections.iter().map(|c| (c.name.as_str(), c)).collect();
    let insensitive_map: HashMap<String, &ConnectionConfig> =
        connections.iter().map(|c| (c.name.to_lowercase(), c)).collect();

    for ref_ in &analysis.table_refs {
        // Skip non-federated references (no connection name)
        if ref_.connection_name.is_empty() {
            continue;
        }

        // Exact-then-insensitive lookup matching analyze_federation
        let config = match resolve_connection(&exact_map, &insensitive_map, &ref_.connection_name) {
            Some(c) => c,
            None => continue, // Unknown connection - will be caught later during execution
        };

        // Check federation_enabled flag
        if !config.federation_enabled {
            return Err(FederationValidationError::FederationNotEnabled(ref_.connection_name.clone()));
        }

        // Check schema visibility if configured
        if let Some(ref schema_name) = ref_.schema_name {
            // Use the actual database from the reference, not the connection's default database
            let database = ref_.database_name.as_deref().unwrap_or(config.database.as_deref().unwrap_or(""));
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
            save_password: false,
            read_only: false,
            is_production: false,
            production_databases: Vec::new(),
            database_info: None,
            federation_enabled: true,
            default_schema: None,
            docs_notes_path: None,
        }
    }

    #[test]
    fn test_case_insensitive_connection_name() {
        // Connection is named "PostgreSQL" but SQL references it as lowercase "postgresql".
        let conn = make_test_connection("PostgreSQL", DatabaseType::Postgres, "mydb");
        let sql = "SELECT u.id FROM postgresql.public.users u WHERE u.active = true";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(analysis.uses_federation_syntax, "should match connection case-insensitively");
        assert!(analysis.is_single_connection);
        // Canonical (config) connection name is used downstream.
        assert_eq!(analysis.single_connection, Some("PostgreSQL".to_string()));
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].connection_name, "PostgreSQL");
        // 3-part: connection.database.table — parts[1] is the database name
        assert_eq!(analysis.table_refs[0].database_name, Some("public".to_string()));
        assert_eq!(analysis.table_refs[0].schema_name, None);
        assert_eq!(analysis.table_refs[0].table_name, "users");

        // Rewrite must strip the lowercase prefix and produce the target table ref.
        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        assert!(
            rewritten.contains("public.users") && !rewritten.contains("postgresql"),
            "rewritten SQL should drop the connection prefix, got: {rewritten}"
        );
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
        // 3-part: connection.database.table — parts[1] is now the database name
        assert_eq!(analysis.table_refs[0].database_name, Some("public".to_string()));
        assert_eq!(analysis.table_refs[0].schema_name, None);
        assert_eq!(analysis.table_refs[0].table_name, "users");
    }

    #[test]
    fn test_multi_connection_federation() {
        let pg_conn = make_test_connection("pg_db", DatabaseType::Postgres, "analytics");
        let mysql_conn = make_test_connection("mysql_db", DatabaseType::Mysql, "shop");

        let sql =
            "SELECT p.name, o.total FROM pg_db.public.products p JOIN mysql_db.shop.orders o ON p.id = o.product_id";

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

        // Verify rewrite removes connection prefix and the matching database part.
        // database 段 (ihrcore) 匹配连接的数据库名，因此被丢弃，退化为 schema: public
        if let Some(rewritten) = rewrite_federated_sql(sql, &analysis, &[conn.clone()]) {
            assert!(rewritten.contains("public.database_connection"));
            assert!(!rewritten.contains("postgresql."));
            assert!(!rewritten.contains("ihrcore."));
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
        // 3-part: connection.database.table
        let sql = "SELECT * FROM my_pg.public.users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]);

        assert!(rewritten.is_some());
        // 3-part strips connection only, keeps database.table
        assert_eq!(rewritten.unwrap(), "SELECT * FROM public.users WHERE id = 1");
    }

    #[test]
    fn test_validate_federation_case_insensitive() {
        // Connection is named "PostgreSQL" but SQL references it as lowercase
        let conn = make_test_connection("PostgreSQL", DatabaseType::Postgres, "mydb");
        let sql = "SELECT u.id FROM postgresql.public.users u";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax);
        assert_eq!(analysis.single_connection, Some("PostgreSQL".to_string()));

        // Validation should pass since canonical name matches
        let result = validate_federation(&analysis, &[conn.clone()]);
        assert!(result.is_ok(), "Validation should succeed for case-insensitive match");
    }

    #[test]
    fn test_validate_federation_disabled_connection() {
        let mut conn = make_test_connection("my_db", DatabaseType::Postgres, "mydb");
        conn.federation_enabled = false;

        let sql = "SELECT * FROM my_db.public.users";
        let analysis = analyze_federation(sql, &[conn.clone()]);

        let result = validate_federation(&analysis, &[conn.clone()]);
        assert!(result.is_err());
        match result.unwrap_err() {
            FederationValidationError::FederationNotEnabled(name) => {
                assert_eq!(name, "my_db");
            }
            _ => panic!("Expected FederationNotEnabled error"),
        }
    }

    #[test]
    fn test_nonexistent_connection_not_matched() {
        // Connection named "MyDB" but SQL uses "nonexistent"
        let conn = make_test_connection("MyDB", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM nonexistent.public.users";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        // Should NOT match as federation since connection doesn't exist
        assert!(!analysis.uses_federation_syntax);
        assert!(analysis.table_refs.is_empty() || analysis.table_refs[0].connection_name.is_empty());
    }

    #[test]
    fn test_4_part_name_rewrite_with_alias() {
        let conn = make_test_connection("postgresql", DatabaseType::Postgres, "ihrcore");
        let sql = r#"SELECT u.id FROM postgresql.ihrcore."public"."users" u"#;

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax);
        assert_eq!(analysis.table_refs[0].database_name, Some("ihrcore".to_string()));
        assert_eq!(analysis.table_refs[0].schema_name, Some("public".to_string()));
        assert_eq!(analysis.table_refs[0].table_name, "users");

        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        // database 段 (ihrcore) 匹配连接的数据库名，被丢弃，退化为 schema: public
        assert!(rewritten.contains("public.users"));
        assert!(!rewritten.contains("postgresql."));
        assert!(!rewritten.contains("ihrcore."));
        assert!(rewritten.contains("u"));
    }

    #[test]
    fn test_hyphenated_connection_name() {
        // Connection name contains a hyphen, which is invalid as an unquoted
        // SQL identifier. The preprocessor should quote it before parsing.
        let conn = make_test_connection("doris-Local", DatabaseType::Doris, "freequery");
        let sql = "SELECT `BM0000`, `MC0000` FROM doris-Local.freequery.DIM_BM_AD_PS";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax, "should detect hyphenated connection name");
        assert!(analysis.is_single_connection);
        assert_eq!(analysis.single_connection, Some("doris-Local".to_string()));
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].connection_name, "doris-Local");
        // 3-part: connection.database.table — parts[1] is the database name
        assert_eq!(analysis.table_refs[0].database_name, Some("freequery".to_string()));
        assert_eq!(analysis.table_refs[0].schema_name, None);
        assert_eq!(analysis.table_refs[0].table_name, "DIM_BM_AD_PS");

        // Rewrite must strip the hyphenated connection prefix entirely.
        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        assert!(
            rewritten.contains("freequery.DIM_BM_AD_PS") && !rewritten.contains("doris-Local"),
            "rewritten SQL should drop the hyphenated connection prefix, got: {rewritten}"
        );
    }

    #[test]
    fn test_hyphenated_connection_name_case_insensitive() {
        // User writes connection name in different case than the config.
        let conn = make_test_connection("doris-Local", DatabaseType::Doris, "freequery");
        let sql = "SELECT * FROM doris-local.freequery.DIM_BM_AD_PS";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax, "should match case-insensitively");
        assert_eq!(analysis.single_connection, Some("doris-Local".to_string()));

        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        assert!(
            !rewritten.contains("doris-local") && !rewritten.contains("doris-Local"),
            "rewritten SQL should drop the connection prefix, got: {rewritten}"
        );
    }

    #[test]
    fn test_normal_connection_name_unaffected_by_preprocessing() {
        // Connection names without special characters should work as before.
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM my_pg.public.users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax);
        assert_eq!(analysis.single_connection, Some("my_pg".to_string()));

        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        assert_eq!(rewritten, "SELECT * FROM public.users WHERE id = 1");
    }

    #[test]
    fn test_fix_leading_dot_table_ref() {
        // Test that malformed table references with leading dots are fixed.
        let sql = "SELECT * FROM .store_sales s JOIN mySQLocal.tpcds.item i ON s.ss_item_sk = i.i_item_sk";
        let fixed = fix_leading_dot_table_refs(sql);
        assert_eq!(fixed, "SELECT * FROM store_sales s JOIN mySQLocal.tpcds.item i ON s.ss_item_sk = i.i_item_sk");
    }

    #[test]
    fn test_federation_with_leading_dot_table_ref() {
        // Test that federation analysis works even when one table has a leading dot.
        let conn = make_test_connection("mySQLocal", DatabaseType::Mysql, "tpcds");
        let sql = "SELECT * FROM .store_sales s JOIN mySQLocal.tpcds.item i ON s.ss_item_sk = i.i_item_sk";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        // Should detect the federation pattern from mySQLocal.tpcds.item
        assert!(analysis.uses_federation_syntax, "should detect federation from 3-part reference");

        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]);
        assert!(rewritten.is_some(), "rewrite should succeed");
        let result = rewritten.unwrap();
        // The leading dot should be stripped
        assert!(!result.contains(".store_sales"), "leading dot should be stripped, got: {result}");
        // The federation prefix should be stripped
        assert!(!result.contains("mySQLocal."), "connection prefix should be stripped, got: {result}");
    }

    #[test]
    fn test_default_schema_by_db_type() {
        use super::get_default_schema;
        use crate::models::connection::DatabaseType as DT;

        // PostgreSQL family → "public"
        assert_eq!(get_default_schema(&DT::Postgres, "mydb"), "public");
        assert_eq!(get_default_schema(&DT::Redshift, "mydb"), "public");
        assert_eq!(get_default_schema(&DT::Gaussdb, "mydb"), "public");

        // MySQL family → database name
        assert_eq!(get_default_schema(&DT::Mysql, "shop"), "shop");
        assert_eq!(get_default_schema(&DT::Doris, "freequery"), "freequery");
        assert_eq!(get_default_schema(&DT::ClickHouse, "mydb"), "mydb");

        // SQL Server → "dbo"
        assert_eq!(get_default_schema(&DT::SqlServer, "mydb"), "dbo");

        // Oracle family → database name
        assert_eq!(get_default_schema(&DT::Oracle, "hr"), "hr");

        // DB2 → database name
        assert_eq!(get_default_schema(&DT::Db2, "mydb"), "mydb");

        // Dameng → database name (用户默认 schema)
        assert_eq!(get_default_schema(&DT::Dameng, "mydb"), "mydb");

        // Hive/Presto/Trino/Spark → database name（保留真实库名）
        assert_eq!(get_default_schema(&DT::Hive, "mydb"), "mydb");
        assert_eq!(get_default_schema(&DT::Trino, "mydb"), "mydb");
        assert_eq!(get_default_schema(&DT::Spark, "mydb"), "mydb");
        // Kylin/Sundb → PUBLIC
        assert_eq!(get_default_schema(&DT::Kylin, "mydb"), "PUBLIC");
        assert_eq!(get_default_schema(&DT::Sundb, "mydb"), "PUBLIC");

        // Snowflake → PUBLIC, Teradata/Exasol → database name
        assert_eq!(get_default_schema(&DT::Snowflake, "mydb"), "PUBLIC");
        assert_eq!(get_default_schema(&DT::Teradata, "mydb"), "mydb");
        assert_eq!(get_default_schema(&DT::Exasol, "mydb"), "mydb");

        // Rqlite/Turso → "main"
        assert_eq!(get_default_schema(&DT::Rqlite, "mydb"), "main");
        assert_eq!(get_default_schema(&DT::Turso, "mydb"), "main");

        // Neo4j/Cassandra/BigQuery → database name
        assert_eq!(get_default_schema(&DT::Neo4j, "graph"), "graph");
        assert_eq!(get_default_schema(&DT::Cassandra, "ks"), "ks");
        assert_eq!(get_default_schema(&DT::Bigquery, "project"), "project");

        // Unknown/other → type-specific default
        assert_eq!(get_default_schema(&DT::Sqlite, "test"), "main");
    }

    #[test]
    fn test_hive_single_connection_rewrite() {
        // Hive: database 段匹配连接数据库名时，保留库名
        let conn = make_test_connection("hiveLocal", DatabaseType::Hive, "mydb");
        let sql = "SELECT * FROM hiveLocal.mydb.events WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax);
        assert!(analysis.is_single_connection);

        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        // database 段 "mydb" 匹配连接数据库名，保留为 mydb.events（Hive 无 schema 概念）
        assert_eq!(rewritten, "SELECT * FROM mydb.events WHERE id = 1");
    }

    #[test]
    fn test_snowflake_single_connection_rewrite() {
        // Snowflake: database 段匹配连接数据库名时，退化为 PUBLIC schema
        let conn = make_test_connection("sfLocal", DatabaseType::Snowflake, "mydb");
        let sql = "SELECT * FROM sfLocal.mydb.users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax);
        assert!(analysis.is_single_connection);

        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        // database 段 "mydb" 匹配连接数据库名，退化为默认 schema "PUBLIC"
        assert_eq!(rewritten, "SELECT * FROM PUBLIC.users WHERE id = 1");
    }

    #[test]
    fn test_4part_pg_public_schema_rewrite() {
        // PostgreSQL 4-part: connection.database.public.table → connection.table
        // 这是多连接联邦查询超时问题的根因修复验证
        let pg_conn = make_test_connection("pgLocal", DatabaseType::Postgres, "tpcds");
        let sql = "SELECT i.i_item_desc FROM pgLocal.tpcds.public.item i";

        let analysis = analyze_federation(sql, &[pg_conn.clone()]);
        assert!(analysis.uses_federation_syntax);
        assert!(analysis.is_single_connection);

        let rewritten = rewrite_federated_sql(sql, &analysis, &[pg_conn.clone()]).expect("rewrite should succeed");
        // 3 段式，database_name=Some("tpcds"), schema_name=None
        // 因为 database==config.database，应退化为 public.item
        assert_eq!(rewritten, "SELECT i.i_item_desc FROM public.item i");
    }

    #[test]
    fn test_4part_named_schema_retained() {
        // PostgreSQL 4-part with non-default schema: should retain schema
        let pg_conn = make_test_connection("pgLocal", DatabaseType::Postgres, "tpcds");
        let sql = "SELECT * FROM pgLocal.tpcds.myschema.item i";

        let analysis = analyze_federation(sql, &[pg_conn.clone()]);
        assert!(analysis.uses_federation_syntax);
        assert!(analysis.is_single_connection);

        let rewritten = rewrite_federated_sql(sql, &analysis, &[pg_conn.clone()]).expect("rewrite should succeed");
        // database==tpcds 匹配，schema==myschema 保留
        assert_eq!(rewritten, "SELECT * FROM myschema.item i");
    }

    #[test]
    fn test_dameng_default_schema() {
        // Dameng: default schema should be user's default, not hardcoded SYSDBA
        let conn = make_test_connection("dmLocal", DatabaseType::Dameng, "DMHR");
        let sql = "SELECT * FROM dmLocal.DMHR.users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax);

        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        // database 段匹配，退化为 default schema = config.database = "DMHR"
        assert_eq!(rewritten, "SELECT * FROM DMHR.users WHERE id = 1");
    }

    #[test]
    fn test_db2_not_oracle_like() {
        // DB2 should NOT be treated as oracle-like (no 3-segment prefix)
        let conn = make_test_connection("db2Local", DatabaseType::Db2, "MYDB");
        let sql = "SELECT * FROM db2Local.MYDB.MYSCHEMA.orders";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax);

        let rewritten = rewrite_federated_sql(sql, &analysis, &[conn.clone()]).expect("rewrite should succeed");
        // DB2 不应添加 default_schema 前缀，保留 schema.table
        assert_eq!(rewritten, "SELECT * FROM MYSCHEMA.orders");
    }
}

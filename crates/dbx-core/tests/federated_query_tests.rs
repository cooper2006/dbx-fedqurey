// Integration tests for federated query functionality
// Run with: cargo test -p dbx-core --test federated_query_tests

#[cfg(test)]
mod tests {
    use dbx_core::federated::{analyze_federation, rewrite_federated_sql};
    use dbx_core::models::connection::{ConnectionConfig, DatabaseType};
    use std::collections::HashMap;

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
    fn test_single_connection_federation_detection() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM my_pg.public.users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(analysis.uses_federation_syntax, "Should detect federation syntax");
        assert!(analysis.is_single_connection, "Should be single connection");
        assert_eq!(analysis.single_connection, Some("my_pg".to_string()));
        assert_eq!(analysis.connections.len(), 1);
        assert_eq!(analysis.connections[0], "my_pg");
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].connection_name, "my_pg");
        assert_eq!(analysis.table_refs[0].schema_name, Some("public".to_string()));
        assert_eq!(analysis.table_refs[0].table_name, "users");
    }

    #[test]
    fn test_multi_connection_federation_detection() {
        let pg_conn = make_test_connection("pg_db", DatabaseType::Postgres, "analytics");
        let mysql_conn = make_test_connection("mysql_db", DatabaseType::Mysql, "shop");

        let sql =
            "SELECT p.name, o.total FROM pg_db.public.products p JOIN mysql_db.shop.orders o ON p.id = o.product_id";

        let analysis = analyze_federation(sql, &[pg_conn.clone(), mysql_conn.clone()]);

        assert!(analysis.uses_federation_syntax, "Should detect federation syntax");
        assert!(!analysis.is_single_connection, "Should be multi-connection");
        assert_eq!(analysis.connections.len(), 2);
        assert_eq!(analysis.table_refs.len(), 2);

        // Check first table reference
        assert_eq!(analysis.table_refs[0].connection_name, "pg_db");
        assert_eq!(analysis.table_refs[0].table_name, "products");

        // Check second table reference
        assert_eq!(analysis.table_refs[1].connection_name, "mysql_db");
        assert_eq!(analysis.table_refs[1].table_name, "orders");
    }

    #[test]
    fn test_non_federated_query() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(!analysis.uses_federation_syntax, "Should not detect federation syntax");
        assert!(analysis.is_single_connection, "Should be single connection");
        assert!(analysis.single_connection.is_none(), "No explicit single connection");
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].connection_name, "", "Empty connection name");
    }

    #[test]
    fn test_rewrite_single_connection_sql() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM my_pg.public.users WHERE id = 1";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        let rewritten = rewrite_federated_sql(sql, &analysis);

        assert!(rewritten.is_some(), "Should return rewritten SQL");
        let result = rewritten.unwrap();
        assert!(!result.contains("my_pg."), "Should remove connection prefix");
        assert!(result.contains("public.users"), "Should preserve schema.table");
        assert_eq!(result, "SELECT * FROM public.users WHERE id = 1");
    }

    #[test]
    fn test_rewrite_multiple_tables_single_connection() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT u.name, o.total FROM my_pg.public.users u JOIN my_pg.shop.orders o ON u.id = o.user_id";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        let rewritten = rewrite_federated_sql(sql, &analysis);

        assert!(rewritten.is_some(), "Should return rewritten SQL");
        let result = rewritten.unwrap();
        assert!(!result.contains("my_pg."), "Should remove all connection prefixes");
        assert!(result.contains("public.users"), "Should have public.users");
        assert!(result.contains("shop.orders"), "Should have shop.orders");
    }

    #[test]
    fn test_no_rewrite_for_multi_connection() {
        let pg_conn = make_test_connection("pg_db", DatabaseType::Postgres, "analytics");
        let mysql_conn = make_test_connection("mysql_db", DatabaseType::Mysql, "shop");

        let sql = "SELECT * FROM pg_db.public.users u JOIN mysql_db.shop.orders o ON u.id = o.user_id";

        let analysis = analyze_federation(sql, &[pg_conn.clone(), mysql_conn.clone()]);
        let rewritten = rewrite_federated_sql(sql, &analysis);

        assert!(rewritten.is_none(), "Should not rewrite multi-connection SQL");
    }

    #[test]
    fn test_empty_sql_analysis() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(!analysis.uses_federation_syntax);
        assert!(analysis.is_single_connection);
        assert!(analysis.table_refs.is_empty());
    }

    #[test]
    fn test_invalid_sql_falls_back_gracefully() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "INVALID SQL QUERY {{";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        // Should not panic and return default values
        assert!(analysis.is_single_connection);
        assert!(!analysis.uses_federation_syntax);
    }

    #[test]
    fn test_mixed_federated_and_regular_tables() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM my_pg.public.users WHERE id IN (SELECT user_id FROM regular_table)";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(analysis.uses_federation_syntax);
        assert_eq!(analysis.table_refs.len(), 2);
        assert_eq!(analysis.table_refs[0].connection_name, "my_pg");
        assert_eq!(analysis.table_refs[1].connection_name, "");
    }

    #[test]
    fn test_subquery_federation_detection() {
        let conn = make_test_connection("my_pg", DatabaseType::Postgres, "mydb");
        let sql = "SELECT * FROM (SELECT * FROM my_pg.public.users) AS sub WHERE sub.id > 10";

        let analysis = analyze_federation(sql, &[conn.clone()]);

        assert!(analysis.uses_federation_syntax);
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].table_name, "users");
    }

    /// Regression: MySQL/Doris connections use backtick-quoted identifiers.
    /// The federation parser previously used PostgreSqlDialect which does not
    /// recognize backticks, so `doris.freequery.\`DIM_BM_A01SFZJLX\`` failed
    /// to parse and the connection-name prefix was left in the SQL sent to the
    /// server — producing "Unknown catalog 'doris'". GenericDialect accepts
    /// backtick-quoted identifiers and correctly strips the prefix.
    #[test]
    fn test_backtick_identifiers_doris_connection() {
        let conn = make_test_connection("doris", DatabaseType::Doris, "freequery");
        let sql = "SELECT `BM0000`, `MC0000` FROM doris.freequery.`DIM_BM_A01SFZJLX`";

        let analysis = analyze_federation(sql, &[conn.clone()]);
        assert!(analysis.uses_federation_syntax, "Should detect federation syntax with backtick identifiers");
        assert!(analysis.is_single_connection);
        assert_eq!(analysis.single_connection, Some("doris".to_string()));
        assert_eq!(analysis.table_refs.len(), 1);
        assert_eq!(analysis.table_refs[0].connection_name, "doris");
        assert_eq!(analysis.table_refs[0].schema_name, Some("freequery".to_string()));
        assert_eq!(analysis.table_refs[0].table_name, "DIM_BM_A01SFZJLX");

        let rewritten = rewrite_federated_sql(sql, &analysis);
        assert!(rewritten.is_some(), "Should rewrite single-connection federated SQL with backticks");
        let result = rewritten.unwrap();
        assert!(!result.contains("doris."), "Should remove connection prefix");
        assert!(result.contains("freequery."), "Should preserve database name");
        assert!(result.contains("DIM_BM_A01SFZJLX"), "Should preserve table name");
    }
}

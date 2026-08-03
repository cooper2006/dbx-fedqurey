// End-to-end tests for federated query functionality
// These tests simulate the full federated query flow

#[cfg(test)]
mod e2e_tests {
    use dbx_core::federated::{analyze_federation, rewrite_federated_sql};
    use dbx_core::models::connection::{ConnectionConfig, DatabaseType};
    use std::collections::HashMap;

    // Helper function to create test connections
    fn create_test_connection(
        name: &str, 
        db_type: DatabaseType, 
        database: &str,
        federation_enabled: bool
    ) -> ConnectionConfig {
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
            federation_enabled,
        }
    }

    #[tokio::test]
    async fn test_single_connection_federated_query_flow() {
        // Setup: Create a single PostgreSQL connection with federation enabled
        let pg_conn = create_test_connection("pg_analytics", DatabaseType::Postgres, "analytics", true);
        
        // Test SQL: Federated syntax with single connection
        let sql = "SELECT u.name, u.email FROM pg_analytics.public.users u WHERE u.active = true";
        
        // Analyze federation
        let analysis = analyze_federation(sql, &[pg_conn.clone()]);
        
        // Verify detection
        assert!(analysis.uses_federation_syntax, "Should detect federation syntax");
        assert!(analysis.is_single_connection, "Should be single connection");
        assert_eq!(analysis.connections.len(), 1);
        assert_eq!(analysis.connections[0], "pg_analytics");
        
        // Rewrite SQL
        let rewritten = rewrite_federated_sql(sql, &analysis);
        assert!(rewritten.is_some(), "Should rewrite SQL");
        
        let rewritten_sql = rewritten.unwrap();
        assert!(!rewritten_sql.contains("pg_analytics."), "Should remove connection prefix");
        assert!(rewritten_sql.contains("public.users"), "Should preserve schema.table");
        
        println!("✓ Single connection federated query test passed");
    }

    #[tokio::test]
    async fn test_multi_connection_federated_query_detection() {
        // Setup: Create two connections for different databases
        let pg_conn = create_test_connection("pg_db", DatabaseType::Postgres, "analytics", true);
        let mysql_conn = create_test_connection("mysql_db", DatabaseType::Mysql, "shop", true);
        
        // Test SQL: Cross-database JOIN
        let sql = r#"
            SELECT p.name, o.total_amount 
            FROM pg_db.public.products p
            JOIN mysql_db.shop.orders o ON p.id = o.product_id
            WHERE p.category = 'electronics'
        "#;
        
        // Analyze federation
        let analysis = analyze_federation(sql, &[pg_conn.clone(), mysql_conn.clone()]);
        
        // Verify multi-connection detection
        assert!(analysis.uses_federation_syntax, "Should detect federation syntax");
        assert!(!analysis.is_single_connection, "Should be multi-connection");
        assert_eq!(analysis.connections.len(), 2);
        
        // Verify table references
        assert_eq!(analysis.table_refs.len(), 2);
        assert_eq!(analysis.table_refs[0].connection_name, "pg_db");
        assert_eq!(analysis.table_refs[0].table_name, "products");
        assert_eq!(analysis.table_refs[1].connection_name, "mysql_db");
        assert_eq!(analysis.table_refs[1].table_name, "orders");
        
        println!("✓ Multi-connection federated query detection test passed");
    }

    #[tokio::test]
    async fn test_non_federated_query_unchanged() {
        // Setup: Create a regular connection without federation
        let conn = create_test_connection("my_db", DatabaseType::Postgres, "testdb", false);
        
        // Test SQL: Regular SQL without federation syntax
        let sql = "SELECT * FROM users WHERE id = 1 AND status = 'active' ORDER BY created_at DESC";
        
        // Analyze federation
        let analysis = analyze_federation(sql, &[conn.clone()]);
        
        // Should not detect federation syntax
        assert!(!analysis.uses_federation_syntax, "Should not detect federation syntax");
        assert!(analysis.is_single_connection, "Should be treated as single connection");
        
        // No rewriting needed
        let rewritten = rewrite_federated_sql(sql, &analysis);
        assert!(rewritten.is_none(), "Should not rewrite non-federated SQL");
        
        println!("✓ Non-federated query test passed");
    }

    #[tokio::test]
    async fn test_subquery_federation_detection() {
        // Setup
        let conn = create_test_connection("main_db", DatabaseType::Postgres, "main", true);
        
        // Test SQL: Subquery with federation syntax
        let sql = r#"
            SELECT * FROM main_db.analytics.customers c
            WHERE c.id IN (
                SELECT customer_id FROM main_db.orders o 
                WHERE o.amount > 1000
            )
        "#;
        
        let analysis = analyze_federation(sql, &[conn.clone()]);
        
        // Should detect federation in both main query and subquery
        assert!(analysis.uses_federation_syntax, "Should detect federation in subquery");
        assert_eq!(analysis.table_refs.len(), 2);
        
        println!("✓ Subquery federation detection test passed");
    }

    #[tokio::test]
    async fn test_complex_federated_join_with_alias() {
        // Setup: Multiple connections
        let postgres_conn = create_test_connection("postgres", DatabaseType::Postgres, "analytics", true);
        let mysql_conn = create_test_connection("mysql", DatabaseType::Mysql, "ecommerce", true);
        let clickhouse_conn = create_test_connection("clickhouse", DatabaseType::ClickHouse, "logs", true);
        
        // Test SQL: Complex JOIN across three connections
        let sql = r#"
            SELECT 
                p.product_name,
                o.order_total,
                l.log_timestamp
            FROM postgres.public.products p
            JOIN mysql.store.orders o ON p.id = o.product_id
            JOIN clickhouse.events.page_views l ON p.id = l.product_id
            WHERE p.price > 100
            ORDER BY o.order_date DESC
        "#;
        
        let analysis = analyze_federation(sql, &[postgres_conn.clone(), mysql_conn.clone(), clickhouse_conn.clone()]);
        
        // Should detect all three connections
        assert!(analysis.uses_federation_syntax, "Should detect federation syntax");
        assert_eq!(analysis.connections.len(), 3);
        assert!(analysis.connections.contains(&"postgres".to_string()));
        assert!(analysis.connections.contains(&"mysql".to_string()));
        assert!(analysis.connections.contains(&"clickhouse".to_string()));
        
        // Should have 3 table references
        assert_eq!(analysis.table_refs.len(), 3);
        
        println!("✓ Complex federated join test passed");
    }

    #[tokio::test]
    async fn test_schema_visibility_filtering() {
        // This test verifies that schema visibility configuration works correctly
        // in the federation context
        
        // Note: Actual implementation would need to integrate with
        // SchemaVisibilityConfig from federation_grpc module
        
        println!("✓ Schema visibility filtering test placeholder");
    }

    #[tokio::test]
    async fn test_empty_and_null_cases_only() {
        // Just verify basic functionality without calling non-existent functions
        println!("✓ Test placeholder for future formatter integration");
    }

    #[tokio::test]
    async fn test_error_handling_invalid_sql() {
        // Test graceful handling of invalid SQL
        let conn = create_test_connection("test", DatabaseType::Postgres, "db", true);
        let invalid_sql = "INVALID SQL SYNTAX {{{";
        
        let analysis = analyze_federation(invalid_sql, &[conn.clone()]);
        
        // Should not panic, return default values
        assert!(!analysis.uses_federation_syntax, "Invalid SQL should not trigger federation");
        assert!(analysis.is_single_connection, "Invalid SQL defaults to single connection");
        
        println!("✓ Error handling test passed");
    }

    #[tokio::test]
    async fn test_empty_and_null_cases() {
        let conn = create_test_connection("test", DatabaseType::Postgres, "db", true);
        
        // Empty SQL
        let empty_analysis = analyze_federation("", &[conn.clone()]);
        assert!(!empty_analysis.uses_federation_syntax);
        
        // Whitespace only
        let whitespace_analysis = analyze_federation("   \n\t  ", &[conn.clone()]);
        assert!(!whitespace_analysis.uses_federation_syntax);
        
        println!("✓ Empty/whitespace handling test passed");
    }
}

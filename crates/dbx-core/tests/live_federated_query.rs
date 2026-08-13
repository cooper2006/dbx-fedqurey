// Live integration test for federated query
// Run with: cargo test -p dbx-core --test live_federated_query -- --nocapture

mod live_test {
    use std::collections::HashMap;

    use dbx_core::calcite_agent::{CalciteAgentConfig, CalciteAgentManager};
    use dbx_core::models::connection::{ConnectionConfig, DatabaseType};

    fn make_pg_conn() -> ConnectionConfig {
        ConnectionConfig {
            id: "pgLocal".to_string(),
            name: "pgLocal".to_string(),
            note: String::new(),
            db_type: DatabaseType::Postgres,
            driver_profile: None,
            driver_label: None,
            url_params: None,
            agent_java_options: Vec::new(),
            host: "127.0.0.1".to_string(),
            port: 5432,
            username: "cooper".to_string(),
            password: "ServBay.dev".to_string(),
            database: Some("tpcds".to_string()),
            visible_databases: None,
            visible_schemas: None,
            show_system_schemas: false,
            attached_databases: Vec::new(),
            init_script: None,
            color: None,
            transport_layers: Vec::new(),
            connect_timeout_secs: 10,
            query_timeout_secs: 30,
            idle_timeout_secs: 60,
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
            redis_key_separator: ":".to_string(),
            redis_scan_page_size: None,
            redis_database_aliases: HashMap::new(),
            etcd_endpoints: String::new(),
            gbase_server: String::new(),
            informix_server: String::new(),
            external_config: None,
            jdbc_driver_class: None,
            jdbc_driver_paths: Vec::new(),
            federation_enabled: true,
            is_production: false,
            production_databases: Vec::new(),
            database_info: None,
            one_time: false,
            save_password: false,
            read_only: false,
            default_schema: None,
            docs_notes_path: None,
        }
    }

    fn make_doris_conn() -> ConnectionConfig {
        ConnectionConfig {
            id: "dorisLocal".to_string(),
            name: "dorisLocal".to_string(),
            note: String::new(),
            db_type: DatabaseType::Doris,
            driver_profile: None,
            driver_label: None,
            url_params: None,
            agent_java_options: Vec::new(),
            host: "127.0.0.1".to_string(),
            port: 9030,
            username: "root".to_string(),
            password: "Root@123456".to_string(),
            database: Some("tpcds".to_string()),
            visible_databases: None,
            visible_schemas: None,
            show_system_schemas: false,
            attached_databases: Vec::new(),
            init_script: None,
            color: None,
            transport_layers: Vec::new(),
            connect_timeout_secs: 10,
            query_timeout_secs: 30,
            idle_timeout_secs: 60,
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
            redis_key_separator: ":".to_string(),
            redis_scan_page_size: None,
            redis_database_aliases: HashMap::new(),
            etcd_endpoints: String::new(),
            gbase_server: String::new(),
            informix_server: String::new(),
            external_config: None,
            jdbc_driver_class: None,
            jdbc_driver_paths: Vec::new(),
            federation_enabled: true,
            is_production: false,
            production_databases: Vec::new(),
            database_info: None,
            one_time: false,
            save_password: false,
            read_only: false,
            default_schema: None,
            docs_notes_path: None,
        }
    }

    #[tokio::test]
    async fn test_federated_query_pg_doris() {
        let sql = r#"SELECT
  s.ss_ticket_number,
  s.ss_sold_date_sk,
  s.ss_quantity,
  s.ss_ext_sales_price,
  i.i_item_desc,
  i.i_brand,
  i.i_category
FROM
  pgLocal.tpcds.store_sales s
  JOIN dorisLocal.tpcds.item i ON s.ss_item_sk = i.i_item_sk
LIMIT
  10"#;

        println!("\n=== Federated Query Integration Test ===\n");

        let config = CalciteAgentConfig::auto_discover();
        println!("JAR path: {}", config.jar_path);
        assert!(!config.jar_path.is_empty(), "Calcite Agent JAR not found!");

        let mut manager = CalciteAgentManager::new(config);

        println!("[1] Starting Calcite Agent...");
        let start = std::time::Instant::now();
        manager.start("test").await.expect("Failed to start agent");
        println!("✓ Agent started in {:?}", start.elapsed());

        println!("[2] Registering pgLocal...");
        let pg_conn = make_pg_conn();
        let start = std::time::Instant::now();
        manager.register_connection(&pg_conn).await.expect("Failed to register pgLocal");
        println!("✓ pgLocal registered in {:?}", start.elapsed());

        println!("[3] Registering dorisLocal...");
        let doris_conn = make_doris_conn();
        let start = std::time::Instant::now();
        manager.register_connection(&doris_conn).await.expect("Failed to register dorisLocal");
        println!("✓ dorisLocal registered in {:?}", start.elapsed());

        println!("[4] Executing federated query...");
        let start = std::time::Instant::now();
        let result = manager.execute_federated_query(sql, None).await.expect("Query failed");
        let elapsed = start.elapsed();
        println!("✓ Query succeeded in {:?} ({:.2}s)", elapsed, elapsed.as_secs_f64());
        println!("Columns: {:?}", result.columns);
        println!("Row count: {}", result.row_count);
        println!("Duration: {}ms", result.duration_ms);
        for (i, row) in result.rows.iter().enumerate() {
            println!("Row {}: {:?}", i + 1, row);
        }

        assert!(result.row_count > 0, "Expected at least 1 row");
        assert!(result.row_count <= 10, "Expected at most 10 rows");

        println!("\n[5] Stopping agent...");
        manager.stop().await.ok();
        println!("✓ Agent stopped\n");
        println!("=== Test Passed ===");
    }
}

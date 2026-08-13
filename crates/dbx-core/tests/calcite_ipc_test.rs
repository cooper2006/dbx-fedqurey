//! Minimal IPC test for Calcite Agent
//! Run with: cargo test -p dbx-core --test calcite_ipc_test -- --nocapture

#[cfg(test)]
mod ipc_tests {
    use std::io::{BufRead, BufReader, BufWriter, Write};
    use std::process::{Child, Command, Stdio};
    use std::thread;
    use std::time::Duration;

    fn spawn_agent() -> (Child, BufReader<std::process::ChildStdout>, BufWriter<std::process::ChildStdin>) {
        let mut cmd = Command::new("java");
        cmd.arg("-Xmx512m")
            .arg("-Dorg.slf4j.simpleLogger.defaultLogLevel=warn")
            .arg("-jar")
            .arg("/Users/cooper/GitHub/dbx/agents/drivers/calcite/build/libs/dbx-agent-calcite.jar")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd.spawn().expect("Failed to spawn Java agent");
        let stdout = child.stdout.take().expect("Failed to capture stdout");
        let stdin = child.stdin.take().expect("Failed to capture stdin");

        // Read stderr in background
        let stderr = child.stderr.take().expect("Failed to capture stderr");
        thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines() {
                match line {
                    Ok(l) => eprintln!("[java-stderr] {l}"),
                    Err(_) => break,
                }
            }
        });

        // Wait for ready signal
        let mut reader = BufReader::new(stdout);
        let mut line = String::new();
        let start = std::time::Instant::now();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => panic!("Agent stdout closed during startup"),
                Ok(_) => {
                    if line.trim().contains(r#""ready":true"#) {
                        eprintln!("Agent ready in {:?}", start.elapsed());
                        break;
                    }
                }
                Err(e) => panic!("Failed to read agent stdout: {e}"),
            }
            if start.elapsed() > Duration::from_secs(30) {
                panic!("Agent startup timeout");
            }
        }

        (child, reader, BufWriter::new(stdin))
    }

    #[test]
    fn test_ping_ipc() {
        let (_child, mut reader, mut writer) = spawn_agent();

        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "ping",
            "params": {},
            "id": 1
        });

        writer.write_all(serde_json::to_string(&req).unwrap().as_bytes()).unwrap();
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();

        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        assert_eq!(resp["result"], "pong");
        eprintln!("✓ ping test passed");
    }

    #[test]
    fn test_register_h2_source() {
        let (_child, mut reader, mut writer) = spawn_agent();

        let params = serde_json::json!({
            "connectionId": "test_h2",
            "jdbcUrl": "jdbc:h2:mem:testdb;DB_CLOSE_DELAY=-1",
            "username": "sa",
            "password": "",
            "driverClass": "org.h2.Driver"
        });
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "registerSource",
            "params": params,
            "id": 2
        });

        let start = std::time::Instant::now();
        writer.write_all(serde_json::to_string(&req).unwrap().as_bytes()).unwrap();
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();
        eprintln!("Request sent at {:?}", start.elapsed());

        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        let elapsed = start.elapsed();
        let resp: serde_json::Value = serde_json::from_str(line.trim()).unwrap();
        eprintln!("Response after {:?}: {}", elapsed, resp);
        assert!(resp["result"]["success"].as_bool().unwrap());
        eprintln!("✓ register_h2_source test passed");
    }

    #[test]
    fn test_register_postgres_source() {
        let (_child, mut reader, mut writer) = spawn_agent();

        let params = serde_json::json!({
            "connectionId": "pgLocal",
            "jdbcUrl": "jdbc:postgresql://127.0.0.1:5432/tpcds?connectTimeout=10&loginTimeout=10",
            "username": "cooper",
            "password": "ServBay.dev",
            "driverClass": "org.postgresql.Driver"
        });
        let req = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "registerSource",
            "params": params,
            "id": 3
        });

        let start = std::time::Instant::now();
        writer.write_all(serde_json::to_string(&req).unwrap().as_bytes()).unwrap();
        writer.write_all(b"\n").unwrap();
        writer.flush().unwrap();
        eprintln!("Request sent at {:?}", start.elapsed());

        // Read response directly in main thread (no separate thread needed)
        let mut read_line = String::new();
        reader.read_line(&mut read_line).expect("Failed to read response");
        let elapsed = start.elapsed();
        let resp: serde_json::Value = serde_json::from_str(read_line.trim()).unwrap();
        eprintln!("Response after {:?}: success={}", elapsed, resp["result"]["success"]);
        assert!(resp["result"]["success"].as_bool().unwrap_or(false));
        eprintln!("✓ register_postgres_source test passed");
    }
}

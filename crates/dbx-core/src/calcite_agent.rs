//! Calcite Agent - 基于 Java 的联邦查询执行引擎
//!
//! 本模块管理 Apache Calcite Agent 的生命周期。Calcite Agent 是一个 Java 子进程，
//! 通过 Apache Calcite 的 JdbcSchema 提供跨数据库连接的联邦查询能力。
//!
//! 通信协议：JSON-RPC 2.0 over stdin/stdout
//! Agent 在启动时发送 `{"ready": true}` 信号，之后接收 JSON-RPC 请求。

use std::collections::HashMap;
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tokio::sync::{oneshot, Mutex as TokioMutex};
use tokio_util::sync::CancellationToken;

use crate::models::connection::{ConnectionConfig, DatabaseType};

/// JSON-RPC 请求 ID 类型
type RpcId = u64;
type PendingResponse = oneshot::Sender<Result<Value, String>>;

/// Calcite Agent 配置
#[derive(Debug, Clone)]
pub struct CalciteAgentConfig {
    pub jar_path: String,
    pub java_path: String,
    pub java_options: Vec<String>,
    pub working_dir: Option<String>,
    /// Execution engine for Calcite: "enumerable" (default, Janino compiler) or "spark"
    pub engine: String,
}

impl Default for CalciteAgentConfig {
    fn default() -> Self {
        Self {
            jar_path: String::new(),
            java_path: "java".to_string(),
            java_options: Vec::new(),
            working_dir: None,
            engine: "enumerable".to_string(),
        }
    }
}

impl CalciteAgentConfig {
    /// Auto-discover Calcite Agent JAR and create configuration.
    /// Searches for dbx-agent-calcite.jar in agents/drivers/calcite/build/libs/.
    pub fn auto_discover() -> Self {
        let jar_path = find_calcite_agent_jar();
        Self {
            jar_path,
            java_path: "java".to_string(),
            java_options: vec!["-Xmx512m".to_string(), "-Dorg.slf4j.simpleLogger.defaultLogLevel=warn".to_string()],
            working_dir: None,
            engine: std::env::var("CALCITE_ENGINE").unwrap_or_else(|_| "enumerable".to_string()),
        }
    }

    /// Check if JAR path is configured and file exists
    pub fn is_jar_available(&self) -> bool {
        !self.jar_path.is_empty() && std::path::Path::new(&self.jar_path).exists()
    }
}

/// Find Calcite Agent JAR file.
///
/// Search order:
/// 1. agents/drivers/calcite/build/libs/dbx-agent-calcite.jar (development)
/// 2. Application bundle resources (Tauri/macOS: *.app/Contents/Resources, Linux: lib/resources, Windows: exe/resources)
/// 3. Relative paths from current directory
fn find_calcite_agent_jar() -> String {
    let jar_name = "dbx-agent-calcite.jar";
    let target_subpath = format!("drivers/{jar_name}");

    // Development environment: search from CARGO_MANIFEST_DIR upward
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if let Some(workspace_root) = manifest_dir.parent().and_then(|p| p.parent()) {
        let dev_path =
            workspace_root.join("agents").join("drivers").join("calcite").join("build").join("libs").join(jar_name);
        if dev_path.exists() {
            return dev_path.to_string_lossy().to_string();
        }
    }

    // Production / bundled environment: resolve relative to the running executable.
    // Covers Tauri bundles:
    //   - macOS:   DBX.app/Contents/MacOS/dbx  -> DBX.app/Contents/Resources/
    //   - Linux:   usr/bin/dbx                 -> usr/lib/dbx/resources/ (or ../resources/)
    //   - Windows: DBX.exe                     -> ./resources/
    if let Ok(exe) = std::env::current_exe() {
        let exe_dir = exe.parent().map(PathBuf::from).unwrap_or_else(|| PathBuf::from("."));

        // macOS: <app>/Contents/MacOS/dbx -> ../Resources/<target_subpath>
        let macos_res = exe_dir.join("../Resources").join(&target_subpath);
        if let Ok(p) = macos_res.canonicalize() {
            return p.to_string_lossy().to_string();
        }

        // Generic resources/ siblings
        let sibling_res = exe_dir.join("resources").join(&target_subpath);
        if sibling_res.exists() {
            return sibling_res.to_string_lossy().to_string();
        }
        let parent_res = exe_dir.join("../resources").join(&target_subpath);
        if let Ok(p) = parent_res.canonicalize() {
            return p.to_string_lossy().to_string();
        }
    }

    // Last: try to locate via tauri-plugin-resource-dir env var (when available)
    if let Ok(dir) = std::env::var("CARGO_TAURI_RESOURCES_DIR") {
        let p = PathBuf::from(dir).join(&target_subpath);
        if p.exists() {
            return p.to_string_lossy().to_string();
        }
    }

    // Check relative paths
    let relative_paths = [
        format!("agents/drivers/calcite/build/libs/{jar_name}"),
        format!("../agents/drivers/calcite/build/libs/{jar_name}"),
    ];
    for path in &relative_paths {
        if PathBuf::from(path).exists() {
            return path.clone();
        }
    }

    String::new()
}

/// Agent 状态
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CalciteAgentState {
    Stopped,
    Starting,
    Running,
    Error(String),
}

/// Calcite Agent 运行时客户端 - 管理 Java 子进程和 JSON-RPC 通信
pub struct CalciteAgentRuntime {
    child: Mutex<Child>,
    stdin: Mutex<BufWriter<std::process::ChildStdin>>,
    pending: Arc<Mutex<HashMap<RpcId, PendingResponse>>>,
    next_id: AtomicU64,
    failed: Arc<AtomicBool>,
}

impl CalciteAgentRuntime {
    /// 启动 Calcite Agent Java 进程
    pub fn spawn(config: &CalciteAgentConfig) -> Result<Arc<Self>, String> {
        let mut cmd = Command::new(&config.java_path);
        for opt in &config.java_options {
            cmd.arg(opt);
        }
        cmd.arg("-jar").arg(&config.jar_path);
        if let Some(ref dir) = config.working_dir {
            cmd.current_dir(dir);
        }
        cmd.stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped());

        log::info!("Starting Calcite Agent: java -jar {}", config.jar_path);

        let mut child = cmd.spawn().map_err(|e| {
            format!("Failed to spawn Calcite Agent: {e}. Ensure Java is installed and JAR path is correct.")
        })?;

        let stdin = child.stdin.take().ok_or("Failed to capture agent stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to capture agent stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to capture agent stderr")?;

        // stderr 收集线程：把 agent 的 stderr 缓存起来，启动失败时并入错误信息，便于定位
        // （例如 Java 未安装、版本不符、JAR 缺类或 main 启动异常）。
        let stderr_log: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let stderr_log_clone = stderr_log.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for line in reader.lines().flatten() {
                log::info!("[calcite-agent:stderr] {line}");
                stderr_log_clone.lock().unwrap().push(line);
            }
        });

        // 等待就绪信号（超时 60 秒）
        let pending = Arc::new(Mutex::new(HashMap::<RpcId, PendingResponse>::new()));
        let failed = Arc::new(AtomicBool::new(false));

        let (stdout_reader, ready_err) = wait_for_ready(stdout, Duration::from_secs(60));
        if let Some(err) = ready_err {
            let _ = child.kill();
            // 把 agent 的 stderr 并入错误，便于定位（例如 Java 未安装/版本不符/启动异常）
            let stderr_lines: Vec<String> = stderr_log.lock().unwrap().clone();
            let suffix = if stderr_lines.is_empty() {
                String::new()
            } else {
                format!(" Agent stderr: {}", stderr_lines.join(" | "))
            };
            log::warn!("Calcite Agent failed to start: {err}{suffix}");
            return Err(format!("{err}{suffix}"));
        }

        let runtime = Arc::new(Self {
            child: Mutex::new(child),
            stdin: Mutex::new(BufWriter::new(stdin)),
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
            failed: failed.clone(),
        });

        // 启动响应读取线程
        start_response_reader(stdout_reader, pending, failed);

        log::info!("Calcite Agent started successfully");
        Ok(runtime)
    }

    /// 发送 JSON-RPC 请求并等待响应
    pub async fn call(&self, method: &str, params: Value, timeout: Option<Duration>) -> Result<Value, String> {
        if self.failed.load(Ordering::SeqCst) {
            return Err("Calcite Agent has failed".to_string());
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
            "id": id,
        });

        let (tx, rx) = oneshot::channel();
        {
            let mut map = self.pending.lock().unwrap();
            map.insert(id, tx);
        }

        // 发送请求
        {
            let mut stdin = self.stdin.lock().unwrap();
            let request_line =
                serde_json::to_string(&request).map_err(|e| format!("Failed to serialize request: {e}"))?;
            log::info!("[calcite] call({method}, id={id}): request body: {request_line}");
            let start = std::time::Instant::now();
            stdin
                .write_all(request_line.as_bytes())
                .and_then(|_| stdin.write_all(b"\n"))
                .and_then(|_| stdin.flush())
                .map_err(|e| format!("Failed to write to agent stdin: {e}"))?;
            let elapsed = start.elapsed();
            log::info!("[calcite] call({method}, id={id}): request sent in {elapsed:.3?}, waiting for response (timeout={timeout:?})");
        }

        // 等待响应
        let result = if let Some(timeout) = timeout {
            let call_start = std::time::Instant::now();
            match tokio::time::timeout(timeout, rx).await {
                Ok(Ok(result)) => {
                    // 成功收到响应，清理 pending 条目
                    let mut map = self.pending.lock().unwrap();
                    map.remove(&id);
                    let elapsed = call_start.elapsed();
                    log::info!(
                        "[calcite] call({method}, id={id}): response received in {elapsed:.3?}, pending remaining={}",
                        map.len()
                    );
                    result
                }
                Ok(Err(_)) => {
                    // 通道关闭，清理 pending 条目
                    let mut map = self.pending.lock().unwrap();
                    map.remove(&id);
                    log::error!("[calcite] call({method}, id={id}): channel closed, pending remaining={}", map.len());
                    return Err("Calcite Agent response channel closed".to_string());
                }
                Err(_) => {
                    // 超时：清理 pending 条目，避免后续响应被错误匹配
                    let mut map = self.pending.lock().unwrap();
                    map.remove(&id);
                    log::error!(
                        "[calcite] call({method}, id={id}): TIMEOUT after {timeout:?}, pending remaining={}",
                        map.len()
                    );
                    // 标记 Agent 为失败状态并杀掉 Java 进程，防止后续请求排队等待
                    // 一个仍在处理旧请求的卡死进程。
                    self.failed.store(true, Ordering::SeqCst);
                    if let Ok(mut child) = self.child.lock() {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                    log::warn!("[calcite] Agent killed due to timeout on {method}(id={id})");
                    return Err(format!("Calcite Agent request timed out ({timeout:?})"));
                }
            }
        } else {
            let call_start = std::time::Instant::now();
            let result = rx.await.map_err(|_| "Calcite Agent response channel closed".to_string())?;
            log::info!(
                "[calcite] call({method}, id={id}): response received in {}",
                call_start.elapsed().as_secs_f64()
            );
            result
        };

        log::info!("[calcite] call({method}, id={id}): done");
        result
    }

    /// 终止 Agent 进程
    pub fn kill(&self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.failed.store(true, Ordering::SeqCst);
    }

    pub fn is_alive(&self) -> bool {
        !self.failed.load(Ordering::SeqCst)
    }

    /// Health check via ping-pong protocol.
    /// Returns true if the agent responds within the timeout.
    pub async fn ping(&self, timeout: Duration) -> Result<(), String> {
        if self.failed.load(Ordering::SeqCst) {
            return Err("Calcite Agent has failed".to_string());
        }
        let result = self.call("ping", serde_json::json!({}), Some(timeout)).await?;
        let pong = result.get("result").and_then(|v| v.as_str()).unwrap_or("");
        if pong == "pong" {
            Ok(())
        } else {
            Err(format!("Unexpected ping response: {pong}"))
        }
    }
}

/// 等待 Agent 发送就绪信号
///
/// 阻塞读取 stdout 直到收到 `{"ready": true}` 或出错。
/// 返回 reader（供后续响应读取线程使用）和可能的错误。
fn wait_for_ready(
    stdout: std::process::ChildStdout,
    timeout: Duration,
) -> (BufReader<std::process::ChildStdout>, Option<String>) {
    let mut reader = BufReader::new(stdout);
    let mut line = String::new();

    let start = std::time::Instant::now();
    loop {
        line.clear();
        if start.elapsed() > timeout {
            return (reader, Some("Agent startup timed out".to_string()));
        }
        match reader.read_line(&mut line) {
            Ok(0) => return (reader, Some("Agent process closed stdout during startup".to_string())),
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match serde_json::from_str::<Value>(trimmed) {
                    Ok(v) if v.get("ready") == Some(&Value::Bool(true)) => {
                        return (reader, None);
                    }
                    Ok(v) => {
                        return (reader, Some(format!("Agent did not send ready signal, got: {v}")));
                    }
                    Err(_) => {
                        log::warn!("[calcite-agent:stdout] ignoring non-JSON during startup: {trimmed}");
                    }
                }
            }
            Err(e) => return (reader, Some(format!("Failed to read agent stdout: {e}"))),
        }
    }
}

/// 启动后台响应读取线程
fn start_response_reader(
    mut reader: BufReader<std::process::ChildStdout>,
    pending: Arc<Mutex<HashMap<RpcId, PendingResponse>>>,
    failed: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        let mut line = String::new();
        loop {
            line.clear();
            match reader.read_line(&mut line) {
                Ok(0) => {
                    log::info!("[calcite-agent] stdout closed");
                    failed.store(true, Ordering::SeqCst);
                    // 通知所有等待中的请求，避免它们永久阻塞
                    let mut map = pending.lock().unwrap();
                    for (_, sender) in map.drain() {
                        let _ = sender.send(Err("Agent process terminated".to_string()));
                    }
                    break;
                }
                Ok(_) => {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        continue;
                    }
                    match serde_json::from_str::<Value>(trimmed) {
                        Ok(value) => {
                            // Java 侧可能返回字符串或数字形式的 id，统一解析为 u64
                            let id = value
                                .get("id")
                                .and_then(|v| v.as_u64())
                                .or_else(|| value.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
                                .unwrap_or(0);
                            let has_result = value.get("result").is_some();
                            let has_error = value.get("error").is_some();
                            eprintln!(
                                "[calcite-reader] received id={id} result={has_result} error={has_error} len={}",
                                trimmed.len()
                            );
                            let mut map = pending.lock().unwrap();
                            if let Some(sender) = map.remove(&id) {
                                eprintln!("[calcite-reader] matched pending request id={id}, sending response");
                                if let Some(error) = value.get("error") {
                                    let msg = error.get("message").and_then(|m| m.as_str()).unwrap_or("Unknown error");
                                    let _ = sender.send(Err(msg.to_string()));
                                } else {
                                    let result = value.get("result").cloned().unwrap_or(Value::Null);
                                    let _ = sender.send(Ok(result));
                                }
                            } else {
                                eprintln!(
                                    "[calcite-reader] NO pending match for id={id}, map has {} entries",
                                    map.len()
                                );
                            }
                        }
                        Err(e) => {
                            log::warn!("[calcite-agent:stdout] failed to parse JSON: {e}: {trimmed}");
                        }
                    }
                }
                Err(e) => {
                    log::error!("[calcite-agent] failed to read stdout: {e}");
                    failed.store(true, Ordering::SeqCst);
                    let mut map = pending.lock().unwrap();
                    for (_, sender) in map.drain() {
                        let _ = sender.send(Err("Agent process terminated".to_string()));
                    }
                    break;
                }
            }
        }

        // 通知所有等待的请求
        failed.store(true, Ordering::SeqCst);
        let mut map = pending.lock().unwrap();
        for (_, sender) in map.drain() {
            let _ = sender.send(Err("Calcite Agent process terminated".to_string()));
        }
    });
}

/// Calcite Agent 生命周期管理器
#[derive(Clone)]
pub struct CalciteAgentManager {
    config: CalciteAgentConfig,
    state: Arc<TokioMutex<CalciteAgentState>>,
    runtime: Arc<TokioMutex<Option<Arc<CalciteAgentRuntime>>>>,
    registered_connections: Arc<TokioMutex<Vec<String>>>,
}

impl std::fmt::Debug for CalciteAgentManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalciteAgentManager")
            .field("config", &self.config)
            .field("state", &"TokioMutex<CalciteAgentState>")
            .finish()
    }
}

impl CalciteAgentManager {
    pub fn new(config: CalciteAgentConfig) -> Self {
        Self {
            config,
            state: Arc::new(TokioMutex::new(CalciteAgentState::Stopped)),
            runtime: Arc::new(TokioMutex::new(None)),
            registered_connections: Arc::new(TokioMutex::new(Vec::new())),
        }
    }

    /// 检查 Agent 是否正在运行
    pub async fn is_running(&self) -> bool {
        // 检查 runtime 是否存在且未失败
        let runtime = self.runtime.lock().await;
        match runtime.as_ref() {
            None => false,
            Some(rt) => !rt.failed.load(Ordering::SeqCst),
        }
    }

    /// 获取已注册连接列表
    pub async fn registered_connections_list(&self) -> Vec<String> {
        self.registered_connections.lock().await.clone()
    }

    /// 注册一个数据连接到 Calcite Agent
    pub async fn register_connection(&self, config: &ConnectionConfig) -> Result<(), String> {
        let runtime_guard = self.runtime.lock().await;
        let runtime = runtime_guard.as_ref().ok_or("Calcite Agent is not running")?;

        let jdbc_url = build_jdbc_url(config)?;
        // 使用 ConnectionConfig 中的 connect_timeout（默认 10 秒，用户可配置），
        // 而不是硬编码 10 秒。这样大库场景下用户可以自行加大超时值。
        let connect_timeout_secs = config.effective_connect_timeout_secs();
        let jdbc_url = with_connect_timeout(&jdbc_url, config.db_type, connect_timeout_secs);
        let driver_class = build_driver_class(config);

        // Hash the password before sending to Java Agent to avoid plaintext transmission.
        // The Agent computes the same SHA-256 hash and uses it as the password token.
        let mut hasher = Sha256::new();
        hasher.update(config.password.as_bytes());
        let password_hash = format!("{:x}", hasher.finalize());

        let params = serde_json::json!({
            "connectionId": config.name,
            "jdbcUrl": jdbc_url,
            "username": config.username,
            "password": config.password,
            "passwordHash": password_hash,
            "driverClass": driver_class,
            "database": config.database.clone().unwrap_or_default(),
            "dbType": format!("{:?}", config.db_type),
        });
        log::info!(
            "[calcite] register_connection params: conn_name={}, db_type={:?}, jdbc_url={}, driver_class={}, username={}, database={:?}",
            config.name, config.db_type, jdbc_url, driver_class, config.username, config.database,
        );
        log::info!(
            "[calcite] register_connection request body: {}",
            serde_json::to_string(&params).unwrap_or_default().replace(&config.password, "***")
        );

        // Calcite 在 JdbcSchema.create() 中会立即加载数据库所有表/列/索引的元数据，
        // 对大型数据库（例如 ihrcore 这种 HR 核心库，可能上千张表）来说这一步可能很久。
        // 因此 RPC 超时使用「查询超时 + 建连超时」之和的上限，且不低于 180 秒；
        // 用户也可通过调大 ConnectionConfig.query_timeout_secs 放宽上限。
        let query_timeout_secs = config.effective_query_timeout_secs();
        let rpc_timeout_secs = if query_timeout_secs == 0 {
            // query_timeout=0 表示用户希望无限制，这里给一个足够大的安全天花板
            600
        } else {
            (query_timeout_secs + connect_timeout_secs).max(180)
        };
        log::info!(
            "[calcite] register_connection RPC timeout: {}s (connect={}s, query={}s)",
            rpc_timeout_secs,
            connect_timeout_secs,
            query_timeout_secs
        );

        let start = std::time::Instant::now();
        let result = runtime.call("registerSource", params, Some(Duration::from_secs(rpc_timeout_secs))).await?;
        let elapsed = start.elapsed();

        let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
        log::info!(
            "[calcite] register_connection conn_name={} success={} elapsed={:.3?}",
            config.name,
            success,
            elapsed
        );

        if !success {
            return Err(result
                .get("error")
                .and_then(|v| v.as_str())
                .unwrap_or("Failed to register connection")
                .to_string());
        }

        let mut registered = self.registered_connections.lock().await;
        if !registered.contains(&config.name) {
            registered.push(config.name.clone());
        }

        log::info!("Registered connection '{}' with Calcite Agent", config.name);
        Ok(())
    }

    /// 通过 Calcite Agent 执行联邦查询
    pub async fn execute_federated_query(
        &self,
        sql: &str,
        cancel_token: Option<CancellationToken>,
    ) -> Result<FederatedQueryResult, String> {
        let runtime_guard = self.runtime.lock().await;
        let runtime = runtime_guard.as_ref().ok_or("Calcite Agent is not running")?;

        let params = serde_json::json!({
            "sql": sql,
            "maxRows": 10000,
            "timeoutMs": 300000,
        });

        let timeout = Duration::from_secs(300);
        let start = std::time::Instant::now();
        let result = if let Some(ref token) = cancel_token {
            tokio::select! {
                _ = token.cancelled() => return Err("Query was cancelled".to_string()),
                result = runtime.call("executeFederatedQuery", params, Some(timeout)) => result?,
            }
        } else {
            runtime.call("executeFederatedQuery", params, Some(timeout)).await?
        };
        let elapsed = start.elapsed();
        log::info!(
            "[calcite] executeFederatedQuery response received in {:.3?}, rowCount from result field={}",
            elapsed,
            result.get("rowCount").and_then(|v| v.as_u64()).unwrap_or(0)
        );

        let success = result.get("success").and_then(|v| v.as_bool()).unwrap_or(false);

        if !success {
            return Err(result.get("error").and_then(|v| v.as_str()).unwrap_or("Query execution failed").to_string());
        }

        let columns: Vec<String> =
            result.get("columns").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();

        let rows: Vec<Vec<Value>> =
            result.get("rows").and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();

        let row_count = result.get("rowCount").and_then(|v| v.as_u64()).unwrap_or(rows.len() as u64) as usize;

        let duration_ms = result.get("durationMs").and_then(|v| v.as_u64()).unwrap_or(0);

        Ok(FederatedQueryResult { columns, rows, row_count, duration_ms })
    }

    /// 启动 Calcite Agent
    pub async fn start(&self, _app_version: &str) -> Result<(), String> {
        // 如果 runtime 已存在且未失败，直接返回
        {
            let runtime = self.runtime.lock().await;
            if let Some(rt) = runtime.as_ref() {
                if !rt.failed.load(Ordering::SeqCst) {
                    let mut state = self.state.lock().await;
                    *state = CalciteAgentState::Running;
                    return Ok(());
                }
            }
        }

        let mut state = self.state.lock().await;
        // 清理旧状态（包括 failed 的 runtime）
        *state = CalciteAgentState::Starting;
        drop(state);

        // 清理旧的 runtime（如果存在且已失败）
        {
            let mut runtime = self.runtime.lock().await;
            if let Some(old_rt) = runtime.take() {
                old_rt.kill();
            }
        }

        // 清除已注册连接列表，新进程需要重新注册
        {
            let mut registered = self.registered_connections.lock().await;
            registered.clear();
        }

        // 验证配置
        if self.config.jar_path.is_empty() {
            let mut state = self.state.lock().await;
            *state = CalciteAgentState::Error("JAR path not configured".to_string());
            return Err("Calcite Agent JAR path is not configured. Please set the JAR path in settings.".to_string());
        }

        if !std::path::Path::new(&self.config.jar_path).exists() {
            let mut state = self.state.lock().await;
            *state = CalciteAgentState::Error(format!("JAR not found: {}", self.config.jar_path));
            return Err(format!(
                "Calcite Agent JAR not found: {}. Please build the JAR first with: cd agents && ./gradlew :drivers:calcite:shadowJar",
                self.config.jar_path
            ));
        }

        // 启动 Java 进程
        log::info!(
            "Starting Calcite Agent: java -Xmx512m -Dorg.slf4j.simpleLogger.defaultLogLevel=debug -jar {}",
            self.config.jar_path
        );
        match CalciteAgentRuntime::spawn(&self.config) {
            Ok(runtime) => {
                // 存储 runtime
                {
                    let mut rt = self.runtime.lock().await;
                    *rt = Some(runtime);
                }
                let mut state = self.state.lock().await;
                *state = CalciteAgentState::Running;
                log::info!("Calcite Agent is now running");
                Ok(())
            }
            Err(e) => {
                let mut state = self.state.lock().await;
                *state = CalciteAgentState::Error(e.clone());
                Err(e)
            }
        }
    }

    /// 停止 Calcite Agent
    pub async fn stop(&self) -> Result<(), String> {
        let mut rt = self.runtime.lock().await;
        if let Some(runtime) = rt.take() {
            runtime.kill();
        }
        let mut state = self.state.lock().await;
        *state = CalciteAgentState::Stopped;
        let mut registered = self.registered_connections.lock().await;
        registered.clear();
        log::info!("Calcite Agent stopped");
        Ok(())
    }

    /// Health check: returns true if the agent responds to ping within timeout.
    pub async fn is_healthy(&self, timeout: Duration) -> bool {
        let rt_guard = self.runtime.lock().await;
        match rt_guard.as_ref() {
            Some(runtime) => runtime.ping(timeout).await.is_ok(),
            None => false,
        }
    }
}

/// 联邦查询结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederatedQueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<Value>>,
    pub row_count: usize,
    pub duration_ms: u64,
}

/// Append SSL query parameters to a JDBC URL based on database type.
fn append_ssl_params(url: &str, ssl: bool) -> String {
    if !ssl {
        return url.to_string();
    }
    let sep = if url.contains('?') { "&" } else { "?" };
    format!("{url}{sep}ssl=true")
}

/// Append a connect timeout to the JDBC URL used by the Calcite Agent so its
/// connections cannot hang indefinitely (which would otherwise trip the Rust-side
/// RPC timeout with "Calcite Agent request timed out").
fn with_connect_timeout(url: &str, db_type: DatabaseType, timeout_secs: u64) -> String {
    // 避免参数重复追加：如果 URL 里已经有 connectTimeout（用户自定义 connection_string 场景），
    // 就尊重用户已经写的值，不再二次覆盖。
    let url_lower = url.to_lowercase();
    if url_lower.contains("connecttimeout") {
        return url.to_string();
    }
    let sep = if url.contains('?') { "&" } else { "?" };
    match db_type {
        // MySQL 系（connectTimeout 单位为毫秒）
        DatabaseType::Mysql
        | DatabaseType::Doris
        | DatabaseType::StarRocks
        | DatabaseType::Goldendb
        | DatabaseType::Gbase
        | DatabaseType::ManticoreSearch => {
            let ms = timeout_secs.saturating_mul(1000);
            format!("{url}{sep}connectTimeout={ms}")
        }
        // PostgreSQL 系（connectTimeout / loginTimeout 单位为秒）
        DatabaseType::Postgres
        | DatabaseType::Redshift
        | DatabaseType::Kingbase
        | DatabaseType::Highgo
        | DatabaseType::Uxdb
        | DatabaseType::Vastbase
        | DatabaseType::Gaussdb
        | DatabaseType::OpenGauss
        | DatabaseType::Kwdb
        | DatabaseType::Oscar => {
            format!("{url}{sep}connectTimeout={timeout_secs}&loginTimeout={timeout_secs}")
        }
        _ => url.to_string(),
    }
}

/// 根据 ConnectionConfig 构建 JDBC URL
pub fn build_jdbc_url(config: &ConnectionConfig) -> Result<String, String> {
    if let Some(ref cs) = config.connection_string {
        if !cs.is_empty() && cs.starts_with("jdbc:") {
            return Ok(cs.clone());
        }
    }

    let host = &config.host;
    let port = config.port;
    let database = config.database.as_deref().unwrap_or("");

    let url = match config.db_type {
        // PostgreSQL 系
        DatabaseType::Postgres
        | DatabaseType::Redshift
        | DatabaseType::Kingbase
        | DatabaseType::Highgo
        | DatabaseType::Uxdb
        | DatabaseType::Vastbase
        | DatabaseType::Gaussdb
        | DatabaseType::OpenGauss
        | DatabaseType::Kwdb
        | DatabaseType::Oscar => {
            format!("jdbc:postgresql://{host}:{port}/{database}")
        }
        // MySQL 系
        DatabaseType::Mysql
        | DatabaseType::Doris
        | DatabaseType::StarRocks
        | DatabaseType::Goldendb
        | DatabaseType::Gbase => {
            format!("jdbc:mysql://{host}:{port}/{database}")
        }
        // SQL Server
        DatabaseType::SqlServer => {
            format!("jdbc:sqlserver://{host}:{port};databaseName={database}")
        }
        // Oracle 系
        DatabaseType::Oracle | DatabaseType::OceanbaseOracle => {
            format!("jdbc:oracle:thin:@//{host}:{port}/{database}")
        }
        // 达梦
        DatabaseType::Dameng => {
            format!("jdbc:dm://{host}:{port}")
        }
        // 人大金仓
        DatabaseType::Yashandb => {
            format!("jdbc:yasdb://{host}:{port}/{database}")
        }
        // H2
        DatabaseType::H2 => config.connection_string.as_deref().unwrap_or("jdbc:h2:mem:test").to_string(),
        // Trino / PrestoSQL
        DatabaseType::Trino | DatabaseType::PrestoSql => {
            format!("jdbc:trino://{host}:{port}/{database}")
        }
        // SAP HANA
        DatabaseType::SapHana => {
            format!("jdbc:sap://{host}:{port}")
        }
        // ClickHouse
        DatabaseType::ClickHouse => {
            format!("jdbc:clickhouse://{host}:{port}/{database}")
        }
        // IBM DB2
        DatabaseType::Db2 => {
            format!("jdbc:db2://{host}:{port}/{database}")
        }
        // Hive
        DatabaseType::Hive => {
            format!("jdbc:hive2://{host}:{port}/{database}")
        }
        // Spark
        DatabaseType::Spark => {
            format!("jdbc:spark://{host}:{port}/{database}")
        }
        // Teradata
        DatabaseType::Teradata => {
            format!("jdbc:teradata://{host}")
        }
        // Vertica
        DatabaseType::Vertica => {
            format!("jdbc:vertica://{host}:{port}/{database}")
        }
        // Firebird
        DatabaseType::Firebird => {
            format!("jdbc:firebirdsql://{host}:{port}/{database}")
        }
        // Exasol
        DatabaseType::Exasol => {
            format!("jdbc:exa:{host}:{port}")
        }
        // Databend
        DatabaseType::Databend => {
            format!("jdbc:databend://{host}:{port}/{database}")
        }
        // Informix
        DatabaseType::Informix => {
            format!("jdbc:informix-sqli://{host}:{port}/{database}:INFORMIXSERVER={}", config.host)
        }
        // Kylin
        DatabaseType::Kylin => {
            format!("jdbc:kylin://{host}:{port}/{database}")
        }
        // 虚谷
        DatabaseType::Xugu => {
            format!("jdbc:xugu://{host}:{port}/{database}")
        }
        // SunDB
        DatabaseType::Sundb => {
            format!("jdbc:sundb://{host}:{port}/{database}")
        }
        // ManticoreSearch (MySQL 协议兼容)
        DatabaseType::ManticoreSearch => {
            format!("jdbc:mysql://{host}:{port}/")
        }
        // MS Access (UCanAccess — 通过 connection_string 指定文件路径)
        DatabaseType::Access => config.connection_string.as_deref().unwrap_or("").to_string(),
        // 云数据仓库
        DatabaseType::Snowflake => config.connection_string.as_deref().unwrap_or("").to_string(),
        // BigQuery (通过 connection_string)
        DatabaseType::Bigquery => config.connection_string.as_deref().unwrap_or("").to_string(),
        // Databricks
        DatabaseType::Databricks => config
            .connection_string
            .as_deref()
            .map(|cs| if cs.starts_with("jdbc:") { cs.to_string() } else { format!("jdbc:databricks://{cs}") })
            .unwrap_or_else(|| "".to_string()),
        // 通用 JDBC
        DatabaseType::Jdbc => config.connection_string.as_deref().unwrap_or("").to_string(),
        _ => {
            return Err(format!("Unsupported database type for federation: {:?}", config.db_type));
        }
    };

    // SSL parameters (deduplicated helper)
    if config.ssl {
        match config.db_type {
            DatabaseType::Postgres
            | DatabaseType::Redshift
            | DatabaseType::Kingbase
            | DatabaseType::Highgo
            | DatabaseType::Uxdb
            | DatabaseType::Vastbase
            | DatabaseType::Gaussdb
            | DatabaseType::OpenGauss
            | DatabaseType::Kwdb
            | DatabaseType::Oscar
            | DatabaseType::ClickHouse => {
                return Ok(append_ssl_params(&url, true));
            }
            DatabaseType::Mysql
            | DatabaseType::Doris
            | DatabaseType::StarRocks
            | DatabaseType::Goldendb
            | DatabaseType::Gbase => {
                let sep = if url.contains('?') { "&" } else { "?" };
                return Ok(format!("{url}{sep}useSSL=true&requireSSL=true"));
            }
            DatabaseType::SqlServer => {
                return Ok(format!("{url};encrypt=true;trustServerCertificate=true"));
            }
            DatabaseType::Oracle | DatabaseType::OceanbaseOracle => {
                // Oracle SSL via system properties — return URL as-is
                return Ok(url);
            }
            DatabaseType::Db2 => {
                let sep = if url.contains(':') { ":" } else { "?" };
                return Ok(format!("{url}{sep}sslConnection=true"));
            }
            DatabaseType::Trino | DatabaseType::PrestoSql => {
                let sep = if url.contains('?') { "&" } else { "?" };
                return Ok(format!("{url}{sep}SSL=true"));
            }
            DatabaseType::Hive => {
                let sep = if url.contains(';') { ";" } else { "?" };
                return Ok(format!("{url}{sep}ssl=true"));
            }
            _ => {
                return Ok(append_ssl_params(&url, true));
            }
        }
    }

    // URL parameters
    if let Some(ref params) = config.url_params {
        if !params.is_empty() {
            let sep = if url.contains('?') { "&" } else { "?" };
            return Ok(format!("{url}{sep}{params}"));
        }
    }

    Ok(url)
}

/// 获取 JDBC 驱动类名
pub fn build_driver_class(config: &ConnectionConfig) -> String {
    match config.db_type {
        // PostgreSQL 系（含国产 PG 兼容数据库）
        DatabaseType::Postgres
        | DatabaseType::Redshift
        | DatabaseType::Kingbase
        | DatabaseType::Highgo
        | DatabaseType::Uxdb
        | DatabaseType::Vastbase
        | DatabaseType::Gaussdb
        | DatabaseType::OpenGauss
        | DatabaseType::Kwdb
        | DatabaseType::Oscar => "org.postgresql.Driver".to_string(),
        // MySQL 系
        DatabaseType::Mysql
        | DatabaseType::Doris
        | DatabaseType::StarRocks
        | DatabaseType::Goldendb
        | DatabaseType::Gbase => "com.mysql.cj.jdbc.Driver".to_string(),
        // SQL Server
        DatabaseType::SqlServer => "com.microsoft.sqlserver.jdbc.SQLServerDriver".to_string(),
        // Oracle 系
        DatabaseType::Oracle | DatabaseType::OceanbaseOracle => "oracle.jdbc.OracleDriver".to_string(),
        // 达梦
        DatabaseType::Dameng => "dm.jdbc.driver.DmDriver".to_string(),
        // 人大金仓
        DatabaseType::Yashandb => "com.yashandb.jdbc.Driver".to_string(),
        // H2
        DatabaseType::H2 => "org.h2.Driver".to_string(),
        // Trino / PrestoSQL
        DatabaseType::Trino | DatabaseType::PrestoSql => "io.trino.jdbc.TrinoDriver".to_string(),
        // SAP HANA
        DatabaseType::SapHana => "com.sap.db.jdbc.Driver".to_string(),
        // Snowflake
        DatabaseType::Snowflake => "net.snowflake.client.jdbc.SnowflakeDriver".to_string(),
        // ClickHouse
        DatabaseType::ClickHouse => "com.clickhouse.jdbc.ClickHouseDriver".to_string(),
        // IBM DB2
        DatabaseType::Db2 => "com.ibm.db2.jcc.DB2Driver".to_string(),
        // Hive
        DatabaseType::Hive => "org.apache.hive.jdbc.HiveDriver".to_string(),
        // Spark
        DatabaseType::Spark => "com.simba.spark.jdbc.Driver".to_string(),
        // Teradata
        DatabaseType::Teradata => "com.teradata.jdbc.TeraDriver".to_string(),
        // Vertica
        DatabaseType::Vertica => "com.vertica.jdbc.VerticaDriver".to_string(),
        // Firebird
        DatabaseType::Firebird => "org.firebirdsql.jdbc.FBDriver".to_string(),
        // Exasol
        DatabaseType::Exasol => "com.exasol.jdbc.EXADriver".to_string(),
        // Databend
        DatabaseType::Databend => "com.databend.jdbc.DatabendDriver".to_string(),
        // Informix
        DatabaseType::Informix => "com.informix.jdbc.IfxDriver".to_string(),
        // Kylin
        DatabaseType::Kylin => "org.apache.kylin.jdbc.Driver".to_string(),
        // 虚谷
        DatabaseType::Xugu => "com.xugu.jdbc.XuguDriver".to_string(),
        // SunDB
        DatabaseType::Sundb => "com.sundb.jdbc.Driver".to_string(),
        // ManticoreSearch (MySQL 协议兼容)
        DatabaseType::ManticoreSearch => "com.mysql.cj.jdbc.Driver".to_string(),
        // MS Access (UCanAccess)
        DatabaseType::Access => "net.ucanaccess.jdbc.UcanaccessDriver".to_string(),
        // Databricks
        DatabaseType::Databricks => "com.databricks.client.jdbc.Driver".to_string(),
        // BigQuery (Simba)
        DatabaseType::Bigquery => "com.simba.googlebigquery.jdbc42.Driver".to_string(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::connection::DatabaseType;
    use std::collections::HashMap;

    fn make_test_conn(db_type: DatabaseType, host: &str, port: u16, database: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: "test".to_string(),
            name: "test_conn".to_string(),
            note: String::new(),
            db_type,
            driver_profile: None,
            driver_label: None,
            url_params: None,
            agent_java_options: Vec::new(),
            host: host.to_string(),
            port,
            username: "user".to_string(),
            password: "pass".to_string(),
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

    #[test]
    fn test_build_jdbc_url_postgres() {
        let config = make_test_conn(DatabaseType::Postgres, "localhost", 5432, "testdb");
        let url = build_jdbc_url(&config).unwrap();
        assert_eq!(url, "jdbc:postgresql://localhost:5432/testdb");
    }

    #[test]
    fn test_build_jdbc_url_mysql() {
        let config = make_test_conn(DatabaseType::Mysql, "localhost", 3306, "testdb");
        let url = build_jdbc_url(&config).unwrap();
        assert_eq!(url, "jdbc:mysql://localhost:3306/testdb");
    }

    #[test]
    fn test_build_jdbc_url_with_ssl() {
        let mut config = make_test_conn(DatabaseType::Postgres, "localhost", 5432, "testdb");
        config.ssl = true;
        let url = build_jdbc_url(&config).unwrap();
        assert_eq!(url, "jdbc:postgresql://localhost:5432/testdb?ssl=true");
    }

    #[test]
    fn test_build_jdbc_url_with_connection_string() {
        let mut config = make_test_conn(DatabaseType::Postgres, "localhost", 5432, "testdb");
        config.connection_string = Some("jdbc:postgresql://customhost:5433/customdb".to_string());
        let url = build_jdbc_url(&config).unwrap();
        assert_eq!(url, "jdbc:postgresql://customhost:5433/customdb");
    }

    #[test]
    fn test_build_driver_class() {
        let pg = make_test_conn(DatabaseType::Postgres, "h", 5432, "db");
        assert_eq!(build_driver_class(&pg), "org.postgresql.Driver");

        let mysql = make_test_conn(DatabaseType::Mysql, "h", 3306, "db");
        assert_eq!(build_driver_class(&mysql), "com.mysql.cj.jdbc.Driver");
    }

    #[test]
    fn test_build_jdbc_url_extended_types() {
        // SQL Server
        let mssql = make_test_conn(DatabaseType::SqlServer, "localhost", 1433, "testdb");
        assert_eq!(build_jdbc_url(&mssql).unwrap(), "jdbc:sqlserver://localhost:1433;databaseName=testdb");

        // Oracle
        let oracle = make_test_conn(DatabaseType::Oracle, "localhost", 1521, "ORCL");
        assert_eq!(build_jdbc_url(&oracle).unwrap(), "jdbc:oracle:thin:@//localhost:1521/ORCL");

        // ClickHouse
        let ch = make_test_conn(DatabaseType::ClickHouse, "localhost", 8123, "testdb");
        assert_eq!(build_jdbc_url(&ch).unwrap(), "jdbc:clickhouse://localhost:8123/testdb");

        // Trino
        let trino = make_test_conn(DatabaseType::Trino, "localhost", 8080, "testdb");
        assert_eq!(build_jdbc_url(&trino).unwrap(), "jdbc:trino://localhost:8080/testdb");

        // DB2
        let db2 = make_test_conn(DatabaseType::Db2, "localhost", 50000, "testdb");
        assert_eq!(build_jdbc_url(&db2).unwrap(), "jdbc:db2://localhost:50000/testdb");

        // Hive
        let hive = make_test_conn(DatabaseType::Hive, "localhost", 10000, "testdb");
        assert_eq!(build_jdbc_url(&hive).unwrap(), "jdbc:hive2://localhost:10000/testdb");

        // ManticoreSearch (MySQL 协议兼容)
        let manticore = make_test_conn(DatabaseType::ManticoreSearch, "localhost", 9306, "");
        assert_eq!(build_jdbc_url(&manticore).unwrap(), "jdbc:mysql://localhost:9306/");

        // Databend
        let databend = make_test_conn(DatabaseType::Databend, "localhost", 8124, "testdb");
        assert_eq!(build_jdbc_url(&databend).unwrap(), "jdbc:databend://localhost:8124/testdb");
    }

    #[test]
    fn test_build_driver_class_extended() {
        // SunDB (修复了拼写错误)
        let sundb = make_test_conn(DatabaseType::Sundb, "h", 1, "db");
        assert_eq!(build_driver_class(&sundb), "com.sundb.jdbc.Driver");

        // YashanDB
        let yashan = make_test_conn(DatabaseType::Yashandb, "h", 1, "db");
        assert_eq!(build_driver_class(&yashan), "com.yashandb.jdbc.Driver");

        // ManticoreSearch (MySQL 驱动)
        let manticore = make_test_conn(DatabaseType::ManticoreSearch, "h", 1, "db");
        assert_eq!(build_driver_class(&manticore), "com.mysql.cj.jdbc.Driver");
    }

    /// 测试响应 ID 解析：Java 侧可能返回字符串或数字形式的 id
    /// 此测试模拟了之前的 bug：Java 返回 id:"1"（字符串），Rust 用 as_u64() 解析失败
    #[test]
    fn test_response_id_parsing_numeric() {
        let value: serde_json::Value = serde_json::json!({"id": 1, "result": {"success": true}});
        let id = value
            .get("id")
            .and_then(|v| v.as_u64())
            .or_else(|| value.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        assert_eq!(id, 1);
    }

    #[test]
    fn test_response_id_parsing_string() {
        // Java 侧使用 ObjectNode.put("id", String) 产生字符串形式的 id
        let value: serde_json::Value = serde_json::json!({"id": "2", "result": {"success": true}});
        let id = value
            .get("id")
            .and_then(|v| v.as_u64())
            .or_else(|| value.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        assert_eq!(id, 2);
    }

    #[test]
    fn test_response_id_parsing_missing() {
        let value: serde_json::Value = serde_json::json!({"result": {"success": true}});
        let id = value
            .get("id")
            .and_then(|v| v.as_u64())
            .or_else(|| value.get("id").and_then(|v| v.as_str()).and_then(|s| s.parse().ok()))
            .unwrap_or(0);
        assert_eq!(id, 0);
    }

    #[test]
    fn test_build_jdbc_url_ssl_extended() {
        // SQL Server SSL
        let mssql_ssl = {
            let mut c = make_test_conn(DatabaseType::SqlServer, "localhost", 1433, "testdb");
            c.ssl = true;
            c
        };
        let url = build_jdbc_url(&mssql_ssl).unwrap();
        assert!(url.contains("encrypt=true"));
        assert!(url.contains("trustServerCertificate=true"));

        // ClickHouse SSL
        let ch_ssl = {
            let mut c = make_test_conn(DatabaseType::ClickHouse, "localhost", 8123, "testdb");
            c.ssl = true;
            c
        };
        let url = build_jdbc_url(&ch_ssl).unwrap();
        assert!(url.contains("ssl=true"));

        // DB2 SSL
        let db2_ssl = {
            let mut c = make_test_conn(DatabaseType::Db2, "localhost", 50000, "testdb");
            c.ssl = true;
            c
        };
        let url = build_jdbc_url(&db2_ssl).unwrap();
        assert!(url.contains("sslConnection=true"));

        // Trino SSL
        let trino_ssl = {
            let mut c = make_test_conn(DatabaseType::Trino, "localhost", 8080, "testdb");
            c.ssl = true;
            c
        };
        let url = build_jdbc_url(&trino_ssl).unwrap();
        assert!(url.contains("SSL=true"));
    }

    #[test]
    fn test_build_jdbc_url_pg_compatible() {
        // GaussDB
        let gaussdb = make_test_conn(DatabaseType::Gaussdb, "localhost", 5432, "testdb");
        assert_eq!(build_jdbc_url(&gaussdb).unwrap(), "jdbc:postgresql://localhost:5432/testdb");

        // OpenGauss
        let opengauss = make_test_conn(DatabaseType::OpenGauss, "localhost", 5432, "testdb");
        assert_eq!(build_jdbc_url(&opengauss).unwrap(), "jdbc:postgresql://localhost:5432/testdb");

        // Kingbase
        let kingbase = make_test_conn(DatabaseType::Kingbase, "localhost", 54321, "testdb");
        assert_eq!(build_jdbc_url(&kingbase).unwrap(), "jdbc:postgresql://localhost:54321/testdb");
    }

    #[test]
    fn test_calcite_agent_config_default() {
        let config = CalciteAgentConfig::default();
        assert_eq!(config.java_path, "java");
        assert!(config.jar_path.is_empty());
    }
}

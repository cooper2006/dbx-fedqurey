# 联邦查询实现说明

## 概述

联邦查询功能允许跨多个数据库连接执行联合查询。本文档描述了已完成的实现工作。

## 已完成的工作

### Phase 1 (P0) - 核心后端

#### 1.1 ConnectionConfig 扩展
**位置**: `crates/dbx-core/src/models/connection.rs:180`

新增字段：
```rust
#[serde(default = "default_federation_enabled")]
pub federation_enabled: bool,
```

默认值为 `false`，可通过 API 启用。

#### 1.2 FederatedResolver 核心模块
**位置**: `crates/dbx-core/src/federated.rs`

实现了以下核心功能：
- `analyze_federation(sql, connections)` - 解析 SQL 并检测联邦模式
- `rewrite_federated_sql(sql, analysis)` - 重写单连接联邦 SQL
- `FederatedTableRef` - 表引用元数据结构
- `FederatedAnalysis` - 联邦分析结果结构

关键特性：
- 支持 `connection.schema.table` 命名约定
- 自动识别单连接 vs 多连接场景
- 单连接时自动去除前缀执行

#### 1.3 单连接快速路径集成
**位置**: `crates/dbx-core/src/query.rs`

在 `execute_sql_statement_with_options()` 入口添加了联邦检测逻辑：
1. 检测 SQL 是否包含联邦语法
2. 单连接：自动重写 SQL（去掉 connection 前缀）后正常执行
3. 多连接：返回错误提示用户启动 Calcite Agent

#### 1.4 Agent 目录注册
**位置**: 
- `crates/dbx-core/src/models/connection.rs` - 新增 `DatabaseType::Calcite`、`VictoriaMetrics`、`Mqtt`
- `crates/dbx-core/src/agent_catalog.rs` - 添加 Calcite Catalog Entry

```rust
AgentCatalogEntry {
    db_type: DatabaseType::Calcite,
    key: "calcite",
    label: "Apache Calcite (Federated)",
    store_visible: true,
    profiles: &[],
}
```

#### 1.6 新增数据库类型支持
**位置**: `crates/dbx-core/src/models/connection.rs`

新增数据库类型：
- `DatabaseType::VictoriaMetrics` - VictoriaMetrics 时序数据库支持
- `DatabaseType::Mqtt` - MQTT 消息队列支持

**位置**: `crates/dbx-core/src/connection.rs`

- 添加 `PoolKind::VictoriaMetrics` 和 `PoolKind::Mqtt` 枚举变体
- 实现连接池创建逻辑（`DatabaseType::VictoriaMetrics` / `DatabaseType::Mqtt`）
- 添加连接健康检查和 keepalive 支持
- 修复 SSH 隧道 `allow_exec_channel_proxy` 参数传递

#### 1.5 AppState 集成
**位置**: `crates/dbx-core/src/connection.rs`

AppState 新增字段：
```rust
pub calcite_agent: Arc<Mutex<Option<crate::calcite_agent::CalciteAgentManager>>>
```

### Phase 2 (P1) - Calcite Agent ✅ 已完成

#### 2.1 Rust 侧 Calcite Agent 生命周期管理
**位置**: `crates/dbx-core/src/calcite_agent.rs`

已实现完整功能：
- `CalciteAgentConfig` - 支持 `CALCITE_ENGINE` 环境变量配置执行引擎
- `CalciteAgentState` - 状态枚举（Stopped/Starting/Running/Error）
- `CalciteAgentRuntime` - JSON-RPC over stdin/stdout 通信
- `CalciteAgentManager` - 管理器，支持单例模式
- `build_jdbc_url()` / `build_driver_class()` - JDBC URL 和驱动类构建
- SSL 参数统一处理（提取 `append_ssl_params` 辅助函数）
- 密码哈希传输（SHA-256 before sending to Java Agent）
- 健康检查（`ping()` 方法）

#### 2.2 Java Calcite Agent
**位置**: `agents/drivers/calcite/src/main/java/com/dbx/agent/calcite/CalciteAgent.java`

已实现完整功能：
- JSON-RPC 2.0 over stdin/stdout 协议
- 线程安全的数据源注册/注销（`synchronized(calciteLock)`）
- `passwordHash` 支持（与 Rust 端 SHA-256 哈希匹配）
- 执行引擎选择（通过 `CALCITE_ENGINE` 环境变量）
- `SimpleDataSource` 包装器
- 联邦 SQL 重写（3-part 和 4-part 命名）

#### 2.3 集成测试
**位置**: `agents/drivers/calcite/src/test/java/com/dbx/agent/calcite/CalciteAgentFederatedIntegrationTest.java`
**位置**: `crates/dbx-core/tests/e2e_federated_query.rs`

### Phase 3 (P2) - 前端增强

#### 3.1 联邦感知格式化器
**位置**: `apps/desktop/src/lib/federated/federatedFormatter.ts`

提供了：
- `formatFederatedSql()` - 保持联邦语法格式化 SQL
- `analyzeFederatedSql()` - 分析 SQL 中的联邦模式
- `stripFederationPrefixes()` - 去除联邦前缀
- `addFederationPrefixes()` - 添加联邦前缀

#### 3.2 方言自动检测
**位置**: `apps/desktop/src/lib/federated/dialectDetector.ts`

实现了：
- `autoDetectDialect()` - 基于 SQL 特征自动检测方言
- `getQuoteCharacter()` - 获取方言相关的引号字符
- `quoteIdentifier()` - 对标识符加引号
- `formatTableReference()` - 格式化表引用
- `isFederatedSql()` - 判断是否为联邦查询
- `getFormatterConfig()` - 获取格式化器配置

## 设计决策

### 透明联邦查询 vs 显式前缀

原始设计假设用户需要在 SQL 中手动输入连接前缀（如 `conn1.public.users`）。
但架构评审报告建议采用**透明重定向**模式：

- **用户写普通 SQL**: `SELECT * FROM users WHERE id = 1`
- **后端自动检测**: 如果 users 属于不同连接的 schema，重写为联邦语法
- **单连接优化**: 自动去除连接前缀执行

当前实现采用的是**混合模式**：
- 支持显式联邦语法 `connection.schema.table`
- 单连接时自动重写并去除前缀
- 多连接时提示需要 Calcite Agent

### SQL 命名规则

根据架构评审修正了原始设计中的误解：

| 数据库类型 | 目标 SQL 格式 | 示例 |
|-----------|--------------|------|
| PostgreSQL | `connection.db.schema."table" alias` | `myconn.mydb.public."users" u` |
| MySQL | `connection.db."table" alias` | `myconn.shop."orders" o` |

注意：原始设计中"不添加任何引号"的说法是错误的。PostgreSQL 规范要求双引号，MySQL 使用反引号。

连接名匹配**不区分大小写**：前缀对应已保存的 `connection.name`，例如 `postgresql.` 可命中连接 `PostgreSQL`，匹配到的引用按连接配置中的规范名称处理（见 `crates/dbx-core/src/federated.rs` 的 `analyze_federation`/`extract_table_refs`）。

## 已知限制

1. **多连接联邦查询需要 Calcite Agent** - Phase 2 尚未完成
2. **仅支持 SELECT** - UPDATE/INSERT/DELETE 不在 Phase 1 范围
3. **Schema 可见性控制** - 未实现敏感表的过滤
4. **方言检测启发式** - 当前基于简单字符串匹配，可能需要更精确的 AST 分析

## 代码审查修复 (2026-08-05)

| 级别 | 问题 | 修复内容 |
|------|------|----------|
| P0 | 密码明文传输 | SHA-256 哈希后通过 `passwordHash` 字段发送 |
| P1 | 线程安全 | `synchronized(calciteLock)` 保护注册/注销操作 |
| P2 | 大小写不一致 | 统一 `validate_federation` 使用小写匹配 |
| P2 | SSL 重复代码 | 提取 `append_ssl_params()` 辅助函数 |
| P2 | 健康检查缺失 | 新增 `ping()` 和 `is_healthy()` 方法 |
| P2 | 引擎硬编码 | 支持 `CALCITE_ENGINE` 环境变量 |

**测试结果**: 20 个单元测试全部通过 ✅

---

## 联邦查询稳定性修复 (2026-08-07)

### 问题 1: 连接名被误判为 catalog 导致 "Backend request failed"

联邦查询使用连接名作为表引用前缀（如 `doris.freequery.DIM_BM_AD_PS`）。可编辑性分析在解析 3-part 名称时，将连接名作为 catalog 参数传递给后端元数据接口。由于连接名并非真实的数据库 catalog，Doris 等引擎会返回 "Backend request failed" 错误。

| 层级 | 文件 | 修复内容 |
|------|------|----------|
| 前端 | `apps/desktop/src/stores/queryStore.ts` | `resolveEditableSourceMetadataTarget()` 检测 catalog 前缀是否匹配某个连接名，若匹配则剥离 catalog，避免传递给后端；同时将元数据请求重定向到联邦目标连接 |
| 后端 | `crates/dbx-core/src/schema.rs` | `resolve_external_doris_catalog()` 在 catalog 与连接名匹配时返回 `None`，回退到默认元数据路径 |

**跨连接联邦元数据重定向**：当 catalog 前缀匹配的是另一个连接（而非当前查询标签页的连接）时，元数据请求自动重定向到被引用的连接，确保从正确的数据库引擎获取列信息。

**非 schema 感知数据库处理**：对 Doris、MySQL 等无 schema 概念的数据库，3-part 联邦名称的第二部分被识别为数据库名（而非 schema），从而正确获取列元数据。

### 问题 2: 含特殊字符的连接名导致 SQL 解析失败

连接名含连字符（如 `doris-Local`）时，SQL 解析器将 `doris-Local` 解释为算术表达式 `doris - Local`，而非单个标识符，导致联邦语法检测失败。

| 文件 | 修复内容 |
|------|----------|
| `crates/dbx-core/src/federated.rs` | 新增 `preprocess_federated_sql()` 函数：在解析前自动为含特殊字符的连接名添加双引号（如 `doris-Local` → `"doris-Local"`）；新增 `validate_connection_name()` 函数：在创建/编辑连接时提前校验名称合法性 |
| `crates/dbx-core/src/federated.rs` | SQL 方言从 `PostgreSqlDialect` 切换为 `GenericDialect`，以支持更广泛的语法兼容性 |

新增测试：
- `test_hyphenated_connection_name` - 含连字符连接名的检测与重写
- `test_hyphenated_connection_name_case_insensitive` - 大小写不敏感匹配
- `test_normal_connection_name_unaffected_by_preprocessing` - 普通连接名不受影响

### 问题 3: Web 端 session 不持久化

Web 服务重启后所有登录 session 丢失，用户需重新登录。

| 文件 | 修复内容 |
|------|----------|
| `crates/dbx-web/src/auth.rs` | 新增 `persist_sessions()` / `restore_sessions()` 函数：将 session token 持久化到 SQLite，服务器启动时自动恢复 |
| `crates/dbx-web/src/main.rs` | 启动时调用 `restore_sessions()` 恢复持久化 session |

### 问题 4: 浏览器刷新后 session 丢失

浏览器刷新后前端未携带 session cookie，导致 API 返回 401 认证失败。

| 文件 | 修复内容 |
|------|----------|
| `apps/desktop/src/lib/backend/http.ts` | `get()`/`del()` 函数添加 `credentials: "include"`，确保 fetch 请求携带 session cookie |

### 问题 5: Calcite Agent 启动超时

联邦查询时 Calcite Agent 启动失败，报错 "Agent process closed stdout during startup"。

| 文件 | 修复内容 |
|------|----------|
| `crates/dbx-core/src/calcite_agent.rs` | 修复 `wait_for_ready()` 的 timeout 参数被忽略问题，增加启动超时到 60 秒，添加超时检查逻辑 |

---

## 下一步计划

### 已完成 ✅
- [x] P0-P2 代码审查修复
- [x] Java Calcite Agent 完整实现
- [x] JSON-RPC 通信协议
- [x] 前端联邦感知格式化器集成
- [x] 联邦表名级联补全
- [x] 端到端测试（29 个 Rust 测试 + Java 集成测试）

### 后续优化
- [ ] 多连接查询谓词下推优化
- [ ] 百万级数据量性能调优
- [ ] Spark 执行引擎完整支持
- [ ] Schema 可见性细粒度控制

## 文件变更清单

### Rust (crates/dbx-core/src/)
- `lib.rs` - 模块声明顺序调整
- `models/connection.rs` - 添加 DatabaseType::Calcite/VictoriaMetrics/Mqtt 和 federation_enabled 字段
- `agent_catalog.rs` - 添加 Calcite Catalog Entry
- `query.rs` - 集成联邦分析逻辑
- `connection.rs` - AppState 添加 calcite_agent 字段、VictoriaMetrics/Mqtt 连接池支持
- `federated.rs` - 核心联邦逻辑实现
- `calcite_agent.rs` - Calcite Agent 生命周期管理
- `federation_schema_visibility.rs` - 联邦模式 Schema 可见性控制

### TypeScript (apps/desktop/src/)
- `lib/federated/federatedFormatter.ts` - 新建
- `lib/federated/dialectDetector.ts` - 新建

## 参考文档

- `design/architecture_review_federated_query.md` - 架构评审报告
- `design/federated_query_sql_formatter_design.md` - SQL 格式化设计
- `design/ui_ux_optimization_federated_formatter.md` - UI/UX 优化设计

---
*实现日期: 2026-08-03*
*最近更新: 2026-08-07*
*版本: 1.4*
*状态: Phase 1-4 全部完成，P0-P2 审查修复已完成，新增 VictoriaMetrics/Mqtt 支持，修复连接名误判 catalog 及含特殊字符连接名解析问题，修复 session cookie 传递和 Calcite Agent 启动超时*

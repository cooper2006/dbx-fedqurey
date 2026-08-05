# 联邦查询落地开发 TODO

## 项目状态评估 (2026-08-05)

联邦查询功能已基本全部落地。本文件记录各阶段的真实落地状态，以及近期完成项的增量变更。

### 已完成部分

#### Phase 1 (P0) - 核心后端
- [x] **1.1 ConnectionConfig 扩展**
  - 位置: `crates/dbx-core/src/models/connection.rs`
  - 字段: `federation_enabled: bool` (默认值 `false`)
  - 通过 `federated::federation_enabled(config)` 读取，默认由 `calcite::try_ensure_calcite_dep()` 推断

- [x] **1.2 FederatedResolver 核心模块**
  - 位置: `crates/dbx-core/src/federated.rs`
  - 功能: SQL AST 解析、联邦表引用检测、多连接/单连接判定、4-part 命名
  - 导出: `analyze_federation()`, `validate_federation()`, `rewrite_federated_sql()`
  - 包含完整单元测试（5 个通过）

- [x] **1.3 集成 federated 模块到 query.rs**
  - `query.rs` 已调用联邦分析；多连接场景走 Calcite Agent 执行路径
  - e2e 覆盖单连接快速路径与多连接决策

- [x] **1.4 Agent 目录注册（calcite 类型）**
  - `agent_catalog.rs` 已注册 Calcite 条目，`FederatedSchemaFactory` 实现标准 `SchemaFactory`

- [x] **1.5 连接树联邦状态图标**
  - 前端连接树节点已显示联邦启用/禁用状态

#### Phase 2 (P1) - Calcite Agent
- [x] **2.1 Java Calcite Agent 项目骨架**
  - 位置: `agents/drivers/calcite/`
  - 双执行引擎: `-Pengine=spark`（Spark 4.0）与默认 enumerable
  - `build.gradle` 已将驱动精简为 Maven Central 可用版本

- [x] **2.2 Calcite 联邦执行服务**
  - 采用 **JSON-RPC over stdin/stdout** 协议（早期 gRPC 方案已废弃，见设计决策 5）
  - 每个 JDBC 连接经 `JdbcSchema` 注册为联邦 Schema
  - 启动脚本: `agents/drivers/calcite/scripts/start-enumerable.sh`、`start-spark.sh`

- [x] **2.3 Rust 侧 Calcite Agent 生命周期管理**
  - 位置: `crates/dbx-core/src/calcite_agent.rs`
  - 已实现: JSON-RPC 客户端、进程启动/停止、JDBC URL/驱动构建、`register_connection()`、`execute_federated_query()`
  - 单元测试 10 个通过

#### Phase 3 (P2) - 前端增强
- [x] **3.1 联邦感知格式化器（federatedFormatter.ts）**
  - 已接线到 `sqlFormatter.ts`，保护 `connection.schema.table` 多部分标识符

- [x] **3.2 方言自动检测（dialectDetector.ts）**
  - `autoDetectDialect()` 已接线到 `formatSqlText()`：当方言未显式指定时自动推断

- [x] **3.3 编辑器联邦状态栏（FederatedQueryStatusBar.vue）**
  - 已接线到 `QueryEditor.vue`（编辑器底部），联邦/启用联邦连接时显示

- [x] **3.4 联邦表名级联补全**
  - `sqlCompletion.ts` 新增 `buildFederatedTableItems()`：顶层补全连接名，`<连接>.` 后补全该连接的表
  - 前端注入 `federatedConnections` 与 `federatedTablesByConnection`
  - 单测: `packages/app-tests/sqlCompletionFederated.test.ts`（4 个通过）

#### Phase 4 (P3) - 集成测试
- [x] **4.1 单元测试**
  - Rust: `federated` / `calcite_agent` 模块单测通过
  - 前端: `identifierQuotes.test.ts`（15）、`sqlCompletionFederated.test.ts`（4）通过

- [x] **4.2 端到端测试**
  - `crates/dbx-core/tests/e2e_federated_query.rs`（29 个通过）：多连接检测、CTE/UNION/JOIN、4-part 命名、SSL URL、联邦启用校验、JAR 缺失错误路径
  - Java 集成测试: `CalciteAgentFederatedIntegrationTest.java`（两个 H2 内存库模拟跨库查询），由 CI 显式运行

- [x] **4.3 文档更新**
  - `FEDERATED_QUERY_SUMMARY.md`、本 TODO、`design/*` 均已同步

### 近期修复记录 (2026-08-05)

1. **接线联邦状态栏** — `FederatedQueryStatusBar.vue` 从死代码接入 `QueryEditor.vue` 底部（编辑器占 `flex-1`，状态栏占底部一条）。
2. **接线方言检测器** — `dialectDetector.ts` 从死代码接入 `sqlFormatter.ts::formatSqlText`（dialect 为 `generic` 时自动推断；Oracle/DuckDB 映射为 PostgreSQL 语法）。
3. **实现联邦级联补全** — `sqlCompletion.ts` 新增连接名→跨连接表补全，前端注入连接数据，含单测。
4. **删除遗留死代码** — 移除 `crates/dbx-core/src/federation_grpc.rs` 及 `lib.rs` 声明（gRPC 方案已被 JSON-RPC 取代）。
5. **清理 i18n 死键** — 移除 `en.ts` 中 5 个无引用的平铺联邦键（`federationEnabled` 等）；真正的嵌套 `federation` 对象两语言 key 已对齐。
6. **补 Java 集成测试 CI** — `ci.yml` 新增显式的 `Calcite federated integration tests` 步骤，独立运行 `CalciteAgentFederatedIntegrationTest`。

### 已知限制 / 后续可扩展

- 跨连接**表名**补全目前注入的是当前连接的表（4-part 限定）；其他连接的表元数据需后端联邦元数据接口按需加载（后续扩展）。
- 真实跨库端到端（spawn JAR + 两个真实连接 JOIN）未在无 Java 运行时的本地环境自动化执行，由 CI 的 JUnit 集成测试覆盖。

---

## 关键设计决策

1. **联邦查询应透明重定向** - 用户写普通 SQL，后端自动检测表归属并重写
2. **命名规则**:
   - PostgreSQL: `connection.db.schema."table" alias`
   - MySQL: `connection.db."table" alias`
   - 统一支持 4-part: `connection.database.schema.table`
3. **Phase 1 策略**: 分片合并（Shard-and-Merge），简单场景足够
4. **Phase 2 策略**: 接入 Calcite 支持复杂 JOIN
5. **进程间通信协议**: 早期架构曾考虑 gRPC；实际实现采用 **JSON-RPC over stdin/stdout**（无需额外端口、依赖更少），相关 gRPC 死代码已移除

---

*最后更新: 2026-08-05*

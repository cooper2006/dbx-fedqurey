# WORK_LOG

## 2026-08-17

### 修复前端缺少 opentenbase DatabaseType

**背景**：`databaseSupport.ts` 中有 7 个"未知连接选项"（opentenbase、meilisearch、elasticsearch、qdrant、milvus、weaviate、chromadb），经逐一核查后端 Rust 代码：
- **elasticsearch**、**meilisearch**、**qdrant**、**milvus**、**weaviate**、**chromadb**：后端已有完整的 `DatabaseType` 枚举值、`PoolKind`、驱动实现，前端 `DatabaseType` 类型也已包含。
- **opentenbase**：后端以 `driver_profile: Some("opentenbase")` + `DatabaseType::Postgres` 方式识别，属于 PostgreSQL 兼容层；但前端 TypeScript `DatabaseType` 联合类型中缺失 `"opentenbase"` 条目，导致该数据库在前端显示为未知。

**修改文件**：
- `apps/desktop/src/types/database.ts`：在 `duckdb` 和 `clickhouse` 之间新增 `| "opentenbase"`

**验证**：TypeScript 编译环境因权限限制无法运行 `tsc`；手动比对 `docs/data/databaseSupport.ts` 的 id 集合与 `database.ts` 的 DatabaseType 联合类型，除以下 19 个数据库外其余均已覆盖：
- 后端使用 `driver_profile` 而非独立 `DatabaseType`：cloudberry, cockroachdb, dolt, greatsql, mariadb, oceanbase, phoenix, polardb, selectdb, tdsql, tidb
- JDBC/消息队列/特殊用途：jdbcx, kafka, rabbitmq, request（链接到 GitHub Discussions，非真实 DB）
- 这些 19 个不在这 7 个目标之内，属于其他已知缺失项。

---

## 项目操作约束

- 默认使用中文回复。
- 只允许操作当前项目文件夹。
- 禁止删除任何文件。
- 需要确认的操作先问用户。
- 无法判断的文件放入「待确认清单」，不强行处理。
- 每次实际修改后更新本文件。
- 每轮只读取 AGENTS.md、本文件最新状态和本轮相关文件。
- 不重复扫描整个项目，不重复读取无关文档。
- 只修改当前任务必要文件，不顺手重构、格式化、升级依赖或扩展功能。
- 已明确的低风险任务一次完成，减少重复确认和拆轮次。
- 测试只做本轮必要范围，未受影响模块不重复全量测试。
- 出错先定位原因，再做最小修改，不盲目重写。

---

## 2026-08-17

### 启动 web 版前后端服务

- **后端**：`dbx-web` 已在运行（PID 46113，端口 4224），验证 HTTP 响应正常（`/` 返回 404 属正常，服务在监听）。
- **前端**：`pnpm dev:web`（vite，端口 5173）后台启动，命令见 `package.json` 的 `dev:web` 脚本；日志写入 `.reasonix/dev-web.log`。
  - 首次启动时 corepack 下载 pnpm 及依赖元数据较慢，等待后 vite 正常就绪。
  - 验证：`http://localhost:5173/` 返回 HTTP 200。
- **备注**：未改动任何源代码；发现一个遗留进程 `pnpm tauri build`（PID 32560，3:36PM 起），与本轮无关，未处理。

### 修复：联邦查询 SQL 语法错误（ERROR 1064 `near '.store_sales'`）

**现象**：多连接联邦查询报 `ERROR 1064 (42000)`，SQL 片段 `FROM pgLocal.tpcds.store_sales s JOIN mySQLocal.tpcds.item i ...` 原样发送给 MySQL，MySQL 把三段表名解析为 `pgLocal.tpcds` + 多余 `.store_sales` 报错。

**根因**（通过 `~/.dbx-web/dbx.db` history 拿到用户实际 SQL + `git log -S` 比对确认）：
- 提交 `21ccd9ead`（fix: resolve merge conflicts）在合并冲突解决时**丢失了 dbx-core 执行层（`crates/dbx-core/src/query.rs`）的联邦处理逻辑**（原版本含：联邦分析、`validate_federation`、单连接 SQL 重写、多连接转发 Calcite Agent）。`git show HEAD:...query.rs | grep -c federat` 结果为 0。
- 上轮修复只在 dbx-web 层做了**单连接**联邦重写（`preprocess_federated_sql`），多连接联邦 SQL 不重写、原样传给 MySQL → 1064。
- 附带问题：`federation_enabled` 字段加入 `ConnectionConfig` 后，dbx-core 多处测试初始化未补字段，导致 `cargo test` 编译失败。

**修改文件**：
1. `crates/dbx-core/src/connection.rs`：`AppState` 增加 `calcite_agent: Mutex<Option<CalciteAgentManager>>` 字段并初始化（惰性创建 Calcite Agent）；测试 `ConnectionConfig` 初始化补 `federation_enabled`。
2. `crates/dbx-core/src/query.rs`：恢复执行层联邦处理——imports、`execute_sql_statement_with_options_typed` 中插入联邦分析/校验/单连接重写/多连接转 Calcite 块（用 `effective_sql` 执行）；新增 `execute_multi_connection_federated_query` 函数（Calcite Agent 惰性启动 + 连接注册 + 联邦查询 + 结果转换）；3 处测试 `ConnectionConfig` 补 `federation_enabled`。
3. `crates/dbx-core/src/schema.rs`：测试 `ConnectionConfig` 补 `federation_enabled`。

**验证**：
- `cargo check -p dbx-web` 编译通过。
- `cargo test -p dbx-core federated`：15 个单元测试 + 5 个 e2e 测试全部通过。
- Calcite Agent JAR 已就位：`agents/drivers/calcite/build/libs/dbx-agent-calcite.jar`（150MB）。

**待办（用户操作）**：重新编译并重启 web 后端（`pnpm dev:backend`，端口 4224）后，多连接联邦查询才会走 Calcite Agent；当前 4224 后端进程已停止。

**更新（重启后端完成）**：
- 后台启动 `pnpm dev:backend`（cargo-watch 模式），dbx-core + dbx-web 重新编译，dbx-web 监听 `http://0.0.0.0:4224`，恢复 11 个会话，HTTP 验证 404（服务正常）。
- 日志：`.reasonix/dev-backend.log`。
- 请在前端重试原 SQL（`FROM pgLocal.tpcds.store_sales s JOIN mySQLocal.tpcds.item i ...`），预期走 Calcite Agent（后端日志出现 `Multi-connection federated query detected, delegating to Calcite`）。

**更新（重启前端完成）**：前端 `pnpm dev:web`（vite, :5173）已重启，`http://localhost:5173/` 返回 200，日志 `.reasonix/dev-web.log`。

**更新（提交 + 打标 v0.5.86）**：
- 提交联邦修复：`56ba379e1`（6 个文件：connection.rs、federated.rs、query.rs(core)、schema.rs、query.rs(web)、WORK_LOG.md）。
- 经用户确认，`git tag -f -a v0.5.86 -m "Release 0.5.86"` 强制移动到 HEAD（原指向 31c392b2c）。
- 未提交文件保留：`App.vue`、`connectionStore.ts`（更早轮次修复，与联邦无关）、`.ohmyagent/settings.json`。

---

## 2026-08-17

### 修复桌面 UI 启动崩溃：usePromptTemplateStore is not defined

- **现象**：桌面应用启动时崩溃，报错 `ReferenceError: usePromptTemplateStore is not defined`（App.vue setup 阶段）。
- **根因**：`apps/desktop/src/App.vue` 第 111 行调用了 `usePromptTemplateStore()`，但缺少对应 import 语句。
- **修改文件**：
  - `apps/desktop/src/App.vue`：在 import 区新增 `import { usePromptTemplateStore } from "@/stores/promptTemplateStore";`
- **验证**：`apps/desktop/src/stores/promptTemplateStore.ts` 第 6 行正确导出 `usePromptTemplateStore`；前端 Vite（localhost:5173）返回 HTTP 200，运行正常。
- **备注**：上一轮已修复的 `recentConnectionIds is not defined` 修复（第 112-113 行 import 与声明）仍保留，未回归。

---

## 2026-08-17

### 修复连接失败：connectDbWithMissingPasswordRetry is not defined

- **现象**：点击数据库连接时崩溃，报错 `ReferenceError: connectDbWithMissingPasswordRetry is not defined`。
- **根因**：上游提交 `d64908c59`（fix(connection): improve synced password recovery）引入了新函数调用，但合并时函数定义丢失。
- **缺失内容**：
  - `CONNECTION_PASSWORD_REQUIRED_MESSAGE` 常量
  - `ensureConnectionPassword()` 函数
  - `persistRememberedConnectionPassword()` 函数
  - `connectDbWithMissingPasswordRetry()` 函数
  - 两处 `let rememberPassword = false;` 变量声明
- **修改文件**：
  - `apps/desktop/src/stores/connectionStore.ts`：从上游 `t8y2/dbx` 补充上述缺失定义
- **验证**：
  - TypeScript 编译通过，无相关报错
  - 前端 Vite（localhost:5173）HTTP 200
  - 后端 dbx-web（localhost:4224）HTTP 401（需认证，正常）

---

## 2026-08-17

### 修复联邦查询 SQL 语法错误：.store_sales

- **现象**：执行联邦查询时报错 `ERROR 1064 (42000): ... near '.store_sales s JOIN mySQLocal.tpcds.item i ON ...'`。
- **根因**：后端 `execute_query` 和 `execute_multi` 路由未调用联邦查询预处理函数，SQL 直接发送给 MySQL，联邦引用未被重写。
- **修改文件**：
  - `crates/dbx-web/src/routes/query.rs`：
    - 导入 `analyze_federation` 和 `rewrite_federated_sql`
    - 新增 `preprocess_federated_sql` 辅助函数：加载所有连接，分析 SQL 中的联邦模式，对单连接联邦查询进行 SQL 重写
    - 在 `execute_query` 和 `execute_multi` 中调用预处理函数
- **验证**：
  - `cargo check -p dbx-web` 编译通过
  - 后端重新启动，监听 http://localhost:4224
  - 前端 Vite（localhost:5173）运行正常

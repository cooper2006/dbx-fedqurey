# WORK_LOG

## 2026-08-22

### 修复：dbx-core 测试编译错误（E0063 × 17）

- **背景**：运行联邦查询后端验证单元测试（`cargo test -p dbx-core --lib -- federated:: agent_connection::`）时编译失败，报 17 处 `missing field 'federation_enabled' in initializer of 'ConnectionConfig'`。
- **根因**：`ConnectionConfig` 新增了 `federation_enabled` 字段，但各模块测试辅助函数中直接构造 `ConnectionConfig` 的地方未补齐该字段。
- **修改**：在 13 个源文件的测试 helper 构造字面量中，`database_info: None,` 之后插入 `federation_enabled: false,`：
  - crates/dbx-core/src/agent_connection.rs
  - crates/dbx-core/src/agent_tools.rs
  - crates/dbx-core/src/cloud_sync.rs（3 处）
  - crates/dbx-core/src/connection_secrets.rs
  - crates/dbx-core/src/db/redis_driver.rs
  - crates/dbx-core/src/mq/config.rs
  - crates/dbx-core/src/mq/service.rs
  - crates/dbx-core/src/nacos/config.rs
  - crates/dbx-core/src/nacos/service.rs（2 处）
  - crates/dbx-core/src/production_safety.rs
  - crates/dbx-core/src/schema.rs
  - crates/dbx-core/src/storage.rs（2 处）
  - crates/dbx-core/src/transfer.rs
- **验证**：`cargo test -p dbx-core --lib --no-run` 编译通过（1m10s）；`cargo test -p dbx-core --lib -- federated:: agent_connection::` 结果 **64 passed; 0 failed**。

### 后台验证：联邦查询支持的数据库

- 通过单元测试验证联邦查询对不同数据库类型的默认 schema 映射与 JDBC 连接串构造：
  - PostgreSQL 系（Postgres/Redshift/Kingbase/Highgo 等）→ `public`
  - MySQL 系（Mysql/Doris/StarRocks/ClickHouse/Databend 等）→ database 名
  - SqlServer → `dbo`；Oracle/OceanbaseOracle → database；Dameng → `SYSDBA`；Hive/Trino → `default`
  - Oracle/Trino/Presto/SAP Hana/OceanBase/H2 等的 JDBC URL 构造与主机端口改写
- 相关单/多连接联邦、4-part 名重写、单连接 SQL 重写测试全部通过。

### 合并 upstream/main（t8y2/dbx）

- **来源**：`git fetch upstream main` → `git merge upstream/main`
- **范围**：212 文件变更，+13565/-931 行
- **合并冲突**：仅 `docs/data/contributors.json`（`generatedAt` 时间戳冲突），已解决
- **主要变更摘要**：
  - 新版本 v0.5.92 发布准备
  - Oracle：支持向已有表添加主键、暴露 sequence
  - Dameng：支持已有列的 identity 切换
  - PostgreSQL：安全重命名数据库
  - SQL Server：传播 USE 上下文、恢复失败 SQL batch
  - Redis：支持 legacy pickle、XML 标签、db0 无 SELECT 命令
  - TDengine：时区时间戳显示、浮点精度保留
  - MCP：新增 ZCode/CodeBuddy 配置引导
  - Sidebar：支持批量移动连接、快速重命名
  - Grid：水平滚轮输入、长单元格双击选择统一、文档页大小修复
  - Agent：导出托管驱动离线使用
  - macOS：防止 Escape 退出原生全屏
  - SQL 补全：优化限定 SQL 补全查询、group-by 多列补全
  - JDBC 插件版本升级
  - 连接配置：可选明文导出、OceanBase JDBC URL 解析
  - 其他多项 bug 修复（Phoenix、Iris、GBase、RocketMQ、Databricks 等）

### 合并到 main 并推送 origin

- **操作**：将 `freebuff/task-3ac34aa5` 分支合并到 `main`（fast-forward），再拉取 origin/main 解决冲突后推送
- **冲突**：`docs/data/contributors.json`（`generatedAt` 时间戳冲突，取 origin 版本）
- **结果**：`origin/main` 已更新至 `759d93bc9`（213 文件，+13589/-932）

## 2026-08-24

### 合并 upstream/main（t8y2/dbx）— 第二次同步

- **来源**：`git fetch upstream main` → `git merge upstream/main`
- **范围**：313 文件变更，+17131/-2400 行，37 个新提交
- **合并冲突**：无
- **主要变更摘要**：
  - AI：添加通用 AI 配置深度链接、可调整大小的全屏聊天面板、连接选择器分组树
  - Schema Diff：选择字段和索引进行同步、DDL 定位高亮
  - MySQL：管理与编辑 MySQL Events
  - Meilisearch：新增 workspace/key/task 管理页面
  - Nacos：支持 DataId 映射配置同步
  - PostgreSQL：支持编辑表 owner
  - 导出：支持 XLSX 表头名称和注释
  - 结果标签：支持固定标签和批量关闭
  - 连接类型：集中化注册、新增 profiles/catalog
  - SSH：Windows 本地账户支持 Pageant
  - Oracle：优先使用物理行标识符
  - SQL Formatter：增强格式化能力
  - Sidebar：扩展过滤器到视图和存储过程
  - DataGrid：手动引用显示值、条件补全增强
  - 多项 CI 修复（Windows Perl、Rust lint、格式检查等）
  - 其他多项 bug 修复（IoTDB、DuckDB、Dameng DDL、Nacos batch 等）
---

## 2026-08-24

### 从 upstream/main 同步代码（保留联邦查询）

- 上游基线：`68179d16f` → `c8dbeeaf7`（23 个新提交）
- 备份分支：`backup/pre-upstream-merge-20260824-144652`
- 冲突：无冲突，自动合并完成
- 联邦查询核心文件完整保留（federated.rs、calcite_agent.rs、Calcite Agent JAR、前端 federated/）
- 上游主要变更：Oracle/PostgreSQL/SQL 编辑器修复、data grid 增强、structure editor 多选列等


---

## 2026-08-24 (续)

### P0 联邦查询框架代码修复

**问题根因**：
- 多连接联邦查询 `pgLocal.tpcds.public.item` 超时 60s
- 3 段式 `pgLocal.tpcds.item` 正常执行
- 根因：`get_default_schema` 对多种数据库类型的默认 schema 映射错误，且 `validate_federation` 使用连接主库名而非引用中的实际库名做白名单校验

**修复内容**：
1. `get_default_schema` 修正：
   - Snowflake: 库名 → `PUBLIC`
   - Hive/Spark/Trino/Presto: `default` → 库名（保留真实库名）
   - Dameng: 硬编码 `SYSDBA` → 库名
   - Sqlite: `public` → `main`
   - Kylin/Sundb: `default` → `PUBLIC`
   - Db2: 从 `is_oracle_like` 移除（不再添加库名前缀）

2. `validate_federation` 修复：
   - 白名单校验使用 `ref_.database_name`（SQL 引用中的库名）而非连接主库名

3. 新增 4 个测试用例：
   - `test_4part_pg_public_schema_rewrite`: PostgreSQL 4 段式 `conn.db.public.table` → `public.table`
   - `test_4part_named_schema_retained`: PostgreSQL 4 段式非默认 schema 保留
   - `test_dameng_default_schema`: 达梦默认 schema 使用库名
   - `test_db2_not_oracle_like`: DB2 不再被当 Oracle 类处理

**验证**：全部 22 个联邦查询测试通过（18 原有 + 4 新增）

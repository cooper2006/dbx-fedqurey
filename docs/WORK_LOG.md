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
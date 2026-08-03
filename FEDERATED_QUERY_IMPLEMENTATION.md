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
- `crates/dbx-core/src/models/connection.rs` - 新增 `DatabaseType::Calcite`
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

#### 1.5 AppState 集成
**位置**: `crates/dbx-core/src/connection.rs`

AppState 新增字段：
```rust
pub calcite_agent: Arc<Mutex<Option<crate::calcite_agent::CalciteAgentManager>>>
```

### Phase 2 (P1) - Calcite Agent

#### 2.3 Rust 侧 Calcite Agent 生命周期管理
**位置**: `crates/dbx-core/src/calcite_agent.rs`

实现了完整的骨架代码：
- `CalciteAgentConfig` - 配置结构
- `CalciteAgentState` - 状态枚举（Stopped/Starting/Running/Error）
- `CalciteAgentHandle` - 句柄结构
- `CalciteAgentManager` - 管理器（单例模式）

待实现：
- Java 进程启动逻辑
- gRPC 客户端集成
- 真正的连接注册逻辑

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

## 已知限制

1. **多连接联邦查询需要 Calcite Agent** - Phase 2 尚未完成
2. **仅支持 SELECT** - UPDATE/INSERT/DELETE 不在 Phase 1 范围
3. **Schema 可见性控制** - 未实现敏感表的过滤
4. **方言检测启发式** - 当前基于简单字符串匹配，可能需要更精确的 AST 分析

## 下一步计划

### P0 优先级（阻塞其他功能）
- [ ] 测试验证所有修改
- [ ] 前端 ConnectionTree.vue 添加联邦状态图标

### P1-P3 优先级
- [ ] Java Calcite Agent 项目骨架
- [ ] gRPC 协议定义和实现
- [ ] 前端联邦感知格式化器集成到 QueryEditor
- [ ] 联邦表名级联补全

## 文件变更清单

### Rust (crates/dbx-core/src/)
- `lib.rs` - 添加 calcite_agent 模块声明
- `models/connection.rs` - 添加 DatabaseType::Calcite 和 federation_enabled 字段
- `agent_catalog.rs` - 添加 Calcite Catalog Entry
- `query.rs` - 集成联邦分析逻辑
- `connection.rs` - AppState 添加 calcite_agent 字段
- `federated.rs` - 核心联邦逻辑实现
- `calcite_agent.rs` - Calcite Agent 生命周期管理

### TypeScript (apps/desktop/src/)
- `lib/federated/federatedFormatter.ts` - 新建
- `lib/federated/dialectDetector.ts` - 新建

## 参考文档

- `design/architecture_review_federated_query.md` - 架构评审报告
- `design/federated_query_sql_formatter_design.md` - SQL 格式化设计
- `design/ui_ux_optimization_federated_formatter.md` - UI/UX 优化设计

---
*实现日期: 2026-08-03*
*版本: 1.0*

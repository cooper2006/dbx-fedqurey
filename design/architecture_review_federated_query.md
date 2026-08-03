# 联邦查询与 SQL 格式化设计文档 — 软件架构师评审报告

**版本**: 1.0  
**评审者**: 软件架构师 (Agnes)  
**日期**: 2026-07-31  
**被评文档**: 
1. `docs/design/federated_query_and_sql_formatter.md` (原始设计)
2. `design/federated_query_sql_formatter_design.md` (后续补充设计)
3. `design/ui_ux_optimization_federated_formatter.md` (UI/UX 优化设计)

---

## 一、总体评分

| 维度 | 原始设计 | 补充架构设计 | UI/UX 设计 | 综合 |
|------|---------|-------------|-----------|------|
| 架构完整性 | ⭐⭐⭐☆☆ | ⭐⭐⭐⭐☆ | N/A | 良好 |
| 技术可行性 | ⭐⭐⭐☆☆ | ⭐⭐⭐⭐☆ | N/A | 良好 |
| 与现有代码库集成度 | ⭐⭐☆☆☆ | ⭐⭐⭐⭐☆ | ⭐⭐⭐⭐☆ | 优秀 |
| 需求覆盖度 | ⭐⭐⭐☆☆ | ⭐⭐⭐⭐☆ | ⭐⭐⭐⭐⭐ | 优秀 |
| 风险评估 | ⭐⭐⭐⭐☆ | ⭐⭐⭐⭐☆ | ⭐⭐⭐☆☆ | 良好 |
| **综合得分** | **B+** | **A-** | **A** | **A-** |

---

## 二、架构层面的主要发现

### 2.1 关键误解：联邦查询的用户体验模型 ❌

**问题所在**：两份设计文档（尤其是原始设计）都假设用户需要在 SQL 中**手动输入连接前缀**，例如：
```sql
-- 设计文档中的示例
SELECT * FROM conn1.public.users AS u;
```

**实际应该是**：联邦查询应该是**透明重定向**的——用户写普通 SQL，后端自动判断各表归属哪个连接，重写后分发执行：
```sql
-- 用户写的（无显式前缀）
SELECT u.name, o.amount 
FROM users u 
JOIN orders o ON u.id = o.user_id;

-- 后端透明重写为（用户不可见）
SELECT "app_db"."public"."users"."name" AS "u.name",
       "app_db"."public"."orders"."amount" AS "o.amount"
FROM "app_db"."public"."users" AS u
JOIN "app_db"."public"."orders" AS o ON u."id" = o."user_id";
```

**为什么这样更好？**
- 用户不需要关心底层物理连接拓扑
- 减少语法错误的可能性
- 符合传统联邦查询工具（如 Dremio、Denodo）的实际工作方式

### 2.2 SQL 命名规则的正确解读 ✅（原始文档有误）

**用户需求原文**（来自 PRD）：
> 如果是 postgresql 数据库，生成 sql 应该是连接.数据库.schema.表 表别名的形式，而非 schema.表的形式；如果是 mysql 数据库，生成 sql 应该是连接.数据库.表 表别名的形式，连接、数据库、表等名称不需要添加引号

**修正后的正确理解**：

| 数据库类型 | 目标 SQL 格式（发送给 Calcite 或后端查询引擎）| 示例 |
|-----------|--------------------------------------------|------|
| PostgreSQL | `connection.db.schema."table" alias` | `myconn.mydb.public."users" u` |
| MySQL | `connection.db."table" alias` | `myconn.shop.`orders` o` |
| 通用规则 | 连接名、数据库名用反引号（MySQL）或双引号（PG），不加额外的 schema | — |

**关键纠正**：原始文档中声称「不添加任何引号」是**错误的**。PostgreSQL 规范要求标识符使用双引号，MySQL 使用反引号。用户说的「不需要添加引号」是指：**不应该像现有系统那样在 schema.table 处添加无关的引号修饰**，而是按照标准 SQL 规范使用正确的引号字符。

### 2.3 联邦连接表的元数据管理缺失 ❌

两份设计均**未明确说明**：当一个表引用涉及联邦连接时，如何获取该表的列信息用于 autocomplete？

**缺失的设计点**：
```typescript
// 应补充的接口
interface FederatedTableReference {
  connectionId: string;
  database?: string;
  schema?: string;
  tableName: string;
  
  // 这些属性需要从连接元数据缓存中查找
  columns: ColumnInfo[];      // 列信息（用于智能补全）
  primaryKey?: string[];      // 主键（用于 JOIN 推荐）
  rowCount?: number;          // 行数估算
}
```

---

## 三、技术方案可行性分析

### 3.1 Calcite 集成方案的优缺点

**原始设计的方案：外部进程模式（gRPC/stdin）**

| 优点 | 缺点 |
|------|------|
| JVM 崩溃不影响 dbx 主进程 | 增加了 IPC 复杂性 |
| 可以独立升级 Calcite 版本 | 需要处理跨进程的数据序列化 |
| 内存隔离，不会 OOM 影响主程序 | 调试困难 |

**建议的调整**：

```
┌─────────────────────────────────────────────────────┐
│                    dbx Desktop                       │
│                                                      │
│  ┌─────────────────┐    gRPC/Channel    ┌─────────┐ │
│  │   Rust Core     │◄──────────────────►│  Calcite │ │
│  │  (Connection    │   via socket       │  Service │ │
│  │   Pool Mgr)     │                    │ (Java)   │ │
│  └─────────────────┘    ← SQL Rewrite → └────┬────┘ │
│          ↓                                     │      │
│  ┌─────────────────┐                         │      │
│  │  Federation     │◄───────┐                 │      │
│  │  Rewriter       │        │                 │      │
│  │  (Rust)         │        │ Sub-query        │      │
│  └─────────────────┘        │ Distribution     │      │
│          ↓                  │                  │      │
│  ┌─────────────────┐        ▼                  │      │
│  │ Result          │  ┌──────────┐            │      │
│  │ Merge & Return  │  │ JDBC     │──┐        │      │
│  └─────────────────┘  │ Drivers  │  │        │      │
│                       └──────────┘  └────────┘      │
└─────────────────────────────────────────────────────┘
```

**Rust 侧重写 vs Java Calcite 重写**：

对于简单的 SELECT（非 JOIN），可以在 Rust 层做重写（更快）。
对于复杂的跨连接 JOIN，必须依赖 Calcite 的执行计划优化。

### 3.2 Rust 与 Java 通信协议设计

**建议的 gRPC 接口定义**：

```protobuf
syntax = "proto3";
package federation.v1;

service FederationService {
  // 注册/注销数据源
  rpc RegisterSource(RegisterSourceRequest) returns (RegisterSourceResponse);
  rpc UnregisterSource(UnregisterSourceRequest) returns (UnregisterSourceResponse);
  
  // 联邦查询执行
  rpc ExecuteFederatedQuery(ExecuteFederatedQueryRequest) 
      returns (stream FederationQueryResult);
  
  // 查询计划预览（explain）
  rpc ExplainFederatedQuery(ExplainFederatedQueryRequest) 
      returns (FederatedExplainPlan);
  
  // 数据源元数据查询
  rpc GetDataSourceMetadata(GetDataSourceMetadataRequest) 
      returns (DataSourceMetadata);
}

message RegisterSourceRequest {
  string connection_id = 1;
  string jdbc_url = 2;
  string username = 3; // 可选，从加密存储获取
  string driver_class = 4;
  map<string, string> properties = 5;
}

message ExecuteFederatedQueryRequest {
  string query_id = 1;
  string sql = 2;
  int32 max_rows = 3; // 限制返回行数
  int64 timeout_ms = 4;
}

message FederationQueryResult {
  oneof payload {
    SchemaChange schema_change = 1;
    QueryProgress progress = 2;
    RowBatch row_batch = 3;
    Error error = 4;
    Done done = 5;
  }
}

message RowBatch {
  repeated string columns = 1;
  repeated bytes rows = 2; // 序列化的行数据（Arrow format）
  int32 row_count = 3;
}
```

### 3.3 联邦查询的执行模型选择

**选项 A：Star Schema（星型架构）**
- 所有连接向 Calcite 注册
- Calcite 统一解析和执行
- 优点：简单，支持复杂 JOIN
- 缺点：Calcite 调度可能成为瓶颈

**选项 B：Shard-and-Merge（分片合并）**
- Rust 层将 SQL 按表拆分到各连接
- 并行执行子查询
- Rust 层在应用层合并结果
- 优点：性能可控，无单点瓶颈
- 缺点：不支持跨连接复杂 JOIN

**推荐方案**：
- **Phase 1**：实现 Shard-and-Merge（简单场景足够）
- **Phase 2**：接入 Calcite 支持复杂 JOIN
- **Phase 3**：智能路由（根据查询模式自动选择最优策略）

---

## 四、与现有代码库的集成点分析

### 4.1 需要修改的核心模块

```
需要修改的 Rust 模块：
┌──────────────────────────────────────────────────────┐
│ crates/dbx-core/src/sql_dialect/identifiers.rs       │
│   └─ 添加 generate_federated_table_name() 函数        │
├──────────────────────────────────────────────────────┤
│ crates/dbx-core/src/query.rs                         │
│   └─ 添加 FederatedQueryContext 和分布式执行逻辑      │
├──────────────────────────────────────────────────────┤
│ crates/dbx-core/src/models/connection.rs             │
│   └─ 扩展 ConnectionConfig 增加 federation 字段       │
├──────────────────────────────────────────────────────┤
│ crates/dbx-core/src/storage.rs                       │
│   └─ 增加 FederationConfig 持久化                   │
├──────────────────────────────────────────────────────┤
│ crates/dbx-core/src/lib.rs                           │
│   └─ 暴露 federation 相关 API                        │
└──────────────────────────────────────────────────────┘

需要修改的 TypeScript 模块：
┌──────────────────────────────────────────────────────┐
│ packages/node-core/src/backend.ts                    │
│   └─ 添加 executeFederatedQuery 接口                  │
├──────────────────────────────────────────────────────┤
│ apps/desktop/src/lib/sql/sqlCompletion.ts            │
│   └─ 扩展 completion 支持联邦表名                    │
├──────────────────────────────────────────────────────┤
│ apps/desktop/src/components/editor/QueryEditor.vue  │
│   └─ 添加联邦状态栏 + 格式化按钮                      │
├──────────────────────────────────────────────────────┤
│ apps/desktop/src/stores/connectionStore.ts           │
│   └─ 扩展连接管理支持 federation 配置                 │
└──────────────────────────────────────────────────────┘
```

### 4.2 现有代码的关键函数参考

| 文件位置 | 函数 | 用途 | 是否需要修改 |
|---------|------|------|-------------|
| `src/sql_dialect/identifiers.rs:119` | `qualified_table_name()` | 现有表名限定逻辑 | ✅ 新增联邦分支 |
| `src/sql_dialect/identifiers.rs:163` | `quote_table_identifier()` | 标识符转义 | ✅ 需支持无引号模式 |
| `src/query.rs:1431` | `execute_sql_statement_with_options()` | 查询执行入口 | ✅ 新增联邦路由判断 |
| `src/query.rs:280` | `sql_for_execution_context()` | SQL 上下文注入 | ✅ 需传入联邦信息 |
| `apps/desktop/src/lib/sql/sqlFormatter.ts` | `formatSqlText()` | 格式化核心 | ✅ 增加联邦感知 |
| `apps/desktop/src/components/editor/QueryEditor.vue` | QueryEditor | 编辑器主组件 | ✅ 增加联邦状态栏 |

---

## 五、设计缺陷与遗漏

### 5.1 重大设计缺陷

#### 缺陷 1：联邦连接的生命周期管理未设计

**问题**：当用户修改连接配置（如切换数据库、修改权限）后，联邦目录需要重新注册。现有设计完全没有提及这个同步机制。

**修复方案**：
```rust
// 在 connectionStore 中监听配置变化
async fn watch_connection_changes() -> Result<()> {
    let mut watcher = connection_change_watcher().await?;
    watcher.watch(|event| async move {
        match event.kind {
            ConnectionChanged::Updated(config) => {
                // 重新注册到 Calcite
                federation_manager.re_register_connection(&config).await?;
            }
            ConnectionChanged::Removed(id) => {
                // 从 Calcite 注销
                federation_manager.unregister_connection(&id).await?;
            }
        }
    }).await
}
```

#### 缺陷 2：联邦查询的超时与取消未设计

**问题**：Calcite 执行联邦查询可能耗时较长，但现有设计没有考虑：
1. 如何取消正在进行的联邦查询？
2. 各子查询的超时如何单独控制？
3. 部分失败的查询如何处理？

**修复方案**：
- 为每个联邦查询分配唯一 `query_id`
- 前端通过 gRPC cancel stream 取消整个查询
- 各子查询设置独立超时（默认 30s，可配置）

#### 缺陷 3：Schema 可见性控制缺失

**问题**：一个连接可能包含敏感表（如 `admin_users`），不应全部暴露在联邦目录中。

**修复方案**：
```typescript
interface FederationConnectionConfig {
  allow_all_schemas: boolean;        // 默认：允许全部
  excluded_schemas: string[];        // 排除的模式列表
  excluded_tables: string[];         // 排除的具体表（优先级更高）
  visible_as: string;                // 在联邦目录中的别名
}
```

#### 缺陷 4：跨连接的 UPDATE/INSERT/DELETE 未设计

**问题**：联邦查询目前只考虑了 SELECT。但如果用户写 `UPDATE conn1.orders SET ... WHERE ...`，是否允许？

**建议**：Phase 1 仅支持 SELECT 联邦查询，DML 在 Phase 3 中实现。

### 5.2 次要设计缺陷

#### 缺陷 5：方言自动检测过于简单

原始设计的 `autoDetectDialect()` 函数存在以下问题：
- `\g` 不是 PostgreSQL 专属（某些 Oracle 兼容库也用）
- `\d` 实际上是 psql CLI 命令，在 SQL 文本中不会出现
- 应基于 SQL 关键字特性更精确地检测

**改进的方言检测策略**：
```typescript
function detectDialect(sql: string): SqlFormatDialect {
  const normalized = sql.toLowerCase().trim();
  
  // 基于特定语法特征优先级排序
  if (/^BEGIN\s+TRANSACTIONS?\s*$/m.test(normalized)) return "sqlserver";
  if (/\\du|\bpg_\w+\b/.test(normalized)) return "postgres";
  if (/SHOW\s+TABLES|\bINFORMATION_SCHEMA\b/.test(normalized)) return "mysql";
  if (/PRAGMA\s|ATTACH\s+DATABASE/.test(normalized)) return "sqlite";
  if (/CREATE\s+TABLE\s+IF\s+NOT\s+EXISTS/.test(normalized)) return "clickhouse";
  
  return "generic";
}
```

#### 缺陷 6：SQL 格式化器对联邦引用的处理

**问题**：现有的 `sql-formatter` npm 包不知道 `conn.db.schema.table` 这种四段式命名，可能会错误地重新排列或截断它。

**解决方案**：
1. 在格式化前，将所有 `connection.*` 引用替换为占位符（如 `<FED_TABLE_0>`）
2. 格式化占位符之间的普通 SQL
3. 将占位符还原回原始引用

---

## 六、数据安全与权限设计

### 6.1 联邦查询的权限模型

```
┌─────────────────────────────────────────────────────┐
│                   权限检查流                         │
│                                                      │
│  用户登录                                            │
│      ↓                                               │
│  检查用户对该连接是否有 SELECT 权限                   │
│      ↓                                               │
│  检查联邦查询是否超出用户的角色权限                   │
│      ↓                                               │
│  执行查询（使用连接的实际凭证）                        │
│      ↓                                               │
│  返回结果（不泄露其他连接的信息）                     │
└─────────────────────────────────────────────────────┘
```

### 6.2 密码安全

联邦查询不能将明文密码传递给 Calcite 服务。应该：
1. dbx 使用已有连接池的加密连接
2. 通过认证令牌或 TLS 双向认证将连接传递给 Calcite
3. Calcite 侧不持久化密码，仅在会话期间使用

---

## 七、实施建议

### Phase 1：基础联邦查询（4-6 周）

**目标**：支持跨连接的简单 SELECT 查询

| 任务 | 工作量 | 优先级 |
|------|--------|--------|
| 1.1 ConnectionConfig 扩展 | 2 天 | P0 |
| 1.2 联邦查询 SQL 解析器（Rust） | 3 天 | P0 |
| 1.3 gRPC 协议定义 | 1 天 | P0 |
| 1.4 简化的分片合并执行器 | 5 天 | P0 |
| 1.5 前端 API 调用封装 | 2 天 | P1 |
| 1.6 单元测试 + 集成测试 | 3 天 | P0 |

### Phase 2：Calcite 集成（4-6 周）

**目标**：支持复杂 JOIN、视图、物化查询

| 任务 | 工作量 | 优先级 |
|------|--------|--------|
| 2.1 Calcite Docker 服务部署 | 3 天 | P1 |
| 2.2 JDBC adapter 开发 | 5 天 | P1 |
| 2.3 查询计划缓存 | 3 天 | P2 |
| 2.4 跨连接 JOIN 性能优化 | 5 天 | P1 |
| 2.5 结果合并优化（流式传输）| 3 天 | P2 |

### Phase 3：高级功能（4-8 周）

**目标**：DML 支持、可视化、性能监控

---

## 八、总结

### 原始设计的优点
- 对用户需求有基本的理解
- API 接口设计思路清晰
- 包含了一定的风险评估

### 原始设计的主要问题
1. **用户界面假设错误**：要求用户手动输入连接前缀是错误的产品决策
2. **SQL 命名规则理解有误**：对用户"不添加引号"的需求解读不够准确
3. **缺少联邦连接生命周期管理**
4. **缺少权限和安全设计**
5. **方言检测算法过于简单且有错误**
6. **未考虑跨连接 DML 的安全边界**

### 补充文档的优点
- 更详细的架构分解
- 明确了 Rust 和 TypeScript 两侧的修改点
- 包含了更具体的 UI/UX 设计

### 补充文档仍需改进的地方
1. 仍需澄清 Calcite 具体如何替代应用层分片合并
2. 联邦查询的性能监控指标未定义
3. 缺少完整的错误码映射表
4. 多租户场景下的资源隔离未讨论

### 最终建议

**推荐分阶段实施，先实现 Phase 1 的基础分片合并方案，再评估是否引入 Calcite。** 理由如下：

1. Phase 1 的成本更低，可以快速验证市场需求
2. Rust 原生实现更容易维护和调试
3. 如果用户反馈强烈需要复杂 JOIN，再投入 Calcite 集成
4. 两个方案在 API 层面是兼容的，可以平滑迁移

---

*评审完成。如有问题请联系架构组。*
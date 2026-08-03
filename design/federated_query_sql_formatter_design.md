# dbx 联邦查询与 SQL 格式化功能设计文档（架构师评审修订版）

**版本**: 2.0  
**角色**: 软件架构师 / UI设计师 / UX架构师  
**日期**: 2026-07-31  

---

## 一、需求澄清与架构修正

### 1.1 联邦查询的正确用户模型

**原始误解**：要求用户在 SQL 中手动输入连接前缀

```sql
-- ❌ 错误的产品假设（来自原始设计）
SELECT * FROM conn1.public.users u;
```

**正确的用户体验**：用户写普通 SQL，后端自动识别并分发

```sql
-- ✅ 正确的产品假设
SELECT u.name, o.amount 
FROM users u 
JOIN orders o ON u.id = o.user_id;
-- 后端透明重写为跨连接的本地 SQL
```

### 1.2 SQL 命名规则的准确解读

根据用户原始需求："连接、数据库、表等名称不需要添加引号"

| 数据库类型 | 目标格式 | 示例 |
|-----------|---------|------|
| PostgreSQL | `连接.schema."表" 别名` | `myconn.public."users" u` |
| MySQL | `连接.数据库.`表` 别名` | `myconn.shop.`orders` o` |

**关键点**：
- "不需要添加引号" = 不添加额外的 schema 限定修饰符
- 标准 SQL 标识符引号仍需遵循各数据库规范（PG用双引号，MySQL用反引号）
- 别名使用 `AS` 关键字或空格分隔

### 1.3 联邦执行模型

```
┌─────────────────────────────────────────────────────────────┐
│                      查询执行流程                            │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│  用户 SQL (无显式前缀)                                      │
│       ↓                                                    │
│  ┌─────────────────┐                                        │
│  │ AST 解析器      │ ← 识别各表所属连接                     │
│  └────────┬────────┘                                        │
│           ↓                                                  │
│  ┌─────────────────┐                                        │
│  │ 连接路由器      │ ← 表 → 连接映射                        │
│  └────────┬────────┘                                        │
│           ↓                                                  │
│  ┌─────────────────┐                                        │
│  │ SQL 重写器      │ ← 生成各连接可执行的本地 SQL            │
│  └────────┬────────┘                                        │
│           ↓                                                  │
│  ┌─────────────────┐    ┌─────────────────┐                  │
│  │ 并发执行引擎    │───▶│ 子查询分发      │                  │
│  │ (Rust原生)      │    │ (gRPC/Channels) │                  │
│  └────────┬────────┘    └────────┬────────┘                  │
│           │                     │                           │
│           ▼                     ▼                           │
│  ┌─────────────────┐    ┌─────────────────┐                  │
│  │ 结果合并器      │◄───│ 执行计划优化    │                  │
│  │ (Arrow格式)     │    │ (Calcite v2)    │                  │
│  └────────┬────────┘    └─────────────────┘                  │
│           ↓                                                  │
│  ┌─────────────────┐                                        │
│  │ 结果返回        │ ← 列对齐、类型转换                       │
│  └─────────────────┘                                        │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 二、核心架构设计

### 2.1 联邦查询调度器

**位置**: `crates/dbx-core/src/query/federated.rs`

```rust
pub struct FederatedQueryContext {
    /// 查询请求ID（用于日志追踪和取消操作）
    pub query_id: String,
    /// 使用的默认连接ID（无显式前缀时的回退）
    pub default_connection_id: Option<String>,
    /// 参与联邦查询的连接集合
    pub involved_connections: Vec<ConnectionRef>,
    /// 查询超时设置
    pub timeout_ms: u64,
}

/// 联邦查询结果
pub struct FederatedQueryResult {
    pub columns: Vec<String>,
    pub column_types: Vec<String>,
    pub rows: Vec<Vec<serde_json::Value>>,
    pub execution_stats: QueryExecutionStats,
    pub warnings: Vec<String>,  // 如部分连接失败
}

pub struct QueryExecutionStats {
    pub total_time_ms: u128,
    pub connections_used: Vec<(String, u128)>,  // connection_id -> time_ms
    pub rows_scanned: u64,
    pub rows_returned: u64,
}
```

### 2.2 连接路由器

**职责**：分析 SQL AST，确定每张表属于哪个连接

```rust
/// 表引用映射
pub struct TableReferenceMapping {
    pub alias: String,
    pub physical_table: String,
    pub connection_id: String,
    pub database: Option<String>,
    pub schema: Option<String>,
}

/// 联邦查询路由结果
pub struct FederationRoute {
    pub table_mappings: Vec<TableReferenceMapping>,
    pub sub_queries: HashMap<String, String>,  // connection_id -> local SQL
    pub needs_merge: bool,  // 是否需要应用层合并
}

impl FederatedQueryScheduler {
    /// 分析 SQL 并生成路由计划
    pub async fn analyze(&self, sql: &str, context: &FederatedQueryContext) 
        -> Result<FederationRoute, FederationError> 
    {
        // 1. 解析 SQL AST
        // 2. 识别 FROM/JOIN 中的表引用
        // 3. 根据表名匹配连接配置
        // 4. 生成各连接的本地化 SQL
    }
}
```

### 2.3 数据源注册表

**位置**: `crates/dbx-core/src/storage/federation_config.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationConfig {
    pub enabled: bool,
    pub default_catalog: String,
    pub sources: Vec<DataSourceRegistration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DataSourceRegistration {
    pub connection_id: String,
    pub federation_name: String,  // 在联邦目录中的名称
    pub allow_federation: bool,
    pub excluded_schemas: Vec<String>,
    pub excluded_tables: Vec<String>,
    pub visible_as: String,       // 联邦查询中的显示名
}

// 连接变更时同步注册状态
impl DataSourceRegistry {
    pub async fn on_connection_changed(&mut self, event: ConnectionChangeEvent) {
        match event.kind {
            ConnectionChanged::Added(config) => self.register(config).await,
            ConnectionChanged::Updated(config) => self.re_register(config).await,
            ConnectionChanged::Removed(id) => self.unregister(&id).await,
        }
    }
}
```

---

## 三、API 接口设计

### 3.1 联邦查询 API

**位置**: `packages/node-core/src/backend.ts`

```typescript
export interface FederatedBackend extends Backend {
  /**
   * 执行联邦查询
   */
  executeFederatedQuery(
    connectionIdOrDefault?: string,
    sql: string,
    options?: FederatedQueryOptions
  ): Promise<QueryResult>;

  /**
   * 预览联邦查询的执行计划
   */
  explainFederatedQuery(
    sql: string,
    connectionId?: string
  ): Promise<FederatedExplain[]>;
}

export interface FederatedQueryOptions {
    maxRows?: number;
    timeoutMs?: number;
    preferLocalExecution?: boolean;  // true: 优先分片合并，false: 尝试 Calcite
}

export interface FederatedExplain {
  connectionId: string;
  targetDatabase?: string;
  targetSchema?: string;
  targetTable: string;
  rewrittenSql: string;
  position: { start: number; end: number };
}
```

### 3.2 gRPC 协议定义

```protobuf
syntax = "proto3";
package dbx.federation.v1;

service FederationService {
  // 注册数据源
  rpc RegisterSource(RegisterSourceRequest) returns (RegisterSourceResponse);
  // 注销数据源
  rpc UnregisterSource(UnregisterSourceRequest) returns (Empty);
  // 执行联邦查询
  rpc ExecuteQuery(ExecuteQueryRequest) returns (stream QueryResultChunk);
  // 取消查询
  rpc CancelQuery(CancelQueryRequest) returns (Empty);
}

message RegisterSourceRequest {
  string source_id = 1;
  string jdbc_url = 2;
  string driver_class = 3;
  map<string, string> properties = 4;
  repeated string excluded_schemas = 5;
}

message ExecuteQueryRequest {
  string query_id = 1;
  string sql = 2;
  int32 max_rows = 3;
  int64 timeout_ms = 4;
}

message QueryResultChunk {
  oneof payload {
    SchemaChange schema = 1;
    RowBatch batch = 2;
    Progress progress = 3;
    Error error = 4;
  }
}

message RowBatch {
  repeated string columns = 1;
  repeated bytes rows = 2;  // Arrow IPC format
  int32 row_count = 3;
}
```

---

## 四、SQL 格式化增强设计

### 4.1 智能方言检测改进

```typescript
function detectDialect(sql: string): SqlFormatDialect {
  const normalized = sql.toLowerCase().trim();
  
  // 基于 SQL 语法特征优先级排序
  if (/^BEGIN\s+TRANSACTIONS?\s*$/m.test(normalized)) return "sqlserver";
  if (/\\du\b|\bpg_\w+\b/.test(normalized)) return "postgres";
  if (/SHOW\s+TABLES\b|\bINFORMATION_SCHEMA\b|\bSHOW\s+GRANTS\b/.test(normalized)) return "mysql";
  if (/PRAGMA\s|ATTACH\s+DATABASE/.test(normalized)) return "sqlite";
  if (/CREATE\s+TABLE\s+IF\s+NOT\s+EXISTS|TRULETABLE/.test(normalized)) return "clickhouse";
  if (/EXPLAIN\s+(ANALYZE|FORMAT\s+JSON)/.test(normalized)) return "postgres";
  
  return "generic";
}
```

### 4.2 联邦感知的格式化服务

```typescript
import { formatSqlText, type SqlFormatDialect } from "@/lib/sql/sqlFormatter";
import { type FederatedConnection } from "@/types/database";

export class FederatedFormatterService {
  private static FED_PLACEHOLDER_PREFIX = "__FED_";
  
  static async format(
    sql: string,
    dialect?: SqlFormatDialect,
    connections?: FederatedConnection[]
  ): Promise<string> {
    // 1. 提取联邦表引用，替换为占位符
    const { processedSql, replacements } = this.extractFederatedRefs(sql);
    
    // 2. 自动检测方言（如果未指定）
    const finalDialect = dialect || this.detectDialect(processedSql);
    
    // 3. 格式化普通 SQL 部分
    let formatted = await formatSqlText(processedSql, finalDialect);
    
    // 4. 还原联邦引用（保留原始格式）
    formatted = this.restoreFederatedRefs(formatted, replacements);
    
    return formatted;
  }
  
  private static extractFederatedRefs(sql: string): { 
    processedSql: string, 
    replacements: Map<number, string> 
  } {
    const replacementMap = new Map<number, string>();
    let counter = 0;
    
    // 匹配 conn.schema.table 或 conn.db.table 模式
    const federatedPattern = /\b(\w+)\.(\w+)\.(\w+)\s*(?:AS\s+)?(\w+)?\b/gi;
    
    return {
      processedSql: sql.replace(federatedPattern, (match, conn, schema, table, alias) => {
        const placeholder = `${this.FED_PLACEHOLDER_PREFIX}${counter++}`;
        replacementMap.set(counter - 1, match);
        return placeholder;
      }),
      replacements: replacementMap
    };
  }
  
  private static restoreFederatedRefs(
    formatted: string, 
    replacements: Map<number, string>
  ): string {
    for (const [idx, original] of replacements.entries()) {
      const placeholder = `${this.FED_PLACEHOLDER_PREFIX}${idx}`;
      formatted = formatted.replace(placeholder, original);
    }
    return formatted;
  }
}
```

---

## 五、UI/UX 设计方案（修订版）

### 5.1 联邦连接状态指示器

**位置**: `components/sidebar/ConnectionTree.vue`

```vue
<!-- 联邦状态图标组 -->
<template>
  <div class="connection-item">
    <span class="connection-icon">{{ getIcon(connection.dbType) }}</span>
    <span class="connection-name">{{ connection.name }}</span>
    
    <!-- 联邦状态徽章 -->
    <span 
      v-if="connection.useForFederation" 
      class="badge federation-enabled"
      :title="'联邦查询已启用'"
    >
      <Database2 size="12" />
    </span>
    <span 
      v-else-if="supportsFederation(connection.dbType)"
      class="badge federation-disabled"
      :title="'联邦查询已禁用'"
    >
      <Slash size="12" />
    </span>
    
    <!-- 联邦前缀显示 -->
    <span v-if="connection.federationPrefix" class="federation-prefix">
      {{ connection.federationPrefix }}
    </span>
  </div>
</template>

<style scoped>
.federation-enabled {
  color: #1E90FF;
  margin-left: auto;
}
.federation-disabled {
  color: #FFA500;
  margin-left: auto;
}
.federation-prefix {
  font-size: 11px;
  color: #888;
  background: #f0f0f0;
  padding: 1px 4px;
  border-radius: 3px;
  margin-right: 8px;
}
</style>
```

### 5.2 SQL 编辑器联邦提示

**位置**: `components/editor/QueryEditor.vue`

```typescript
// 联邦连接前缀自动补全扩展
function getFederationCompletions(context: CompletionContext): CompletionResult {
  const prefix = getCurrentWordPrefix(context);
  
  // 检查是否以连接名开头
  const federationConnections = getFederationConnections();
  if (prefix && federationConnections.some(c => c.catalogName.startsWith(prefix))) {
    return {
      items: federationConnections
        .filter(c => c.catalogName.startsWith(prefix))
        .map(c => ({
          label: c.catalogName,
          detail: `${c.dbType} - ${c.description}`,
          apply: `${c.catalogName}.`,
          kind: CompletionItemKind.Property,
          icon: 'database'
        }))
    };
  }
  
  return null;
}

// 联邦表悬停信息
function getFederatedTableHover(sql: string, pos: number): HoverInfo | null {
  const ref = findFederatedTableAt(sql, pos);
  if (!ref) return null;
  
  const connection = getFederatedConnection(ref.connectionId);
  if (!connection) return null;
  
  return {
    content: `联邦表: ${ref.connectionId}.${ref.database}.${ref.schema}.${ref.table}\n连接: ${connection.name}\n行数: ~${connection.rowCount}`,
    range: ref.range
  };
}
```

### 5.3 格式化控制面板（优化版）

```
┌─────────────────────────────────────────────────────────────────┐
│ 🎨 SQL Formatter  [⚙️] [Auto-save: ✓]                    [-X] │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  关键字大小写: [▼ upper ▼]              [📋 复制配置]          │
│  标识符大小写: [▼ preserve ▼]                                           │
│  函数大小写:   [▼ preserve ▼]                                           │
│  缩进样式:     [▼ standard ▼]                                           │
│                                                                  │
│  Tab 宽度:    [  2  ]  ─────○─────  [8]                          │
│  语句间空行:  [  1  ]  ─────○─────  [5]                          │
│                                                                  │
│  ─────────────────────────────────────────────────────────────  │
│                                                                  │
│  ☑ 启用联邦查询感知格式化                                       │
│  ☑ 格式化后自动保存配置                                         │
│                                                                  │
│              [应用格式 (⌘⇧F)]  [重置]  [高级 JSON 配置]         │
│                                                                  │
│  ┌─ 预览 ─────────────────────────────────────────────────────┐ │
│  │ Before:          After:                                    │ │
│  │ select * from     SELECT *                                   │ │
│  │   users;          FROM users;                               │ │
│  └─────────────────────────────────────────────────────────────┘ │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 5.4 联邦查询执行状态栏

```
┌─────────────────────────────────────────────────────────────────────────┐
│ 🌐 联邦查询                                                            │
│ 涉及连接: conn1(PostgreSQL) · conn2(MySQL)                              │
│ 执行时间: 1.234s · 扫描: 10,245 行 · 返回: 1,245 行                     │
│ [查看详细计划]  [重试失败连接]                                          │
└─────────────────────────────────────────────────────────────────────────┘
```

**交互行为**：
- 点击"查看详细计划"展开各连接的子查询详情表格
- 点击"重试失败连接"仅重新执行失败的子查询

---

## 六、安全与权限设计

### 6.1 联邦查询权限模型

```
┌─────────────────────────────────────────────────────────────┐
│                    权限检查流                                │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  用户登录                                                    │
│       ↓                                                      │
│  ┌──────────────────┐                                       │
│  │ 检查联邦连接权限  │ ← 确认用户对涉及的连接有 SELECT 权限  │
│  └────────┬─────────┘                                       │
│           ↓                                                  │
│  ┌──────────────────┐                                       │
│  │ 检查表级权限     │ ← 过滤 excluded_tables                 │
│  └────────┬─────────┘                                       │
│           ↓                                                  │
│  ┌──────────────────┐                                       │
│  │ 执行查询         │ ← 使用连接的实际凭证（不传递密码）      │
│  └────────┬─────────┘                                       │
│           ↓                                                  │
│  ┌──────────────────┐                                       │
│  │ 返回结果         │ ← 脱敏处理敏感列                       │
│  └──────────────────┘                                       │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 6.2 敏感数据保护

```typescript
interface FederationSecurityPolicy {
  // 需要脱敏的列模式
  sensitiveColumnPatterns: RegExp[];
  
  // 最大返回行数限制
  maxReturnRows: number;
  
  // 查询超时（秒）
  queryTimeoutSeconds: number;
  
  // 是否允许跨连接 DML
  allowCrossConnectionDml: boolean;
  
  // 审计日志
  enableAuditLog: boolean;
}

// 结果脱敏
function sanitizeQueryResult(
  result: QueryResult,
  policy: FederationSecurityPolicy
): QueryResult {
  const sanitizedRows = result.rows.map(row => 
    row.map((cell, idx) => {
      if (isSensitiveColumn(idx, policy)) {
        return maskValue(cell);  // 脱敏处理
      }
      return cell;
    })
  );
  
  return { ...result, rows: sanitizedRows };
}
```

---

## 七、错误处理策略

### 7.1 联邦查询错误码

| 错误码 | 含义 | 处理方式 |
|--------|------|---------|
| `FED_001` | 连接不存在 | 提示用户检查连接名称 |
| `FED_002` | 连接无联邦权限 | 提示用户联系管理员 |
| `FED_003` | 表不存在于目标连接 | 提示用户检查表名 |
| `FED_004` | 查询超时 | 增加超时时间或简化查询 |
| `FED_005` | 部分连接失败 | 返回部分结果 + 警告 |
| `FED_006` | 结果合并失败 | 提示用户检查列类型兼容性 |
| `FED_007` | 非法 SQL 语法 | 定位到具体位置并提示 |

### 7.2 错误提示对话框

```
┌─────────────────────────────────────────────────────────┐
│ ⚠️ 联邦查询执行失败                                     │
│                                                         │
│  FED_005: 部分连接执行失败                              │
│                                                         │
│  以下连接执行失败:                                      │
│  • conn2 (MySQL) - Connection refused                   │
│                                                         │
│  已返回 conn1 的结果: 42 行                             │
│                                                         │
│  [重试失败连接]  [完整重试]  [忽略并继续]               │
└─────────────────────────────────────────────────────────┘
```

---

## 八、实施计划

### Phase 1: 基础联邦查询（4-6周）

| 任务 | 工作量 | 交付物 |
|------|--------|--------|
| 1.1 ConnectionConfig 扩展 | 2天 | `use_for_federation` 字段 |
| 1.2 联邦查询 SQL 解析器（Rust）| 5天 | `FederatedQueryScheduler` 类 |
| 1.3 分片合并执行引擎 | 5天 | 多连接并行执行 + 结果合并 |
| 1.4 前端联邦连接标识 | 2天 | 连接树图标 + 状态指示 |
| 1.5 单元测试 | 3天 | 覆盖率 ≥ 80% |

### Phase 2: Calcite 集成（4-6周）

| 任务 | 工作量 | 交付物 |
|------|--------|--------|
| 2.1 gRPC 协议定义 | 2天 | `.proto` 文件 |
| 2.2 Calcite Docker 服务 | 3天 | 可独立部署的服务镜像 |
| 2.3 复杂 JOIN 支持 | 5天 | 跨连接关联查询 |
| 2.4 执行计划缓存 | 2天 | 相同查询快速响应 |

### Phase 3: 高级功能（4-8周）

| 任务 | 工作量 | 交付物 |
|------|--------|--------|
| 3.1 DML 联邦查询 | 5天 | UPDATE/INSERT 跨连接支持 |
| 3.2 性能监控面板 | 3天 | 可视化查询耗时分布 |
| 3.3 联邦关系图 | 4天 | 连接拓扑可视化 |
| 3.4 多租户隔离 | 3天 | 资源配额管理 |

---

## 九、风险评估

| 风险 | 概率 | 影响 | 缓解措施 |
|------|------|------|---------|
| Rust + Java 集成复杂度高 | 中 | 高 | Phase 1 纯 Rust 实现，Phase 2 再引入 Calcite |
| 跨连接 JOIN 性能不可控 | 高 | 中 | 设置合理的默认超时和结果行数限制 |
| Schema 漂移导致兼容问题 | 中 | 低 | 查询前验证目标表的 schema 一致性 |
| 方言自动检测误判 | 中 | 低 | 提供手动覆盖选项 |
| 大 SQL 格式化内存溢出 | 低 | 中 | 实施字符上限 + 流式处理 |

---

## 十、附录

### A. 相关代码位置参考

```
后端核心（Rust）:
├── crates/dbx-core/src/sql_dialect/identifiers.rs  ← 新增联邦命名规则
├── crates/dbx-core/src/query.rs                    ← 新增联邦查询执行路径
├── crates/dbx-core/src/models/connection.rs        ← 扩展连接配置
├── crates/dbx-core/src/storage/federation_config.rs ← 新增联邦配置存储

前端（TypeScript/Vue）:
├── apps/desktop/src/lib/sql/sqlCompletion.ts       ← 扩展联邦补全
├── apps/desktop/src/lib/sql/sqlFormatter.ts        ← 联邦感知格式化
├── apps/desktop/src/components/sidebar/ConnectionTree.vue ← 联邦状态图标
├── apps/desktop/src/components/editor/QueryEditor.vue    ← 联邦提示栏
└── apps/desktop/src/stores/connectionStore.ts      ← 联邦配置管理

测试:
├── packages/app-tests/sqlFormatter.test.ts         ← 新增联邦格式化测试
├── packages/app-tests/federation.test.ts           ← 联邦查询集成测试
└── packages/app-tests/federation.security.test.ts  ← 安全测试
```

### B. SQL 联邦查询示例

#### PostgreSQL 联邦查询
```sql
-- 用户输入（普通 SQL）
SELECT u.name, o.order_date 
FROM users u 
JOIN orders o ON u.id = o.user_id 
WHERE o.status = 'completed';

-- 后端透明重写（发送给 Calcite/Rust 引擎）
SELECT 
    "conn1"."public"."users"."name" AS "u.name",
    "conn1"."public"."orders"."order_date" AS "o.order_date"
FROM "conn1"."public"."users" AS u
JOIN "conn1"."public"."orders" AS o ON u."id" = o."user_id"
WHERE o."status" = 'completed';
```

#### MySQL 联邦查询
```sql
-- 用户输入
SELECT c.customer_name, p.product_name 
FROM customers c 
JOIN products p ON c.pid = p.id;

-- 后端透明重写
SELECT 
    `conn1`.`sales`.`customers`.`customer_name` AS `c.customer_name`,
    `conn2`.`inventory`.`products`.`product_name` AS `p.product_name`
FROM `conn1`.`sales`.`customers` AS c
JOIN `conn2`.`inventory`.`products` AS p ON c.`pid` = p.`id`;
```

---

*本设计文档综合了软件架构师、UI设计师、UX架构师三个角色的专业意见，经架构评审后修订。实施阶段需严格按照 Phase 1→2→3 顺序推进，确保基础功能稳定后再引入高级特性。*
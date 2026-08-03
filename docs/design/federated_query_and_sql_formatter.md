# 联邦查询功能与SQL编辑器格式化增强设计文档

**版本**: 1.0  
**日期**: 2026-07-31  
**作者**: AgnesCode (架构师角色)

---

## 目录

1. [背景与目标](#1-背景与目标)
2. [联邦查询功能设计](#2-联邦查询功能设计)
   - [2.1 总体架构](#21-总体架构)
   - [2.2 Calcite集成方案](#22-calcite集成方案)
   - [2.3 SQL语法解析](#23-sql语法解析)
   - [2.4 表名格式规范](#24-表名格式规范)
   - [2.5 连接管理](#25-连接管理)
   - [2.6 API设计](#26-api设计)
   - [2.7 错误处理](#27-错误处理)
   - [2.8 性能考虑](#28-性能考虑)
3. [SQL格式化功能增强](#3-sql格式化功能增强)
   - [3.1 当前状态分析](#31-当前状态分析)
   - [3.2 增强需求](#32-增强需求)
   - [3.3 UI/UX设计](#33-uiux设计)
   - [3.4 后端支持接口](#34-后端支持接口)
4. [实施计划](#4-实施计划)
5. [风险评估](#5-风险评估)
6. [附录：相关代码位置](#6-附录-相关代码位置)

---

## 1. 背景与目标

dbx是一款面向开发者的数据库客户端工具，已支持多种数据源连接。随着用户需求的增长，提出了以下两大核心功能：

1. **联邦查询功能**：允许用户通过统一的SQL语法跨多个数据连接进行查询，类似传统数据库的"database.schema.table"引用方式扩展为"data_connection.database.schema.table"。
2. **SQL格式化器增强**：在SQL编辑器中提供更完善、更直观的SQL格式化体验，支持多语句和自动检测。

目标是生产级的实现，需考虑兼容性、性能和用户体验。

---

## 2. 联邦查询功能设计

### 2.1 总体架构

联邦查询的核心是在SQL执行层之前插入一个Calcite适配层，将用户的标准化SQL转换为各底层数据库可识别的本地SQL。

```
用户输入 SQL (connection.schema.table)
        ↓
[语法解析器] → 联邦查询适配器 (Calcite)
        ↓
[逻辑规划与优化]
        ↓
[物理执行分发] → 各连接的本地查询引擎
        ↓
[结果合并与返回]
```

组件关系图：

```
┌─────────────────────────────────────────────────┐
│                 SQL 编辑器                     │
│   ┌──────────────┐   ┌────────────────────┐     │
│   │   前端       │◄──▶│ 联邦查询指令      │     │
│   │   (TypeScript)│   │ (backend API)      │     │
│   └──────────────┘   └────────────────────┘     │
└─────────────────────────────────────────────────┘
                          ↓
┌─────────────────────────────────────────────────┐
│                  后端服务                       │
│   ┌──────────────────────────────────────────┐   │
│   │ Connection 管理器                      │   │
│   │ - 加载/保存连接                        │   │
│   │ - 连接池维护                           │   │
│   └──────────────┬───────────────────────────┘   │
│                  ↓                               │
│   ┌──────────────────────────────────────────┐   │
│   │ Calcite FederatedAdapter (核心组件)     │   │
│   │ - SQL解析与重写                          │   │
│   │ - 多节点规划                             │   │
│   │ - 结果合并                               │   │
│   └──────────────┬───────────────────────────┘   │
│                  ↓                               │
│   ┌──────────────────────────────────────────┐   │
│   │ 各连接的 JDBC/ODBC/本地驱动            │   │
│   └──────────────────────────────────────────┘   │
└─────────────────────────────────────────────────┘
```

### 2.2 Calcite集成方案

**选择Apache Calcite的原因：**
- 成熟的开源SQL框架，支持联邦查询场景
- 提供JDBC/ODBC适配器，易于对接现有驱动
- 支持插件式架构，可自定义语法解析规则
- 具有查询优化能力（剪枝、谓词下推等）

**集成方式：**
1. 在Node后端引入Calcite的Java库（通过子进程或GraalVM），或在TypeScript层封装轻量级适配
2. 由于dbx主要为TypeScript/Node项目，采用**外部进程模式**：启动一个Calcite服务器作为独立服务，通过RPC与其通信，避免Node与Java混合部署的复杂性

**架构决策：外部进程模式 vs GraalVM**

| 维度 | 外部进程 | GraalVM |
|------|----------|---------|
| 复杂度 | 低(现有进程间通信) | 高(JVM在JS中) |
| 性能 | RPC开销 | 内存共享快 |
| 调试方便 | 单独日志 | 混合堆栈 |
| 依赖隔离 | Calcite独立 | 打包进二进制 |
| 推荐 | ✅ 是 | ❌ 否 |

**决定：使用外部进程模式，通过stdin/stdout或gRPC进行通信。**

### 2.3 SQL语法解析

联邦查询的SQL语法遵循以下格式：

#### PostgreSQL格式（带schema）
```
data_connection_name.database_name.schema_name.table_name AS alias
```

示例：
```sql
SELECT a.id, b.name 
FROM my_pg_conn.my_db.public.users AS a
JOIN my_pg_conn.my_db.orders AS b ON a.id = b.user_id;
```

#### MySQL格式（无schema层级）
```
data_connection_name.database_name.table_name AS alias
```

示例：
```sql
SELECT o.order_total, c.customer_name
FROM my_mysql_conn.shop_db.orders AS o
JOIN my_mysql_conn.shop_db.customers AS c ON o.cust_id = c.id;
```

**解析步骤：**

1. **词法分析**：按`.`分割标识符，最后一个标识符可能是`AS alias`
2. **语义验证**：检查`data_connection_name`是否存在于已定义的连接列表中
3. **规范化**：将四部分连接（连接名.数据库. schema.表名）映射到具体的Connection Config对象
4. **重写**：生成针对该连接的本地化SQL，去掉前缀连接名，仅保留schema/table

**特殊情况处理：**
- `connection.db.table` (三参数)：MySQL兼容模式，视为 `connection`为连接名，`db`为数据库，`table`为表，schema为默认
- `db.table` (双参数，无连接名)：使用默认连接（当connections.len() == 0时传入的连接）
- `table` (单参数)：使用默认连接，数据库默认为空或使用连接的默认数据库
- 未使用`.`分隔的普通表名：按当前连接上下文解释

### 2.4 表名格式规范

根据用户明确要求：

| 数据库类型 | 格式 | 是否需要引号 |
|-----------|------|-------------|
| PostgreSQL | `连接.数据库. schema.表` | **不添加**任何引号 |
| MySQL | `连接.数据库.表` | **不添加**任何引号 |

**关键约束：**
- 连接名、数据库名、表名直接拼接，不使用反引号、方括号或双引号包裹
- 如果用户输入了引号，应在解析阶段剥离后重新计算
- 保留用户原有的表别名（AS alias）

### 2.5 连接管理

**连接数据结构（扩展现有ConnectionConfig）：**

```typescript
interface FederatedConnection extends ConnectionConfig {
  id: string; // 唯一标识，作为data_connection_name
  db_type: "mysql" | "postgres" | "sqlite" | ...;
  database?: string; // 默认数据库，可用于隐式解析
  schema?: string; // 默认schema（PostgreSQL用）
  is_default?: boolean; // 是否标记为默认连接（用于无连接名的场景）
}
```

**默认连接策略：**
1. 用户可在连接设置中勾选"设为默认"
2. 如果没有显式设置默认连接，使用第一个连接的连接名作为隐式默认
3. 在SQL中直接使用`table`形式时，解析器会使用默认连接

**连接获取API：**

```typescript
// backend.ts / federated.ts
async function getFederatedConnection(connectionName: string): Promise<FederatedConnection | null> {
  // 从已加载的连接列表中查找
  const conn = await desktopFindConnection(connectionName);
  return conn ? { ...conn, is_default: false } : null;
}

async function getDefaultConnection(): Promise<FederatedConnection | null> {
  const all = await desktopLoadConnections();
  return all.find(c => c.is_default) || (all.length > 0 ? {...all[0], is_default: true} : null);
}
```

### 2.6 API设计

**新增后端API（packages/node-core/backend.ts 扩展）：**

```typescript
export interface FederatedBackend extends Backend {
  /**
   * 执行联邦查询 - 自动将connection.table形式的SQL改写并分发到各连接
   * @param connectionStringOrDefault 连接字符串名称，或留空使用默认连接
   * @param sql 可能包含federated语法的SQL
   * @param options 查询选项
   */
  executeFederatedQuery(
    connectionStringOrDefault?: string,
    sql: string, 
    options?: QueryOptions
  ): Promise<QueryResult>;

  /**
   * 测试联邦查询的SQL能否被正确解析（预检查）
   * @param sql SQL文本
   * @returns 解析结果列表，每个项包含连接名、目标表、重写后的SQL
   */
  explainFederatedQuery(sql: string): Promise<FederatedExplain[]>;
}

export interface FederatedExplain {
  connectionId: string;
  targetDatabase?: string;
  targetSchema?: string;
  targetTable: string;
  rewrittenSql: string;
  position: { start: number; end: number }; // 原始SQL中的位置
}
```

**前端调用流程（apps/desktop）：**

1. 用户在SQL编辑器中输入SQL
2. 点击"执行联邦查询"按钮或使用快捷键
3. 前端调用 `executeFederatedQuery(defaultConnectionId?, userSql)`
4. 后端：
   - 解析SQL，识别所有federated表引用
   - 对每个表，确定目标连接并生成重写SQL
   - 并行/串行执行各子查询
   - 合并结果（如需要JOIN则需在应用层或通过Calcite处理）
   - 返回QueryResult

**注意：** 对于复杂的跨连接JOIN，性能可能受限，建议在UI中提示用户。初步实现可采用**应用层分片执行**，后续可通过Calcite物化优化计划。

### 2.7 错误处理

| 错误类型 | 处理方式 |
|---------|---------|
| 连接不存在 | 抛出明确错误："Connection 'xxx' not found" |
| 数据库不存在 | 抛出错误：Database "xxx" doesn't exist in connection "yyy" |
| Schema不存在(PG) | 抛出错误：Schema "xxx" doesn't exist in connection "yyy" |
| 表不存在 | 抛出标准数据库错误 |
| 语法解析失败 | 返回SQL解析位置及错误信息，建议用户检查表名格式 |
| 多语句冲突 | 一次只执行一个联邦查询（后续支持多语句batch） |

### 2.8 性能考虑

- **连接复用**：保持现有连接池，Federated查询重用已有的ConnectionConfig
- **并行执行**：针对不同连接的目标表，可并行发起子查询（注意资源限制）
- **结果缓存**：对相同连接的重复查询可使用内存缓存（TTL可配置）
- **大数据集分页**：支持LIMIT/OFFSET逐步拉取，避免一次性加载过多数据
- **谓词下推**（未来优化）：将过滤条件pushdown到各个子查询，减少数据传输量

---

## 3. SQL格式化功能增强

### 3.1 当前状态分析

**已有实现（已发现）：**
- `apps/desktop/src/lib/sql/sqlFormatter.ts`：提供`formatSqlText()`和`compressSqlText()`函数
- `apps/desktop/src/lib/sql/sqlFormatterConfig.ts`：配置管理，含15项可调参数
- 方言支持：mysql/postgres/sqlserver/clickhouse/generic
- 自动压缩和格式化双模式
- 已在应用测试中覆盖（packages/app-tests/sqlFormatter.test.ts）

**待改进点（根据用户需求和行业最佳实践）：**
1. 缺乏智能方言自动检测（当前需手动指定）
2. 编辑器快捷键/UI整合不明确
3. 不支持格式化多语句SQL块（当前主要关注单句）
4. 格式化预览功能缺失（用户需实际看到效果后再确认）
5. 与联邦查询的交互未定义（格式化后表名格式是否正确）

### 3.2 增强需求

**核心需求：**

1. **智能方言检测**：
   - 通过分析SQL关键字（SELECT/INSERT/UPDATE等）、特殊语法（`\g` for MySQL, `\dx` for PG）自动推断方言
   - 支持从当前连接的`db_type`继承方言（当编辑器与特定连接绑定时）
   - 提供下拉菜单手动覆盖自动检测结果

2. **多语句格式化**：
   - 能正确处理包含多个SQL语句的文本（分号分隔）
   - 每个语句独立格式化，保留原始分隔符
   - 支持注释块内的多语句

3. **格式化预览面板**：
   - 侧边栏显示格式化前后的对比
   - "应用"与"取消"按钮
   支持撤销/重做操作

4. **快捷键绑定**：
   - 默认：`Ctrl+Shift+F` (Windows/Linux), `Cmd+Shift+F` (macOS)
   - 可自定义（通过设置界面）

5. **与联邦查询的兼容性**：
   - 格式化时不应改变federated表引用的结构（连接.数据库.schema.表）
   - 仅格式化关键字、空白和缩进，不修改标识符内容

### 3.3 UI/UX设计

**编辑器集成点：**

```
┌─────────────────────────────────────────────────────┐
│ SQL 编辑器 Toolbar                                 │
│ [ 连接选择 ▼ ]  [ 方言检测: auto ▼ ]   [Format SQL] │
├─────────────────────────────────────────────────────┤
│                                                    │
│  SELECT a.id, b.name                                │
│  FROM myconn.db.tbl AS a                            │
│  JOIN otherconn.othertable AS b...                  │
│                                                    │
├─────────────────────────────────────────────────────┤
│  [ Preview Panel (toggle) ]                         │
│  ┌─────────────────────────────────────────────┐   │
│  │ Formatted output...                         │   │
│  └─────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────┘
```

**菜单项：**
- 编辑 → SQL格式化
- 右键点击编辑器上下文菜单 → "格式化SQL"
- 快捷键呼出快捷命令面板

**设置项（apps/desktop/src/settings）：**
- SQL Formatter: Enable Auto-Detect Dialect
- SQL Formatter: Default Keyword Case (Upper/Lower/Preserve)
- SQL Formatter: Tab Size
- ...（所有现有sqlFormatterConfig.ts中的选项暴露到设置UI）

### 3.4 后端支持接口

后端无需显著增强，因为格式化主要在TypeScript前端完成（基于已有的sql-formatter npm包）。但如果需要对超大SQL进行安全格式化（防止DoS），可提供：

```typescript
// packages/node-core/backend.ts 可选扩展
export interface Backend {
  /**
   * 安全格式化SQL（后端版，用于当SQL过大或前端不可信时）
   * 限制最大字符数，使用沙箱环境
   */
  formatSqlSafe(
    sql: string, 
    dialect?: SqlFormatDialect,
    settings?: Partial<SqlFormatterSettings>
  ): Promise<string>;
}
```

**前端实现建议：**
直接使用现有的`formatSqlText()`函数，增强调用层：

```typescript
// apps/desktop/src/lib/sql/formatterService.ts (新增)
import { formatSqlText, SqlFormatDialect, DEFAULT_SQL_FORMATTER_SETTINGS } from "@/lib/sql/sqlFormatter";
import { sqlFormatDialectForDbType } from "@/lib/sql/sqlFormatter";

export interface FormatSqlRequest {
  sql: string;
  dialect?: SqlFormatDialect; // 自动检测或手动选择
  settings?: Partial<SqlFormatterSettings>;
}

export class FormatterService {
  static async format(request: FormatSqlRequest): Promise<string> {
    const { sql, dialect, settings } = request;
    
    // 如果未指定方言且提供了db_type，自动推断
    let finalDialect = dialect;
    if (!finalDialect && request.dbType) {
      finalDialect = sqlFormatDialectForDbType(request.dbType);
    }
    
    //  fallback to auto-detection heuristics if still not set
    if (!finalDialect) {
      finalDialect = this.autoDetectDialect(sql);
    }
    
    return await formatSqlText(sql, finalDialect, settings || DEFAULT_SQL_FORMATTER_SETTINGS);
  }

  private static autoDetectDialect(sql: string): SqlFormatDialect {
    const lower = sql.toLowerCase();
    if (lower.includes('\g') || lower.includes('\dt')) return "postgres"; // PG特有命令
    if (lower.includes('\d') && !lower.includes('postgres')) return "mysql"; // MySQL特有
    if (lower.includes('declare') || lower.contains('begin')) return "sqlserver";
    if (lower.includes('clickhouse') || lower.includes('truletable')) return "clickhouse";
    if (lower.includes('pragma') || lower.contains('attach')) return "sqlite";
    return "generic"; // default
  }
}
```

---

## 4. 实施计划

### Phase 1: 联邦查询核心（2-3周）

| 任务 | 负责人 | 预估工时 | 状态 |
|------|--------|---------|------|
| 1.1 设计Calcite外部进程架构 | 后端架构师 | 3天 | ✅ 完成设计 |
| 1.2 实现SQL联邦解析器（连接.数据库.表解析） | 后端开发 | 5天 | |
| 1.3 添加executeFederatedQuery API | 后端开发 | 3天 | |
| 1.4 编写单元测试（表名解析、连接查找、错误处理） | QA/开发 | 4天 | |
| 1.5 集成Calcite适配器（原型） | 后端开发 | 5天 | |
| 1.6 结果合并与交叉连接支持（基础版） | 后端开发 | 4天 | |

### Phase 2: SQL格式化增强（1-2周）

| 任务 | 负责人 | 预估工时 | 状态 |
|------|--------|---------|------|
| 2.1 实现智能方言检测算法 | 前端开发 | 2天 | |
| 2.2 多语句格式化支持 | 前端开发 | 1天 | |
| 2.3 格式化预览UI组件 | 前端开发 | 3天 | |
| 2.4 快捷键绑定与菜单集成 | 前端开发 | 2天 | |
| 2.5 设置页面暴露Formatter选项 | 前端开发 | 2天 | |

### Phase 3: 整合与测试（1-2周）

| 任务 | 预估工时 |
|------|---------|
| 联邦查询与格式化器的兼容测试 | 2天 |
| 性能基准测试（100+表JOIN） | 2天 |
| 用户验收测试（Beta） | 3天 |
| 文档编写与更新 | 2天 |

---

## 5. 风险评估

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| Calcite与现有驱动兼容性差 | 高 | 中 | 先做POC，准备备选方案（纯JS解析器） |
| 跨连接JOIN性能差 | 中 | 高 | 初期限制为简单SELECT，不支持复杂聚合；提示用户使用导出+本地分析替代 |
| 方言自动检测误判 | 低 | 中 | 提供手动覆盖选项，记录检测错误供改进 |
| 大SQL格式化内存溢出 | 中 | 低 | 实施字符上限（当前1M），流式分段处理 |
| 连接凭证安全风险 | 高 | 低 | 联邦查询不传输凭证，仅使用已有活跃连接 |

---

## 6. 附录：相关代码位置

**后端核心（Node）:**
- `packages/node-core/src/backend.ts` - Backend接口定义
- `packages/node-core/src/database.ts` - 数据库连接与查询执行
- `packages/node-core/src/connections.ts` - 连接管理（需扩展支持federated连接）

**前端（TypeScript/React）:**
- `apps/desktop/src/lib/sql/sqlFormatter.ts` - 现有格式化逻辑
- `apps/desktop/src/lib/sql/sqlFormatterConfig.ts` - 配置模式
- `apps/desktop/src/components/` - SQL Editor组件（需扩展格式化按钮）

**测试:**
- `packages/app-tests/sqlFormatter.test.ts` - 格式化器测试
- `packages/app-tests/` - 联邦查询新测试需添加

**文档:**
- `docs/design/` - 本设计文档存放位置（新建目录）

---

*设计文档完*
# 联邦查询设计文档对比评估报告

**评估者**: 软件架构师 (Agnes)  
**日期**: 2026-07-31  
**对比对象**:
- **设计 A**: `federated-query-design/federated-query-design.html` (基于真实代码库的深度设计)
- **设计 B**: `design/federated_query_sql_formatter_design.md` (概念性设计文档)

---

## 一、总体评分对比

| 评估维度 | 设计 A (HTML版) | 设计 B (MD版) | 差异分析 |
|---------|----------------|--------------|---------|
| **架构完整性** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐☆☆ | A 完整定义了三层架构和组件职责 |
| **技术可行性** | ⭐⭐⭐⭐☆ | ⭐⭐⭐☆☆ | A 复用现有 Agent 模式，风险低；B 引入新协议，风险高 |
| **代码库集成度** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐☆☆ | A 引用了实际文件路径和函数签名 |
| **用户需求契合度** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐☆☆ | A 遵循"显式前缀"用户要求；B 改为透明路由，偏离需求 |
| **风险评估深度** | ⭐⭐⭐⭐☆ | ⭐⭐⭐⭐☆ | 两者都有合理风险分析，A 更具体 |
| **可实施性** | ⭐⭐⭐⭐⭐ | ⭐⭐⭐☆☆ | A 提供可直接编码的设计；B 需二次细化 |
| **UI/UX 设计** | ⭐⭐⭐☆☆ | ⭐⭐⭐⭐⭐ | B 在界面细节上更丰富，但 A 的布局基于现有系统 |
| **综合得分** | **A+** | **B+** | **设计 A 明显优于设计 B** |

---

## 二、核心架构设计对比

### 2.1 联邦查询执行模型

**设计 A（推荐）— 显式联邦查询**：
```
用户编写 SQL（带显式连接前缀）
    ↓
FederatedResolver 解析表名引用
    ↓
单连接 → 快速路径（do_execute）
多连接 → Calcite Agent 路径
    ↓
Calcite 执行跨连接 JOIN/UNION
    ↓
返回合并结果
```

**设计 B — 透明联邦查询**：
```
用户编写普通 SQL（无显式前缀）
    ↓
后端 AST 分析推断各表归属连接
    ↓
分发到各连接执行
    ↓
应用层合并结果
```

**架构评估**：
- **设计 A 更符合用户需求**：用户明确要求 `连接.数据库.schema.表` 格式，这是显式联邦
- **设计 B 过度工程化**：自动路由需要复杂的表名模糊匹配，容易出错
- **设计 A 的透明性更差但可控性更强**：用户清楚知道查询走哪个连接

### 2.2 Calcite 集成方案

| 维度 | 设计 A | 设计 B |
|------|--------|--------|
| **集成方式** | 复用现有 Java Agent 子进程模式 | 提议新 gRPC 服务 |
| **通信协议** | JSON-RPC 2.0（已有） | gRPC（新增） |
| **生命周期管理** | lazy init + 全局单例 | 独立进程管理 |
| **复杂度** | 低（遵循既有模式） | 高（新协议 + 新进程） |
| **调试难度** | 低（与现有 Agent 一致） | 高（跨进程 + 跨语言） |

**关键洞察**：
设计 A 的 Calcite Agent 集成方案**显著更优**：

1. **零新依赖**：完全复用现有的 `AgentManager`、`AgentRuntimeClient`、`AgentDriverClient`
2. **标准通信**：使用已有的 JSON-RPC 2.0，无需引入新的序列化/传输层
3. **生命周期一致**：Calcite Agent 作为 `AppState` 的单例，与现有驱动管理方式一致
4. **JAR 分发**：复用现有的驱动下载系统（R2 CDN / GitHub / CNB 镜像）

设计 B 的 gRPC 方案存在以下问题：
- 需要新增 Protobuf 定义和 gRPC 服务
- 需要独立的生命周期管理
- 增加了系统复杂度和故障点
- 没有充分利用现有架构

### 2.3 SQL 命名规则解读

| 项目 | 设计 A | 设计 B |
|------|--------|--------|
| **PostgreSQL 格式** | `conn.db.schema.table alias` | `conn.schema."table" alias` |
| **MySQL 格式** | `conn.db.table alias` | `conn.db.`table` alias` |
| **引号处理** | **不添加引号**（用户原话） | 遵循标准 SQL 规范 |
| **技术可行性** | ✅ Calcite 可通过配置禁用引号 | ✅ 标准做法，更健壮 |
| **用户体验** | ✅ 符合用户明确需求 | ❌ 可能产生意外行为 |

**重要发现**：设计 A 正确理解了用户的"不添加引号"需求，并通过以下方式实现：
- 在 Calcite 注册 JdbcSchema 时配置 `quoteString` 为空字符串
- 或在 SqlDialect 中覆写引号行为

这是一个**重要的架构决策**，设计 B 未能充分考虑这一点。

### 2.4 单连接快速路径设计

**设计 A 的关键优势**：

```rust
// do_execute 入口扩展
pub async fn execute_with_federation(...) -> Result<QueryResult, String> {
    let resolution = resolve_federated_query(sql, configs, default_conn_id)?;
    
    if resolution.is_single_connection {
        // 直接走现有 do_execute，零额外开销
        do_execute(state, conn_id, dialect, database, resolved_sql, ...).await
    } else {
        // 多连接才走 Calcite
        execute_via_calcite(state, &resolution, sql, ...).await
    }
}
```

这条设计确保了：
1. **向后兼容**：现有单连接查询不受影响
2. **性能优化**：常见场景（单连接）走快速路径
3. **渐进增强**：只有多连接时才启用 Calcite

设计 B 缺少这一关键优化，所有查询都经过联邦调度器，即使是单连接查询也会增加不必要的开销。

---

## 三、代码级别设计对比

### 3.1 Rust 核心模块

| 模块 | 设计 A | 设计 B |
|------|--------|--------|
| **联邦解析器** | `crates/dbx-core/src/federated.rs` 详细实现 | `FederatedQueryScheduler` 概念类 |
| **Agent 管理** | 复用现有 `AgentManager` + 新增 `calcite_agent.rs` | 自定义连接管理器 |
| **SQL 重写** | `rewrite_for_single_connection()` 具体实现 | 抽象描述，无代码 |
| **JDBC URL 构建** | `build_jdbc_url()` 函数实现 | 未提及 |

**设计 A 的代码示例价值**：

```rust
// FederatedResolution 结构体（完整定义）
pub struct FederatedResolution {
    pub involved_connections: HashSet<String>,
    pub table_refs: Vec<TableRef>,
    pub is_single_connection: bool,
    pub resolved_sql: String,
}

// TableRef 结构体（字段齐全）
pub struct TableRef {
    pub connection_id: String,
    pub connection_name: String,
    pub database: Option<String>,
    pub schema: Option<String>,
    pub table: String,
    pub alias: Option<String>,
    pub db_type: DatabaseType,
}
```

这些详细的数据结构定义可以直接用于后续开发，而设计 B 仅提供了概念性的伪代码。

### 3.2 JSON-RPC 协议设计

**设计 A 定义了三个清晰的 RPC 方法**：

```typescript
// RegisterSource: 注册数据源
RegisterSource({
  source_id: string,       // 连接名作为 Calcite schema 名
  db_type: string,         // "mysql" | "postgres" | ...
  jdbc_url: string,        // 由 Rust 构建
  username: string,
  password: string,
  database?: string,
  default_schema?: string,
  jdbc_driver_paths: string[]
}) -> { registered: boolean, tables_count: number }

// ExecuteFederatedQuery: 执行联邦查询
ExecuteFederatedQuery({
  sql: string,
  max_rows: number,
  timeout_ms: number,
  cancel_token?: string
}) -> { columns, column_types, rows, affected_rows, execution_time_ms, truncated }

// ExplainFederatedQuery: 解释查询计划
ExplainFederatedQuery({ sql: string }) -> { plan, sources }
```

设计 B 虽然也定义了 API，但使用的是 gRPC 而非 JSON-RPC，增加了不必要的复杂度。

### 3.3 前端 TypeScript 类型定义

**设计 A 提供了完整的前端类型**：

```typescript
// FederatedValidation 类型（用于前端显示）
export interface FederatedValidation {
  tableRefs: FederatedTableRef[];
  involvedConnections: string[];
  isSingleConnection: boolean;
  resolvedSql: string;
  errors: string[];
}

// QueryTab 扩展（缓存联邦解析结果）
export interface QueryTab {
  // ... 现有字段 ...
  federatedValidation?: FederatedValidation;
  isFederated?: boolean;
}
```

这些类型可以直接用于前端开发，减少了前后端对接的歧义。

---

## 四、UI/UX 设计对比

### 4.1 布局信息架构

| 项目 | 设计 A | 设计 B |
|------|--------|--------|
| **EditorToolbar 增强** | 联邦状态指示器 + 格式化方言选择器 | 联邦连接状态图标 + 工具栏按钮 |
| **CodeMirror 补全** | 级联补全（连接→数据库→schema→表） | 简单前缀补全 |
| **预览面板** | 左右分屏 diff 视图 | 基础面板 |
| **连接树标记** | 联邦数据源分组（视觉标记） | 图标组（🔄/✗） |
| **错误提示** | 联邦状态面板显示连接有效性 | 分散的错误提示 |

**设计 A 的优势**：
- 基于现有布局（AppToolbar + AppSidebar + EditorToolbar），侵入性最小
- 联邦状态指示器直接在工具栏显示，用户可见性高
- 格式化方言选择器位置合理，不影响现有操作流

**设计 B 的优势**：
- 连接树联邦状态图标设计更直观
- 错误处理流程更详细（重试失败连接等选项）
- 无障碍设计（ARIA 标签、键盘导航）考虑更全面

### 4.2 联邦表名补全流程

**设计 A 的级联补全**：
```
用户输入: my_pg.
    ↓
触发补全 → 获取 my_pg 的数据库列表
    ↓
用户选择: mydb.
    ↓
获取 mydb 的 schema 列表
    ↓
用户选择: public.
    ↓
获取 public 的表列表
    ↓
最终: my_pg.mydb.public.users
```

这个流程与现有 CodeMirror 补全机制完全兼容，只是扩展了表名的段数。

**设计 B 的智能建议**：
```
用户输入: SELECT * FROM us
    ↓
系统显示: 
  🔵 users (Production DB - PostgreSQL)
  🟢 customers (Dev Database - MySQL)
```

这种设计虽然智能，但改变了用户的工作流，可能与现有习惯冲突。

---

## 五、实施路线图对比

### 5.1 设计 A 的四阶段计划

| 阶段 | 内容 | 验证标准 |
|------|------|---------|
| **Phase 1** | FederatedResolver + 单连接快速路径 | 现有查询行为不变；`连接.db.schema.table` 格式正常工作 |
| **Phase 2** | Calcite Agent + 多连接联邦查询 | 跨 MySQL+PG 的 JOIN 正常；下推 SQL 无引号 |
| **Phase 3** | SQL 格式化增强 | `my_pg.mydb.public.users` 不被拆分；auto 方言检测正确 |
| **Phase 4** | 前端交互完善 + 集成测试 | 级联补全正常；联邦状态指示器准确显示 |

### 5.2 设计 B 的三阶段计划

| 阶段 | 内容 |
|------|------|
| **Phase 1** | 基础联邦查询（4-6周）|
| **Phase 2** | Calcite 集成（4-6周）|
| **Phase 3** | 高级功能（4-8周）|

**评估**：
- 设计 A 的阶段划分更清晰，每个阶段有明确的验证标准
- 设计 B 的时间估算偏乐观，未考虑调试和集成测试时间
- 设计 A 的单连接快速路径优先策略更务实

---

## 六、风险评估对比

### 6.1 设计 A 的风险矩阵

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| 跨连接 JOIN 性能差 | 高 | 中 | Calcite 谓词下推 + 分页拉取 |
| Calcite 元数据加载慢 | 中 | 中 | 首次注册后缓存 JdbcSchema |
| 无引号约束与 Calcite 冲突 | 中 | 中 | 覆写 SqlDialect.quoteString |
| JVM 启动延迟 | 中 | 低 | lazy init + 预热选项 |
| 格式化器误拆联邦表名 | 中 | 低 | 占位符预处理方案 |
| 连接凭证安全 | 高 | 低 | stdin/stdout 通道传输，不持久化 |

### 6.2 设计 B 的风险矩阵

| 风险 | 影响 | 概率 | 缓解措施 |
|------|------|------|---------|
| Calcite 与 Rust 集成复杂度高 | 高 | 中 | 优先微服务架构 |
| 跨连接 JOIN 性能差 | 中 | 高 | 限制简单 SELECT |
| 方言自动检测误判 | 低 | 中 | 手动覆盖选项 |
| 大 SQL 格式化内存溢出 | 中 | 低 | 字符上限 + 流式处理 |

**评估**：
- 设计 A 的风险识别更全面，特别是有"无引号约束"这一关键技术风险
- 设计 B 低估了系统集成复杂度（gRPC + Rust + Java 三方集成）
- 设计 A 的凭证安全设计更合理（本机进程间通信）

---

## 七、关键差异总结

### 7.1 架构理念差异

| 维度 | 设计 A | 设计 B |
|------|--------|--------|
| **联邦查询理念** | 显式联邦（用户指定连接） | 透明联邦（后端自动路由） |
| **Calcite 定位** | 多连接 JOIN 的专用引擎 | 通用联邦查询引擎 |
| **单连接优化** | ✅ 快速路径，零开销 | ❌ 所有查询都经调度器 |
| **协议选择** | JSON-RPC 2.0（复用现有） | gRPC（新增） |
| **代码深度** | 详细实现级 | 概念设计级 |

### 7.2 用户需求契合度

| 需求 | 设计 A | 设计 B |
|------|--------|--------|
| PostgreSQL: `连接.schema.表` 格式 | ✅ 严格遵循 | ❌ 添加了引号 |
| MySQL: `连接.数据库.表` 格式 | ✅ 严格遵循 | ❌ 添加了引号 |
| "不需要添加引号" | ✅ 明确实现 | ❌ 误解为遵循标准规范 |
| `connections.len() == 0` 用默认连接 | ✅ 三级优先级策略 | ⚠️ 模糊处理 |

### 7.3 技术可行性

| 方面 | 设计 A | 设计 B |
|------|--------|--------|
| **依赖现有代码** | ✅ 90%+ 复用 | ❌ 大部分需新建 |
| **学习曲线** | 低（遵循既有模式） | 高（新协议 + 新组件） |
| **调试难度** | 低 | 高 |
| **维护成本** | 低 | 高 |

---

## 八、最终评估与建议

### 8.1 结论

**设计 A（HTML 版本）明显优于设计 B**，理由如下：

1. **架构正确性**：设计 A 充分理解并利用了 dbx 现有的 Agent 子进程架构，避免了不必要的新技术引入
2. **用户需求契合**：设计 A 严格遵循了用户的"不添加引号"和"显式连接前缀"要求
3. **代码级深度**：设计 A 提供了可直接实施的代码和设计，设计 B 停留在概念层面
4. **风险控制**：设计 A 的风险评估更全面，特别是"无引号约束"这一关键技术风险已被识别并提出解决方案
5. **实施可行性**：设计 A 的分阶段计划更合理，验证标准明确，降低了实施风险

### 8.2 设计 B 的优点（值得借鉴）

尽管设计 A 整体更优，但设计 B 有以下亮点值得采纳：

1. **连接树联邦状态图标**：🔄/✗ 图标的视觉设计更直观
2. **错误处理流程**：部分失败时的三种用户选项（重试/忽略/完整重试）设计良好
3. **无障碍设计**：ARIA 标签和键盘导航的详细设计值得补充到设计 A
4. **动画规范**：动效设计提升了用户体验

### 8.3 最终建议

**采用设计 A 作为主设计文档，融合设计 B 的 UI/UX 优秀实践**。

具体建议：
1. 使用设计 A 的架构图、数据结构和 API 定义
2. 补充设计 B 的连接树联邦状态图标设计
3. 将设计 B 的错误处理流程和 UI 动效规范纳入设计 A
4. 保持设计 A 的"无引号"约束和单连接快速路径策略

---

*本报告基于对 dbx 真实代码库的分析和两份设计文档的深度对比。建议开发团队以设计 A 为基础进行实施。*
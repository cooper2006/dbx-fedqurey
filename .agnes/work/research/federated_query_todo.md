# 联邦查询落地开发 TODO

## 项目状态评估 (2026-08-03)

### 已完成部分

#### Phase 1 (P0) - 核心后端
- [x] **1.1 ConnectionConfig 扩展** 
  - 位置: `crates/dbx-core/src/models/connection.rs:180`
  - 字段: `federation_enabled: bool` (默认值 `false`)
  
- [x] **1.2 FederatedResolver 核心模块**
  - 位置: `crates/dbx-core/src/federated.rs`
  - 功能: SQL AST 解析、联邦表引用检测、单连接快速路径判断
  - 导出函数: `analyze_federation()`, `rewrite_federated_sql()`
  - 包含完整单元测试

- [ ] **1.3 单连接快速路径（修改 query.rs）**
  - 需要集成 `federated` 模块到 `query.rs`
  - 当前 query.rs 未调用 `analyze_federation()`

- [ ] **1.4 Agent 目录注册（calcite 类型）**
  - 需要在 `agent_catalog.rs` 添加 Calcite 类型条目
  
- [ ] **1.5 连接树联邦状态图标（ConnectionTree.vue）**
  - 需要在树形组件中显示联邦状态指示器

#### Phase 2 (P1) - Calcite Agent
- [ ] **2.1 Java Calcite Agent 项目骨架**
  - 待创建新的 Java 项目

- [ ] **2.2 Calcite 联邦执行服务**
  - 待实现 gRPC 协议

- [ ] **2.3 Rust 侧 Calcite Agent 生命周期管理**
  - 位置: `crates/dbx-core/src/calcite_agent.rs`
  - 当前状态: 骨架代码存在，但有重复结构体定义，需清理

#### Phase 3 (P2) - 前端增强
- [ ] **3.1 联邦感知格式化器（federatedFormatter.ts）**
  - 文件不存在，需新建

- [ ] **3.2 方言自动检测（dialectDetector.ts）**
  - 文件不存在，需新建

- [ ] **3.3 编辑器联邦状态栏**
  - QueryEditor.vue 未添加联邦状态显示

- [ ] **3.4 联邦表名级联补全**
  - sqlCompletion.ts 未集成联邦表名支持

#### Phase 4 (P3) - 集成测试
- [ ] **4.1 单元测试**
- [ ] **4.2 端到端测试**
- [ ] **4.3 文档更新**

---

## 下一步行动计划

### P0 优先级任务（阻塞其他功能）

#### 1. 修复 calcite_agent.rs 重复结构体问题
```rust
// 当前文件有两个 CalciteAgentConfig 定义，需要合并
```

#### 2. 在 agent_catalog.rs 添加 Calcite 类型
```rust
// 需要添加类似这样的条目:
AgentCatalogEntry {
    db_type: DatabaseType::Calcite, // 或新增枚举值
    key: "calcite",
    label: "Apache Calcite",
    store_visible: true,
    profiles: &[],
},
```

#### 3. 集成 federated 模块到 query.rs
关键修改点:
- `execute_sql_statement_with_options()` 入口
- 调用 `analyze_federation()` 检测是否需要联邦路由
- 单连接场景: 使用 `rewrite_federated_sql()` 重写后正常执行
- 多连接场景: 暂返回错误提示用户启用 Calcite Agent

#### 4. 前端 ConnectionTree.vue 添加联邦状态图标
- 在连接节点旁显示联邦启用/禁用图标
- 使用 Lucide 图标库中的相关图标（如 Network 或 Layers）

### P1 优先级任务

#### 1. 清理并完善 calcite_agent.rs
- 移除重复结构体
- 实现实际的进程启动逻辑
- 添加 gRPC 客户端集成

#### 2. 创建 Java Calcite Agent 项目骨架
- 建议位置: `agents/calcite-agent/`
- 使用 Maven 或 Gradle 构建
- 依赖: Apache Calcite, gRPC, JDBC 驱动

---

## 设计文档参考

详见:
- `design/architecture_review_federated_query.md` - 架构评审报告
- `design/federated_query_sql_formatter_design.md` - SQL 格式化设计
- `design/ui_ux_optimization_federated_formatter.md` - UI/UX 优化设计
- `federated-query-design/federated-query-design.html` - 交互式设计方案

---

## 关键设计决策

1. **联邦查询应透明重定向** - 用户写普通 SQL，后端自动检测表归属并重写
2. **命名规则**: 
   - PostgreSQL: `connection.db.schema."table" alias`
   - MySQL: `connection.db."table" alias`
3. **Phase 1 策略**: 分片合并（Shard-and-Merge），简单场景足够
4. **Phase 2 策略**: 接入 Calcite 支持复杂 JOIN

---

*最后更新: 2026-08-03*

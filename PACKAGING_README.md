# 🚀 DBX 桌面应用打包指南

## 快速开始

### 一键打包（推荐）
```bash
cd /Users/cooper/GitHub/dbx
./scripts/pack-desktop.sh
```

脚本会自动：
1. 检测已安装的工具（Node.js、Rust、Java）
2. 安装缺失的依赖
3. 根据您的选择构建相应组件

---

## 构建选项

| 选项 | 功能 | 所需工具 |
|------|------|----------|
| 1 | Web 前端 | Node.js, pnpm |
| 2 | 桌面应用 | Node.js, pnpm, Rust |
| 3 | Java Agent | Java, Gradle |
| 4 | 全部构建 | 所有上述工具 |
| 5 | 仅安装依赖 | Node.js, pnpm |

---

## 当前环境状态

```
✅ Node.js: v24.15.0 (可用)
✅ pnpm: 已安装 (可用)
⚠️  Rust/Cargo: 未安装 (需要构建桌面应用)
⚠️  Java: 未安装 (需要构建 Calcite Agent)
```

---

## 安装缺失工具

### 安装 Rust（用于桌面构建）
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 安装 Java（用于 Calcite Agent）
```bash
brew install openjdk@21
export JAVA_HOME=$(/usr/libexec/java_home -v 21)
```

### 安装 Gradle（用于 Java Agent）
```bash
brew install gradle
```

---

## 独立构建命令

### 仅构建 Web 前端
```bash
pnpm install
pnpm build
# 输出: dist/
```

### 仅构建桌面应用
```bash
pnpm install
pnpm tauri build
# 输出: target/release/bundle/macos/
```

### 仅构建 Java Calcite Agent
```bash
cd agents
./gradlew :drivers:calcite:shadowJar
# 输出: agents/drivers/calcite/build/libs/dbx-agent-calcite.jar
```

---

## 联邦查询功能说明

本版本集成了完整的联邦查询功能：

### ✅ 已实现功能

1. **核心后端** (Rust)
   - `federated.rs`: SQL 联邦分析引擎
   - `calcite_agent.rs`: Calcite Agent 生命周期管理
   - `federation_grpc.rs`: gRPC 协议定义
   - `federation_schema_visibility.rs`: Schema 可见性控制

2. **前端增强** (TypeScript/Vue)
   - 联邦状态图标显示（连接树）
   - 联邦感知格式化器
   - 方言自动检测
   - 联邦查询状态栏

3. **Java Calcite Agent**
   - JSON-RPC 2.0 通信
   - 多数据源注册/注销
   - 联邦查询执行
   - Schema 可见性控制

4. **测试套件**
   - 单元测试: `federated_query_tests.rs`
   - 端到端测试: `e2e_federated_query.rs`

### 📦 构建产物

| 组件 | 位置 | 大小 |
|------|------|------|
| Web 前端 | `dist/` | ~50MB |
| macOS 应用 | `target/release/bundle/macos/DBX.app` | ~150MB |
| macOS DMG | `target/release/bundle/macos/DBX.dmg` | ~150MB |
| Java Agent | `target/agents/dbx-agent-calcite.jar` | ~80MB |

---

## 验证安装

### 运行桌面应用
```bash
open target/release/bundle/macos/DBX.app
```

### 测试联邦查询
1. 打开 DBX
2. 添加一个 PostgreSQL 连接（启用 Federation Enabled）
3. 执行查询：
   ```sql
   SELECT * FROM my_conn.public.users WHERE id = 1;
   ```

### 测试跨连接查询
```sql
SELECT a.name, b.amount 
FROM conn1.public.orders a 
JOIN conn2.shop.sales b ON a.id = b.order_id;
```

---

## 故障排除

### 错误：找不到 cargo
```bash
# 解决方案
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
cargo --version  # 验证
```

### 错误：Tauri 编译失败
```bash
# macOS 系统依赖
brew install webkitgtk librsvg
```

### 错误：Java 版本不兼容
```bash
# 确保使用 Java 21
export JAVA_HOME=$(/usr/libexec/java_home -v 21)
java -version
```

---

## 详细文档

- [完整构建指南](./DESKTOP_BUILD_GUIDE.md)
- [联邦查询实现](./FEDERATED_QUERY_IMPLEMENTATION.md)
- [快速开始](./PACKING_QUICKSTART.md)
- [Makefile 参考](./Makefile)

---

*创建时间: 2026-08-03*  
*版本: v1.0*

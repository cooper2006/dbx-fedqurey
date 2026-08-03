# DBX 桌面应用打包 - 快速开始

## 🎯 当前状态

```
✅ Node.js: v24.15.0 (已安装)
✅ pnpm: 已安装
⚠️  Rust/Cargo: 未安装（需要用于桌面构建）
⚠️  Java: 未找到（需要用于 Calcite Agent）
```

## 🚀 一键快速构建

### 方案一：仅构建 Web 前端（无需额外依赖）

```bash
cd /Users/cooper/GitHub/dbx
pnpm build
```

**输出位置**: `dist/`

### 方案二：构建桌面应用（需要安装 Rust）

#### 步骤 1：安装 Rust
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

#### 步骤 2：验证安装
```bash
rustc --version
cargo --version
```

#### 步骤 3：构建桌面应用
```bash
cd /Users/cooper/GitHub/dbx
pnpm tauri build
```

**输出位置**: `target/release/bundle/macos/`

### 方案三：使用便捷脚本

```bash
# 查看可用目标
./scripts/quick-build.sh

# 运行构建
./scripts/quick-build.sh all
```

---

## 📦 构建产物说明

| 类型 | 路径 | 大小 | 用途 |
|------|------|------|------|
| Web 前端 | `dist/` | ~50MB | 部署到服务器 |
| macOS 应用 | `target/release/bundle/macos/DBX.app` | ~150MB | 本地使用 |
| macOS 安装包 | `target/release/bundle/macos/DBX.dmg` | ~150MB | 分发用户 |
| Java Agent | `agents/drivers/calcite/build/libs/*.jar` | ~80MB | 联邦查询后端 |

---

## 🔧 故障排除

### 问题：找不到 cargo

```bash
# 解决方案
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

### 问题：Java 未找到

```bash
# macOS 安装 Java
brew install openjdk@21
export JAVA_HOME=$(/usr/libexec/java_home -v 21)
```

### 问题：构建超时

```bash
# 增加超时时间（编辑 src-tauri/tauri.conf.json）
{
  "tauri": {
    "bundle": {
      "timeout": 1800
    }
  }
}
```

---

## 📋 完整检查清单

在打包前确认以下依赖：

```bash
# 检查所有必需工具
which node && node --version
which pnpm && pnpm --version
which cargo && cargo --version  # 可选，仅桌面构建需要
which java && java -version     # 可选，仅 Java Agent 需要
```

---

## 🎯 联邦查询功能验证

构建完成后，验证联邦查询功能：

```bash
# 1. 启动应用
open target/release/bundle/macos/DBX.app

# 2. 创建测试连接
# - 连接一个 PostgreSQL 数据库
# - 启用 "Federation Enabled" 选项

# 3. 测试联邦查询
SELECT * FROM my_conn.public.users;

# 4. 测试跨连接 JOIN（需要启动 Calcite Agent）
SELECT * FROM conn1.public.users u 
JOIN conn2.shop.orders o ON u.id = o.user_id;
```

---

## 📚 相关文档

- [完整构建指南](./DESKTOP_BUILD_GUIDE.md)
- [联邦查询实现](./FEDERATED_QUERY_IMPLEMENTATION.md)
- [Makefile 命令参考](./Makefile)

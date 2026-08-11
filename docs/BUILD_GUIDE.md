# DBX 桌面应用打包指南

## 构建要求

### 必需工具链

#### 1. Rust 工具链
```bash
# macOS - 使用 Homebrew
brew install rustup
rustup-init

# 或手动安装
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env
```

验证安装：
```bash
rustc --version  # 需要 >= 1.80.0
cargo --version  # 需要 >= 1.80.0
```

#### 2. Node.js (已安装)
- 版本: v24.x (hermit 环境已配置)
- pnpm: `npm install -g pnpm`

#### 3. Java (用于 Calcite Agent)
```bash
# macOS
brew install openjdk@21
export JAVA_HOME=$(/usr/libexec/java_home -v 21)
```

#### 4. Tauri 依赖 (macOS)
```bash
# 安装系统依赖
brew install webkitgtk
brew install librsvg
```

### 可选工具

- **Docker**: 用于运行数据库测试
- **Gradle**: 用于编译 Java Calcite Agent

---

## 打包步骤

### 方式一：使用 Makefile（推荐）

```bash
cd /Users/cooper/GitHub/dbx

# 1. 安装依赖
make install

# 2. 类型检查和构建前端
make build

# 3. 打包桌面应用
make package
```

### 方式二：使用 pnpm 命令

```bash
cd /Users/cooper/GitHub/dbx

# 安装依赖
pnpm install --frozen-lockfile

# 完整构建（类型检查 + Vite 构建）
pnpm build:checked

# Tauri 打包
pnpm tauri build
```

### 方式三：仅打包前端静态文件

```bash
# 构建 Web 版本
pnpm build

# 输出目录: dist/
ls dist/
```

---

## 输出位置

打包完成后，应用位于：

```
target/release/bundle/
├── macos/
│   ├── DBX.app              # macOS 应用包
│   └── DBX.dmg              # macOS 安装包
├── deb/
│   └── dbx_*.amd64.deb      # Linux Debian 包
└── rpm/
    └── dbx-*.x86_64.rpm     # Linux RPM 包
```

macOS 上也可查看：
```bash
open target/release/bundle/macos/DBX.app
```

---

## 针对联邦查询功能的特殊说明

### 编译联邦查询功能

默认构建已包含联邦查询支持。如需启用额外特性：

```bash
# 启用 DuckDB Sidecar 支持
pnpm tauri build --features duckdb-sidecar

# 启用消息队列管理
pnpm tauri build --features mq-admin
```

### Java Calcite Agent 打包

在 `agents` 目录下构建：

```bash
cd /Users/cooper/GitHub/dbx/agents

# 使用 Gradle 构建 Shadow JAR
./gradlew :drivers:calcite:shadowJar

# 输出位置: agents/drivers/calcite/build/libs/dbx-agent-calcite.jar
```

生成的 JAR 可独立运行或嵌入桌面应用。

---

## 故障排除

### 问题：找不到 cargo/rustc

**解决方案**：
```bash
# 检查安装路径
which cargo || which rustc

# 添加环境变量
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### 问题：Tauri 编译失败 - "rustc not found"

**解决方案**：
```bash
# 重新安装 Rust
rustup update stable
cargo --version
```

### 问题：Java 运行时报错 "Unable to locate a Java Runtime"

**解决方案**：
```bash
# 设置 JAVA_HOME
export JAVA_HOME=$(/usr/libexec/java_home)
java -version

# 或在 .zshrc 中添加
echo 'export JAVA_HOME=$(/usr/libexec/java_home)' >> ~/.zshrc
```

### 问题：pnpm 未找到

**解决方案**：
```bash
npm install -g pnpm
# 或使用 corepack
corepack enable
corepack prepare pnpm@latest --activate
```

---

## CI/CD 配置示例

### GitHub Actions

```yaml
name: Build DBX

on:
  push:
    tags: ['v*']

jobs:
  build:
    runs-on: macos-latest
    
    steps:
    - uses: actions/checkout@v4
    
    - name: Install Rust
      uses: dtolnay/rust-toolchain@stable
      
    - name: Install Node.js
      uses: actions/setup-node@v4
      with:
        node-version: '20'
        cache: 'pnpm'
    
    - name: Install dependencies
      run: pnpm install --frozen-lockfile
      
    - name: Build and package
      run: make package
      
    - name: Upload artifacts
      uses: actions/upload-artifact@v4
      with:
        name: dbx-release
        path: target/release/bundle/macos/
```

---

## 验证安装

构建完成后验证应用：

```bash
# 启动应用
open target/release/bundle/macos/DBX.app

# 或使用命令行
./target/release/bundle/macos/macos_universal/DBX
```

检查联邦查询功能：
1. 打开 DBX
2. 连接一个支持联邦的数据库
3. 启用 `Federation Enabled` 选项
4. 尝试跨连接查询：`SELECT * FROM conn1.schema1.table1 JOIN conn2.schema2.table2 ON ...`

---

## 文档链接

- [Tauri 官方文档](https://tauri.app/)
- [Rust 安装指南](https://www.rust-lang.org/tools/install)
- [联邦查询实现文档](./FEDERATED_QUERY_IMPLEMENTATION.md)

---

*最后更新: 2026-08-03*

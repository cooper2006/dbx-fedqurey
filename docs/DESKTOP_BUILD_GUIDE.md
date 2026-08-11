# DBX 桌面应用打包指南

## 📋 环境检查清单

在开始打包之前，请确保已安装以下依赖：

### 必需依赖
- [x] **Node.js** v20+ ✅ (已安装)
- [x] **pnpm** ✅ (已安装)
- [ ] **Rust/Cargo** - 用于 Tauri 桌面构建 ⚠️ 未找到
- [ ] **Java 21** - 用于 Calcite Agent ⚠️ 未找到

### 可选依赖
- Docker - 用于数据库测试
- Gradle - 用于 Java 项目构建

---

## 🚀 快速打包命令

```bash
cd /Users/cooper/GitHub/dbx

# 方式一：使用 Makefile（推荐）
make build        # 仅构建前端
make package      # 构建桌面应用（需要 Rust）
make install      # 安装依赖

# 方式二：使用 pnpm 脚本
pnpm build        # 构建前端
pnpm tauri build  # 构建桌面应用

# 方式三：使用便捷脚本
./scripts/quick-build.sh web    # 仅前端
./scripts/quick-build.sh desktop  # 桌面应用
./scripts/quick-build.sh agent  # Java Agent
./scripts/quick-build.sh all    # 完整构建
```

---

## 📦 打包步骤详解

### 第一步：安装构建工具

#### 安装 Rust（必须）
```bash
# macOS - 推荐方式
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 按提示安装后，执行：
source $HOME/.cargo/env

# 验证安装
rustc --version
cargo --version
```

**或手动下载：**
1. 访问 https://www.rust-lang.org/tools/install
2. 下载并运行 rustup-init
3. 重启终端

#### 安装 Java（用于 Calcite Agent）
```bash
# macOS - 使用 Homebrew
brew install openjdk@21
export JAVA_HOME=$(/usr/libexec/java_home -v 21)

# 验证安装
java -version
```

#### 安装 Tauri 系统依赖（macOS）
```bash
brew install webkitgtk librsvg
```

### 第二步：安装项目依赖
```bash
cd /Users/cooper/GitHub/dbx
pnpm install --frozen-lockfile
```

### 第三步：选择构建目标

#### 选项 A：仅 Web 前端（最快，无需额外依赖）
```bash
pnpm build
# 输出: dist/
```

#### 选项 B：桌面应用（需要 Rust）
```bash
pnpm tauri build
# 输出: target/release/bundle/macos/DBX.app
```

#### 选项 C：Java Calcite Agent（需要 Java + Gradle）
```bash
cd agents
./gradlew :drivers:calcite:shadowJar
# 输出: agents/drivers/calcite/build/libs/dbx-agent-calcite.jar
```

#### 选项 D：完整构建
```bash
./scripts/quick-build.sh all
```

---

## 🎯 桌面应用打包流程

### 配置检查

查看 `src-tauri/tauri.conf.json` 确认：
- productName: "DBX"
- version: "0.5.71"
- 输出格式：macOS DMG/App Bundle

### 构建命令

```bash
# 开发模式（快速迭代）
pnpm tauri dev

# 生产构建
pnpm tauri build

# 带特性标志构建
pnpm tauri build --features duckdb-sidecar
pnpm tauri build --features mq-admin
```

### 构建产物位置

```
target/release/bundle/
├── macos/
│   ├── DBX.app              # macOS 应用包
│   └── DBX.dmg              # macOS 安装包
├── deb/
│   └── dbx_0.5.71_amd64.deb # Linux Debian 包
└── rpm/
    └── dbx-0.5.71.x86_64.rpm # Linux RPM 包
```

### 安装和验证

```bash
# 打开应用
open target/release/bundle/macos/DBX.app

# 或使用命令行
./target/release/bundle/macos/macos_universal/DBX

# 创建快捷方式（可选）
ln -s ~/GitHub/dbx/target/release/bundle/macos/DBX.app \
     ~/Desktop/DBX.app
```

---

## 🔧 故障排除

### 问题 1：找不到 cargo/rustc

**症状：**
```
error: could not find `cargo`
```

**解决方案：**
```bash
# 检查是否已安装
which cargo || which rustc

# 重新安装
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source $HOME/.cargo/env

# 添加到 PATH（永久）
echo 'export PATH="$HOME/.cargo/bin:$PATH"' >> ~/.zshrc
source ~/.zshrc
```

### 问题 2：Tauri 编译失败 - "webkit2gtk not found"

**症状：**
```
could not find WebKit2GTK with pkg-config
```

**解决方案：**
```bash
# macOS
brew install webkitgtk
brew install librsvg

# 如果仍然失败，设置环境变量
export WEBKITGTK_CFLAGS="-I$(brew --prefix)/include/webkitgtk-4.1"
export WEBKITGTK_LIBS="-L$(brew --prefix)/lib"
```

### 问题 3：Java 版本不兼容

**症状：**
```
Unsupported class file major version 65
```

**解决方案：**
```bash
# 安装 Java 21
brew install openjdk@21
export JAVA_HOME=$(/usr/libexec/java_home -v 21)

# 验证版本
java -version
# 应显示: openjdk version "21.x.x"
```

### 问题 4：pnpm 安装失败

**症状：**
```
ERR_PNPM_UNSUPPORTED_ENGINE
```

**解决方案：**
```bash
# 升级 pnpm
npm install -g pnpm@latest

# 或使用 corepack
corepack enable
corepack prepare pnpm@latest --activate
```

### 问题 5：构建超时

**症状：**
```
command timed out after 1200 seconds
```

**解决方案：**
```bash
# 增加超时时间
pnpm tauri build -- --config '{"tauri": {"bundle": {"timeout": 1800}}}'

# 或分步构建
pnpm build checked && pnpm tauri build
```

---

## 📝 自定义构建配置

### 修改应用信息

编辑 `src-tauri/tauri.conf.json`：

```json
{
  "productName": "DBX",
  "version": "0.5.71",
  "identifier": "com.dbx.app",
  "build": {
    "frontendDist": "../dist",
    "devUrl": "http://localhost:1420"
  }
}
```

### 启用特定功能

编辑 `Cargo.toml`：
```toml
[features]
default = ["duckdb"]
duckdb-sidecar = []
mq-admin = []
```

构建时指定：
```bash
pnpm tauri build --features duckdb-sidecar,mq-admin
```

### 自定义图标

```bash
# 替换 macOS 图标
cp your-icon.png src-tauri/icons/*.png

# 或批量更新
for size in 16 32 64 128 256 512; do
    sips -Z ${size} your-icon.png --out src-tauri/icons/icon_${size}.png
done
```

---

## 🔄 CI/CD 自动化构建

### GitHub Actions 示例

创建 `.github/workflows/build.yml`：

```yaml
name: Build DBX

on:
  push:
    tags: ['v*']
  workflow_dispatch:

jobs:
  build:
    runs-on: macos-latest
    
    steps:
    - uses: actions/checkout@v4
    
    - name: Setup Node.js
      uses: actions/setup-node@v4
      with:
        node-version: '20'
        cache: 'pnpm'
    
    - name: Install Rust
      uses: dtolnay/rust-toolchain@stable
    
    - name: Install dependencies
      run: pnpm install --frozen-lockfile
    
    - name: Build web frontend
      run: pnpm build
    
    - name: Build desktop app
      run: pnpm tauri build
      
    - name: Upload artifacts
      uses: actions/upload-artifact@v4
      with:
        name: dbx-release
        path: target/release/bundle/macos/
```

---

## 📊 构建产物说明

### 文件结构

```
target/release/bundle/
├── macos/
│   ├── DBX.app                 # 应用包（可直接运行）
│   ├── DBX.dmg                 # 安装包（推荐分发）
│   └── macos_universal/
│       └── DBX                  # 可执行文件
├── deb/
│   └── dbx_0.5.71_amd64.deb   # Linux 包
└── rpm/
    └── dbx-0.5.71.x86_64.rpm  # Linux 包
```

### 各格式特点

| 格式 | 用途 | 大小 | 签名 |
|------|------|------|------|
| `.app` | 本地运行、开发测试 | ~150MB | 否 |
| `.dmg` | 分发安装 | ~150MB | 是 |
| `.deb` | Ubuntu/Debian 系 | ~150MB | 否 |
| `.rpm` | Fedora/RHEL 系 | ~150MB | 否 |

---

## 🎯 联邦查询功能打包

确保在打包时包含联邦查询相关代码：

### Rust 侧
```bash
# 验证模块已编译
cargo check -p dbx-core --lib

# 运行单元测试
cargo test -p dbx-core --lib federated
```

### Java Agent 侧
```bash
# 编译 Calcite Agent
cd agents
./gradlew :drivers:calcite:shadowJar

# 验证 JAR
jar tf drivers/calcite/build/libs/dbx-agent-calcite-*.jar | grep -E "\.class$" | head -10
```

### 前端侧
```bash
# 验证 TypeScript 类型检查
pnpm typecheck
```

---

## 📚 参考资源

- [Tauri 官方文档](https://tauri.app/)
- [Rust 安装指南](https://www.rust-lang.org/tools/install)
- [联邦查询实现文档](./FEDERATED_QUERY_IMPLEMENTATION.md)
- [构建指南](./BUILD_GUIDE.md)

---

## ✅ 检查清单

打包前确认：
- [ ] Node.js v20+ 已安装
- [ ] pnpm 已安装
- [ ] Rust/Cargo 已安装
- [ ] Java 21 已安装（如需 Java Agent）
- [ ] 所有依赖已安装：`pnpm install`
- [ ] TypeScript 检查通过：`pnpm typecheck`
- [ ] 单元测试通过：`cargo test -p dbx-core`

打包后验证：
- [ ] 应用能正常启动
- [ ] 连接功能正常
- [ ] 联邦查询功能可用
- [ ] 前端交互流畅

---

*最后更新: 2026-08-03*  
*版本: v1.0*

# Rust 编译加速配置指南

## 背景

项目原有 `release` profile 同时开启全量 LTO + 单 codegen-unit + 体积优化，且无增量编译，导致每次编译耗时 15-17 分钟。本方案通过分层优化将日常开发编译时间降至 1-3 分钟。

## 优化方案概览

| 优化项 | 之前 | 之后 | 效果 |
|--------|------|------|------|
| Profile | 仅 `release`（全量 LTO） | 新增 `fast-release`（增量编译） | 日常编译 15min → 1-3min |
| LTO | `true`（全量） | `fast-release` 中 `false` | 跳过跨 crate 全局优化 |
| codegen-units | `1`（串行） | `fast-release` 中 `16`（并行） | 利用多核并行生成代码 |
| opt-level | `"s"`（体积优化） | `fast-release` 中 `1` | 减少优化遍数 |
| incremental | 默认关闭 | `fast-release` 中 `true` | 仅重编变更的 crate |
| sccache | 未安装 | 已安装并启用 | 缓存编译产物，二次编译跳过未变更依赖 |
| lld 链接器 | 未安装 | 已安装并启用 | 链接阶段提速 2-3x |

## 配置详情

### 1. Cargo.toml — Profile 配置

```toml
# 最终打包发布 —— 全量 LTO，单 codegen-unit，体积优化
[profile.release]
panic = "abort"
strip = true
lto = true
codegen-units = 1
opt-level = "s"

# 日常开发 / 快速验证 —— 增量编译，无 LTO，并行 codegen
[profile.fast-release]
inherits = "release"
lto = false
codegen-units = 16
opt-level = 1
incremental = true
strip = false
```

### 2. .cargo/config.toml — 编译工具链配置

```toml
[build]
# sccache 缓存编译产物
rustc-wrapper = "sccache"

# macOS 使用 lld 链接器加速链接
[target.aarch64-apple-darwin]
rustflags = ["-C", "link-arg=-fuse-ld=lld"]
```

### 3. 已安装工具

| 工具 | 安装方式 | 作用 |
|------|----------|------|
| sccache | `cargo install sccache` | 编译缓存，跳过未变更依赖的重编译 |
| lld | `brew install lld` | LLVM 高速链接器，链接阶段提速 2-3x |

## 使用方式

### 日常开发（默认使用）

```bash
cargo build --profile fast-release
```

- 增量编译，仅重编变更的 crate
- 无 LTO，16 并行 codegen-unit
- 首次编译约 3-5 分钟，增量编译约 1-3 分钟
- 产物路径：`target/fast-release/dbx`

### 最终打包发布

```bash
cargo build --release
```

- 全量 LTO，单 codegen-unit，体积优化
- 编译时间约 15-17 分钟
- 产物路径：`target/release/dbx`
- 体积比 fast-release 小约 30-40%

### 查看缓存统计

```bash
sccache --show-stats
```

## 两个 Profile 对比

| 维度 | `fast-release` | `release` |
|------|----------------|-----------|
| 用途 | 日常开发、快速验证 | 最终打包发布 |
| LTO | 关闭 | 开启（全量） |
| codegen-units | 16（并行） | 1（串行） |
| opt-level | 1（基础优化） | "s"（体积优化） |
| incremental | 开启 | 关闭 |
| strip | 关闭 | 开启 |
| 编译时间 | 1-3 分钟（增量） | 15-17 分钟 |
| 产物体积 | 较大 | 较小（约 -30%） |

## 注意事项

1. **fast-release 产物体积较大**：因为关闭了 LTO 和 strip，产物体积会比 release 大约 30-40%，不影响功能使用
2. **sccache 缓存位置**：默认在 `~/.cache/sccache`，可通过 `SCCACHE_DIR` 环境变量修改
3. **lld 兼容性**：macOS 上 lld 通过 `brew install lld` 安装，已自动配置到 `.cargo/config.toml`
4. **打包时切换**：最终打包时务必使用 `cargo build --release` 以获得最小体积和最优性能
5. **Tauri 打包**：Tauri 默认使用 `release` profile，打包命令 `npx tauri build` 不受影响

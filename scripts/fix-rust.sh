#!/bin/bash
# Fix Rust toolchain issue

export PATH="$HOME/.cargo/bin:$PATH"

echo "检查 Rust 安装状态..."
rustup --version 2>/dev/null || {
    echo "Rustup 未找到，正在重新安装..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
}

# 修复损坏的 toolchain manifest
echo "尝试修复 toolchain..."
rustup component remove rustc 2>/dev/null || true
rustup component add rustc 2>/dev/null || true
rustup toolchain install stable 2>/dev/null || true

# 验证
cargo --version
rustc --version

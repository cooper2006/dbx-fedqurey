#!/bin/bash
# DBX 桌面应用打包脚本
# 自动化安装依赖并构建应用

set -e

echo "=========================================="
echo "DBX 桌面应用打包脚本"
echo "=========================================="
echo ""

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

REPO_ROOT="/Users/cooper/GitHub/dbx"
BUILD_DIR="$REPO_ROOT/target/release/bundle/macos"

cd "$REPO_ROOT"

# 检查命令是否存在
check_command() {
    if command -v "$1" &> /dev/null; then
        return 0
    else
        return 1
    fi
}

# 打印带颜色的消息
print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

# 1. 检查 Node.js 和 pnpm
echo "[步骤 1/5] 检查基础工具..."
print_info "Node.js 版本: $(node --version)"
print_info "pnpm 版本: $(pnpm --version)"

if ! check_command pnpm; then
    print_warning "pnpm 未安装，正在安装..."
    npm install -g pnpm
    print_success "pnpm 安装完成"
fi

# 2. 检查 Rust（用于桌面构建）
RUST_AVAILABLE=false
if check_command cargo; then
    RUST_VERSION=$(cargo --version)
    print_success "Rust/Cargo 已安装: $RUST_VERSION"
    RUST_AVAILABLE=true
else
    print_warning "Rust/Cargo 未安装"
    print_info "如需构建桌面应用，请访问: https://rustup.rs/"
fi

# 3. 检查 Java（用于 Calcite Agent）
JAVA_AVAILABLE=false
if java -version &> /dev/null 2>&1; then
    JAVA_VERSION=$(java -version 2>&1 | head -1)
    print_success "Java 已安装: $JAVA_VERSION"
    JAVA_AVAILABLE=true
else
    print_warning "Java 未安装"
    print_info "如需构建 Java Calcite Agent，请运行: brew install openjdk@21"
fi

echo ""

# 4. 询问构建目标
echo "=========================================="
echo "选择构建目标："
echo "=========================================="
echo ""
echo "  1. Web 前端 ($YELLOW)无需 Rust${NC})"
echo "  2. 桌面应用 (需要 Rust)"
echo "  3. Java Calcite Agent (需要 Java)"
echo "  4. 全部构建"
echo "  5. 仅安装依赖"
echo "  6. 退出"
echo ""

read -p "请输入选项 [1-6]: " BUILD_CHOICE

case $BUILD_CHOICE in
    1)
        echo ""
        print_info "开始构建 Web 前端..."
        cd "$REPO_ROOT"
        pnpm install --frozen-lockfile
        pnpm build
        print_success "Web 前端构建完成！"
        echo "  输出目录: dist/"
        ;;
    
    2)
        if [ "$RUST_AVAILABLE" = false ]; then
            print_error "Rust 未安装，无法构建桌面应用"
            echo ""
            echo "安装方法:"
            echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
            exit 1
        fi
        
        print_info "开始构建桌面应用..."
        cd "$REPO_ROOT"
        pnpm install --frozen-lockfile
        pnpm tauri build
        print_success "桌面应用构建完成！"
        echo "  输出目录: $BUILD_DIR/"
        if [ -d "$BUILD_DIR" ]; then
            ls -lh "$BUILD_DIR"/*.app 2>/dev/null | head -1
        fi
        ;;
    
    3)
        if [ "$JAVA_AVAILABLE" = false ]; then
            print_error "Java 未安装，无法构建 Calcite Agent"
            echo ""
            echo "安装方法:"
            echo "  brew install openjdk@21"
            exit 1
        fi
        
        print_info "开始构建 Java Calcite Agent..."
        cd "$REPO_ROOT/agents"
        chmod +x ./gradlew 2>/dev/null || true
        ./gradlew :drivers:calcite:shadowJar
        print_success "Java Calcite Agent 构建完成！"
        
        # 查找生成的 JAR 文件
        JAR_FILE=$(find drivers/calcite/build/libs -name "*.jar" -type f 2>/dev/null | head -1)
        if [ -n "$JAR_FILE" ]; then
            echo "  JAR 位置: $JAR_FILE"
            echo "  大小: $(ls -lh "$JAR_FILE" | awk '{print $5}')"
            
            # 复制到目标目录
            mkdir -p "$REPO_ROOT/target/agents"
            cp "$JAR_FILE" "$REPO_ROOT/target/agents/"
            print_success "已复制到 target/agents/"
        fi
        ;;
    
    4)
        print_info "开始完整构建..."
        cd "$REPO_ROOT"
        
        # 安装依赖
        print_info "安装依赖..."
        pnpm install --frozen-lockfile
        
        # 构建 Web 前端
        print_info "[1/3] 构建 Web 前端..."
        pnpm build
        print_success "Web 前端构建完成"
        
        # 构建桌面应用
        if [ "$RUST_AVAILABLE" = true ]; then
            print_info "[2/3] 构建桌面应用..."
            pnpm tauri build
            print_success "桌面应用构建完成"
        else
            print_warning "跳过桌面应用构建 (Rust 未安装)"
        fi
        
        # 构建 Java Agent
        if [ "$JAVA_AVAILABLE" = true ]; then
            print_info "[3/3] 构建 Java Calcite Agent..."
            cd agents
            ./gradlew :drivers:calcite:shadowJar >/dev/null 2>&1
            
            JAR_FILE=$(find drivers/calcite/build/libs -name "*.jar" -type f 2>/dev/null | head -1)
            if [ -n "$JAR_FILE" ]; then
                mkdir -p "$REPO_ROOT/target/agents"
                cp "$JAR_FILE" "$REPO_ROOT/target/agents/"
                print_success "Java Calcite Agent 构建完成"
            fi
            cd "$REPO_ROOT"
        else
            print_warning "跳过 Java Agent 构建 (Java 未安装)"
        fi
        
        echo ""
        echo "=========================================="
        echo "完整构建完成！"
        echo "=========================================="
        echo ""
        echo "构建产物："
        echo "  📦 Web 前端:   dist/"
        [ "$RUST_AVAILABLE" = true ] && echo "  🖥️  桌面应用:   $BUILD_DIR/"
        [ "$JAVA_AVAILABLE" = true ] && echo "  ☕ Java Agent:  target/agents/"
        ;;
    
    5)
        print_info "安装项目依赖..."
        cd "$REPO_ROOT"
        pnpm install --frozen-lockfile
        print_success "依赖安装完成"
        ;;
    
    6)
        print_info "退出构建"
        exit 0
        ;;
    
    *)
        print_error "无效选项"
        exit 1
        ;;
esac

echo ""
echo "=========================================="
echo "构建流程已完成"
echo "=========================================="
echo ""
echo "如需查看详细文档，请参考："
echo "  - DESKTOP_BUILD_GUIDE.md (完整构建指南)"
echo "  - PACKING_QUICKSTART.md (快速开始)"
echo ""

#!/bin/bash
set -e

# DBX Desktop App Auto-Build Script
# This script checks for required dependencies and builds the application

echo "=========================================="
echo "DBX Desktop App Build Script"
echo "=========================================="
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if running on macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    echo -e "${YELLOW}Warning: This build script is optimized for macOS.${NC}"
    echo "Linux users may need to install additional dependencies."
fi

# Function to check command availability
check_command() {
    if command -v "$1" &> /dev/null; then
        echo -e "${GREEN}✓${NC} $1 found: $(command -v $1)"
        return 0
    else
        echo -e "${RED}✗${NC} $1 not found"
        return 1
    fi
}

echo "Checking dependencies..."
echo ""

# Check Node.js
NODE_FOUND=false
if check_command node; then
    NODE_VERSION=$(node --version)
    echo "  Node.js version: $NODE_VERSION"
    NODE_FOUND=true
fi

# Check pnpm
PNPM_FOUND=false
if check_command pnpm; then
    PNPM_VERSION=$(pnpm --version)
    echo "  pnpm version: $PNPM_VERSION"
    PNPM_FOUND=true
else
    echo -e "${YELLOW}Tip: Install pnpm with 'npm install -g pnpm'${NC}"
fi

echo ""

# Check Rust (optional for desktop build, required for features)
RUST_FOUND=false
if check_command cargo; then
    RUST_VERSION=$(cargo --version)
    echo "  Rust/Cargo: $RUST_VERSION"
    RUST_FOUND=true
else
    echo -e "${YELLOW}Rust not found. Desktop build may work without Rust features.${NC}"
    echo "  To install Rust: https://rustup.rs/"
fi

echo ""

# Check Java (required for Calcite Agent)
JAVA_FOUND=false
if java -version &> /dev/null; then
    JAVA_VERSION=$(java -version 2>&1 | head -1)
    echo "  Java: $JAVA_VERSION"
    JAVA_FOUND=true
else
    echo -e "${YELLOW}Java not found. Required for Java Calcite Agent.${NC}"
    echo "  Install with: brew install openjdk@21"
fi

echo ""
echo "=========================================="
echo "Build Status Summary"
echo "=========================================="
echo ""

# Determine what we can build
CAN_BUILD_WEB=false
CAN_BUILD_DESKTOP=false

if [ "$NODE_FOUND" = true ] && [ "$PNPM_FOUND" = true ]; then
    CAN_BUILD_WEB=true
    echo -e "${GREEN}Web/Frontend build:${NC} Available"
else
    echo -e "${RED}Web/Frontend build:${NC} Requires Node.js and pnpm"
fi

if [ "$NODE_FOUND" = true ] && [ "$RUST_FOUND" = true ]; then
    CAN_BUILD_DESKTOP=true
    echo -e "${GREEN}Desktop/Tauri build:${NC} Available"
elif [ "$NODE_FOUND" = true ] && [ "$RUST_FOUND" = false ]; then
    echo -e "${YELLOW}Desktop/Tauri build:${NC} Requires Rust/Cargo"
else
    echo -e "${RED}Desktop/Tauri build:${NC} Cannot build without Node.js"
fi

if [ "$JAVA_FOUND" = true ]; then
    echo -e "${GREEN}Java Calcite Agent:${NC} Available"
else
    echo -e "${YELLOW}Java Calcite Agent:${NC} Requires Java"
fi

echo ""

# Ask user what to build
echo "=========================================="
echo "Select build target:"
echo "=========================================="
echo ""
echo "  1. Web frontend only (fastest)"
echo "  2. Desktop app with Tauri (requires Rust)"
echo "  3. Java Calcite Agent only (requires Java)"
echo "  4. Full build (all components)"
echo "  5. Exit"
echo ""

read -p "Enter your choice (1-5): " BUILD_CHOICE

case $BUILD_CHOICE in
    1)
        echo ""
        echo "Building web frontend..."
        cd "$(dirname "$0")"
        if [ "$PNPM_FOUND" = true ]; then
            pnpm build
            echo ""
            echo "✓ Web build complete!"
            echo "  Output: dist/"
        else
            echo -e "${RED}Error: pnpm is required.${NC}"
            exit 1
        fi
        ;;
    2)
        echo ""
        echo "Building desktop app with Tauri..."
        if [ "$CAN_BUILD_DESKTOP" = true ]; then
            cd "$(dirname "$0")"
            pnpm tauri build
            echo ""
            echo "✓ Desktop build complete!"
            echo "  Output: target/release/bundle/"
        else
            echo -e "${RED}Error: Rust/Cargo is required for desktop build.${NC}"
            exit 1
        fi
        ;;
    3)
        echo ""
        echo "Building Java Calcite Agent..."
        if [ "$JAVA_FOUND" = true ]; then
            AGENTS_DIR="$(dirname "$0")/agents"
            if [ -f "$AGENTS_DIR/gradlew" ]; then
                cd "$AGENTS_DIR"
                ./gradlew :drivers:calcite:shadowJar
                echo ""
                echo "✓ Java Calcite Agent built!"
                echo "  Output: agents/drivers/calcite/build/libs/dbx-agent-calcite.jar"
            else
                echo -e "${RED}Error: agents/gradlew not found.${NC}"
                exit 1
            fi
        else
            echo -e "${RED}Error: Java is required for Calcite Agent build.${NC}"
            exit 1
        fi
        ;;
    4)
        echo ""
        echo "Performing full build..."
        cd "$(dirname "$0")"
        
        # Step 1: Install dependencies
        echo "Step 1: Installing dependencies..."
        pnpm install --frozen-lockfile
        
        # Step 2: Build frontend
        echo "Step 2: Building frontend..."
        pnpm build:checked
        
        # Step 3: Build desktop app
        if [ "$CAN_BUILD_DESKTOP" = true ]; then
            echo "Step 3: Building desktop app..."
            pnpm tauri build
            echo -e "${GREEN}✓ Desktop app built!${NC}"
        fi
        
        # Step 4: Build Java agent
        if [ "$JAVA_FOUND" = true ]; then
            echo "Step 4: Building Java Calcite Agent..."
            cd agents
            ./gradlew :drivers:calcite:shadowJar
            echo -e "${GREEN}✓ Java agent built!${NC}"
            cd ..
        fi
        
        echo ""
        echo "=========================================="
        echo "Full build complete!"
        echo "=========================================="
        echo ""
        echo "Artifacts:"
        echo "  - Web frontend: dist/"
        if [ "$CAN_BUILD_DESKTOP" = true ]; then
            echo "  - Desktop app: target/release/bundle/"
        fi
        if [ "$JAVA_FOUND" = true ]; then
            echo "  - Java agent: agents/drivers/calcite/build/libs/dbx-agent-calcite.jar"
        fi
        ;;
    5)
        echo "Exiting..."
        exit 0
        ;;
    *)
        echo "Invalid choice. Exiting."
        exit 1
        ;;
esac

echo ""
echo "=========================================="
echo "Build completed successfully!"
echo "=========================================="

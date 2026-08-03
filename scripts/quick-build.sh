#!/bin/bash
# Quick build script for DBX Desktop App
# Usage: ./scripts/quick-build.sh [target]
# Targets: web, desktop, agent, all

set -e

BUILD_TARGET="${1:-all}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

cd "$REPO_ROOT"

echo "=========================================="
echo "DBX Quick Build Script"
echo "Target: $BUILD_TARGET"
echo "=========================================="
echo ""

# Check Node.js
if ! command -v node &> /dev/null; then
    echo "Error: Node.js not found"
    exit 1
fi
echo "✓ Node.js: $(node --version)"

# Check pnpm
if ! command -v pnpm &> /dev/null; then
    echo "Installing pnpm..."
    npm install -g pnpm
fi
echo "✓ pnpm: $(pnpm --version)"

# Install dependencies if needed
if [ ! -d "node_modules" ]; then
    echo "Installing dependencies..."
    pnpm install --frozen-lockfile
fi

case $BUILD_TARGET in
    web)
        echo "Building web frontend..."
        pnpm build
        echo ""
        echo "✓ Web build complete: dist/"
        ;;
    
    desktop)
        # Check Rust
        if ! command -v cargo &> /dev/null; then
            echo ""
            echo "Warning: Rust/Cargo not found."
            echo "Desktop app requires Rust."
            echo "Install from: https://rustup.rs/"
            echo ""
            read -p "Continue with web-only build? (y/N): " -n 1 -r
            echo
            if [[ ! $REPLY =~ ^[Yy]$ ]]; then
                exit 1
            fi
            pnpm build
            exit 0
        fi
        echo "✓ Rust: $(cargo --version)"
        
        echo "Building desktop app..."
        pnpm tauri build
        echo ""
        echo "✓ Desktop build complete: target/release/bundle/"
        
        # Open the app bundle location
        open target/release/bundle/macos/ 2>/dev/null || true
        ;;
    
    agent)
        # Check Java
        if ! java -version &> /dev/null 2>&1; then
            echo ""
            echo "Warning: Java not found."
            echo "Java Calcite Agent requires Java."
            echo "Install from: https://adoptium.net/"
            echo ""
            exit 1
        fi
        echo "✓ Java: $(java -version 2>&1 | head -1)"
        
        echo "Building Java Calcite Agent..."
        cd agents
        if [ -f "./gradlew" ]; then
            ./gradlew :drivers:calcite:shadowJar
            echo ""
            echo "✓ Java agent built: agents/drivers/calcite/build/libs/dbx-agent-calcite.jar"
            
            # Copy to target directory for easy access
            JAR_FILE=$(ls -t agents/drivers/calcite/build/libs/dbx-agent-calcite-*.jar 2>/dev/null | head -1)
            if [ -n "$JAR_FILE" ]; then
                mkdir -p "$REPO_ROOT/target/agents"
                cp "$JAR_FILE" "$REPO_ROOT/target/agents/"
                echo "✓ Copied to: target/agents/"
            fi
        else
            echo "Warning: Gradle wrapper not found in agents/"
        fi
        cd "$REPO_ROOT"
        ;;
    
    all)
        echo "Starting full build..."
        echo ""
        
        # Step 1: Web Frontend
        echo "[1/3] Building web frontend..."
        pnpm build
        echo "      ✓ Web build complete"
        echo ""
        
        # Step 2: Desktop App
        echo "[2/3] Building desktop app..."
        if command -v cargo &> /dev/null; then
            pnpm tauri build
            echo "      ✓ Desktop build complete"
        else
            echo "      ⚠ Skipping desktop build (Rust not found)"
        fi
        echo ""
        
        # Step 3: Java Agent
        echo "[3/3] Building Java Calcite Agent..."
        if java -version &> /dev/null 2>&1; then
            cd agents
            if [ -f "./gradlew" ]; then
                ./gradlew :drivers:calcite:shadowJar >/dev/null 2>&1
                JAR_FILE=$(ls -t drivers/calcite/build/libs/dbx-agent-calcite-*.jar 2>/dev/null | head -1)
                if [ -n "$JAR_FILE" ]; then
                    mkdir -p "$REPO_ROOT/target/agents"
                    cp "$JAR_FILE" "$REPO_ROOT/target/agents/"
                    echo "      ✓ Java agent built"
                fi
            fi
            cd "$REPO_ROOT"
        else
            echo "      ⚠ Skipping Java agent (Java not found)"
        fi
        echo ""
        
        echo "=========================================="
        echo "Full build complete!"
        echo "=========================================="
        echo ""
        echo "Artifacts:"
        echo "  📦 Web frontend:   dist/"
        if [ -d "target/release/bundle/macos" ]; then
            echo "  🖥️  Desktop app:   target/release/bundle/macos/"
        fi
        if [ -d "target/agents" ]; then
            echo "  ☕ Java agent:     target/agents/"
        fi
        echo ""
        ;;
    
    *)
        echo "Unknown target: $BUILD_TARGET"
        echo ""
        echo "Available targets:"
        echo "  web     - Build web frontend only"
        echo "  desktop - Build Tauri desktop app"
        echo "  agent   - Build Java Calcite Agent"
        echo "  all     - Build everything (default)"
        echo ""
        exit 1
        ;;
esac

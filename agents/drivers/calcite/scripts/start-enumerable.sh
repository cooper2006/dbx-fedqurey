#!/bin/bash
# ============================================================
# Calcite Agent 启动脚本 — Enumerable 引擎（默认，轻量级）
# 适用场景：中小数据量（< 100 万行），快速启动，低内存开销
#
# 技术栈：Calcite 1.39.0 + Janino 编译器 + Java 21
# 构建方式：
#   gradle :calcite:shadowJar
# ============================================================

JAR_PATH="$(dirname "$0")/../build/libs/dbx-agent-calcite.jar"

# JVM 参数
# -Xmx1g: 1GB 堆内存（Enumerable 引擎建议值）
# -Xms256m: 初始堆内存
# -XX:+UseG1GC: G1 垃圾回收器，适合大内存分配
JAVA_OPTS="-Xms256m -Xmx1g -XX:+UseG1GC -XX:MaxGCPauseMillis=100"

# Calcite 系统属性
JAVA_OPTS="$JAVA_OPTS -Dcalcite.bindableCacheMaxSize=1000"

# 执行引擎：enumerable（默认）
export CALCITE_ENGINE="enumerable"

echo "Starting Calcite Agent with Enumerable engine (Janino compiler)..."
echo "  JAR: $JAR_PATH"
echo "  JVM opts: $JAVA_OPTS"
echo "  Engine: $CALCITE_ENGINE"
echo ""

exec java $JAVA_OPTS -jar "$JAR_PATH"

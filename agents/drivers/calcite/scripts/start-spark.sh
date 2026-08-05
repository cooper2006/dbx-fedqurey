#!/bin/bash
# ============================================================
# Calcite Agent 启动脚本 — Spark 引擎（Java 21 兼容）
#
# 技术栈：Calcite 1.39.0 + Spark 4.0.x + Scala 2.13 + Java 21
# 适用场景：百万级以上数据量，需要 spill-to-disk 能力
#
# 构建方式：
#   gradle :calcite:shadowJar -Pengine=spark
# ============================================================

JAR_PATH="$(dirname "$0")/../build/libs/dbx-agent-calcite.jar"

# JVM 参数（Spark 4.0 引擎需要更多内存）
# -Xmx4g: 4GB 堆内存（Spark 建议值）
# -Xms1g: 初始堆内存
# -XX:+UseG1GC: G1 垃圾回收器
# -XX:MaxGCPauseMillis=200: GC 停顿目标
# --add-opens: Java 21 模块系统需要，Spark 反射访问内部 API
JAVA_OPTS="-Xms1g -Xmx4g -XX:+UseG1GC -XX:MaxGCPauseMillis=200"

# Java 21 模块开放（Spark 4.0 需要反射访问 JDK 内部模块）
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.lang=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.lang.invoke=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.lang.reflect=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.io=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.net=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.nio=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.util=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.util.concurrent=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/java.util.concurrent.atomic=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/sun.nio.ch=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/sun.nio.cs=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/sun.security.action=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.base/sun.util.calendar=ALL-UNNAMED"
JAVA_OPTS="$JAVA_OPTS --add-opens=java.security.jgss/sun.security.krb5=ALL-UNNAMED"

# Calcite 系统属性
JAVA_OPTS="$JAVA_OPTS -Dcalcite.bindableCacheMaxSize=1000"

# Spark 4.0 系统属性
JAVA_OPTS="$JAVA_OPTS -Dspark.ui.enabled=false"
JAVA_OPTS="$JAVA_OPTS -Dspark.driver.bindAddress=localhost"
JAVA_OPTS="$JAVA_OPTS -Dspark.driver.host=localhost"
# Spark 4.0 使用 log4j2，设置日志级别
JAVA_OPTS="$JAVA_OPTS -Dorg.slf4j.simpleLogger.defaultLogLevel=warn"
JAVA_OPTS="$JAVA_OPTS -Dlog4j2.level=WARN"

# 临时目录（Spark 编译类文件用）
SPARK_TMPDIR="${SPARK_TMPDIR:-/tmp/calcite-spark-$$}"
mkdir -p "$SPARK_TMPDIR/classes"
JAVA_OPTS="$JAVA_OPTS -Dspark.repl.class.dir=$SPARK_TMPDIR/classes"
JAVA_OPTS="$JAVA_OPTS -Djava.io.tmpdir=$SPARK_TMPDIR"

# 执行引擎：spark
export CALCITE_ENGINE="spark"

echo "Starting Calcite Agent with Spark engine (Spark 4.0.x + Scala 2.13, Java 21)..."
echo "  JAR:       $JAR_PATH"
echo "  Engine:    $CALCITE_ENGINE"
echo "  Temp dir:  $SPARK_TMPDIR"
echo "  JVM opts:  $JAVA_OPTS"
echo ""
echo "  确保 JAR 使用 -Pengine=spark 构建"
echo ""

exec java $JAVA_OPTS -jar "$JAR_PATH"

package com.dbx.agent.calcite;

import com.fasterxml.jackson.databind.ObjectMapper;
import com.fasterxml.jackson.databind.node.ObjectNode;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.PrintWriter;
import java.sql.*;
import java.util.*;
import java.util.concurrent.ConcurrentHashMap;

import org.apache.calcite.adapter.jdbc.JdbcSchema;
import org.apache.calcite.jdbc.CalciteConnection;
import org.apache.calcite.schema.Schema;
import org.apache.calcite.schema.SchemaPlus;
import org.apache.calcite.schema.impl.AbstractSchema;

/**
 * Apache Calcite 联邦查询 Agent
 *
 * 通过 Apache Calcite 的 JdbcSchema 实现跨数据连接的联邦查询：
 * 1. 将每个 JDBC 连接注册为 Calcite 的一个 Schema
 * 2. SQL 中的 连接名.Schema.表名 被映射到对应的 Calcite Schema
 * 3. Calcite 优化器自动处理跨连接的 JOIN/UNION 查询
 *
 * 通信协议：JSON-RPC 2.0 over stdin/stdout
 */
public class CalciteAgent {

    private static final Logger logger = LoggerFactory.getLogger(CalciteAgent.class);
    private static final ObjectMapper MAPPER = new ObjectMapper();

    // 执行引擎: "enumerable"（默认，Janino 编译器）或 "spark"（Spark RDD）
    private final String engine;

    // 已注册的数据源配置
    private final ConcurrentHashMap<String, DataSourceConfig> registeredSources = new ConcurrentHashMap<>();

    // Calcite 连接（延迟初始化）
    private CalciteConnection calciteConnection;
    private final Object calciteLock = new Object();

    /**
     * Default constructor with enumerable engine.
     */
    public CalciteAgent() {
        this(loadEngineFromEnv());
    }

    /**
     * Constructor with specified execution engine.
     *
     * @param engine "enumerable" or "spark"
     */
    public CalciteAgent(String engine) {
        this.engine = engine != null ? engine.toLowerCase() : "enumerable";
    }

    /**
     * Load execution engine from environment variable CALCITE_ENGINE.
     * Falls back to "enumerable" if not set.
     */
    private static String loadEngineFromEnv() {
        String engine = System.getenv("CALCITE_ENGINE");
        return (engine == null || engine.isEmpty()) ? "enumerable" : engine.toLowerCase();
    }

    public static void main(String[] args) {
        logger.info("Starting Calcite Federated Query Agent...");

        // Cache dynamically generated Bindable Java classes to reduce compilation overhead
        System.setProperty("calcite.bindableCacheMaxSize", "1000");

        // Load execution engine from environment variable (default: enumerable)
        // spark: requires calcite-spark dependency on classpath
        String engine = loadEngineFromEnv();
        logger.info("Calcite execution engine: {}", engine);

        CalciteAgent agent = new CalciteAgent(engine);
        try {
            agent.runLoop();
        } catch (Exception e) {
            logger.error("Fatal error in Calcite Agent", e);
            System.err.println("Error: " + e.getMessage());
            System.exit(1);
        }
    }

    /**
     * 主消息处理循环 - 从 stdin 读取，写入 stdout
     */
    public void runLoop() throws IOException {
        BufferedReader reader = new BufferedReader(new InputStreamReader(System.in));
        PrintWriter writer = new PrintWriter(System.out, true);

        // 发送就绪信号
        ObjectNode ready = MAPPER.createObjectNode();
        ready.put("ready", true);
        writer.println(MAPPER.writeValueAsString(ready));

        String line;
        while ((line = reader.readLine()) != null) {
            try {
                String response = handleRequest(line);
                writer.println(response);
            } catch (Exception e) {
                logger.error("Error handling request: {}", e.getMessage(), e);
                writer.println(createErrorResponse(e.getMessage()));
            }
        }
    }

    /**
     * 处理 JSON-RPC 请求
     */
    public String handleRequest(String jsonRequest) throws Exception {
        ObjectNode request = MAPPER.readValue(jsonRequest, ObjectNode.class);

        String method = request.path("method").asText();
        com.fasterxml.jackson.databind.JsonNode paramsNode = request.path("params");
        ObjectNode params = paramsNode.isObject()
            ? (ObjectNode) paramsNode
            : MAPPER.createObjectNode();
        String id = request.path("id").asText(null);

        logger.debug("Received method: {} with params: {}", method, params);

        switch (method) {
            case "registerSource":
                return handleRegisterSource(params, id);
            case "unregisterSource":
                return handleUnregisterSource(params, id);
            case "executeFederatedQuery":
                return handleExecuteFederatedQuery(params, id);
            case "explainFederatedQuery":
                return handleExplainFederatedQuery(params, id);
            case "getDataSourceMetadata":
                return handleGetDataSourceMetadata(params, id);
            case "ping":
                return createSuccessResponse("pong", id);
            default:
                throw new IllegalArgumentException("Unknown method: " + method);
        }
    }

    /**
     * 处理注册数据源请求
     * 将 JDBC 连接注册为 Calcite 的一个 Schema
     */
    private String handleRegisterSource(ObjectNode params, String id) throws Exception {
        String connectionId = params.path("connectionId").asText();
        String jdbcUrl = params.path("jdbcUrl").asText();
        String username = params.path("username").asText("");
        // Accept either plaintext password or hashed password from Rust side
        String password = params.path("password").asText("");
        if (password.isEmpty()) {
            password = params.path("passwordHash").asText("");
        }
        String driverClass = params.path("driverClass").asText("");

        // 加载 JDBC 驱动
        if (!driverClass.isEmpty()) {
            try {
                Class.forName(driverClass);
                logger.info("Loaded JDBC driver: {}", driverClass);
            } catch (ClassNotFoundException e) {
                throw new RuntimeException("Failed to load JDBC driver: " + driverClass, e);
            }
        }

        // 测试连接并获取元数据
        String databaseProduct;
        String databaseVersion;
        try (Connection testConn = DriverManager.getConnection(jdbcUrl, username, password)) {
            DatabaseMetaData meta = testConn.getMetaData();
            databaseProduct = meta.getDatabaseProductName();
            databaseVersion = meta.getDatabaseProductVersion();
            logger.info("Successfully connected to: {} (Product: {}, Version: {})",
                connectionId, databaseProduct, databaseVersion);
        }

        // 存储数据源配置（使用传入的密码或哈希值）
        DataSourceConfig config = new DataSourceConfig(connectionId, jdbcUrl, username, password, driverClass);

        // Use synchronized block to ensure thread-safe registration
        synchronized (calciteLock) {
            registeredSources.put(connectionId, config);
            // 在 Calcite 中注册此数据源为一个 Schema
            registerSchemaInCalcite(config);
        }

        Map<String, Object> result = new LinkedHashMap<>();
        result.put("connectionId", connectionId);
        result.put("databaseProduct", databaseProduct);
        result.put("databaseVersion", databaseVersion);
        result.put("success", true);

        return createSuccessResponse(result, id);
    }

    /**
     * 处理注销数据源请求
     */
    private String handleUnregisterSource(ObjectNode params, String id) throws Exception {
        String connectionId = params.path("connectionId").asText();

        synchronized (calciteLock) {
            if (registeredSources.remove(connectionId) != null) {
                // 从 Calcite 中移除 Schema
                if (calciteConnection != null) {
                    SchemaPlus rootSchema = calciteConnection.getRootSchema();
                    // Calcite 不支持直接移除 Schema，需要重建连接
                    rebuildCalciteSchemas();
                }
                logger.info("Unregistered source: {}", connectionId);
                return createSuccessResponse(Map.of("connectionId", connectionId, "success", true), id);
            }
        }
        return createErrorResponse("Source not found: " + connectionId);
    }

    /**
     * 处理联邦查询执行
     * 使用 Calcite 优化器执行跨连接查询
     */
    private String handleExecuteFederatedQuery(ObjectNode params, String id) throws Exception {
        String sql = params.path("sql").asText();
        int maxRows = params.path("maxRows").asInt(1000);
        long timeoutMs = params.path("timeoutMs").asLong(30000);

        logger.info("Executing federated query: {}", sql);

        long startTime = System.currentTimeMillis();

        // 重写 SQL：将 连接名.Schema.表名 转换为 Calcite 可识别的格式
        String rewrittenSql = rewriteFederatedSql(sql);
        logger.debug("Rewritten SQL: {}", rewrittenSql);

        synchronized (calciteLock) {
            if (calciteConnection == null) {
                throw new RuntimeException("No data sources registered. Call registerSource first.");
            }

            // 注意：不使用 try-with-resources 关闭 calciteConnection，因为它是跨查询复用的共享连接
            PreparedStatement stmt = null;
            ResultSet rs = null;
            try {
                stmt = calciteConnection.prepareStatement(rewrittenSql);
                stmt.setMaxRows(maxRows);
                stmt.setQueryTimeout((int) Math.min(timeoutMs / 1000, Integer.MAX_VALUE));

                rs = stmt.executeQuery();
                ResultSetMetaData rsMeta = rs.getMetaData();
                int columnCount = rsMeta.getColumnCount();

                List<String> columns = new ArrayList<>();
                for (int i = 1; i <= columnCount; i++) {
                    columns.add(rsMeta.getColumnLabel(i));
                }

                List<List<Object>> rows = new ArrayList<>();
                int rowCount = 0;
                while (rs.next() && rowCount < maxRows) {
                    List<Object> row = new ArrayList<>(columnCount);
                    for (int i = 1; i <= columnCount; i++) {
                        Object value = rs.getObject(i);
                        // 处理特殊类型
                        if (value instanceof byte[]) {
                            row.add("[BINARY]");
                        } else if (value instanceof java.sql.Timestamp) {
                            row.add(value.toString());
                        } else if (value instanceof java.sql.Date) {
                            row.add(value.toString());
                        } else if (value instanceof java.sql.Time) {
                            row.add(value.toString());
                        } else if (value instanceof java.math.BigDecimal) {
                            row.add(((java.math.BigDecimal) value).toPlainString());
                        } else {
                            row.add(value);
                        }
                    }
                    rows.add(row);
                    rowCount++;
                }

                long duration = System.currentTimeMillis() - startTime;

                Map<String, Object> result = new LinkedHashMap<>();
                result.put("columns", columns);
                result.put("rows", rows);
                result.put("rowCount", rowCount);
                result.put("durationMs", duration);
                result.put("success", true);

                return createSuccessResponse(result, id);
            } finally {
                // 只关闭 Statement 和 ResultSet，不关闭共享的 CalciteConnection
                if (rs != null) {
                    try { rs.close(); } catch (SQLException ignored) {}
                }
                if (stmt != null) {
                    try { stmt.close(); } catch (SQLException ignored) {}
                }
            }
        }
    }

    /**
     * 处理查询解释
     */
    private String handleExplainFederatedQuery(ObjectNode params, String id) throws Exception {
        String sql = params.path("sql").asText();

        String rewrittenSql = rewriteFederatedSql(sql);

        synchronized (calciteLock) {
            if (calciteConnection == null) {
                throw new RuntimeException("No data sources registered.");
            }

            String explainSql = "EXPLAIN PLAN FOR " + rewrittenSql;
            String plan = "";

            // 注意：不使用 try-with-resources 关闭 calciteConnection，因为它是跨查询复用的共享连接
            PreparedStatement stmt = null;
            ResultSet rs = null;
            try {
                stmt = calciteConnection.prepareStatement(explainSql);
                rs = stmt.executeQuery();
                StringBuilder sb = new StringBuilder();
                while (rs.next()) {
                    sb.append(rs.getString(1)).append("\n");
                }
                plan = sb.toString();
            } finally {
                if (rs != null) {
                    try { rs.close(); } catch (SQLException ignored) {}
                }
                if (stmt != null) {
                    try { stmt.close(); } catch (SQLException ignored) {}
                }
            }

            Map<String, Object> result = new LinkedHashMap<>();
            result.put("plan", plan);
            result.put("success", true);

            return createSuccessResponse(result, id);
        }
    }

    /**
     * 处理元数据获取
     */
    private String handleGetDataSourceMetadata(ObjectNode params, String id) throws Exception {
        String connectionId = params.path("connectionId").asText();

        DataSourceConfig config = registeredSources.get(connectionId);
        if (config == null) {
            return createErrorResponse("Source not found: " + connectionId);
        }

        try (Connection conn = DriverManager.getConnection(config.jdbcUrl, config.username, config.password)) {
            DatabaseMetaData dbMeta = conn.getMetaData();

            // 获取所有 Schema
            List<String> schemas = new ArrayList<>();
            try (ResultSet rs = dbMeta.getSchemas()) {
                while (rs.next()) {
                    schemas.add(rs.getString("TABLE_SCHEM"));
                }
            }

            // 获取每个 Schema 的表数量
            Map<String, Integer> tableCounts = new LinkedHashMap<>();
            for (String schema : schemas) {
                int count = 0;
                try (ResultSet rs = dbMeta.getTables(null, schema, "%", new String[]{"TABLE"})) {
                    while (rs.next()) {
                        count++;
                    }
                }
                tableCounts.put(schema, count);
            }

            Map<String, Object> metadata = new LinkedHashMap<>();
            metadata.put("connectionId", connectionId);
            metadata.put("databaseProductName", dbMeta.getDatabaseProductName());
            metadata.put("databaseProductVersion", dbMeta.getDatabaseProductVersion());
            metadata.put("driverName", dbMeta.getDriverName());
            metadata.put("url", config.jdbcUrl);
            metadata.put("availableSchemas", schemas);
            metadata.put("tableCounts", tableCounts);

            return createSuccessResponse(metadata, id);
        }
    }

    // ========== Calcite Schema 管理 ==========

    /**
     * 在 Calcite 中注册一个数据源为 Schema
     */
    private void registerSchemaInCalcite(DataSourceConfig config) throws SQLException {
        synchronized (calciteLock) {
            ensureCalciteConnection();

            SchemaPlus rootSchema = calciteConnection.getRootSchema();

            // 创建 DataSource 包装器（Calcite 的 JdbcSchema 需要 DataSource 而非 Connection）
            javax.sql.DataSource dataSource = new SimpleDataSource(config.jdbcUrl, config.username, config.password);

            // 查询数据库默认 Schema（H2 为 "PUBLIC"，PostgreSQL 为 "public" 等）
            String defaultSchema;
            try (Connection metaConn = dataSource.getConnection()) {
                defaultSchema = metaConn.getSchema();
            }

            // 使用 connectionId 作为 JdbcSchema 的唯一名称
            // 这样每个连接的 JdbcConvention 名称不同，避免 Calcite 优化器规则冲突
            // （多个名为 "PUBLIC" 的子 Schema 会导致 JdbcToEnumerableConverterRule 重复注册）
            JdbcSchema jdbcSchema = JdbcSchema.create(
                rootSchema,
                config.connectionId,  // 唯一名称 → 唯一 Convention
                dataSource,
                null,           // catalog - 使用默认
                defaultSchema   // 数据库默认 Schema
            );

            rootSchema.add(config.connectionId, jdbcSchema);

            logger.info("Registered Calcite schema for connection: {} (default schema: {})",
                config.connectionId, defaultSchema);
        }
    }

    /**
     * 重建所有 Calcite Schema（在注销数据源后调用）
     */
    private void rebuildCalciteSchemas() throws SQLException {
        if (calciteConnection != null) {
            try {
                calciteConnection.close();
            } catch (SQLException e) {
                logger.warn("Error closing old Calcite connection: {}", e.getMessage());
            }
            calciteConnection = null;
        }

        // 重新创建连接并注册所有数据源
        ensureCalciteConnection();
        for (DataSourceConfig config : registeredSources.values()) {
            registerSchemaInCalcite(config);
        }
    }

    /**
     * 确保 CalciteConnection 已初始化
     */
    private void ensureCalciteConnection() throws SQLException {
        if (calciteConnection == null) {
            try {
                Class.forName("org.apache.calcite.jdbc.Driver");
            } catch (ClassNotFoundException e) {
                throw new SQLException("Calcite JDBC driver not found", e);
            }
            Connection conn = DriverManager.getConnection("jdbc:calcite:");
            calciteConnection = conn.unwrap(CalciteConnection.class);

            // 启用 Calcite 的查询优化
            calciteConnection.getProperties().setProperty("calciteOptimize", "true");
            // 关闭大小写敏感：不同数据库对标识符大小写处理不同
            // （H2 用大写、PostgreSQL 用小写、MySQL 依赖操作系统）
            // 关闭后 Calcite 在查找表/列时忽略大小写
            calciteConnection.getProperties().setProperty("caseSensitive", "false");

            // ===== 百万级数据量优化参数 =====
            // 启用物化视图重写：CTE 和重复子查询结果可被物化，避免重复计算
            calciteConnection.getProperties().setProperty("materializationsEnabled", "true");
            // 去关联化：将相关子查询转换为 Join，减少嵌套循环
            calciteConnection.getProperties().setProperty("forceDecorrelate", "true");
            // 大结果集自动存入临时表，减少内存压力
            calciteConnection.getProperties().setProperty("autoTemp", "true");
            // 注：topDownOpt 在 1.37.0 的 JdbcSchema 联邦场景下触发 EnumerableMergeJoin 断言错误，暂不启用

            // ===== 执行引擎选择 =====
            if ("spark".equals(engine)) {
                // Spark 引擎：通过反射加载 SparkHandlerImpl
                // 需要 classpath 中包含 calcite-spark 依赖
                // Spark 提供 spill-to-disk 能力，适合百万级以上数据量
                // 注意：calcite-spark 1.37.0 依赖 Spark 2.2.2 + Scala 2.10
                try {
                    calciteConnection.getProperties().setProperty("spark", "true");
                    logger.info("Spark engine enabled (spark=true)");
                } catch (Exception e) {
                    logger.warn("Failed to enable Spark engine, falling back to enumerable: {}",
                        e.getMessage());
                    calciteConnection.getProperties().setProperty("spark", "false");
                }
            } else {
                // Enumerable 引擎：使用 Janino 编译器在运行时编译生成的 Java 代码
                // 轻量级，无额外依赖，适合中小数据量
                calciteConnection.getProperties().setProperty("spark", "false");
            }

            logger.info("Initialized Calcite connection (engine={}, caseSensitive=false, " +
                "materializations=true, forceDecorrelate=true, autoTemp=true)", engine);
        }
    }

    /**
     * 重写联邦 SQL
     * 将 连接名.Schema.表名 转换为 Calcite 格式："连接名"."Schema"."表名"
     * Calcite 会通过已注册的 Schema 自动路由到正确的数据源
     */
    private String rewriteFederatedSql(String sql) {
        // 将 连接名.Schema.表名 和 连接名.Database.Schema.表名 重写为 "连接名"."表名"
        // 因为每个连接注册为单个 JdbcSchema（映射到数据库默认 Schema），表直接在连接名下可访问
        String result = sql;

        for (String connId : registeredSources.keySet()) {
            // 处理四段式：connId.database.schema.table → "connId"."table"
            // 必须先处理四段式（更长的匹配），再处理三段式
            String pattern4 = "(?i)\\b" + java.util.regex.Pattern.quote(connId) +
                "\\.([a-zA-Z_][a-zA-Z0-9_]*)\\.([a-zA-Z_][a-zA-Z0-9_]*)\\.([a-zA-Z_][a-zA-Z0-9_]*)";
            result = result.replaceAll(pattern4, "\"" + connId + "\".\"$3\"");

            // 匹配三段式 connId.schema.table → "connId"."table"
            String pattern3 = "(?i)\\b" + java.util.regex.Pattern.quote(connId) +
                "\\.([a-zA-Z_][a-zA-Z0-9_]*)\\.([a-zA-Z_][a-zA-Z0-9_]*)";
            result = result.replaceAll(pattern3, "\"" + connId + "\".\"$2\"");
        }

        return result;
    }

    // ========== 辅助方法 ==========

    String createSuccessResponse(Object result, String id) throws Exception {
        ObjectNode response = MAPPER.createObjectNode();
        response.put("jsonrpc", "2.0");
        response.set("result", MAPPER.valueToTree(result));
        if (id != null) {
            response.put("id", id);
        }
        return MAPPER.writeValueAsString(response);
    }

    String createErrorResponse(String message) {
        try {
            ObjectNode response = MAPPER.createObjectNode();
            response.put("jsonrpc", "2.0");
            ObjectNode error = MAPPER.createObjectNode();
            error.put("code", -32000);
            error.put("message", message);
            response.set("error", error);
            return MAPPER.writeValueAsString(response);
        } catch (Exception e) {
            return "{\"jsonrpc\":\"2.0\",\"error\":{\"code\":-32603,\"message\":\"Internal error\"}}";
        }
    }

    // ========== 内部类 ==========

    public static class DataSourceConfig {
        final String connectionId;
        final String jdbcUrl;
        final String username;
        final String password;
        final String driverClass;

        DataSourceConfig(String connectionId, String jdbcUrl, String username,
                        String password, String driverClass) {
            this.connectionId = connectionId;
            this.jdbcUrl = jdbcUrl;
            this.username = username;
            this.password = password;
            this.driverClass = driverClass;
        }
    }

    /**
     * 简单的 DataSource 实现，用于将 DriverManager 连接包装为 DataSource
     * Calcite 的 JdbcSchema 需要 DataSource 接口
     */
    public static class SimpleDataSource implements javax.sql.DataSource {
        private final String jdbcUrl;
        private final String username;
        private final String password;

        SimpleDataSource(String jdbcUrl, String username, String password) {
            this.jdbcUrl = jdbcUrl;
            this.username = username;
            this.password = password;
        }

        @Override
        public Connection getConnection() throws SQLException {
            return DriverManager.getConnection(jdbcUrl, username, password);
        }

        @Override
        public Connection getConnection(String username, String password) throws SQLException {
            return DriverManager.getConnection(jdbcUrl, username, password);
        }

        @Override
        public java.io.PrintWriter getLogWriter() { return null; }
        @Override
        public void setLogWriter(java.io.PrintWriter out) {}
        @Override
        public void setLoginTimeout(int seconds) {}
        @Override
        public int getLoginTimeout() { return 0; }
        @Override
        public java.util.logging.Logger getParentLogger() {
            return java.util.logging.Logger.getLogger("SimpleDataSource");
        }
        @Override
        public <T> T unwrap(Class<T> iface) throws SQLException {
            throw new SQLException("Not a wrapper");
        }
        @Override
        public boolean isWrapperFor(Class<?> iface) { return false; }
    }
}

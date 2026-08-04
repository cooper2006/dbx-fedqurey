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

    // 已注册的数据源配置
    private final ConcurrentHashMap<String, DataSourceConfig> registeredSources = new ConcurrentHashMap<>();

    // Calcite 连接（延迟初始化）
    private CalciteConnection calciteConnection;
    private final Object calciteLock = new Object();

    public static void main(String[] args) {
        logger.info("Starting Calcite Federated Query Agent...");

        CalciteAgent agent = new CalciteAgent();
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
        ObjectNode params = request.path("params").isObject()
            ? request.path("params")
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
        String password = params.path("password").asText("");
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

        // 存储数据源配置
        DataSourceConfig config = new DataSourceConfig(connectionId, jdbcUrl, username, password, driverClass);
        registeredSources.put(connectionId, config);

        // 在 Calcite 中注册此数据源为一个 Schema
        registerSchemaInCalcite(config);

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

        if (registeredSources.remove(connectionId) != null) {
            // 从 Calcite 中移除 Schema
            synchronized (calciteLock) {
                if (calciteConnection != null) {
                    SchemaPlus rootSchema = calciteConnection.getRootSchema();
                    // Calcite 不支持直接移除 Schema，需要重建连接
                    rebuildCalciteSchemas();
                }
            }
            logger.info("Unregistered source: {}", connectionId);
            return createSuccessResponse(Map.of("connectionId", connectionId, "success", true), id);
        } else {
            return createErrorResponse("Source not found: " + connectionId);
        }
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

            try (Connection conn = calciteConnection;
                 PreparedStatement stmt = conn.prepareStatement(rewrittenSql)) {

                stmt.setMaxRows(maxRows);
                stmt.setQueryTimeout((int) Math.min(timeoutMs / 1000, Integer.MAX_VALUE));

                try (ResultSet rs = stmt.executeQuery()) {
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

            try (Connection conn = calciteConnection;
                 PreparedStatement stmt = conn.prepareStatement(explainSql)) {
                try (ResultSet rs = stmt.executeQuery()) {
                    StringBuilder sb = new StringBuilder();
                    while (rs.next()) {
                        sb.append(rs.getString(1)).append("\n");
                    }
                    plan = sb.toString();
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

            // 创建 JDBC 连接池（使用单个连接，Calcite 会在需要时获取元数据）
            Connection rawConn = DriverManager.getConnection(config.jdbcUrl, config.username, config.password);

            // 使用 JdbcSchema 将 JDBC 连接的 Schema 注册到 Calcite
            // JdbcSchema 会自动发现表和列信息
            JdbcSchema jdbcSchema = JdbcSchema.create(
                rootSchema,
                config.connectionId,
                rawConn,
                null,  // catalog - 使用默认
                null   // schema - 使用默认
            );

            rootSchema.add(config.connectionId, jdbcSchema);

            logger.info("Registered Calcite schema for connection: {}", config.connectionId);
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

            logger.info("Initialized Calcite connection");
        }
    }

    /**
     * 重写联邦 SQL
     * 将 连接名.Schema.表名 转换为 Calcite 格式："连接名"."Schema"."表名"
     * Calcite 会通过已注册的 Schema 自动路由到正确的数据源
     */
    private String rewriteFederatedSql(String sql) {
        // 使用正则匹配 连接名.Schema.表名 模式
        // 需要确保连接名匹配已注册的数据源
        String result = sql;

        for (String connId : registeredSources.keySet()) {
            // 匹配 connId.schema.table 或 connId.database.schema.table
            // 替换为 "connId"."schema"."table"
            String pattern = "(?i)\\b" + java.util.regex.Pattern.quote(connId) + "\\.([a-zA-Z_][a-zA-Z0-9_]*)\\.([a-zA-Z_][a-zA-Z0-9_]*)";
            result = result.replaceAll(pattern, "\"$1\".\"$2\"");

            // 处理四段式：connId.database.schema.table
            String pattern4 = "(?i)\\b" + java.util.regex.Pattern.quote(connId) + "\\.([a-zA-Z_][a-zA-Z0-9_]*)\\.([a-zA-Z_][a-zA-Z0-9_]*)\\.([a-zA-Z_][a-zA-Z0-9_]*)";
            result = result.replaceAll(pattern4, "\"$2\".\"$3\"");
        }

        return result;
    }

    // ========== 辅助方法 ==========

    private String createSuccessResponse(Object result, String id) throws Exception {
        ObjectNode response = MAPPER.createObjectNode();
        response.put("jsonrpc", "2.0");
        response.set("result", MAPPER.valueToTree(result));
        if (id != null) {
            response.put("id", id);
        }
        return MAPPER.writeValueAsString(response);
    }

    private String createErrorResponse(String message) {
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
}

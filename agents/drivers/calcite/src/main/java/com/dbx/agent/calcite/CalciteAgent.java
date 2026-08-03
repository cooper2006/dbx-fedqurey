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

/**
 * Apache Calcite Federated Query Agent for dbx.
 * 
 * This agent provides federated query capabilities by:
 * 1. Managing JDBC connections to multiple database sources
 * 2. Executing federated SQL queries across multiple databases
 * 3. Returning results in JSON format
 * 
 * Communication: JSON-RPC 2.0 over stdin/stdout
 */
public class CalciteAgent {
    
    private static final Logger logger = LoggerFactory.getLogger(CalciteAgent.class);
    private static final ObjectMapper MAPPER = new ObjectMapper();
    
    // Registered JDBC connections
    private final ConcurrentHashMap<String, DataSourceConfig> registeredSources = new ConcurrentHashMap<>();
    
    // Schema visibility configuration
    private final ConcurrentHashMap<String, SchemaVisibilityConfig> schemaVisibility = new ConcurrentHashMap<>();
    
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
     * Main message processing loop - reads from stdin, writes to stdout
     */
    public void runLoop() throws IOException {
        BufferedReader reader = new BufferedReader(new InputStreamReader(System.in));
        PrintWriter writer = new PrintWriter(System.out, true);
        
        String line;
        while ((line = reader.readLine()) != null) {
            try {
                String response = handleRequest(line);
                writer.println(response);
            } catch (Exception e) {
                logger.error("Error handling request: {}", e.getMessage(), e);
                writer.println(MAPPER.writeValueAsString(createErrorResponse(e.getMessage())));
            }
        }
    }
    
    /**
     * Handle incoming JSON-RPC requests
     */
    private String handleRequest(String jsonRequest) throws Exception {
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
     * Handle register source request
     */
    private String handleRegisterSource(ObjectNode params, String id) throws Exception {
        String connectionId = params.path("connectionId").asText();
        String jdbcUrl = params.path("jdbcUrl").asText();
        String username = params.path("username").asText("");
        String password = params.path("password").asText("");
        String driverClass = params.path("driverClass").asText("");
        
        // Extract schema visibility configuration
        SchemaVisibilityConfig visibilityConfig = parseVisibilityConfig(params.path("schemaVisibility"));
        
        // Load driver if specified
        if (!driverClass.isEmpty()) {
            try {
                Class.forName(driverClass);
                logger.info("Loaded JDBC driver: {}", driverClass);
            } catch (ClassNotFoundException e) {
                throw new RuntimeException("Failed to load JDBC driver: " + driverClass, e);
            }
        }
        
        // Test connection
        try (Connection testConn = DriverManager.getConnection(jdbcUrl, username, password)) {
            DatabaseMetaData meta = testConn.getMetaData();
            logger.info("Successfully connected to: {} (Product: {}, Version: {})", 
                connectionId, meta.getDatabaseProductName(), meta.getDatabaseProductVersion());
            
            // Store source configuration
            DataSourceConfig config = new DataSourceConfig(connectionId, jdbcUrl, username, password, driverClass);
            registeredSources.put(connectionId, config);
            schemaVisibility.put(connectionId, visibilityConfig);
            
            Map<String, Object> result = new LinkedHashMap<>();
            result.put("connectionId", connectionId);
            result.put("databaseProduct", meta.getDatabaseProductName());
            result.put("databaseVersion", meta.getDatabaseProductVersion());
            result.put("jdbcDriver", meta.getDriverName());
            
            return createSuccessResponse(result, id);
        }
    }
    
    /**
     * Handle unregister source request
     */
    private String handleUnregisterSource(ObjectNode params, String id) throws Exception {
        String connectionId = params.path("connectionId").asText();
        
        if (registeredSources.remove(connectionId) != null) {
            schemaVisibility.remove(connectionId);
            logger.info("Unregistered source: {}", connectionId);
            return createSuccessResponse(Map.of("connectionId", connectionId), id);
        } else {
            return createErrorResponse("Source not found: " + connectionId);
        }
    }
    
    /**
     * Handle federated query execution
     * For now, delegates to individual source execution (no real Calcite optimization)
     */
    private String handleExecuteFederatedQuery(ObjectNode params, String id) throws Exception {
        String sql = params.path("sql").asText();
        int maxRows = params.path("maxRows").asInt(1000);
        long timeoutMs = params.path("timeoutMs").asLong(30000);
        
        logger.info("Executing federated query: {}", sql);
        
        long startTime = System.currentTimeMillis();
        
        // Parse the SQL to determine which connections are involved
        Set<String> involvedConnections = extractConnectionsFromSql(sql);
        
        // If only one connection, execute directly
        if (involvedConnections.size() <= 1) {
            return executeSingleConnectionQuery(sql, involvedConnections.iterator().next(), maxRows, timeoutMs, id);
        }
        
        // For multi-connection queries, we attempt to execute via each involved connection
        // In a real implementation, this would use Calcite's federation engine
        return executeMultiConnectionQuery(sql, involvedConnections, maxRows, timeoutMs, id);
    }
    
    /**
     * Execute query on a single connection
     */
    private String executeSingleConnectionQuery(String sql, String connectionId, int maxRows, long timeoutMs, String id) throws Exception {
        DataSourceConfig config = registeredSources.get(connectionId);
        if (config == null) {
            throw new RuntimeException("Connection not registered: " + connectionId);
        }
        
        try (Connection conn = DriverManager.getConnection(config.jdbcUrl, config.username, config.password);
             PreparedStatement stmt = conn.prepareStatement(sql)) {
            
            stmt.setMaxRows(maxRows);
            stmt.setQueryTimeout((int) Math.min(timeoutMs / 1000, Integer.MAX_VALUE));
            
            try (ResultSet rs = stmt.executeQuery()) {
                ResultSetMetaData rsMeta = rs.getMetaData();
                int columnCount = rsMeta.getColumnCount();
                
                List<String> columns = new ArrayList<>();
                for (int i = 1; i <= columnCount; i++) {
                    columns.add(rsMeta.getColumnLabel(i));
                }
                
                List<Map<String, Object>> rows = new ArrayList<>();
                int rowCount = 0;
                while (rs.next() && rowCount < maxRows) {
                    Map<String, Object> row = new LinkedHashMap<>();
                    for (int i = 1; i <= columnCount; i++) {
                        row.put(columns.get(i - 1), rs.getObject(i));
                    }
                    rows.add(row);
                    rowCount++;
                }
                
                long duration = System.currentTimeMillis() - System.currentTimeMillis();
                
                return createSuccessResponse(Map.of(
                    "connectionId", connectionId,
                    "columns", columns,
                    "rows", rows,
                    "rowCount", rowCount,
                    "durationMs", duration
                ), id);
            }
        }
    }
    
    /**
     * Execute multi-connection query (placeholder for future Calcite integration)
     */
    private String executeMultiConnectionQuery(String sql, Set<String> involvedConnections, int maxRows, long timeoutMs, String id) throws Exception {
        logger.warn("Multi-connection query detected: {}", involvedConnections);
        logger.warn("This feature requires Apache Calcite with federation capabilities.");
        
        // For now, return an error indicating Calcite is needed
        throw new UnsupportedOperationException(
            "Federated query across multiple connections requires Apache Calcite with federation enabled. " +
            "Connections involved: " + involvedConnections + ". " +
            "Please ensure Calcite federation plugin is properly configured."
        );
    }
    
    /**
     * Handle query explanation
     */
    private String handleExplainFederatedQuery(ObjectNode params, String id) throws Exception {
        String sql = params.path("sql").asText();
        
        // For now, just return the SQL as-is (no real explain plan)
        Map<String, Object> result = new LinkedHashMap<>();
        result.put("sql", sql);
        result.put("note", "Full query planning requires Apache Calcite with federation engine");
        
        return createSuccessResponse(result, id);
    }
    
    /**
     * Handle metadata retrieval
     */
    private String handleGetDataSourceMetadata(ObjectNode params, String id) throws Exception {
        String connectionId = params.path("connectionId").asText();
        
        DataSourceConfig config = registeredSources.get(connectionId);
        if (config == null) {
            return createErrorResponse("Source not found: " + connectionId);
        }
        
        try (Connection conn = DriverManager.getConnection(config.jdbcUrl, config.username, config.password)) {
            DatabaseMetaData dbMeta = conn.getMetaData();
            
            SchemaVisibilityConfig visibility = schemaVisibility.getOrDefault(
                connectionId, SchemaVisibilityConfig.defaultConfig());
            
            Map<String, Object> metadata = new LinkedHashMap<>();
            metadata.put("connectionId", connectionId);
            metadata.put("databaseProductName", dbMeta.getDatabaseProductName());
            metadata.put("databaseProductVersion", dbMeta.getDatabaseProductVersion());
            metadata.put("driverName", dbMeta.getDriverName());
            metadata.put("url", config.jdbcUrl);
            metadata.put("defaultSchema", visibility.getDefaultSchema());
            metadata.put("allowedSchemas", new ArrayList<>(visibility.getAllowedSchemas()));
            metadata.put("excludedSchemas", new ArrayList<>(visibility.getExcludedSchemas()));
            metadata.put("excludedTables", new ArrayList<>(visibility.getExcludedTables()));
            
            return createSuccessResponse(metadata, id);
        }
    }
    
    /**
     * Parse schema visibility configuration from params
     */
    private SchemaVisibilityConfig parseVisibilityConfig(ObjectNode node) {
        if (node == null || !node.isObject()) {
            return SchemaVisibilityConfig.defaultConfig();
        }
        
        SchemaVisibilityConfig config = new SchemaVisibilityConfig();
        
        // Default schema
        config.setDefaultSchema(node.path("defaultSchema").asText("public"));
        
        // Allow all schemas flag
        config.setAllowAllSchemas(node.path("allowAllSchemas").asBoolean(false));
        
        // Allowed schemas
        if (node.has("allowedSchemas") && node.get("allowedSchemas").isArray()) {
            var array = node.get("allowedSchemas");
            for (int i = 0; i < array.size(); i++) {
                config.addAllowedSchema(array.getString(i));
            }
        }
        
        // Excluded schemas
        if (node.has("excludedSchemas") && node.get("excludedSchemas").isArray()) {
            var array = node.get("excludedSchemas");
            for (int i = 0; i < array.size(); i++) {
                config.addExcludedSchema(array.getString(i));
            }
        }
        
        // Excluded tables
        if (node.has("excludedTables") && node.get("excludedTables").isArray()) {
            var array = node.get("excludedTables");
            for (int i = 0; i < array.size(); i++) {
                config.addExcludedTable(array.getString(i));
            }
        }
        
        return config;
    }
    
    /**
     * Extract connection IDs from SQL (simple heuristic)
     */
    private Set<String> extractConnectionsFromSql(String sql) {
        Set<String> connections = new HashSet<>();
        
        // Simple pattern matching for connection.schema.table references
        String pattern = "(?i)\\b(\\w+)\\.\\w+\\.\\w+\\b";
        java.util.regex.Matcher matcher = java.util.regex.Pattern.compile(pattern).matcher(sql);
        
        while (matcher.find()) {
            String potentialConnection = matcher.group(1);
            // Skip common SQL keywords
            if (!isSqlKeyword(potentialConnection)) {
                connections.add(potentialConnection);
            }
        }
        
        return connections;
    }
    
    private boolean isSqlKeyword(String word) {
        Set<String> keywords = Set.of(
            "select", "from", "where", "join", "on", "and", "or", "not", "in", "is",
            "null", "true", "false", "values", "insert", "update", "delete", "create",
            "drop", "alter", "set", "table", "index", "view", "as", "by", "group",
            "order", "having", "limit", "offset", "union", "all", "distinct", "case",
            "when", "then", "else", "end", "cast", "exists", "between", "like", "into"
        );
        return keywords.contains(word.toLowerCase());
    }
    
    // ========== Helper Methods ==========
    
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
        ObjectNode response = MAPPER.createObjectNode();
        response.put("jsonrpc", "2.0");
        ObjectNode error = MAPPER.createObjectNode();
        error.put("code", -32000);
        error.put("message", message);
        response.set("error", error);
        return MAPPER.writeValueAsString(response);
    }
    
    // ========== Inner Classes ==========
    
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
    
    public static class SchemaVisibilityConfig {
        private String defaultSchema = "public";
        private final Set<String> allowedSchemas = new HashSet<>();
        private final Set<String> excludedSchemas = new HashSet<>();
        private final Set<String> excludedTables = new HashSet<>();
        private boolean allowAllSchemas = false;
        
        public static SchemaVisibilityConfig defaultConfig() {
            return new SchemaVisibilityConfig();
        }
        
        public String getDefaultSchema() {
            return defaultSchema;
        }
        
        public void setDefaultSchema(String defaultSchema) {
            this.defaultSchema = defaultSchema;
        }
        
        public Set<String> getAllowedSchemas() {
            return Collections.unmodifiableSet(allowedSchemas);
        }
        
        public void addAllowedSchema(String schema) {
            allowedSchemas.add(schema);
        }
        
        public Set<String> getExcludedSchemas() {
            return Collections.unmodifiableSet(excludedSchemas);
        }
        
        public void addExcludedSchema(String schema) {
            excludedSchemas.add(schema);
        }
        
        public Set<String> getExcludedTables() {
            return Collections.unmodifiableSet(excludedTables);
        }
        
        public void addExcludedTable(String table) {
            excludedTables.add(table);
        }
        
        public boolean isAllowAllSchemas() {
            return allowAllSchemas;
        }
        
        public void setAllowAllSchemas(boolean allowAllSchemas) {
            this.allowAllSchemas = allowAllSchemas;
        }
        
        public boolean isSchemaAllowed(String schema) {
            if (allowAllSchemas && allowedSchemas.isEmpty()) {
                return !excludedSchemas.contains(schema);
            }
            return allowedSchemas.contains(schema);
        }
        
        public boolean isTableAllowed(String tableName) {
            return !excludedTables.contains(tableName);
        }
    }
}

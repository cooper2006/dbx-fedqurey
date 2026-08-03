package com.dbx.agent.calcite;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.*;
import org.junit.jupiter.api.extension.ExtendWith;
import org.mockito.junit.jupiter.MockitoExtension;

import java.util.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Unit tests for Calcite Agent functionality
 */
@ExtendWith(MockitoExtension.class)
class CalciteAgentTest {
    
    private static final ObjectMapper MAPPER = new ObjectMapper();
    private CalciteAgent agent;
    
    @BeforeEach
    void setUp() {
        agent = new CalciteAgent();
    }
    
    @AfterEach
    void tearDown() {
        // Clean up resources if needed
    }
    
    // ========== Schema Visibility Tests ==========
    
    @Test
    @DisplayName("Schema visibility config - default allows all schemas")
    void testDefaultVisibilityConfig() {
        CalciteAgent.SchemaVisibilityConfig config = CalciteAgent.SchemaVisibilityConfig.defaultConfig();
        
        assertTrue(config.isAllowAllSchemas());
        assertTrue(config.getAllowedSchemas().isEmpty());
        assertTrue(config.getExcludedSchemas().isEmpty());
        assertEquals("public", config.getDefaultSchema());
    }
    
    @Test
    @DisplayName("Schema visibility config - allowAllSchemas with excluded schemas")
    void testAllowAllWithExclusions() {
        CalciteAgent.SchemaVisibilityConfig config = CalciteAgent.SchemaVisibilityConfig.defaultConfig();
        config.addExcludedSchema("sensitive_schema");
        config.addExcludedSchema("admin_schema");
        
        assertTrue(config.isSchemaAllowed("public"));
        assertTrue(config.isSchemaAllowed("analytics"));
        assertFalse(config.isSchemaAllowed("sensitive_schema"));
        assertFalse(config.isSchemaAllowed("admin_schema"));
    }
    
    @Test
    @DisplayName("Schema visibility config - explicit allowed schemas")
    void testExplicitAllowedSchemas() {
        CalciteAgent.SchemaVisibilityConfig config = new CalciteAgent.SchemaVisibilityConfig();
        config.setAllowAllSchemas(false);
        config.addAllowedSchema("public");
        config.addAllowedSchema("analytics");
        
        assertTrue(config.isSchemaAllowed("public"));
        assertTrue(config.isSchemaAllowed("analytics"));
        assertFalse(config.isSchemaAllowed("other_schema"));
    }
    
    @Test
    @DisplayName("Schema visibility config - table exclusion takes precedence")
    void testTableExclusionPrecedence() {
        CalciteAgent.SchemaVisibilityConfig config = CalciteAgent.SchemaVisibilityConfig.defaultConfig();
        config.addExcludedTable("users");
        config.addExcludedTable("secret_data");
        
        assertTrue(config.isTableAllowed("orders"));
        assertFalse(config.isTableAllowed("users"));
        assertFalse(config.isTableAllowed("secret_data"));
    }
    
    // ========== SQL Parsing Tests ==========
    
    @Test
    @DisplayName("Extract connections from federated SQL")
    void testExtractConnectionsFromSql() throws Exception {
        // Use reflection to access private method for testing
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "extractConnectionsFromSql", String.class);
        method.setAccessible(true);
        
        String sql = "SELECT u.name, o.total FROM my_pg.public.users u JOIN my_mysql.shop.orders o ON u.id = o.user_id";
        
        Set<String> connections = (Set<String>) method.invoke(agent, sql);
        
        assertNotNull(connections);
        assertTrue(connections.contains("my_pg"));
        assertTrue(connections.contains("my_mysql"));
        assertEquals(2, connections.size());
    }
    
    @Test
    @DisplayName("Do not extract SQL keywords as connections")
    void testNotExtractKeywordsAsConnections() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "extractConnectionsFromSql", String.class);
        method.setAccessible(true);
        
        String sql = "SELECT * FROM users WHERE id = 1";
        
        Set<String> connections = (Set<String>) method.invoke(agent, sql);
        
        assertNotNull(connections);
        assertFalse(connections.contains("select"));
        assertFalse(connections.contains("from"));
        assertFalse(connections.contains("where"));
    }
    
    @Test
    @DisplayName("Handle SQL without federation syntax")
    void testNoFederationSyntax() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "extractConnectionsFromSql", String.class);
        method.setAccessible(true);
        
        String sql = "SELECT * FROM users u JOIN orders o ON u.id = o.user_id";
        
        Set<String> connections = (Set<String>) method.invoke(agent, sql);
        
        assertNotNull(connections);
        assertTrue(connections.isEmpty());
    }
    
    // ========== Multi-connection Detection ==========
    
    @Test
    @DisplayName("Detect single connection query")
    void testSingleConnectionQuery() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "extractConnectionsFromSql", String.class);
        method.setAccessible(true);
        
        String sql = "SELECT * FROM my_db.public.users";
        
        Set<String> connections = (Set<String>) method.invoke(agent, sql);
        
        assertEquals(1, connections.size());
        assertTrue(connections.contains("my_db"));
    }
    
    @Test
    @DisplayName("Detect multi-connection query")
    void testMultiConnectionQuery() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "extractConnectionsFromSql", String.class);
        method.setAccessible(true);
        
        String sql = """
            SELECT a.name, b.amount 
            FROM db1.schema1.tableA a 
            JOIN db2.schema2.tableB b ON a.id = b.a_id
            """;
        
        Set<String> connections = (Set<String>) method.invoke(agent, sql);
        
        assertEquals(2, connections.size());
        assertTrue(connections.contains("db1"));
        assertTrue(connections.contains("db2"));
    }
    
    // ========== Error Handling ==========
    
    @Test
    @DisplayName("Create error response with proper JSON format")
    void testErrorResponseFormat() {
        String response = agent.createErrorResponse("Test error message");
        
        assertNotNull(response);
        assertTrue(response.contains("\"jsonrpc\":\"2.0\""));
        assertTrue(response.contains("\"error\""));
        assertTrue(response.contains("Test error message"));
    }
    
    @Test
    @DisplayName("Parse unknown method should throw exception")
    void testUnknownMethodHandling() throws Exception {
        String jsonRequest = "{\"jsonrpc\":\"2.0\",\"method\":\"unknownMethod\",\"id\":1}";
        
        assertThrows(Exception.class, () -> agent.handleRequest(jsonRequest));
    }
    
    // ========== Ping Test ==========
    
    @Test
    @DisplayName("Ping returns pong")
    void testPingResponse() throws Exception {
        String request = "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":1}";
        String response = agent.handleRequest(request);
        
        JsonNode result = MAPPER.readTree(response);
        assertEquals("pong", result.path("result").asText());
        assertEquals("1", result.path("id").asText());
    }
}

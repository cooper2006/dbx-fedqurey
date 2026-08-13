package com.dbx.agent.calcite;

import com.fasterxml.jackson.databind.JsonNode;
import com.fasterxml.jackson.databind.ObjectMapper;
import org.junit.jupiter.api.*;

import static org.junit.jupiter.api.Assertions.*;

/**
 * Calcite Agent 单元测试
 */
class CalciteAgentTest {

    private static final ObjectMapper MAPPER = new ObjectMapper();
    private CalciteAgent agent;

    @BeforeEach
    void setUp() {
        agent = new CalciteAgent();
    }

    // ========== JSON-RPC 协议测试 ==========

    @Test
    @DisplayName("Ping 返回 pong")
    void testPingResponse() throws Exception {
        String request = "{\"jsonrpc\":\"2.0\",\"method\":\"ping\",\"id\":1}";
        String response = agent.handleRequest(request);

        JsonNode result = MAPPER.readTree(response);
        assertEquals("pong", result.path("result").asText());
        assertEquals("1", result.path("id").asText());
    }

    @Test
    @DisplayName("未知方法应抛出异常")
    void testUnknownMethodHandling() {
        String jsonRequest = "{\"jsonrpc\":\"2.0\",\"method\":\"unknownMethod\",\"id\":1}";
        assertThrows(Exception.class, () -> agent.handleRequest(jsonRequest));
    }

    // ========== SQL 重写测试 ==========

    @Test
    @DisplayName("未注册数据源时 SQL 不被重写")
    void testRewriteWithoutRegisteredSources() throws Exception {
        // 使用反射访问私有方法
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "rewriteFederatedSql", String.class);
        method.setAccessible(true);

        String sql = "SELECT * FROM my_pg.public.users";
        String result = (String) method.invoke(agent, sql);

        // 没有注册数据源，SQL 不应该被修改
        assertEquals(sql, result);
    }

    // ========== 错误处理测试 ==========

    @Test
    @DisplayName("错误响应格式正确")
    void testErrorResponseFormat() {
        String response = agent.createErrorResponse("Test error message");

        assertNotNull(response);
        assertTrue(response.contains("\"jsonrpc\":\"2.0\""));
        assertTrue(response.contains("\"error\""));
        assertTrue(response.contains("Test error message"));
    }

    @Test
    @DisplayName("成功响应格式正确")
    void testSuccessResponseFormat() throws Exception {
        String response = agent.createSuccessResponse(java.util.Map.of("key", "value"), "42");

        JsonNode node = MAPPER.readTree(response);
        assertEquals("2.0", node.path("jsonrpc").asText());
        assertEquals("value", node.path("result").path("key").asText());
        assertEquals("42", node.path("id").asText());
    }

    // ========== 查询执行测试（无数据源注册时应报错） ==========

    @Test
    @DisplayName("未注册数据源时执行查询应返回错误")
    void testExecuteWithoutSources() throws Exception {
        String request = "{\"jsonrpc\":\"2.0\",\"method\":\"executeFederatedQuery\",\"params\":{\"sql\":\"SELECT 1\"},\"id\":1}";
        assertThrows(Exception.class, () -> agent.handleRequest(request));
    }

    // ========== parseIntParamFromUrl 单元测试 ==========

    @Test
    @DisplayName("parseIntParamFromUrl 解析 ?connectTimeout=10")
    void testParseIntParamFromUrl_question_mark() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "parseIntParamFromUrl", String.class, String.class, int.class);
        method.setAccessible(true);

        int result = (int) method.invoke(agent,
            "jdbc:postgresql://127.0.0.1:5432/tpcds?connectTimeout=10&loginTimeout=10",
            "connectTimeout", 5);
        assertEquals(10, result);
    }

    @Test
    @DisplayName("parseIntParamFromUrl 解析 &connectTimeout=10")
    void testParseIntParamFromUrl_ampersand() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "parseIntParamFromUrl", String.class, String.class, int.class);
        method.setAccessible(true);

        int result = (int) method.invoke(agent,
            "jdbc:mysql://127.0.0.1:9030/tpcds?foo=bar&connectTimeout=20",
            "connectTimeout", 5);
        assertEquals(20, result);
    }

    @Test
    @DisplayName("parseIntParamFromUrl 参数不存在时返回默认值")
    void testParseIntParamFromUrl_notFound() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "parseIntParamFromUrl", String.class, String.class, int.class);
        method.setAccessible(true);

        int result = (int) method.invoke(agent,
            "jdbc:postgresql://127.0.0.1:5432/tpcds",
            "connectTimeout", 30);
        assertEquals(30, result);
    }

    @Test
    @DisplayName("parseIntParamFromUrl 参数为空时返回默认值")
    void testParseIntParamFromUrl_null() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "parseIntParamFromUrl", String.class, String.class, int.class);
        method.setAccessible(true);

        int result = (int) method.invoke(agent,
            (String) null,
            "connectTimeout", 15);
        assertEquals(15, result);
    }

    @Test
    @DisplayName("parseIntParamFromUrl 参数值无效时返回默认值")
    void testParseIntParamFromUrl_invalidValue() throws Exception {
        java.lang.reflect.Method method = CalciteAgent.class.getDeclaredMethod(
            "parseIntParamFromUrl", String.class, String.class, int.class);
        method.setAccessible(true);

        int result = (int) method.invoke(agent,
            "jdbc:postgresql://127.0.0.1:5432/tpcds?connectTimeout=abc",
            "connectTimeout", 10);
        assertEquals(10, result);
    }

    // ========== SimpleDataSource.configure 单元测试 ==========

    @Test
    @DisplayName("SimpleDataSource 构造函数参数正确传递")
    void testSimpleDataSourceFields() throws Exception {
        CalciteAgent.SimpleDataSource ds = new CalciteAgent.SimpleDataSource(
            "jdbc:h2:mem:test", "user", "pass", 5000, 30);

        assertEquals("jdbc:h2:mem:test", ds.jdbcUrl);
        assertEquals("user", ds.username);
        assertEquals("pass", ds.password);
        assertEquals(5000, ds.networkTimeoutMs);
        assertEquals(30, ds.queryTimeoutSecs);
    }

    @Test
    @DisplayName("SimpleDataSource 使用 H2 数据库可正常获取连接")
    void testSimpleDataSourceWithH2() throws Exception {
        CalciteAgent.SimpleDataSource ds = new CalciteAgent.SimpleDataSource(
            "jdbc:h2:mem:unittest;DB_CLOSE_DELAY=-1", "sa", "", 30000, 60);

        java.sql.Connection conn = ds.getConnection();
        assertNotNull(conn);
        assertFalse(conn.isClosed());
        conn.close();
    }

    @Test
    @DisplayName("SimpleDataSource getConnection(username,password) 使用正确的凭证")
    void testSimpleDataSourceWithCredentials() throws Exception {
        CalciteAgent.SimpleDataSource ds = new CalciteAgent.SimpleDataSource(
            "jdbc:h2:mem:unittest2;DB_CLOSE_DELAY=-1", "sa", "", 30000, 60);

        java.sql.Connection conn = ds.getConnection("sa", "");
        assertNotNull(conn);
        assertFalse(conn.isClosed());
        conn.close();
    }

    @Test
    @DisplayName("SimpleDataSource 可正常关闭")
    void testSimpleDataSourceClose() throws Exception {
        CalciteAgent.SimpleDataSource ds = new CalciteAgent.SimpleDataSource(
            "jdbc:h2:mem:unittest3;DB_CLOSE_DELAY=-1", "sa", "", 30000, 60);

        java.sql.Connection conn = ds.getConnection();
        conn.close();
        assertTrue(conn.isClosed());
    }
}

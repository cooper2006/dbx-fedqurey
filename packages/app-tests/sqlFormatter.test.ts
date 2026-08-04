import { strict as assert } from "node:assert";
import { afterEach, test, vi } from "vitest";
import { compressSqlText, formatSqlText, MAX_SQL_FORMAT_CHARS } from "../../apps/desktop/src/lib/sql/sqlFormatter.ts";

afterEach(() => {
  vi.doUnmock("sql-formatter");
});

test("rejects very large SQL before importing formatter", async () => {
  vi.resetModules();
  vi.doMock("sql-formatter", () => {
    throw new Error("formatter should not load");
  });

  const { formatSqlText: isolatedFormatSqlText, MAX_SQL_FORMAT_CHARS: isolatedMaxSqlFormatChars } = await import("../../apps/desktop/src/lib/sql/sqlFormatter.ts");

  await assert.rejects(() => isolatedFormatSqlText("x".repeat(isolatedMaxSqlFormatChars + 1), "generic"), /too large/i);
});

test("formats SQL with uppercase keywords and readable line breaks by default", async () => {
  const formatted = await formatSqlText("select id, name from users where active = 1 order by name", "postgres");

  assert.match(formatted, /^SELECT\b/);
  assert.match(formatted, /\nFROM\b/);
  assert.match(formatted, /\nWHERE\b/);
  assert.match(formatted, /\nORDER BY\b/);
});

test("formats SQL with custom keyword case and indentation settings", async () => {
  const formatted = await formatSqlText("select id from users where active = 1", "postgres", {
    keywordCase: "lower",
    dataTypeCase: "preserve",
    functionCase: "preserve",
    useTabs: true,
    tabWidth: 2,
    logicalOperatorNewline: "before",
    expressionWidth: 50,
    linesBetweenQueries: 1,
    denseOperators: false,
    newlineBeforeSemicolon: false,
  });

  assert.match(formatted, /^select\b/);
  assert.match(formatted, /\nfrom\b/);
  assert.doesNotMatch(formatted, /^SELECT\b/);
});

test("leaves blank SQL unchanged", async () => {
  assert.equal(await formatSqlText("  \n\t", "mysql"), "  \n\t");
});

test("rejects very large SQL before loading formatter work", async () => {
  await assert.rejects(() => formatSqlText("x".repeat(MAX_SQL_FORMAT_CHARS + 1), "generic"), /too large/i);
});

test("compressSqlText collapses whitespace into single spaces", () => {
  const sql = "SELECT   id,\n\t\tname\n  FROM\n   users\nWHERE   active = 1";
  assert.equal(compressSqlText(sql), "SELECT id, name FROM users WHERE active = 1");
});

test("compressSqlText collapses leading/trailing whitespace and trims", () => {
  const sql = "\n\n  SELECT 1  \n\n";
  assert.equal(compressSqlText(sql), "SELECT 1");
});

test("compressSqlText collapses a multi-statement script into one line", () => {
  const sql = "SELECT 1;\nSELECT 2;\n  SELECT 3;";
  assert.equal(compressSqlText(sql), "SELECT 1; SELECT 2; SELECT 3;");
});

test("compressSqlText removes single-line comments", () => {
  const sql = "-- top comment\nSELECT id -- trailing\nFROM users -- table\nWHERE 1 = 1";
  assert.equal(compressSqlText(sql), "SELECT id FROM users WHERE 1 = 1");
});

test("compressSqlText removes block comments and collapses surrounding whitespace", () => {
  const sql = "SELECT id /* inline */ , name /*\n multi\n line */ FROM users";
  assert.equal(compressSqlText(sql), "SELECT id , name FROM users");
});

test("compressSqlText preserves single-quoted string literals including escaped quotes", () => {
  const sql = "SELECT 'hello   world'\n, 'it''s  ok' FROM t";
  assert.equal(compressSqlText(sql), "SELECT 'hello   world' , 'it''s  ok' FROM t");
});

test("compressSqlText does not strip -- inside single-quoted strings", () => {
  const sql = "SELECT 'a -- b'\nFROM t";
  assert.equal(compressSqlText(sql), "SELECT 'a -- b' FROM t");
});

test("compressSqlText preserves double-quoted identifiers including escaped quotes", () => {
  const sql = 'SELECT "my   column"\n, "quote""inside" FROM "my table"';
  assert.equal(compressSqlText(sql), 'SELECT "my   column" , "quote""inside" FROM "my table"');
});

test("compressSqlText preserves backtick-quoted identifiers with internal whitespace", () => {
  const sql = "SELECT `my   col`\nFROM `weird\t name`";
  assert.equal(compressSqlText(sql), "SELECT `my   col` FROM `weird\t name`");
});

test("compressSqlText returns empty/whitespace input unchanged", () => {
  assert.equal(compressSqlText(""), "");
  assert.equal(compressSqlText("   \n\t "), "   \n\t ");
});

test("compressSqlText leaves already-compressed SQL unchanged", () => {
  const sql = "SELECT id FROM users WHERE active = 1";
  assert.equal(compressSqlText(sql), sql);
});

test("compressSqlText keeps SQL executable: commas stay adjacent-free via single space", () => {
  const sql = "SELECT a\n  , b\n  , c FROM t";
  assert.equal(compressSqlText(sql), "SELECT a , b , c FROM t");
});

// ── 方言感知回归测试 ──

test("MySQL: preserves /*! ... */ executable comment content", () => {
  const sql = "SELECT /*! SQL_CALC_FOUND_ROWS */ id\nFROM users";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT /*! SQL_CALC_FOUND_ROWS */ id FROM users");
});

test("MySQL: preserves /*!50000 ... */ versioned executable comment", () => {
  const sql = "SELECT /*!50000 id, */ name\nFROM t";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT /*!50000 id, */ name FROM t");
});

test("MySQL: preserves /*+ ... */ optimizer hint", () => {
  const sql = "SELECT /*+ NO_RANGE_OPTIMIZATION(t) */ id\nFROM t";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT /*+ NO_RANGE_OPTIMIZATION(t) */ id FROM t");
});

test("MySQL: removes ordinary /* ... */ block comment while preserving executable comments", () => {
  const sql = "SELECT /* plain */ /*! KEEP */ id\nFROM t";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT /*! KEEP */ id FROM t");
});

test("MySQL: preserves backslash escapes inside single-quoted strings", () => {
  const sql = "SELECT 'it\\'s  ok'\n, 'tab\\there' FROM t";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT 'it\\'s  ok' , 'tab\\there' FROM t");
});

test("MySQL: backslash escape prevents premature string termination", () => {
  // \' should not end the string; the real closing quote is the last one
  const sql = "SELECT 'a\\'b'\nFROM t";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT 'a\\'b' FROM t");
});

test("MySQL: preserves backslash escapes inside double-quoted strings", () => {
  const sql = 'SELECT "a\\" -- still a string"\nFROM t';
  assert.equal(compressSqlText(sql, "mysql"), 'SELECT "a\\" -- still a string" FROM t');
});

test("MySQL: removes # line comments", () => {
  const sql = "SELECT 1 # comment\n+ 2";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT 1 + 2");
});

test("MySQL: does not treat -- without following whitespace as a comment", () => {
  const sql = "SELECT 1--2\nFROM t";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT 1--2 FROM t");
});

test("MySQL: executable comments preserve whitespace inside strings", () => {
  const sql = "SELECT /*! CONCAT('a   b',\n'c') */ 1";
  assert.equal(compressSqlText(sql, "mysql"), "SELECT /*! CONCAT('a   b', 'c') */ 1");
});

test("generic dialect does NOT apply MySQL backslash escaping", () => {
  // In standard SQL, backslash is not an escape; the string 'a\' ends at the second quote
  const sql = "SELECT 'a\\' + 1\nFROM t";
  assert.equal(compressSqlText(sql), "SELECT 'a\\' + 1 FROM t");
});

test("PostgreSQL: preserves dollar-quoted string $$...$$", () => {
  const sql = "SELECT $$a -- b\n c$$\nFROM t";
  assert.equal(compressSqlText(sql, "postgres"), "SELECT $$a -- b\n c$$ FROM t");
});

test("PostgreSQL: preserves tagged dollar-quoted string $tag$...$tag$", () => {
  const sql = "SELECT $body$ /* not a comment */\n  x := 1;$body$\nFROM fn()";
  assert.equal(compressSqlText(sql, "postgres"), "SELECT $body$ /* not a comment */\n  x := 1;$body$ FROM fn()");
});

test("PostgreSQL: dollar-quoted string preserves internal $$ without early termination", () => {
  // $$...$$ body may contain $$ only as the closing delimiter; inner content is verbatim
  const sql = "SELECT $$line1\nline2$$\nFROM t";
  assert.equal(compressSqlText(sql, "postgres"), "SELECT $$line1\nline2$$ FROM t");
});

test("PostgreSQL: removes ordinary block comment but preserves dollar-quoted content", () => {
  const sql = "SELECT /* plain */ $$keep me$$\nFROM t";
  assert.equal(compressSqlText(sql, "postgres"), "SELECT $$keep me$$ FROM t");
});

test("PostgreSQL: preserves backslash escapes inside E strings", () => {
  const sql = "SELECT E'a\\' -- still a string'\nFROM t";
  assert.equal(compressSqlText(sql, "postgres"), "SELECT E'a\\' -- still a string' FROM t");
});

test("PostgreSQL: removes nested block comments", () => {
  const sql = "SELECT 1 /* outer /* inner */ still outer */ + 2";
  assert.equal(compressSqlText(sql, "postgres"), "SELECT 1 + 2");
});

test("PostgreSQL: keeps the newline required between adjacent string literals", () => {
  const sql = "SELECT 'foo'\n'bar'";
  assert.equal(compressSqlText(sql, "postgres"), "SELECT 'foo'\n'bar'");
});

test("PostgreSQL: dollar tags inside identifiers are not treated as strings", () => {
  const sql = "SELECT foo$tag$bar -- comment\nFROM t";
  assert.equal(compressSqlText(sql, "postgres"), "SELECT foo$tag$bar FROM t");
});

test("SQL Server: preserves bracket identifiers [weird name]", () => {
  const sql = "SELECT [my   column]\nFROM [order]";
  assert.equal(compressSqlText(sql, "sqlserver"), "SELECT [my   column] FROM [order]");
});

test("SQL Server: preserves escaped ]] inside bracket identifiers", () => {
  const sql = "SELECT [a]]b]\nFROM t";
  assert.equal(compressSqlText(sql, "sqlserver"), "SELECT [a]]b] FROM t");
});

test("generic dialect does NOT treat [ as identifier delimiter", () => {
  // In generic mode, [ is just a normal character
  const sql = "SELECT [1, 2, 3]\nFROM t";
  assert.equal(compressSqlText(sql), "SELECT [1, 2, 3] FROM t");
});

test("compressSqlText preserves unterminated block comments", () => {
  const sql = "DELETE FROM users /* unfinished";
  assert.equal(compressSqlText(sql), sql);
});

// ── 联邦查询格式化测试 ──

test("formats federated SQL with double-quoted identifiers (4-part names)", async () => {
  // 4-part federated reference: connection.database."schema"."table"
  const sql = 'SELECT "id", "connection_name", "db_type" FROM postgresql.ihrcore."public"."database_connection"';
  const formatted = await formatSqlText(sql, "postgres");

  // Should preserve the federated reference
  assert.match(formatted, /postgresql\.ihrcore/);
  assert.match(formatted, /\"public\"\."database_connection"/);
});

test("formats federated SQL with plain identifiers (3-part names)", async () => {
  const sql = 'SELECT u.id, u.name FROM my_pg.mydb.public.users u JOIN my_pg.mydb.public.orders o ON u.id = o.user_id';
  const formatted = await formatSqlText(sql, "postgres");

  // Should preserve federated references
  assert.match(formatted, /my_pg\.mydb\.public\.users/);
  assert.match(formatted, /my_pg\.mydb\.public\.orders/);
});

test("formats federated SQL with mixed plain and quoted identifiers", async () => {
  const sql = 'SELECT p.name, o.total FROM mysql_shop.shop.products p JOIN mysql_shop.shop.orders o ON p.id = o.product_id';
  const formatted = await formatSqlText(sql, "mysql");

  // Should preserve federated references
  assert.match(formatted, /mysql_shop\.shop\.products/);
  assert.match(formatted, /mysql_shop\.shop\.orders/);
});

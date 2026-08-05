import { strict as assert } from "node:assert";
import { test, describe } from "vitest";
import { stripIdentifierQuotes } from "../../apps/desktop/src/lib/sql/identifierQuotes.ts";

describe("stripIdentifierQuotes", () => {
  test("removes double quotes from simple PostgreSQL identifiers", () => {
    const sql = 'SELECT "id", "name" FROM "users"';
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, "SELECT id, name FROM users");
  });

  test("removes double quotes from federated query identifiers", () => {
    const sql = 'SELECT "id", "connection_name" FROM postgresql.ihrcore."public"."database_connection"';
    const result = stripIdentifierQuotes(sql, "postgres");
    // The federated reference should keep its quotes for backend detection
    // But SELECT clause identifiers can have quotes stripped
    assert.equal(result, 'SELECT id, connection_name FROM postgresql.ihrcore."public"."database_connection"');
  });

  test("preserves single-quoted string literals", () => {
    const sql = "SELECT 'hello world', 'it''s ok' FROM users";
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, "SELECT 'hello world', 'it''s ok' FROM users");
  });

  test("preserves quoted identifiers with spaces", () => {
    const sql = 'SELECT "first name" FROM "my table"';
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, 'SELECT "first name" FROM "my table"');
  });

  test("preserves quoted identifiers with special characters", () => {
    const sql = 'SELECT "user-name" FROM "my-table"';
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, 'SELECT "user-name" FROM "my-table"');
  });

  test("preserves dollar-quoted strings in PostgreSQL", () => {
    const sql = "SELECT $$hello world$$ FROM users";
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, "SELECT $$hello world$$ FROM users");
  });

  test("preserves dollar-quoted strings with tags", () => {
    const sql = "SELECT $body$hello$body$ FROM users";
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, "SELECT $body$hello$body$ FROM users");
  });

  test("strips backticks from MySQL identifiers", () => {
    const sql = "SELECT `id`, `name` FROM `users`";
    const result = stripIdentifierQuotes(sql, "mysql");
    assert.equal(result, "SELECT id, name FROM users");
  });

  test("preserves backticks for MySQL identifiers with spaces", () => {
    const sql = "SELECT `first name` FROM `my table`";
    const result = stripIdentifierQuotes(sql, "mysql");
    assert.equal(result, "SELECT `first name` FROM `my table`");
  });

  test("handles empty SQL", () => {
    assert.equal(stripIdentifierQuotes("", "postgres"), "");
  });

  test("handles SQL with no quotes", () => {
    const sql = "SELECT id, name FROM users";
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, sql);
  });

  test("mixed quoted and unquoted identifiers", () => {
    const sql = 'SELECT id, "name", age FROM "users" WHERE "status" = 1';
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, "SELECT id, name, age FROM users WHERE status = 1");
  });

  test("preserves double quotes for reserved keywords used as identifiers", () => {
    // "order" is a reserved keyword but we still strip quotes for consistency
    // The user can add them back if needed
    const sql = 'SELECT "order" FROM "table" WHERE "select" = 1';
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, "SELECT order FROM table WHERE select = 1");
  });

  test("preserves double quotes for federated query references", () => {
    // Federated references should keep their quotes so backend can detect them
    const sql = 'SELECT "id" FROM postgresql.ihrcore."public"."database_connection"';
    const result = stripIdentifierQuotes(sql, "postgres");
    // The federated reference should keep quotes
    assert.equal(result, 'SELECT id FROM postgresql.ihrcore."public"."database_connection"');
  });

  test("strips quotes from non-federated identifiers", () => {
    const sql = 'SELECT "id", "name" FROM "users" WHERE "status" = 1';
    const result = stripIdentifierQuotes(sql, "postgres");
    assert.equal(result, "SELECT id, name FROM users WHERE status = 1");
  });
});

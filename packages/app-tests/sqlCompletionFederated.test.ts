import { strict as assert } from "node:assert";
import { test } from "vitest";
import { buildSqlCompletionItems, type SqlCompletionTable } from "../../apps/desktop/src/lib/sql/sqlCompletion";

const connections = ["sales", "hr"];

const salesTables: SqlCompletionTable[] = [
  { name: "orders", schema: "public", detail: "sales.public.orders" },
  { name: "customers", schema: "public", detail: "sales.public.customers" },
];

const hrTables: SqlCompletionTable[] = [
  { name: "employees", schema: "hr", detail: "hr.hr.employees" },
  { name: "departments", schema: "hr", detail: "hr.hr.departments" },
];

const baseInput = {
  tables: salesTables,
  columnsByTable: new Map(),
  schemas: ["public"],
  federatedConnections: connections,
  federatedTablesByConnection: { sales: salesTables, hr: hrTables },
};

test("offers configured connection names as qualified completions at the top level", () => {
  const items = buildSqlCompletionItems("select * from sa", "select * from sa".length, baseInput);
  const connectionItems = items.filter((i) => i.type === "schema" && i.detail === "Connection");
  assert.ok(connectionItems.length >= 1, `expected at least one connection item, got ${connectionItems.length}`);
  assert.ok(connectionItems.some((i) => i.label === "sales"), "expected 'sales' connection completion");
  assert.equal(connectionItems[0].apply, "sales.");
});

test("completes cross-connection tables once a connection qualifier is typed", () => {
  const items = buildSqlCompletionItems("select * from sales.cu", "select * from sales.cu".length, baseInput);
  const tableItems = items.filter((i) => i.type === "table");
  const labels = tableItems.map((i) => i.label);
  assert.ok(labels.includes("customers"), "expected customers from the sales connection");
  const customers = tableItems.find((i) => i.label === "customers")!;
  assert.match(customers.apply ?? "", /sales/);
});

test("filters federated tables by the typed schema qualifier", () => {
  const items = buildSqlCompletionItems("select * from sales.other", "select * from sales.other".length, baseInput);
  // No sales table lives in the "other" schema, so nothing should complete.
  assert.equal(items.filter((i) => i.type === "table").length, 0);
});

test("does not offer another connection's tables from an unrelated qualifier", () => {
  const items = buildSqlCompletionItems("select * from sales.", "select * from sales.".length, baseInput);
  const labels = items.filter((i) => i.type === "table").map((i) => i.label);
  assert.ok(!labels.includes("employees"), "hr tables must not leak into the sales connection");
});

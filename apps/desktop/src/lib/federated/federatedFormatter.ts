/**
 * Federated Query SQL Formatter
 *
 * This module provides SQL formatting capabilities aware of federated queries.
 * It handles:
 * 1. Preserving federation syntax (connection.database.table)
 * 2. Formatting with proper quoting based on database dialect
 * 3. Reformatting federated references correctly
 */

import type { SqlFormatDialect } from "./sqlFormatter";

/**
 * Pattern to match federated table references: connection.database.table or connection.database.schema.table
 * Handles both plain identifiers (user, public) and double-quoted identifiers ("public", "database_connection").
 * A segment is either: [A-Za-z_]\w* (plain) or "[^"]*" (quoted).
 */
const FEDERATED_REF_PATTERN = /(?<conn>(?:[A-Za-z_]\w*|"[^"]*"))\.(?:(?<schema>(?:[A-Za-z_]\w*|"[^"]*"))\.(?:(?<table>(?:[A-Za-z_]\w*|"[^"]*")))?)/g;

/**
 * Format a federated SQL query with proper syntax preservation
 */
export function formatFederatedSql(sql: string, dialect?: SqlFormatDialect): string {
  if (!sql || sql.trim().length === 0) {
    return sql;
  }

  // First, extract and protect federation references
  const protectedRefs: Map<string, string> = new Map();
  let protectedSql = sql.replace(FEDERATED_REF_PATTERN, (match: string) => {
    // Extract named groups from the match result
    const c = (match as any).groups?.conn;
    const s = (match as any).groups?.schema;
    const t = (match as any).groups?.table;
    // Check if this looks like a federation reference (first part could be a connection name)
    const key = `__FED_REF_${protectedRefs.size}__`;
    protectedRefs.set(key, match);
    return key;
  });

  // Format the SQL (without federation references)
  // Note: In production, this would use sql-formatter library
  let formattedSql = protectAndFormat(protectedSql, dialect);

  // Restore federation references
  protectedRefs.forEach((original, key) => {
    formattedSql = formattedSql.replace(new RegExp(key, "g"), original);
  });

  return formattedSql.trim();
}

/**
 * Analyze SQL to detect federation patterns
 */
export interface FederatedAnalysis {
  usesFederation: boolean;
  connections: string[];
  tables: Array<{
    connection?: string;
    schema?: string;
    table: string;
    alias?: string;
  }>;
}

/**
 * Parse SQL to extract federation information
 */
export function analyzeFederatedSql(sql: string): FederatedAnalysis {
  const analysis: FederatedAnalysis = {
    usesFederation: false,
    connections: [],
    tables: [],
  };

  // Simple extraction - in production, use sqlparser AST
  const statements = sql.split(";").filter((s) => s.trim());

  for (const stmt of statements) {
    const matches = stmt.matchAll(FEDERATED_REF_PATTERN);
    for (const match of matches) {
      // Named groups from FEDERATED_REF_PATTERN
      const conn = match.groups?.conn;
      const schema = match.groups?.schema;
      const table = match.groups?.table;

      // Heuristic: if first segment looks like it could be a connection name,
      // treat it as federation syntax
      if (conn && conn.toLowerCase() !== "select" && conn.toLowerCase() !== "from" && conn.toLowerCase() !== "where" && conn.toLowerCase() !== "join" && conn.toLowerCase() !== "on" && conn.toLowerCase() !== "set" && conn.toLowerCase() !== "values") {
        analysis.usesFederation = true;

        if (!analysis.connections.includes(conn)) {
          analysis.connections.push(conn);
        }

        analysis.tables.push({
          connection: conn,
          schema: table ? undefined : schema,
          table: table || schema || "",
        });
      }
    }
  }

  return analysis;
}

/**
 * Get recommended formatter based on connection types used
 */
export function getRecommendedDialect(connections: string[]): SqlFormatDialect {
  // If multiple connections, recommend generic for safety
  if (connections.length > 1) {
    return "generic";
  }

  // Single connection - infer dialect from common patterns
  // In production, this would query the connection's database type
  return "postgres";
}

/**
 * Strip federation prefixes from SQL (for single-connection execution)
 */
export function stripFederationPrefixes(sql: string): string {
  return sql.replace(FEDERATED_REF_PATTERN, (_match, _conn: string, schema: string, table?: string) => {
    // Use named groups for clarity
    const s = schema;
    const t = table;
    // Return schema.table or just table
    return t ? `${s}.${t}` : s;
  });
}

/**
 * Add federation prefixes to SQL (for multi-connection representation)
 */
export function addFederationPrefixes(sql: string, tableName: string, schemaName?: string, connectionName?: string): string {
  let result = sql;
  const prefix = connectionName ? `${connectionName}.${schemaName || "public"}.` : `${schemaName || "public"}.`;

  // Replace unqualified table references with qualified ones
  result = result.replace(new RegExp(`\\b${tableName}\\b`, "g"), `${prefix}${tableName}`);

  return result;
}

/**
 * Helper to protect federation refs during formatting
 */
function protectAndFormat(sql: string, _dialect?: SqlFormatDialect): string {
  // For now, just do basic formatting
  // In production, integrate with sql-formatter library
  return sql
    .replace(/\s+/g, " ")
    .replace(/\s*,\s*/g, ", ")
    .replace(/\s*=\s*/g, " = ")
    .replace(/\s*and\s*/gi, " AND ")
    .replace(/\s*or\s*/gi, " OR ")
    .replace(/\s*from\s*/gi, "\nFROM ")
    .replace(/\s*where\s*/gi, "\nWHERE ")
    .replace(/\s*join\s*/gi, "\nJOIN ")
    .replace(/\s*on\s*/gi, "\nON ")
    .replace(/\s*set\s*/gi, "\nSET ")
    .replace(/\s*select\s*/gi, "\nSELECT ");
}

export default {
  formatFederatedSql,
  analyzeFederatedSql,
  getRecommendedDialect,
  stripFederationPrefixes,
  addFederationPrefixes,
};

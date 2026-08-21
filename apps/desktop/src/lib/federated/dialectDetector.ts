/**
 * Dialect Auto-Detection for Federated Queries
 *
 * This module provides SQL dialect detection capabilities to ensure
 * proper formatting and syntax validation across different database engines.
 */

export type SqlFormatDialect = "postgres" | "mysql" | "sqlite" | "sqlserver" | "oracle" | "clickhouse" | "duckdb" | "generic";

/**
 * Detect the SQL dialect from a SQL query
 */
export function autoDetectDialect(sql: string): SqlFormatDialect {
  const normalized = sql.toLowerCase().trim();

  // Check for PostgreSQL-specific syntax
  if (/\bpg_\w+\b/.test(normalized) || /\bcstring\b/.test(normalized)) {
    return "postgres";
  }

  // Check for MySQL-specific syntax
  if (/\blimit\s+\d+(?:\s*,\s*\d+)?\b/.test(normalized) && !normalized.includes("top")) {
    return "mysql";
  }

  // Check for SQL Server-specific syntax
  if (/\\btop\s+\d+\s+with\s+ties\b/i.test(normalized) || /\bdatetimeoffset\b/i.test(normalized)) {
    return "sqlserver";
  }

  // Check for Oracle-specific syntax
  if (/\bconnect\s+by\b/i.test(normalized) || /\bsys\.date\b/i.test(normalized)) {
    return "oracle";
  }

  // Check for ClickHouse-specific syntax
  if (/\barrayJoin\b/.test(normalized) || /\bglobal\s+(in|not\s+in)\b/i.test(normalized)) {
    return "clickhouse";
  }

  // Check for DuckDB-specific syntax
  if (/\bqualify\b/i.test(normalized) || /\bunnest\s*\(/i.test(normalized)) {
    return "duckdb";
  }

  // Fallback to generic
  return "generic";
}

/**
 * Get quote characters for identifiers based on dialect
 */
export function getQuoteCharacter(dialect: SqlFormatDialect): string {
  switch (dialect) {
    case "mysql":
    case "clickhouse":
      return "`";
    case "sqlserver":
      return "[";
    case "postgres":
    case "oracle":
    case "sqlite":
    case "duckdb":
    default:
      return '"';
  }
}

/**
 * Quote an identifier according to dialect rules
 */
export function quoteIdentifier(identifier: string, dialect: SqlFormatDialect = "generic"): string {
  const quote = getQuoteCharacter(dialect);

  if (quote === "[") {
    return `${quote}${identifier.replace(/\]/g, "]]")}${quote}`;
  }

  return `${quote}${identifier}${quote}`;
}

/**
 * Format a table reference with proper quoting
 */
export function formatTableReference(connection: string, schema: string, table: string, dialect: SqlFormatDialect): string {
  const quote = getQuoteCharacter(dialect);

  // For MySQL, don't quote the connection part
  if (dialect === "mysql") {
    return `${connection}.${schema}.${table}`;
  }

  // For other dialects, quote schema and table
  const quotedSchema = quote + schema + quote;
  const quotedTable = quote + table + quote;

  return `${connection}.${quotedSchema}.${quotedTable}`;
}

/**
 * Detect if SQL uses federation syntax
 */
export function isFederatedSql(sql: string): boolean {
  // Check for connection.database.table pattern
  const parts = sql.matchAll(/(\w+)\.(\w+)\.(\w+)/g);
  let hasFederation = false;

  for (const match of parts) {
    const [, conn, _schema, table] = match;

    // Skip common keywords that might appear in this pattern
    const skipKeywords = new Set(["select", "from", "where", "join", "on", "set", "values", "insert", "update", "delete", "create", "drop", "alter"]);

    if (!skipKeywords.has(conn.toLowerCase())) {
      hasFederation = true;
      break;
    }
  }

  return hasFederation;
}

/**
 * Get the appropriate formatter configuration for a connection type
 */
export interface FormatterConfig {
  dialect: SqlFormatDialect;
  indentSize: number;
  keywords: string[];
  functions: string[];
}

export function getFormatterConfig(databaseType: string): FormatterConfig {
  const config: Record<string, Partial<FormatterConfig>> = {
    postgres: {
      dialect: "postgres",
      keywords: ["SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "ON", "AND", "OR"],
      functions: ["COUNT", "SUM", "AVG", "MIN", "MAX", "COALESCE", "NULLIF"],
    },
    mysql: {
      dialect: "mysql",
      keywords: ["SELECT", "FROM", "WHERE", "JOIN", "LEFT", "RIGHT", "INNER", "OUTER", "ON", "AND", "OR", "LIMIT", "OFFSET"],
      functions: ["COUNT", "SUM", "AVG", "MIN", "MAX", "CONCAT", "IFNULL"],
    },
    clickhouse: {
      dialect: "clickhouse",
      keywords: ["SELECT", "FROM", "WHERE", "JOIN", "GROUP", "ORDER", "HAVING", "LIMIT"],
      functions: ["arrayJoin", "map"],
    },
  };

  const key = databaseType.toLowerCase();
  const baseConfig = config[key] || config["postgres"];

  return {
    dialect: baseConfig.dialect || "generic",
    indentSize: 2,
    keywords: baseConfig.keywords || [],
    functions: baseConfig.functions || [],
  };
}

export default {
  autoDetectDialect,
  getQuoteCharacter,
  quoteIdentifier,
  formatTableReference,
  isFederatedSql,
  getFormatterConfig,
};

/**
 * SQL Identifier Quote Stripper
 *
 * Removes unnecessary double-quote identifiers (PostgreSQL style) and
 * backtick identifiers (MySQL style) from SQL statements before execution.
 * This makes SQL look cleaner without affecting semantics for simple names.
 *
 * IMPORTANT: This function intentionally does NOT strip quotes from federated
 * query references (connection.database.table) because the backend relies on the
 * original quoted form to detect federation patterns.
 *
 * Preserves:
 * - Single-quoted string literals (with content intact)
 * - Dollar-quoted strings (PostgreSQL)
 * - Bracket identifiers (SQL Server [...])
 * - Qualified federated references (connection.database.table)
 * - Double-quoted identifiers with spaces or special characters
 */

/**
 * Strip unnecessary quotes from SQL identifiers.
 *
 * For PostgreSQL-compatible dialects, removes double quotes from simple
 * identifiers that don't need them (no spaces, special chars, or reserved words).
 * For MySQL, removes backticks from simple identifiers.
 *
 * IMPORTANT: This function preserves quotes around federated query references
 * (connection.database.table) because the backend uses the quoted form to detect
 * federation patterns. Use `protectFederatedRefs` in sqlFormatter.ts for that logic.
 *
 * @param sql - The SQL statement to clean
 * @param dialect - The SQL dialect (default: "postgres")
 * @returns The cleaned SQL with unnecessary identifier quotes removed
 */
export function stripIdentifierQuotes(sql: string, dialect: "postgres" | "mysql" | "generic" | "sqlserver" = "postgres"): string {
  if (!sql || sql.trim().length === 0) return sql;

  // First, protect federated references by replacing them with placeholders
  const federatedRefs: string[] = [];
  const protectedSql = sql.replace(/(?<![\w.])(?:(?:[A-Za-z_]\w*|"[^"]*")\.){2,}(?:[A-Za-z_]\w*|"[^"]*")/g, (match) => {
    federatedRefs.push(match);
    return `\x00FED${federatedRefs.length - 1}\x00`;
  });

  // Now strip quotes from non-federated identifiers
  const result = stripQuotesFromSimpleIdentifiers(protectedSql, dialect);

  // Restore federated references
  return result.replace(/\x00FED(\d+)\x00/g, (_match, idx: string) => {
    return federatedRefs[parseInt(idx)];
  });
}

/**
 * Internal function to strip quotes from simple identifiers
 */
function stripQuotesFromSimpleIdentifiers(sql: string, dialect: "postgres" | "mysql" | "generic" | "sqlserver"): string {
  const result: string[] = [];
  let i = 0;
  const len = sql.length;

  while (i < len) {
    const ch = sql[i];

    // Single-quoted string literals: preserve completely
    if (ch === "'") {
      result.push(ch);
      i++;
      while (i < len) {
        const c = sql[i];
        result.push(c);
        if (c === "'") {
          // Check for escaped quote ('')
          if (sql[i + 1] === "'") {
            result.push(sql[i + 1]);
            i += 2;
            continue;
          }
          i++;
          break;
        }
        i++;
      }
      continue;
    }

    // PostgreSQL dollar-quoted strings: preserve completely
    if (ch === "$" && dialect === "postgres") {
      const tag = matchDollarQuoteTag(sql, i);
      if (tag) {
        result.push(tag);
        i += tag.length;
        const end = sql.indexOf(tag, i);
        if (end < 0) {
          result.push(sql.slice(i));
          break;
        }
        result.push(sql.slice(i, end + tag.length));
        i = end + tag.length;
        continue;
      }
    }

    // Double-quoted identifiers (PostgreSQL style)
    if (ch === '"' && dialect !== "mysql") {
      const identifier = extractDoubleQuotedIdentifier(sql, i);
      if (identifier !== null) {
        // Only strip quotes if the identifier is simple (no spaces, special chars)
        // Simple identifiers: [A-Za-z_][A-Za-z0-9_$]*
        const unquoted = identifier.slice(1, -1);
        if (/^[A-Za-z_][A-Za-z0-9_$]*$/.test(unquoted)) {
          result.push(unquoted);
        } else {
          // Keep the quotes for complex identifiers
          result.push(identifier);
        }
        i += identifier.length;
        continue;
      }
    }

    // Backtick-quoted identifiers (MySQL style)
    if (ch === "`" && dialect === "mysql") {
      const endIdx = sql.indexOf("`", i + 1);
      if (endIdx >= 0) {
        const identifier = sql.slice(i, endIdx + 1);
        const unquoted = identifier.slice(1, -1);
        // Only strip if simple identifier
        if (/^[A-Za-z_][A-Za-z0-9_$]*$/.test(unquoted)) {
          result.push(unquoted);
        } else {
          result.push(identifier);
        }
        i = endIdx + 1;
        continue;
      }
    }

    // SQL Server bracket identifiers: preserve
    if (ch === "[" && dialect === "sqlserver") {
      const endIdx = sql.indexOf("]", i + 1);
      if (endIdx >= 0) {
        result.push(sql.slice(i, endIdx + 1));
        i = endIdx + 1;
        continue;
      }
    }

    // Regular character
    result.push(ch);
    i++;
  }

  return result.join("");
}

/**
 * Extract a double-quoted identifier starting at position i.
 * Returns the full quoted string including quotes, or null if not a valid identifier.
 */
function extractDoubleQuotedIdentifier(sql: string, i: number): string | null {
  if (sql[i] !== '"') return null;

  let j = i + 1;
  while (j < sql.length) {
    if (sql[j] === '"') {
      // Check for escaped quote ("")
      if (sql[j + 1] === '"') {
        j += 2;
        continue;
      }
      // End of identifier
      return sql.slice(i, j + 1);
    }
    j++;
  }
  return null;
}

/**
 * Match a PostgreSQL dollar-quoted string tag at position i.
 * Returns the tag (e.g., "$$", "$tag$") or null.
 */
function matchDollarQuoteTag(sql: string, i: number): string | null {
  if (sql[i] !== "$") return null;

  if (i + 1 < sql.length && sql[i + 1] === "$") {
    return "$$";
  }

  // Tag must start with letter or underscore
  if (!/[A-Za-z_]/.test(sql[i + 1] ?? "")) return null;

  let end = i + 2;
  while (end < sql.length && /[A-Za-z0-9_]/.test(sql[end] ?? "")) {
    end++;
  }

  // Must end with $
  if (sql[end] !== "$") return null;

  return sql.slice(i, end + 1);
}

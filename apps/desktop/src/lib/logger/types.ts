export enum LogLevel {
  DEBUG = "debug",
  INFO = "info",
  WARN = "warn",
  ERROR = "error",
  FATAL = "fatal",
}

export enum LogCategory {
  APP = "app",
  API = "api",
  SQL = "sql",
  CONNECTION = "connection",
  UI = "ui",
  PERFORMANCE = "perf",
  ERROR = "error",
}

export interface LogEntry {
  id: string;
  timestamp: number;
  level: LogLevel;
  category: LogCategory;
  message: string;
  data?: Record<string, unknown>;
  sessionId: string;
}

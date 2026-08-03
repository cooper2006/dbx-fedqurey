export enum ErrorSeverity {
  INFO = "info",
  WARNING = "warning",
  ERROR = "error",
  CRITICAL = "critical",
}

export enum ErrorCategory {
  NETWORK = "network",
  AUTH = "auth",
  VALIDATION = "validation",
  DATABASE = "database",
  TIMEOUT = "timeout",
  CANCELED = "canceled",
  FILE = "file",
  UNKNOWN = "unknown",
}

export interface AppError {
  code: string;
  message: string;
  category: ErrorCategory;
  severity: ErrorSeverity;
  cause?: unknown;
  context?: Record<string, unknown>;
  timestamp: number;
  retryable: boolean;
}

export type ErrorHandler = (error: AppError) => boolean | Promise<boolean>;

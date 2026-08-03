import { appendDebugLog } from "@/lib/backend/debugLog";
import { useToast } from "@/composables/useToast";
import type { AppError, ErrorHandler } from "./types";
import { ErrorCategory, ErrorSeverity } from "./types";

class ErrorManager {
  private handlers: Map<ErrorCategory, ErrorHandler[]> = new Map();
  private globalHandlers: ErrorHandler[] = [];

  use(handler: ErrorHandler) {
    this.globalHandlers.push(handler);
  }

  on(category: ErrorCategory, handler: ErrorHandler) {
    if (!this.handlers.has(category)) {
      this.handlers.set(category, []);
    }
    this.handlers.get(category)!.push(handler);
  }

  async handle(error: unknown, context?: Record<string, unknown>): Promise<AppError> {
    const appError = this.normalize(error, context);
    this.report(appError);

    const categoryHandlers = this.handlers.get(appError.category) || [];
    for (const handler of categoryHandlers) {
      const handled = await handler(appError);
      if (handled) return appError;
    }

    for (const handler of this.globalHandlers) {
      const handled = await handler(appError);
      if (handled) return appError;
    }

    this.defaultHandle(appError);
    return appError;
  }

  private normalize(error: unknown, context?: Record<string, unknown>): AppError {
    const timestamp = Date.now();
    let code = "UNKNOWN_ERROR";
    let message = "Unknown error occurred";
    let category = ErrorCategory.UNKNOWN;
    let severity = ErrorSeverity.ERROR;
    let retryable = false;
    let cause = error;

    if (error instanceof Error) {
      message = error.message;
      cause = error.cause ?? error;

      if (error.name === "TypeError" || error.message.includes("fetch") || error.message.includes("network")) {
        category = ErrorCategory.NETWORK;
        retryable = true;
      } else if (error.message.includes("timeout")) {
        category = ErrorCategory.TIMEOUT;
        code = "TIMEOUT_ERROR";
        retryable = true;
      } else if (error.message.toLowerCase().includes("cancel")) {
        category = ErrorCategory.CANCELED;
        code = "CANCELED_ERROR";
        severity = ErrorSeverity.INFO;
      } else if (error.message.toLowerCase().includes("auth") || error.message.toLowerCase().includes("login")) {
        category = ErrorCategory.AUTH;
        code = "AUTH_ERROR";
      }
    } else if (typeof error === "string") {
      message = error;
      if (message.includes("timeout")) {
        category = ErrorCategory.TIMEOUT;
        code = "TIMEOUT_ERROR";
        retryable = true;
      } else if (message.toLowerCase().includes("cancel")) {
        category = ErrorCategory.CANCELED;
        code = "CANCELED_ERROR";
        severity = ErrorSeverity.INFO;
      }
    }

    if (context?.["operation"]) {
      code = `${String(context["operation"]).toUpperCase().replace(/\W/g, "_")}_ERROR`;
    }

    return {
      code,
      message,
      category,
      severity,
      cause,
      context,
      timestamp,
      retryable,
    };
  }

  private report(error: AppError) {
    const level = error.severity === ErrorSeverity.CRITICAL || error.severity === ErrorSeverity.ERROR ? "error" : "warn";
    appendDebugLog(level, `[DBX][error:${error.category}]`, {
      code: error.code,
      message: error.message,
      context: error.context,
      retryable: error.retryable,
    });
  }

  private defaultHandle(error: AppError) {
    const { toast } = useToast();
    const duration = error.severity === ErrorSeverity.ERROR || error.severity === ErrorSeverity.CRITICAL ? 4000 : 2000;
    toast(error.message, { type: error.severity, duration });
  }
}

export const errorManager = new ErrorManager();

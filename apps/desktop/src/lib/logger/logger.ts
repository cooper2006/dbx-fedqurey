import { appendDebugLog } from "@/lib/backend/debugLog";
import type { LogEntry } from "./types";
import { LogLevel, LogCategory } from "./types";

const SESSION_ID = crypto.randomUUID();
const MAX_BUFFER_SIZE = 1000;
const MAX_PERSISTED_SIZE = 10000;

type DebugLogLevel = "debug" | "info" | "log" | "warn" | "error";

class Logger {
  private buffer: LogEntry[] = [];
  private minLevel: LogLevel = LogLevel.DEBUG;
  private persisted: LogEntry[] = [];

  debug(category: LogCategory, message: string, data?: Record<string, unknown>) {
    this.log(LogLevel.DEBUG, category, message, data);
  }

  info(category: LogCategory, message: string, data?: Record<string, unknown>) {
    this.log(LogLevel.INFO, category, message, data);
  }

  warn(category: LogCategory, message: string, data?: Record<string, unknown>) {
    this.log(LogLevel.WARN, category, message, data);
  }

  error(category: LogCategory, message: string, data?: Record<string, unknown>) {
    this.log(LogLevel.ERROR, category, message, data);
  }

  fatal(category: LogCategory, message: string, data?: Record<string, unknown>) {
    this.log(LogLevel.FATAL, category, message, data);
  }

  private log(level: LogLevel, category: LogCategory, message: string, data?: Record<string, unknown>) {
    if (this.levelValue(level) < this.levelValue(this.minLevel)) return;

    const entry: LogEntry = {
      id: crypto.randomUUID(),
      timestamp: Date.now(),
      level,
      category,
      message,
      data,
      sessionId: SESSION_ID,
    };

    this.consoleOutput(entry);
    const debugLevel: DebugLogLevel = level === LogLevel.FATAL ? "error" : (level as DebugLogLevel);
    appendDebugLog(debugLevel, `[${category.toUpperCase()}]`, message, data);

    this.buffer.push(entry);
    if (this.buffer.length > MAX_BUFFER_SIZE) {
      const toPersist = this.buffer.splice(0, Math.floor(MAX_BUFFER_SIZE / 2));
      this.persist(toPersist);
    }
  }

  private consoleOutput(entry: LogEntry) {
    const prefix = `[${entry.category.toUpperCase()}]`;
    const method = entry.level === LogLevel.ERROR || entry.level === LogLevel.FATAL ? "error" : entry.level === LogLevel.WARN ? "warn" : entry.level === LogLevel.DEBUG ? "debug" : "log";
    console[method](prefix, entry.message, entry.data ?? "");
  }

  private persist(entries: LogEntry[]) {
    this.persisted.push(...entries);
    if (this.persisted.length > MAX_PERSISTED_SIZE) {
      this.persisted = this.persisted.slice(-MAX_PERSISTED_SIZE);
    }
  }

  exportLogs(options?: { level?: LogLevel; category?: LogCategory; startTime?: number; endTime?: number }): LogEntry[] {
    let entries = [...this.buffer, ...this.persisted];

    const level = options?.level;
    const startTime = options?.startTime;
    const endTime = options?.endTime;

    if (level) {
      entries = entries.filter((e) => this.levelValue(e.level) >= this.levelValue(level));
    }
    if (options?.category) {
      entries = entries.filter((e) => e.category === options.category);
    }
    if (startTime !== undefined) {
      entries = entries.filter((e) => e.timestamp >= startTime);
    }
    if (endTime !== undefined) {
      entries = entries.filter((e) => e.timestamp <= endTime);
    }

    return entries.sort((a, b) => b.timestamp - a.timestamp);
  }

  getSessionId() {
    return SESSION_ID;
  }

  setMinLevel(level: LogLevel) {
    this.minLevel = level;
  }

  private levelValue(level: LogLevel): number {
    return {
      [LogLevel.DEBUG]: 0,
      [LogLevel.INFO]: 1,
      [LogLevel.WARN]: 2,
      [LogLevel.ERROR]: 3,
      [LogLevel.FATAL]: 4,
    }[level];
  }
}

export const logger = new Logger();

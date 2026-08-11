export function isTauriRuntime(globalObject: Record<string, unknown> = globalThis as Record<string, unknown>): boolean {
  // In production builds, import.meta.env.TAURI is injected by Vite and is always truthy.
  // At runtime this property is removed by the bundler, so we fall back to the WebView
  // globals that Tauri injects into the page. Using the env variable first avoids a
  // race where the globals have not yet been attached to globalThis at module-evaluation time.
  if (typeof import.meta !== "undefined" && (import.meta as unknown as Record<string, unknown>).TAURI != null) return true;
  return Boolean(globalObject.__TAURI_INTERNALS__ || globalObject.__TAURI__);
}

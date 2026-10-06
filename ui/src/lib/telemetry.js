// Sends UI errors to the backend so they land in logs/telemetry.jsonl next to agent and validation events.
import { invoke } from "@tauri-apps/api/core";

export function logError(kind, data = {}) {
  invoke("log_client", { level: "error", kind, data }).catch(() => {});
}

window.addEventListener("error", (e) =>
  logError("window_error", { message: e.message, source: e.filename, line: e.lineno, col: e.colno, stack: e.error?.stack })
);
window.addEventListener("unhandledrejection", (e) =>
  logError("unhandled_rejection", { reason: String(e.reason), stack: e.reason?.stack })
);

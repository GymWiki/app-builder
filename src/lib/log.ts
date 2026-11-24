import { call } from "./ipc";

type Level = "error" | "warn" | "info" | "debug";

/** Stuurt een logregel naar het logbestand (de Rust-kant redigeert geheimen weg). */
export function log(level: Level, message: string) {
  if (import.meta.env.DEV) console[level === "debug" ? "log" : level](message);
  void call("log_event", { level, message }).catch(() => undefined);
}

export function installGlobalErrorHandlers() {
  window.addEventListener("error", (e) => log("error", `window error: ${e.message}`));
  window.addEventListener("unhandledrejection", (e) =>
    log("error", `unhandled rejection: ${e.reason instanceof Error ? e.reason.message : String(e.reason)}`),
  );
}

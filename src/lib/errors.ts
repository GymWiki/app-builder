import type { TFunction } from "i18next";

/** Fout zoals de Rust-kant die serialiseert: `{ code, message }`. */
export class AppError extends Error {
  readonly code: string;
  constructor(code: string, message: string) {
    super(message);
    this.name = "AppError";
    this.code = code;
  }
}

export function toAppError(e: unknown): AppError {
  if (e instanceof AppError) return e;
  if (e && typeof e === "object" && "code" in e && "message" in e) {
    const o = e as { code: unknown; message: unknown };
    return new AppError(String(o.code), String(o.message));
  }
  return new AppError("unknown", e instanceof Error ? e.message : String(e));
}

export interface FriendlyError {
  title: string;
  fix: string;
  /** Hint voor een gerichte actie in de UI. */
  action?: "setup";
}

/** Vertaalt elke fout naar gewone taal, mét een oplossing (nooit alleen "er ging iets mis"). */
export function friendlyError(e: unknown, t: TFunction): FriendlyError {
  const err = toAppError(e);
  // Een afgewezen waarde (kluis): `message` is de redencode ("pattern", "too_short:6"); de uitleg bevat altijd een volgende stap.
  if (err.code === "rejected") {
    const [reason = "", ...rest] = err.message.split(":");
    return {
      title: t("errors.rejected.title"),
      fix: t(`sec.secrets.errors.${reason}`, { detail: rest.join(":"), defaultValue: t("sec.secrets.errors.generic") }),
    };
  }
  const known = [
    "db",
    "io",
    "not_found",
    "invalid",
    "tool_missing",
    "command_failed",
    "timeout",
    "path_exists",
    "unsafe",
    "pty",
    "keychain",
    "unsupported",
  ];
  const code = known.includes(err.code) ? err.code : "unknown";
  // Alleen bekende tool-id's tonen; technische tekst hoort nooit in een melding voor de gebruiker.
  const candidate = err.message.split(":").pop()?.trim() ?? "";
  const toolName = ["git", "flutter", "android", "claude", "keytool", "fastlane", "browser"].includes(candidate)
    ? t(`tools.${candidate}.name`)
    : t("tools.unknown");
  return {
    title: t(`errors.${code}.title`, { tool: toolName }),
    fix: t(`errors.${code}.fix`, { tool: toolName }),
    action: code === "tool_missing" ? "setup" : undefined,
  };
}

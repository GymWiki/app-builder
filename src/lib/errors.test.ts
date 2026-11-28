import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import en from "@/i18n/en.json";
import { AppError, friendlyError, toAppError } from "./errors";

// Minimale t(): zoekt een sleutel in en.json en vult {{placeholders}} in.
const t = ((key: string, opts?: Record<string, string>) => {
  const v = key.split(".").reduce<unknown>((o, k) => (o as Record<string, unknown>)?.[k], en);
  const s = typeof v === "string" ? v : (opts?.defaultValue ?? key);
  return s.replace(/{{(\w+)}}/g, (_, n: string) => opts?.[n] ?? "");
}) as unknown as TFunction;

describe("toAppError", () => {
  it("accepts the shape Rust serializes", () => {
    const e = toAppError({ code: "tool_missing", message: "required tool is missing: flutter" });
    expect(e).toBeInstanceOf(AppError);
    expect(e.code).toBe("tool_missing");
  });
  it("wraps anything else as unknown", () => {
    expect(toAppError("boom").code).toBe("unknown");
    expect(toAppError(new Error("x")).message).toBe("x");
  });
});

describe("friendlyError", () => {
  it("always gives a title and a fix, never raw technical text", () => {
    const codes = [
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
      "whatever",
    ];
    for (const code of codes) {
      const f = friendlyError({ code, message: "internal: stack trace 0xdead" }, t);
      expect(f.title.length, code).toBeGreaterThan(5);
      expect(f.fix.length, code).toBeGreaterThan(10);
      expect(f.title + f.fix).not.toContain("0xdead");
      expect(f.title + f.fix).not.toMatch(/errors\./); // geen ontbrekende vertaalsleutel
    }
  });
  it("names the missing tool and points to setup", () => {
    const f = friendlyError({ code: "tool_missing", message: "required tool is missing: flutter" }, t);
    expect(f.title).toContain("Flutter SDK");
    expect(f.action).toBe("setup");
  });
});

import { describe, expect, it } from "vitest";
import en from "./en.json";
import nl from "./nl.json";

type Json = { [k: string]: Json | string | string[] };
const flatten = (o: Json, prefix = ""): Record<string, string | string[]> =>
  Object.entries(o).reduce<Record<string, string | string[]>>((acc, [k, v]) => {
    if (typeof v === "object" && !Array.isArray(v)) Object.assign(acc, flatten(v, `${prefix}${k}.`));
    else acc[`${prefix}${k}`] = v;
    return acc;
  }, {});

const a = flatten(en as Json);
const b = flatten(nl as Json);

describe("translations", () => {
  it("EN and NL have exactly the same keys", () => {
    expect(Object.keys(b).sort()).toEqual(Object.keys(a).sort());
  });

  it("lists (setup steps) have equal length in both languages", () => {
    for (const [k, v] of Object.entries(a)) {
      if (Array.isArray(v)) expect((b[k] as string[]).length, k).toBe(v.length);
    }
  });

  it("uses the same {{placeholders}} in both languages", () => {
    const ph = (s: string | string[]) => [...String(s).matchAll(/{{(\w+)}}/g)].map((m) => m[1]).sort();
    for (const k of Object.keys(a)) expect(ph(b[k]!), k).toEqual(ph(a[k]!));
  });

  it("has no empty strings", () => {
    for (const [k, v] of [...Object.entries(a), ...Object.entries(b)]) expect(String(v).trim(), k).not.toBe("");
  });
});

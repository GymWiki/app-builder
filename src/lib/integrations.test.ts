import { describe, expect, it } from "vitest";
import apple from "../../integrations/apple.json";
import googlePlay from "../../integrations/google_play.json";
import supabase from "../../integrations/supabase.json";
import firebase from "../../integrations/firebase.json";
import { parseRejection, validateFile, validateText, type Manifest, type ManifestField } from "./integrations";

const field = (m: unknown, id: string) => (m as Manifest).velden.find((f) => f.id === id) as ManifestField;
const b64 = (o: object) => btoa(JSON.stringify(o)).replace(/=+$/, "").replace(/\+/g, "-").replace(/\//g, "_");
const jwt = (role: string) => `${b64({ alg: "HS256" })}.${b64({ role })}.sig`;

describe("text validation (same rules as the Rust side)", () => {
  it("checks patterns, length and empty input with codes the UI translates", () => {
    expect(validateText(field(apple, "key_id"), "ABCDE12345")).toBeNull();
    expect(validateText(field(apple, "key_id"), "abc")?.code).toBe("pattern");
    expect(validateText(field(apple, "key_id"), "   ")?.code).toBe("empty");
    expect(validateText(field(googlePlay, "keystore_password"), "abc")).toEqual({ code: "too_short", detail: "6" });
    expect(validateText(field(supabase, "url"), "https://a.supabase.co\nhttps://b")?.code).toBe("multiline");
  });

  it("refuses the secret Supabase key in the public field, accepts the publishable and anon keys", () => {
    const f = field(supabase, "publishable_key");
    expect(validateText(f, "sb_publishable_abcdefghijkl")).toBeNull();
    expect(validateText(f, jwt("anon"))).toBeNull();
    expect(validateText(f, jwt("service_role"))?.code).toBe("secret_in_public_field");
    expect(validateText(f, "sb_secret_abcdefghijkl")?.code).toBe("pattern");
    expect(validateText(field(supabase, "secret_key"), jwt("service_role"))).toBeNull();
  });
});

describe("file validation", () => {
  const sa = JSON.stringify({
    type: "service_account",
    project_id: "p1",
    client_email: "bot@p1.iam.gserviceaccount.com",
    private_key: "-----BEGIN PRIVATE KEY-----\nx\n-----END PRIVATE KEY-----",
  });
  it("reads non-secret details from a service account and refuses other JSON files", () => {
    const f = field(googlePlay, "service_account");
    const ok = validateFile(f, "key.json", sa, sa.length);
    expect(ok.rejection).toBeNull();
    expect(ok.derived).toEqual({ client_email: "bot@p1.iam.gserviceaccount.com", project_id: "p1" });
    expect(Object.values(ok.derived).join()).not.toContain("PRIVATE");
    expect(validateFile(f, "key.json", '{"type":"authorized_user"}', 26).rejection?.code).toBe("json_wrong");
    expect(validateFile(f, "key.json", '{"type":"service_account"}', 26).rejection?.code).toBe("json_missing");
    expect(validateFile(f, "key.json", "nope", 4).rejection?.code).toBe("json_invalid");
    expect(validateFile(f, "key.txt", sa, sa.length).rejection?.code).toBe("file_type");
  });

  it("checks the .p8 content, size and binary keystores", () => {
    const p8 = field(apple, "private_key");
    expect(
      validateFile(p8, "AuthKey.p8", "-----BEGIN PRIVATE KEY-----\nabc\n-----END PRIVATE KEY-----", 60).rejection,
    ).toBeNull();
    expect(validateFile(p8, "AuthKey.p8", "hello", 5).rejection?.code).toBe("content");
    expect(validateFile(p8, "AuthKey.p8", null, 5).rejection?.code).toBe("not_text");
    expect(validateFile(p8, "AuthKey.p8", "x", 0).rejection?.code).toBe("empty");
    expect(validateFile(p8, "AuthKey.p8", "x", 600 * 1024).rejection?.code).toBe("too_large");
    expect(validateFile(field(googlePlay, "keystore"), "up.jks", null, 2000).rejection).toBeNull();
    expect(
      validateFile(
        field(firebase, "google_services_json"),
        "google-services.json",
        '{"project_info":{"project_id":"fb1"}}',
        40,
      ).derived,
    ).toEqual({ project_id: "fb1" });
  });
});

describe("rejections from the backend", () => {
  it("splits code and detail", () => {
    expect(parseRejection("too_short:6")).toEqual({ code: "too_short", detail: "6" });
    expect(parseRejection("pattern")).toEqual({ code: "pattern", detail: undefined });
    expect(parseRejection("vault_has_value:apple.private_key").detail).toBe("apple.private_key");
  });
});

describe("manifests", () => {
  it("every field of every manifest has a label and help key and a valid scope", () => {
    for (const m of [apple, googlePlay, supabase, firebase] as unknown as Manifest[]) {
      for (const f of m.velden) {
        expect(f.label).toMatch(/^integrations\./);
        expect(f.hulptekst).toMatch(/^integrations\./);
        expect(["global", "project"]).toContain(f.scope);
        expect(f.guideId ?? null).toBeNull();
        if (f.type === "password") expect(f.geheim).toBe(true);
      }
    }
  });
});

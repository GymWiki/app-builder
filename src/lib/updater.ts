import { call, inTauri } from "./ipc";

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string | null;
  /** Downloadt en installeert; `onProgress` krijgt 0–100 (of `null` als de grootte onbekend is). Daarna start Studio opnieuw. */
  install: (onProgress: (pct: number | null) => void) => Promise<void>;
}

/** Vraagt de update-server of er een nieuwere versie is (handtekening gecontroleerd door de updater-plugin). `null` = bij de nieuwste of niet te bereiken. */
export async function checkForUpdate(): Promise<UpdateInfo | null> {
  if (!inTauri()) {
    // Demo in de browser: alleen met ?mock=update, zodat de rest van de demo niet gestoord wordt.
    if (new URLSearchParams(location.search).get("mock") !== "update") return null;
    return {
      version: "0.2.0",
      current: "0.1.0",
      notes: "Faster preview and a calmer chat.",
      install: async (onProgress) => {
        for (let p = 0; p <= 100; p += 25) {
          onProgress(p);
          await new Promise((r) => setTimeout(r, 250));
        }
      },
    };
  }
  const { check } = await import("@tauri-apps/plugin-updater");
  const u = await check();
  if (!u) return null;
  return {
    version: u.version,
    current: u.currentVersion,
    notes: u.body ?? null,
    install: async (onProgress) => {
      let total = 0;
      let got = 0;
      await u.downloadAndInstall((e) => {
        if (e.event === "Started") {
          total = e.data.contentLength ?? 0;
          onProgress(total ? 0 : null);
        } else if (e.event === "Progress") {
          got += e.data.chunkLength;
          onProgress(total ? Math.min(100, Math.round((got / total) * 100)) : null);
        }
      });
      await call<void>("app_restart");
    },
  };
}

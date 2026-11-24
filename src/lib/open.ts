import { openUrl } from "@tauri-apps/plugin-opener";
import { inTauri } from "./ipc";

/** Opent een webpagina in de standaardbrowser (alleen https). */
export async function openExternal(url: string) {
  if (!url.startsWith("https://")) return;
  if (inTauri()) await openUrl(url);
  else window.open(url, "_blank", "noopener");
}

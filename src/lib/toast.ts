import type { TFunction } from "i18next";
import { toast } from "sonner";
import { friendlyError } from "./errors";
import { log } from "./log";

/** Toont een foutmelding in gewone taal, met oplossing, en logt de technische kant. */
export function toastError(e: unknown, t: TFunction) {
  const f = friendlyError(e, t);
  log("error", `${f.title} — ${e instanceof Error ? e.message : JSON.stringify(e)}`);
  toast.error(f.title, { description: f.fix });
}

import { useTranslation } from "react-i18next";
import { inTauri } from "@/lib/ipc";

const SCENARIOS = ["fresh", "ready", "empty"] as const;

/** Alleen in de browser (zonder desktop-shell): zegt eerlijk dat de backend gesimuleerd is. */
export function DemoBanner() {
  const { t } = useTranslation();
  if (inTauri()) return null;
  const current = new URLSearchParams(location.search).get("mock") ?? "ready";
  const choose = (value: string) => {
    const url = new URL(location.href);
    url.searchParams.set("mock", value);
    url.hash = "#/";
    location.href = url.toString();
  };
  return (
    <div
      role="note"
      className="bg-accent text-accent-foreground t-small flex flex-wrap items-center justify-center gap-x-4 gap-y-1 px-4 py-1.5"
    >
      <span>{t("demo.banner")}</span>
      <label className="flex items-center gap-1.5 font-medium">
        {t("demo.scenario")}
        <select
          value={current}
          onChange={(e) => choose(e.target.value)}
          className="bg-card text-foreground rounded-md border px-1.5 py-0.5"
        >
          {SCENARIOS.map((s) => (
            <option key={s} value={s}>
              {t(`demo.${s}`)}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}

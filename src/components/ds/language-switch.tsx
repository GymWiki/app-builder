import { useTranslation } from "react-i18next";
import { LANGUAGES } from "@/i18n";
import { cn } from "@/lib/utils";

export function LanguageSwitch({ className }: { className?: string }) {
  const { t, i18n } = useTranslation();
  const current = i18n.language.slice(0, 2);
  return (
    <div
      role="radiogroup"
      aria-label={t("language.label")}
      className={cn("bg-muted inline-flex rounded-lg p-0.5", className)}
    >
      {LANGUAGES.map((lng) => (
        <button
          key={lng}
          type="button"
          role="radio"
          aria-checked={current === lng}
          lang={lng}
          aria-label={t(`language.${lng}`)}
          title={t(`language.${lng}`)}
          onClick={() => void i18n.changeLanguage(lng)}
          className={cn(
            "t-small h-7 rounded-md px-2.5 font-semibold uppercase transition-colors duration-[var(--dur)]",
            current === lng ? "bg-card text-foreground shadow-soft" : "text-muted-foreground hover:text-foreground",
          )}
        >
          {lng}
        </button>
      ))}
    </div>
  );
}

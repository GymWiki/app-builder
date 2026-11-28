import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { cn } from "@/lib/utils";

export type DeviceId = "small" | "medium" | "large" | "tablet";
export const DEVICES: Record<DeviceId, { w: number; h: number }> = {
  small: { w: 320, h: 640 },
  medium: { w: 360, h: 740 },
  large: { w: 412, h: 860 },
  tablet: { w: 600, h: 820 },
};

/** Telefoonframe dat altijd in het middenpaneel past (schaalt mee, nooit groter dan 100%). */
export function PhoneFrame({
  device,
  dark,
  children,
}: {
  device: DeviceId;
  dark: boolean;
  children?: React.ReactNode;
}) {
  const { t } = useTranslation();
  const box = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(1);
  const { w, h } = DEVICES[device];
  const bezel = 12;

  useEffect(() => {
    const el = box.current;
    if (!el) return;
    const update = () => {
      const pad = 48;
      const s = Math.min(1, (el.clientWidth - pad) / (w + bezel * 2), (el.clientHeight - pad) / (h + bezel * 2));
      setScale(Math.max(0.3, s));
    };
    update();
    const ro = new ResizeObserver(update);
    ro.observe(el);
    return () => ro.disconnect();
  }, [w, h]);

  return (
    <div ref={box} className="bg-muted/50 relative grid h-full min-h-0 place-items-center overflow-hidden">
      <div
        style={{ width: (w + bezel * 2) * scale, height: (h + bezel * 2) * scale }}
        className="transition-[width,height] duration-[var(--dur)]"
      >
        <div
          role="img"
          aria-label={`${t("workspace.previewLabel")} · ${t(`workspace.device.${device}`)}`}
          style={{
            width: w + bezel * 2,
            height: h + bezel * 2,
            transform: `scale(${scale})`,
            transformOrigin: "top left",
            borderWidth: bezel,
          }}
          className={cn(
            "shadow-pop box-border border-neutral-800 bg-neutral-800",
            device === "tablet" ? "rounded-[2rem]" : "rounded-[2.75rem]",
          )}
        >
          <div
            className={cn(
              "relative h-full w-full overflow-hidden",
              device === "tablet" ? "rounded-[1.25rem]" : "rounded-[2rem]",
              dark ? "bg-neutral-900 text-neutral-100" : "bg-white text-neutral-900",
            )}
          >
            {children}
          </div>
        </div>
      </div>
    </div>
  );
}

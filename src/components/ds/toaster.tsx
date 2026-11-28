import { Toaster as Sonner } from "sonner";
import { resolveDark, useUi } from "@/stores/ui";

/** Toasts met dezelfde tokens als de rest van de app. */
export function Toaster() {
  const theme = useUi((s) => s.theme);
  return (
    <Sonner
      theme={resolveDark(theme) ? "dark" : "light"}
      position="bottom-right"
      toastOptions={{
        classNames: {
          toast: "!rounded-xl !border !border-border !bg-popover !text-popover-foreground !shadow-pop !font-sans",
          description: "!text-muted-foreground",
        },
      }}
    />
  );
}

import { Activity, ListChecks, Square, Trophy, type LucideIcon } from "lucide-react";
import type { TemplateId } from "@/lib/types";

export const TEMPLATES: { id: TemplateId; icon: LucideIcon }[] = [
  { id: "blank", icon: Square },
  { id: "scoreboard", icon: Trophy },
  { id: "checklist", icon: ListChecks },
  { id: "tracker", icon: Activity },
];

/** Icoon van een sjabloon (als component, zodat het niet tijdens het renderen wordt aangemaakt). */
export function TemplateIcon({ id, ...props }: { id: string } & React.ComponentProps<LucideIcon>) {
  const Icon = TEMPLATES.find((t) => t.id === id)?.icon ?? Square;
  return <Icon {...props} />;
}

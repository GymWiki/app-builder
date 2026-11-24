import { Database, GitBranch, Image, LayoutGrid, Palette, Rocket, Settings2, type LucideIcon } from "lucide-react";

export type SectionId = "screens" | "data" | "theme" | "assets" | "settings" | "versions" | "publish";

export const SECTIONS: { id: SectionId; icon: LucideIcon }[] = [
  { id: "screens", icon: LayoutGrid },
  { id: "data", icon: Database },
  { id: "theme", icon: Palette },
  { id: "assets", icon: Image },
  { id: "settings", icon: Settings2 },
  { id: "versions", icon: GitBranch },
  { id: "publish", icon: Rocket },
];

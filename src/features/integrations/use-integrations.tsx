import { useQuery, useQueryClient } from "@tanstack/react-query";
import {
  Apple,
  Database,
  Flame,
  GitPullRequest,
  HardDrive,
  KeyRound,
  Play,
  Plug,
  Triangle,
  type LucideIcon,
} from "lucide-react";
import { api } from "@/lib/ipc";

export const useIntegrations = (projectId: string) =>
  useQuery({ queryKey: ["integrations", projectId], queryFn: () => api.integrationList(projectId), staleTime: 0 });

/** Na elke wijziging ook de checklist verversen: daar hangen punten van de geheimen af. */
export function useRefreshIntegrations(projectId: string) {
  const qc = useQueryClient();
  return () =>
    Promise.all([
      qc.invalidateQueries({ queryKey: ["integrations", projectId] }),
      qc.invalidateQueries({ queryKey: ["checklist", projectId] }),
    ]);
}

const ICONS: Record<string, LucideIcon> = {
  apple: Apple,
  play: Play,
  key: KeyRound,
  database: Database,
  flame: Flame,
  "hard-drive": HardDrive,
  github: GitPullRequest,
  triangle: Triangle,
};
/** Icoon van een integratie, bij naam uit het manifest. */
export function ManifestIcon({ name, className }: { name: string; className?: string }) {
  const Icon: LucideIcon = ICONS[name] ?? Plug;
  return <Icon aria-hidden className={className} />;
}

/** Waar de gebruiker naartoe wordt gebracht (kaart in Integraties of kopje in Geheimen), met een teller zodat dezelfde bestemming opnieuw werkt. */
export interface JumpTarget {
  integration: string;
  field?: string;
  nonce: number;
}

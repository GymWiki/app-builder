import { createContext, useContext } from "react";
import { useTranslation } from "react-i18next";
import { Button } from "@/components/ui/button";

/**
 * Plek voor de latere visuele gids per veld. Nu bestaat er geen gids: een veld met een `guideId` toont een uitgeschakelde
 * knop "Instructie volgt". Later levert een provider een component per gids-id; het veld hoeft dan niet te veranderen:
 *
 *   <GuideRegistry.Provider value={{ render: (id) => <MyGuide id={id} /> }}>…</GuideRegistry.Provider>
 */
export interface GuideRenderer {
  render: ((guideId: string) => React.ReactNode) | null;
}

export const GuideRegistry = createContext<GuideRenderer>({ render: null });

export function GuideSlot({ guideId }: { guideId?: string | null }) {
  const { t } = useTranslation();
  const reg = useContext(GuideRegistry);
  if (!guideId) return null;
  const guide = reg.render?.(guideId);
  if (guide) return <div data-guide={guideId}>{guide}</div>;
  return (
    <Button size="xs" variant="outline" disabled data-guide-slot={guideId}>
      {t("sec.secrets.guideSoon")}
    </Button>
  );
}

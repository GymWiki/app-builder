import { useQuery } from "@tanstack/react-query";
import { api } from "@/lib/ipc";

export const useChecklist = (projectId: string) =>
  useQuery({ queryKey: ["checklist", projectId], queryFn: () => api.checklistEvaluate(projectId), staleTime: 0 });

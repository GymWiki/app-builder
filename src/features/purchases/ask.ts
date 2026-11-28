import { useChat } from "@/features/chat/chat-store";

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));

/** Stelt Claude een vraag via de chat van het project en geeft het laatste antwoord terug (voor "laat de AI het plan maken"). */
export async function askClaude(
  projectId: string,
  prompt: string,
  language: string,
  signal?: { cancelled: boolean },
): Promise<string> {
  const chat = useChat.getState();
  await chat.send(projectId, prompt, null, language, "helper");
  const running = () => useChat.getState().byProject[projectId]?.running ?? false;
  // eerst even wachten tot de run echt gestart is
  for (let i = 0; i < 25 && !running(); i++) await sleep(200);
  const end = Date.now() + 5 * 60_000;
  while (running() && Date.now() < end && !signal?.cancelled) await sleep(300);
  const items = useChat.getState().byProject[projectId]?.items ?? [];
  // Het antwoord met het json-blok gaat voor; anders het laatste antwoord.
  const texts = items.flatMap((it) => (it.kind === "assistant" && it.text.trim() ? [it.text] : []));
  const withJson = [...texts].reverse().find((t) => t.includes("```"));
  const answer = withJson ?? texts[texts.length - 1];
  if (!answer) throw new Error("no answer");
  return answer;
}

/** Haalt het eerste json-blok (of object) uit een antwoord. */
export function jsonFrom(answer: string): unknown {
  const fence = answer.match(/```(?:json)?\s*([\s\S]*?)```/);
  const raw = fence ? fence[1]! : answer.slice(answer.indexOf("{"), answer.lastIndexOf("}") + 1);
  return JSON.parse(raw);
}

export function paywallTweakPrompt(current: object, request: string): string {
  return `STUDIO PAYWALL TWEAK. Do NOT change any file. The user wants this change to the paywall: "${request}".\nCurrent paywall settings (json): ${JSON.stringify(current)}\nAnswer with ONE json code block and nothing else: the full settings object with the same fields, changed only where the request needs it. Allowed values: layout = compact | fullScreen | sheet; keep productIds and featured as they are unless asked. Keep texts short and plain. Never put prices in the texts.`;
}

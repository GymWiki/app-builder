import { Fragment } from "react";

/** Minimale markdown (alinea's, lijsten, **vet**, `code`, ``` blokken). Geen HTML, dus veilig; geen extra dependency. */
function inline(text: string): React.ReactNode[] {
  return text.split(/(`[^`]+`|\*\*[^*]+\*\*)/g).map((part, i) => {
    if (part.startsWith("`") && part.endsWith("`") && part.length > 2)
      return (
        <code key={i} className="bg-muted rounded px-1 py-0.5 font-mono text-[0.9em]">
          {part.slice(1, -1)}
        </code>
      );
    if (part.startsWith("**") && part.endsWith("**") && part.length > 4)
      return <strong key={i}>{part.slice(2, -2)}</strong>;
    return <Fragment key={i}>{part}</Fragment>;
  });
}

type Block =
  | { type: "p"; text: string }
  | { type: "ul" | "ol"; items: string[] }
  | { type: "code"; text: string }
  | { type: "h"; text: string };

export function parseBlocks(src: string): Block[] {
  const blocks: Block[] = [];
  const lines = src.replace(/\r\n/g, "\n").split("\n");
  for (let i = 0; i < lines.length; i++) {
    const line = lines[i]!;
    if (line.trimStart().startsWith("```")) {
      const code: string[] = [];
      i++;
      while (i < lines.length && !lines[i]!.trimStart().startsWith("```")) code.push(lines[i++]!);
      blocks.push({ type: "code", text: code.join("\n") });
    } else if (/^\s*[-*]\s+/.test(line) || /^\s*\d+[.)]\s+/.test(line)) {
      const ordered = /^\s*\d+[.)]\s+/.test(line);
      const re = ordered ? /^\s*\d+[.)]\s+/ : /^\s*[-*]\s+/;
      const items: string[] = [];
      while (i < lines.length && re.test(lines[i]!)) items.push(lines[i++]!.replace(re, ""));
      i--;
      blocks.push({ type: ordered ? "ol" : "ul", items });
    } else if (/^#{1,4}\s+/.test(line)) {
      blocks.push({ type: "h", text: line.replace(/^#{1,4}\s+/, "") });
    } else if (line.trim()) {
      const para = [line];
      while (
        i + 1 < lines.length &&
        lines[i + 1]!.trim() &&
        !/^(\s*[-*]\s|\s*\d+[.)]\s|#{1,4}\s|\s*```)/.test(lines[i + 1]!)
      )
        para.push(lines[++i]!);
      blocks.push({ type: "p", text: para.join(" ") });
    }
  }
  return blocks;
}

export function Markdown({ text }: { text: string }) {
  return (
    <div className="space-y-2 break-words">
      {parseBlocks(text).map((b, i) => {
        switch (b.type) {
          case "code":
            return (
              <pre key={i} className="bg-muted overflow-x-auto rounded-lg p-3 font-mono text-[0.85em] leading-relaxed">
                {b.text}
              </pre>
            );
          case "ul":
            return (
              <ul key={i} className="list-disc space-y-1 pl-5">
                {b.items.map((it, n) => (
                  <li key={n}>{inline(it)}</li>
                ))}
              </ul>
            );
          case "ol":
            return (
              <ol key={i} className="list-decimal space-y-1 pl-5">
                {b.items.map((it, n) => (
                  <li key={n}>{inline(it)}</li>
                ))}
              </ol>
            );
          case "h":
            return (
              <p key={i} className="font-semibold">
                {inline(b.text)}
              </p>
            );
          default:
            return <p key={i}>{inline(b.text)}</p>;
        }
      })}
    </div>
  );
}

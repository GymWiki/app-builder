#!/usr/bin/env node
// Vat de bouwrapporten (`studio/build-report.json`) van een of meer projecten samen in een tabel:
// duur, AI-aanroepen en tokens per fase, pagina's in één keer gelukt, analyse. Gebruik:
//   node scripts/build-report-summary.mjs <projectmap> [<projectmap> ...]
import { readFileSync } from "node:fs";
import { join } from "node:path";

const rows = [];
for (const dir of process.argv.slice(2)) {
  let r;
  try {
    r = JSON.parse(readFileSync(join(dir, "studio", "build-report.json"), "utf8"));
  } catch {
    console.error(`geen bouwrapport in ${dir}`);
    continue;
  }
  const phase = (n) => r.phases.find((p) => p.name === n) ?? { seconds: 0, aiCalls: 0, tokens: 0 };
  const calls = r.phases.reduce((a, p) => a + p.aiCalls, 0);
  const tokens = r.phases.reduce((a, p) => a + p.tokens, 0);
  rows.push({
    project: dir.split("/").filter(Boolean).pop(),
    "duur (s)": r.totalSeconds,
    parallel: r.parallel,
    "AI-aanroepen": calls,
    tokens,
    "tokens kern": phase("core").tokens,
    "tokens pagina's": phase("pages").tokens + phase("pages-retry").tokens,
    "tokens integratie": phase("integrate").tokens,
    "pagina's 1e keer": `${r.pagesFirstTry}/${r.pagesTotal}`,
    "pagina's klaar": `${r.pagesDone}/${r.pagesTotal}`,
    analyse: r.analyzeOk === null ? "niet gedraaid" : r.analyzeOk ? "schoon" : "fouten",
    limiet: r.limited ? "ja" : "nee",
  });
}
console.table(rows);

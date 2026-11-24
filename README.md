# Studio

Een lichtgewicht desktop-app (Tauri 2 + React) waarmee niet-technische mensen een **Flutter-app** (iOS en Android) bouwen door te chatten met hun **eigen Claude Code-installatie**. De gebruiker houdt alle code: een gewone Flutter-projectmap met git. Denk "FlutterFlow, maar alleen AI en veel lichter".

> Status: **Fase 0 (fundament) en Fase 1 (projecten + eerste-start-wizard)** zijn klaar. Chat, preview, linkermenu-secties en publiceren volgen in fase 2–5. Zie [DECISIONS.md](DECISIONS.md) voor gemaakte keuzes en wat nog niet is geverifieerd.

## Principes

- Studio praat **nooit** zelf met de Anthropic-API en leest of bewaart **nooit** Claude-tokens. Alleen de officiële `claude` CLI wordt als subproces gestart; de gebruiker logt zelf in. Studio leest uitsluitend de boolean `loggedIn` uit `claude auth status --json`.
- Geheimen horen alleen in de OS-keychain (komt met de eerste feature die er een nodig heeft), nooit in projectbestanden, logs of chat. Logs gaan door `redact()`.
- Destructieve acties (project en bestanden verwijderen, installaties) vragen altijd bevestiging in de UI.

## Ontwikkelen

Vereisten: Node 22+, Rust (stable, ≥ 1.85) en de [Tauri-vereisten](https://tauri.app/start/prerequisites/) voor je OS.
Linux: `libwebkit2gtk-4.1-dev libgtk-3-dev librsvg2-dev libssl-dev libxdo-dev build-essential`.

```bash
npm install
npm run tauri dev      # de echte desktop-app
npm run dev            # alleen de UI in de browser, met een in-memory mock-backend
                       #   ?mock=fresh (eerste start) | ?mock=ready (3 projecten) | ?mock=empty
npm run check          # typecheck + eslint + prettier + vitest + clippy + cargo test
npm run tauri build    # installers
```

## Structuur

```
src/                    React-frontend
  components/ui/        shadcn/ui-primitives (via de shadcn CLI toegevoegd)
  components/ds/        eigen designsysteem: Card/Panel, StatusBadge, EmptyState, Toaster, thema/taal-schakelaars
  features/             home · setup (wizard) · project (werkruimte) · settings (+ /design)
  i18n/                 en.json + nl.json (pariteit wordt getest); UI-tekst nooit hardcoded (ESLint-regel)
  lib/ipc.ts            alle backend-aanroepen (getypeerd); lib/mock.ts alleen in dev
src-tauri/              Rust-backend
  db.rs                 SQLite (rusqlite), migraties via user_version
  toolchain.rs          Git / Flutter / Android / Claude detectie + versiecontrole
  projects.rs           flutter create + sjabloon + git + STUDIO.md; veilig verwijderen
  pty.rs                ingebouwde terminal (portable-pty) met backlog
  install.rs            vaste lijst begeleide installaties (de UI kiest nooit zelf een commando)
  templates/*.dart      sjablonen: leeg, scorebord, checklist, tracker
```

## Een project

Studio maakt per project een map in `Documenten/Studio/<naam>` met `flutter create`, een sjabloon, `git init` + eerste commit, en twee geheugenbestanden: `STUDIO.md` (wat de app doet, keuzes) en `CLAUDE.md` (importeert `STUDIO.md`, zodat Claude Code het automatisch leest).

# Studio

Een lichtgewicht desktop-app (Tauri 2 + React) waarmee niet-technische mensen een **Flutter-app** (iOS en Android) bouwen door te chatten met hun **eigen Claude Code-installatie**. De gebruiker houdt alle code: een gewone Flutter-projectmap met git. Denk "FlutterFlow, maar alleen AI en veel lichter".

> Status: **Fase 0–10** zijn gebouwd (fundament, projecten + wizard, chat + MCP, live preview, linkermenu, publiceren met checklist-poort en AI-testrun, projectprofiel voor Claude Code, automatisch werkgebied met code-index en controle na elke run, metingen, integraties en een kluis voor geheimen). Android eerst; iOS-publiceren is een ontworpen interface zonder implementatie. Lees _Nog niet geverifieerd_ in DECISIONS.md voordat je erop vertrouwt. Zie [DECISIONS.md](DECISIONS.md) voor gemaakte keuzes en wat nog niet is geverifieerd.

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
  engine/               AiEngine-trait · claude.rs (CLI, stream-json) · stream.rs (parser)
  chat.rs               één chatbeurt: sessie, opslag, MCP-config, engine, automatische momentopname
  mcp.rs                lokale MCP-server (project_status, list_screens, get/set_theme, create_snapshot, bump_version, get_checklist, preview_*)
  preview.rs            flutter run (web-server/apparaat), logparser, hot restart via notify, apparaten
  screenshot.rs         headless Edge/Chrome → PNG voor preview_screenshot
  checklist.rs          checklist-poort (puur, getest) · testrun.rs AI-testrunner + Dart-crawler
  signing.rs · publish.rs  keystore (keytool), App Bundle, Fastlane-upload, iOS-interface
  platform.rs · assets.rs · screens.rs · meta.rs · keychain.rs · integrations.rs  instellingen, iconen, schermenboom, vermelding, geheimen, Supabase/Firebase
  profile.rs · templates.rs  projectprofiel (CLAUDE.md, skills, regels, hooks, rechten, plugin) en de feature-first startbasis
  permissions.rs        bevestigingen in de UI voor netwerk/commando's/buiten-project
  studio_tools.rs       de implementatie van de MCP-tools
  install.rs            vaste lijst begeleide installaties (de UI kiest nooit zelf een commando)
  templates/*.dart      sjablonen: leeg, scorebord, checklist, tracker
```

## Een project

Studio maakt per project een map in `Documenten/Studio/<naam>` met `flutter create`, een sjabloon, `git init` + eerste commit, en twee geheugenbestanden: `STUDIO.md` (wat de app doet, keuzes) en `CLAUDE.md` (importeert `STUDIO.md`, zodat Claude Code het automatisch leest).

## Chat

De chat draait `claude -p --output-format stream-json` in de projectmap (prompt via stdin, sessie hervatten met `--resume`). Studio start Claude met een eigen MCP-server zodat Claude de app kan sturen. Netwerk, onbekende commando's en paden buiten het project vragen eerst bevestiging in de chat. Elke beurt met wijzigingen eindigt met een automatische git-momentopname.

Echte-CLI-rooktest (kost een paar tokens van je eigen account): `cargo test --manifest-path src-tauri/Cargo.toml real_claude_smoke -- --ignored --nocapture`.

## Live preview

De preview draait `flutter run -d web-server` en toont de app in het telefoonframe; elke wijziging in `lib/` herlaadt hem vanzelf. Bouwfouten verschijnen als banner met "Laat Claude dit oplossen". Voor een echte telefoon: knop "Echte telefoon" (USB of draadloos, met uitleg).

Extra rooktests (alleen met echte Chromium): `STUDIO_BROWSER=<pad> STUDIO_BROWSER_NO_SANDBOX=1 cargo test --manifest-path src-tauri/Cargo.toml -- --ignored real_browser real_screenshot_tool`.

## Browserdemo en e2e

`npm run build:demo` bouwt een statische demo (gesimuleerde backend, scenario-kiezer bovenin) in `dist-demo/`. De scripts in `e2e/` draaien de hele UI in Chromium tegen die mock (zie `e2e/README.md`).

## Projectprofiel

Elk project krijgt automatisch een profiel zodat Claude Code snel, goedkoop en Flutter-correct werkt, zonder dat de gebruiker iets instelt: een korte `CLAUDE.md`, path-gebonden regels (`.claude/rules/`), zes kleine skills (`.claude/skills/studio-*`), rechten (deny op `build/`, `.dart_tool/`, `.git/`, gegenereerde bestanden), een formatteer-hook en de officiële `dart-flutter`-plugin. Het profiel is versioned (`profile_version`) en wordt veilig gemigreerd: bestanden van de gebruiker worden nooit overschreven. Ontwikkelaarsmodus: zeven keer klikken op de versieregel in Instellingen.

Echte-tools-tests (Flutter, git en `claude` nodig): `cargo test --manifest-path src-tauri/Cargo.toml -- --ignored real_flutter_templates real_profile_end_to_end real_crawler --nocapture`.

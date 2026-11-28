# Fase 14 — Onboarding: van beschrijving naar complete app (ontwerp ter goedkeuring)

Status: **alleen ontwerp, nog niets gebouwd.** Wacht op "ga door". Hieronder: de flow met schermen (wireframes), het blueprint-schema, de capabilities-catalogus, de bouwpijplijn met tijd- en tokenverwachting, en wat ik eerlijk nog niet weet.

## 1. De flow (volledig scherm, 4 stappen, voortgangsstip, Esc/terug overal, voortgang wordt bewaard)

Elke stap heeft één primaire knop rechtsonder, "Terug" links, en bovenaan stippen ● ○ ○ ○. Alles is bedienbaar met toetsenbord (Tab/Enter, pijltjes in kaartgroepen). De toolchain-installatie loopt op de achtergrond; een smalle balk onderaan toont dat ("Android wordt klaargezet… 3 van 7"), zonder te blokkeren.

### Stap 1 — Het idee
```
 ● ○ ○ ○                                         [Verras me met standaardkeuzes]
 Wat voor app wil je maken en voor wie?
 ┌──────────────────────────────────────────────────────────────┐
 │ Een scorebord voor onze vrijdagavond-spelletjes met vrienden…│
 │                                                              │
 └──────────────────────────────────────────────────────────────┘
 Voorbeelden:  [Hardloopschema] [Boodschappenlijst] [Huishoudboekje] [Klantenkaart]
 Snelstart:    [Scorebord] [Checklist] [Tracker] [Rekentool]      (vult het veld voor)
                                                              [ Volgende → ]
```
- Alleen het tekstveld is verplicht. "Verras me" slaat stap 2–3 over met standaardkeuzes en springt naar het plan.
- Kort/vaag? Geen vragen: plan met expliciete aannames. Maximaal één keuzevraag als chips ("Voor jezelf of ook voor anderen? [Alleen ik] [Met vrienden/team]"), alleen als het antwoord het hele plan verandert.

### Stap 2 — De functies
```
 ○ ● ○ ○
 Dit lijkt me handig voor jouw app (zet aan/uit, pas aan):
 ┌────────────────────────────────────────────────────────────┐
 │ ☑ ⠿ Spelers toevoegen en verwijderen              [bewerk] │
 │ ☑ ⠿ Punten bijhouden per spel                     [bewerk] │
 │ ☑ ⠿ Ranglijst van het seizoen                      [bewerk] │
 │ ☐ ⠿ Grafiek van de punten                         [later ▾]│
 │ ⚠ Live meespelen op meerdere telefoons                     │
 │   Dat kan Studio nog niet (realtime). Wel mogelijk: één    │
 │   telefoon voor iedereen, of delen als afbeelding.  [Neem  │
 │   alternatief]                                             │
 └────────────────────────────────────────────────────────────┘
 [+ Functie toevoegen]                                [ Volgende → ]
```
- De AI stelt de lijst voor (één aanroep, §3); gebruiker herordent, hernoemt, voegt toe, zet "later".
- Niet-ondersteund (buiten `studio/capabilities.json`): uitleg in gewone taal + haalbaar alternatief; nooit stilletjes weglaten.

### Stap 3 — De stijl
```
 ○ ○ ● ○
 Kies een stijl (één klik):
 ┌───────────┐ ┌───────────┐ ┌───────────┐
 │ [mini UI] │ │ [mini UI] │ │ [mini UI] │   mini-voorbeeld = echte themakleuren
 │ Rustig    │ │ Levendig  │ │ Strak     │   + lettertype + UI-kit
 │ teal·Inter│ │ roze·Poppins│ │ blauw·Roboto│
 └───────────┘ └───────────┘ └───────────┘
 Eigen hoofdkleur: [ #______ ]   Logo: [Kies afbeelding…]   (optioneel)
                                                              [ Volgende → ]
```
- Palet en thematokens volgen deterministisch uit kleur/lettertype/kit (de configuratiemotor uit fase 13; `studio/config.json`).

### Stap 4 — Het plan (blueprint) controleren
```
 ○ ○ ○ ●
 Naam:  [Friday Scores ▾]   (suggesties: Vrijdagscore · Spelavond · Puntjes)
 Pagina's                                   Navigatie: (•) Tabbladen ( ) Menu
 ┌───────────────────────────────────────────────────────────┐
 │ ⠿ Start        Overzicht en ranglijst        [hernoem][✕]│
 │ ⠿ Spel         Punten invoeren per ronde     [hernoem][✕]│
 │ ⠿ Spelers      Spelers beheren               [hernoem][✕]│
 │ ⠿ Instellingen Naam, thema                   [hernoem][✕]│
 │ [+ Pagina]                                                │
 └───────────────────────────────────────────────────────────┘
 Bewaart: Spelers (naam, kleur) · Rondes (speler, punten, datum)
 Koppelingen: alleen op dit toestel, geen account.  [ ] Inloggen  [ ] Synchroniseren
 Aannames:  · Alleen lokaal opslaan  [bewerk]   · Nederlandstalig  [bewerk]
                                                    [ Bouw mijn app ]
```
- Inloggen/synchronisatie alleen voorgesteld als een functie het vereist (met uitleg); anders uit.

### Tijdens het bouwen (volledig scherm → daarna project)
Canvas met raster van pagina-miniaturen; pagina's verschijnen één voor één. Tekst: "Pagina 3 van 7 wordt gebouwd · nog ongeveer 4 min" (op basis van gemeten tijden, eerlijk als bereik). Rondkijken in klare pagina's kan. **Annuleren** laat een consistente staat achter (laatste momentopname). Mislukte pagina: één gerichte herpoging, daarna lijst "nog niet klaar: X, Y" met "Opnieuw maken" per pagina.

### Daarna
Project opent op de startpagina met "Hele app" beschikbaar en één tip. Blueprint + scherm-index blijven synchroon; beschrijving en functies voeden store-tekst, privacybeleid en checklist (niet opnieuw invullen).

## 2. Blueprint-schema (`studio/blueprint.json`, versie 1, streng gevalideerd, geen geheimen)

```jsonc
{
  "version": 1,
  "app": { "name": "string ≤30", "purpose": "string ≤300", "audience": "string ≤200", "language": "nl|en|…", "description": "oorspronkelijke tekst ≤4000" },
  "style": { "card": "calm|lively|sleek", "seed": "#RRGGBB", "font": "Inter|Poppins|Lora|Roboto", "kit": "material|cupertino|rounded|minimal", "logo": "relatief pad|null" },
  "navigation": { "kind": "tabs|menu|single", "order": ["pageId", "…"] },
  "pages": [{
    "id": "slug [a-z0-9_]{1,30}", "name": "string ≤30", "purpose": "string ≤200",
    "route": "/pad of '/'", "components": ["korte omschrijving", "…"],
    "models": ["modelId"], "features": ["featureId"], "links": ["pageId"]
  }],
  "models": [{
    "id": "slug", "name": "string",
    "fields": [{ "name": "camelCase", "type": "text|int|double|bool|date|enum|ref", "ref": "modelId|null", "values": ["enum waarden"] }],
    "sample": [ { "veldnaam": "waarde" } ]            // voorbeelddata (preview-modus)
  }],
  "features": [{ "id": "slug", "title": "string", "capability": "catalogus-id", "pages": ["pageId"], "status": "now|later" }],
  "integrations": { "storage": "local|supabase|firebase", "auth": ["email","anonymous"], "purchases": false, "notifications": false },
  "assumptions": [{ "id": "slug", "text": "één regel", "editable": true }]
}
```
Validatieregels: unieke id's; elke verwijzing (pagina↔model↔feature↔nav) bestaat; elke `capability` staat in de catalogus; `storage ≠ local` of `auth ≠ []` alleen als een feature dat vereist; 1–12 pagina's; route-uniek; `sample` past bij de velden; geen velden met geheime namen (`password`, `token`, `secret`). Bij afwijking: één automatische herstelronde (zelfde aanroep met de foutlijst), daarna een nette melding met het plan zoals het is.

## 3. Capabilities-catalogus (`studio/capabilities.json`, als data in de app, versiebeheerd)
Ondersteund (basis): `local_storage`, `lists`, `forms`, `detail_pages`, `search_filter`, `charts`, `timers`, `settings`, `share_text_image`, `theme_choice`, `onboarding_screens`, `calculations`. Optioneel (schakelt koppelingen in): `auth_email`, `auth_anonymous`, `cloud_sync_supabase`, `cloud_sync_firebase`, `notifications_local`, `purchases` (alleen via Aankopen-pagina). Niet ondersteund met alternatief: `realtime_multiplayer` → één toestel / delen; `video_streaming`, `3d`, `ar` → afbeeldingen/lijsten; `custom_native_sdk` → nee/zelf later. Het plan blijft binnen de catalogus; de AI krijgt de catalogus als vaste invoer.

## 4. Bouwpijplijn en verwachting
1. **Scaffold (geen AI, seconden):** sjabloon, `config.json` (thema), routes + lege pagina's feature-first, navigatie, scherm-index, preview-modus, instellingen, `blueprint.json`, `STUDIO.md`. Idempotent vanuit de blueprint.
2. **Kern (1 run, sterk model):** modellen, opslaglaag, voorbeelddata, gedeelde componenten; interface-samenvatting (`studio/core-summary.md`) voor de paginataken.
3. **Pagina's (fan-out):** per pagina een verse sessie met blueprint-uitsnede + kern-samenvatting + stijlregels; lichtste volstaande model volgens de bestaande mapping; max 2 parallel (zet lager bij limieten van het abonnement); gericht opnieuw bij falen (1×).
4. **Integratie:** `flutter analyze`, gericht herstel (bestaande reparatielogica), rooktest op alle pagina's.
5. **Afronden:** screenshots in de preview-cache, momentopname "Eerste versie van je app".

Ruwe verwachting (te meten, nog geen cijfers): planning ~1 aanroep; kern ~1 grote run; N kleine paginataken; analyse/herstel 0–2 ronden. Totale duur dus grofweg: kern 2–5 min + pagina's N × 1–2 min (gedeeltelijk parallel) — **dit is een schatting, niet gemeten**; de echte cijfers komen uit de metingen met 3 voorbeeldbeschrijvingen + 1 met een niet-ondersteunde functie, zoals gevraagd.

## 5. Wat ik eerlijk niet kan beloven / risico's
- De kwaliteit hangt van de gebruikte AI; "complete werkende app in één keer" slaagt niet altijd voor elke pagina — daarom herpoging, rapportage van mislukte pagina's en geen half project zonder melding.
- Echte metingen (duur, tokens) vragen echte runs met de CLI van de gebruiker; in deze omgeving kan ik ze niet draaien zonder ingelogde Claude. Ik kan ze alleen met een nagebootste engine (structuur) of op jouw machine meten.
- Parallel draaien is begrensd door het abonnement; standaard 2.

## 6. Open keuzes voor jou
1. Standaard maximaal 2 pagina's parallel — goed?
2. Plannen met het sterkste model (Opus-klasse) en kern ook; pagina's met het lichtste dat volstaat — akkoord?
3. Taal van de gegenereerde app: de taal van de beschrijving (standaard) — akkoord?

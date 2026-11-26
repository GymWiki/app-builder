# UX-audit: Eén-handeling-principe (Fase 13, opdracht 1)

Status: **alleen audit, nog niets aangepast.** Gebaseerd op het doorlopen van de code en de e2e-scripts. De getallen zijn
tellingen uit de UI-code ("handelingen" = klikken + losse invoervelden die ingevuld moeten worden), **geen metingen met
echte gebruikers**; opdracht 3 voegt de echte meting toe. Rangschikking: frequentie × impact (1 = eerst doen).

Legenda: **Chat** = de gebruiker wordt naar de AI gestuurd (een knop die een prompt in de chat zet, of een open vraag).
**Det.** = kan deterministisch (parsers/sjablonen/CLI), zonder AI.

## Overzicht rangschikking

| # | Flow | Waarom hoog | Belangrijkste winst |
|---|------|-------------|---------------------|
| 1 | Thema | Elke gebruiker, vaak | Hele palet uit één kleur, lettertype/UI-kit deterministisch i.p.v. Claude |
| 2 | Pagina's / preview / selectievak | Dagelijks | Al grotendeels klaar (preview altijd aan); testrun-knop is nog een chatprompt |
| 3 | Data & Auth | Bijna elk project | Nu volledig via een chatprompt: provider-pakket, bestanden en login zijn voor een groot deel deterministisch |
| 4 | Project aanmaken | Elk project | Naam → bundle-ID, taal, voorbeelddata afleiden; 1 beschrijving → eerste preview |
| 5 | Instellingen (platform) | Elk project | Al deterministisch; afleidbare velden (bundle-ID uit naam) en dubbele invoer met Publiceren |
| 6 | Assets | Elk project | Icoon is deterministisch; splash en afbeeldingen nog via Chat |
| 7 | Publiceren + checklist | Eenmalig per release, hoge impact | Preflight met "Los voor mij op"; afleidbare items vooraf groen |
| 8 | Aankopen / paywall | Minder vaak, complex | Producten/prijzen al formulier; code via paginaflow (bewust AI) |
| 9 | Geheimen en Integraties | Eenmalig | Auto-detectie, directe links, voorinvullen |
| 10 | Versies | Elke release | Al automatisch; changelog nog via Chat |
| 11 | Eerste-start-wizard + CLI-login | Eenmalig | Zelfde installatiepad, minder uitleg; portal-stappen blijven handmatig |
| 12 | Testrun | Voor elke release | Start via Chat-achtige taak; hoort een achtergrondtaak met voortgang te zijn |

## Per flow

### 1. Thema
| Onderdeel | Nu |
|---|---|
| Stappen | Kleur kiezen (1 klik + hex of picker) → opgeslagen (`theme_set`, schrijft seed, **Det.**). Als het project het thema nog niet gebruikt: knop "Koppelen" = **Chat**. Lettertype kiezen = **Chat** (prompt per font). UI-kit kiezen = **Chat**. |
| Handelingen | Kleur: 2–3. Lettertype: 2 + wachten op Claude + chatbeurt. UI-kit: idem. |
| Chat / dubbel | Lettertype, UI-kit en "koppelen" gaan altijd via de chat. |
| Det. mogelijk | Alles: `google_fonts`-pakket + `pubspec`-regel + thematokens-bestand schrijven; UI-kit als sjabloon; thema "koppelen" is een vaste wijziging in `main.dart` (parser/sjabloon). Palet (primair, secundair, achtergrond, donker) uit één kleur (Material-3 seed-algoritme zit al in Flutter `ColorScheme.fromSeed`; tokens deterministisch schrijven). |
| Afleidbaar | Volledig palet + donker thema uit één hoofdkleur; tekstkleur-contrast. |
| Doodlopend | Als koppelen faalt blijft de kleur opgeslagen maar zonder effect, zonder uitleg. |
| Voorstel | `studio/config.json` → `theme.{seed,font,kit}`; toepasser schrijft `lib/core/theme_tokens.dart`; autosave + hot reload; geen knop. Doel: **1 handeling**. |

### 2. Pagina's, preview, selectievak, testrun
| Onderdeel | Nu |
|---|---|
| Pagina's | Lijst = preview (sinds Preview-altijd-aan). "Schermen verbinden" en "Verbinden" = **Chat** (bewust: navigatie ontwerpen vraagt maatwerk; wel als achtergrondtaak met structuur als invoer). |
| Preview | 0 handelingen om te starten. Fout → kaart met "Laat Claude dit oplossen" (**Chat** met foutregels; prima, maar zou een achtergrondtaak met voortgang moeten zijn). |
| Selectievak | 1 slepen; Studio zoekt de widgets zelf (**Det.**); de wens typen in de chat is hier legitiem (vrije wens). |
| Testrun | De knop "Start testrun" start nu de taak en springt naar Publiceren → Test. Het "los op"-pad gaat via Chat. |
| Voorstel | Fix-acties als achtergrondtaak met voortgang/annuleren; geen chatbericht vereist. |

### 3. Data & Auth
| Onderdeel | Nu |
|---|---|
| Stappen | Provider kiezen → auth-methodes aanvinken → token plakken → "Instellen" = **Chat** (`dataSetupPrompt`). Daarna volgt Claude's werk in de chat. |
| Handelingen | ≈ 5–8 + chat-uitvoering. |
| Chat / dubbel | De instelling is pas klaar als Claude klaar is; vragen van Claude in de chat zijn mogelijk. Vragenlijst (heeft accounts?) in Publiceren vraagt deels hetzelfde als auth-methodes (**dubbel**). |
| Det. mogelijk | `flutter pub add supabase_flutter`, init-bestand, `.env`-vrije configuratie (publishable key mag in code, secret nooit), login-scherm uit sjabloon, platform-callbacks (deep link in Manifest/Info.plist via de bestaande parsers). Alleen eigen tabellen/logica = AI. |
| Afleidbaar | `hasAccounts` uit gekozen auth-methodes; URL/anon-key uit het gekoppelde Supabase-project (MCP/CLI) i.p.v. plakken. |
| Doodlopend | Ontbrekend token → melding zonder direct invulveld op die plek. |
| Voorstel | Declaratief `data.{provider,auth[]}` in config; toepasser doet pakketten + sjablonen; AI alleen voor maatwerk (achtergrondtaak). Doel: **≤ 3 handelingen** (provider, methodes, token). |

### 4. Project aanmaken
| Onderdeel | Nu |
|---|---|
| Stappen | Naam (verplicht) + beschrijving + sjabloon kiezen → Maken (6 vaste stappen met voortgang, **Det.**). |
| Handelingen | 3–4 en daarna de eerste chatwens. Eerste preview start vanzelf (sinds Preview-altijd-aan). |
| Dubbel | Beschrijving wordt later opnieuw gevraagd in Publiceren (store-tekst). |
| Afleidbaar | Bundle-ID uit naam (nu alleen in Instellingen, later), taal uit systeem, voorbeelddata uit sjabloon, store-tekst uit beschrijving. |
| Voorstel | Beschrijving hergebruiken als store-basistekst; bundle-ID bij aanmaken afleiden (`nl.<ontwikkelaar>.<naam>`, ontwikkelaar uit globale standaard). Doel: **1 beschrijving → eerste preview** (is al zo, mits flutter/Chrome aanwezig). |

### 5. Instellingen (platform)
| Onderdeel | Nu |
|---|---|
| Stappen | App-naam, bundle-ID, minimale versies, permissies: elk veld **Det.** (parsers voor Manifest/Info.plist/gradle), eigen "Toepassen". |
| Handelingen | Per veld 2 (invullen + toepassen); permissie = 1 klik + uitleg-tekst voor iOS. |
| Dubbel | Bundle-ID/naam ook in Publiceren (Apple/Google-vermelding). |
| Det. mogelijk | Alles al; ontbreekt: autosave i.p.v. knop, afleiden bundle-ID uit naam, iOS-permissietekst voorstellen per permissie (vaste teksten). |
| Voorstel | Toepassers op config; autosave met validatie (formaat bundle-ID, beschikbaarheid in stores = niet check-baar zonder account → melden). |

### 6. Assets
| Onderdeel | Nu |
|---|---|
| Stappen | Icoon kiezen → alle formaten genereren (`generate_icons`, **Det.**). Splash en afbeeldingen: knoppen = **Chat**. |
| Voorstel | Splash: `flutter_native_splash` + kleur uit het thema is Det. (config `splash.{color,image}`); afbeeldingen optimaliseren is Det. (formaat/gewicht). Chat alleen voor "ontwerp een logo". |

### 7. Publiceren en checklist
| Onderdeel | Nu |
|---|---|
| Stappen | Tabbladen: Checklist, Test, Vermelding, Juridisch, Ondertekening, Release. Vermelding genereren, Juridisch genereren en ondertekening-hulp = **Chat**. Checklist toont wat ontbreekt, per item een sprong. |
| Handelingen | Ruim 20 invullen/klikken voor de eerste release; veel dubbel met Instellingen en Data (naam, accounts, privacy-vragen). |
| Chat / dubbel | Teksten, privacybeleid, ondertekening via Chat; "heeft accounts" en "aankopen" worden apart gevraagd terwijl Data en Aankopen het al weten. |
| Det. mogelijk | Store-teksten uit beschrijving (sjabloon + lengtelimieten valideren), privacy-/voorwaardenpagina uit sjabloon met ingevulde gegevens (de **AI blijft optioneel voor taalgebruik**), keystore genereren (`keytool`, al deels), screenshots maken met de preview (Chrome) per pagina in de gevraagde formaten, versie/buildnummer (klaar). |
| Eerlijk | Niet automatiseerbaar: Apple Developer/Play Console-account, betalen, identiteitscontrole, uploaden naar App Store Connect/Play (portalstappen), juridische toestemming. Daar: auto-detectie van de status, directe links naar het juiste scherm in de portal, voorinvullen (kopieerbare velden). |
| Voorstel | Preflight-framework: één lijst controles met per item "Los voor mij op" (Det. fix of achtergrondtaak) of sprong naar het veld; checklist = preflight-resultaat. |

### 8. Aankopen en paywall
| Onderdeel | Nu |
|---|---|
| Stappen | Producten/abonnementen als formulier (prijzen uit de store), permanente product-ID's, paywall-pagina's via de paginaflow (**Chat**, bewust: code), checklist-koppeling. |
| Voorstel | Toepassers voor niet-creatieve delen (`pub add purchases`, `purchases.json` → vaste Dart-constanten); paywall-ontwerp = achtergrondtaak met producten als gestructureerde invoer, zonder open vragen. Store-producten aanmaken blijft een expliciete knop met samenvatting. Preflight vóór release. |

### 9. Geheimen en Integraties
| Onderdeel | Nu |
|---|---|
| Stappen | Integratie aanzetten → velden uit het manifest invullen (kluis); lekcontrole met "Vraag Claude" (**Chat**) per bestand. |
| Voorstel | Lek opruimen als Det. waar het kan (waarde naar kluis, verwijderen uit bestand + `.gitignore`), anders achtergrondtaak. Directe links naar de pagina waar het token gehaald wordt (staat al deels in de manifesten) en plakveld op de plek van het probleem. |

### 10. Versies
Automatisch (volgend uploadnummer) + handmatig dialoog: **Det.**, klaar. Changelog genereren = **Chat** → achtergrondtaak met commits als invoer (kan ook Det. uit `git log` als kale lijst).

### 11. Eerste-start-wizard en CLI-login
| Onderdeel | Nu |
|---|---|
| Stappen | Detectie van tools (Flutter, Git, Java/Android SDK, Claude CLI); per ontbrekende tool installatie-knop (`install.rs`) of uitleg; inloggen met de eigen CLI via een ingebouwde terminal (de gebruiker logt zelf in; Studio raakt tokens niet aan). |
| Eerlijk | Inloggen en licentieaccepties (Android SDK, Xcode) blijven handmatig; wel: auto-detectie na installatie en één "Alles installeren"-knop waar het veilig kan. |

### 12. Testrun
Start vanuit Publiceren/Preview; resultaten in de checklist. Fixen = **Chat** (`fixPrompt`). Voorstel: achtergrondtaak met voortgang en annuleren; falende stappen één voor één met "Los voor mij op".

## Doorlopende bevindingen

- **Dubbele invoer:** app-naam/beschrijving (Nieuw project ↔ Instellingen ↔ Vermelding), "heeft accounts" (Data ↔ Vragenlijst), "aankopen" (Aankopen ↔ Vragenlijst), ontwikkelaar/e-mail (Vermelding ↔ Juridisch ↔ Integraties).
- **Instellingen die via de chat lopen:** thema-koppeling, lettertype, UI-kit, data-instellen, splash, afbeeldingen, changelog, vermelding/juridisch genereren, lekken opruimen, testfouten, pagina's verbinden.
- **Geen momentopname/terugdraaien** bij Det. instellingen buiten de chat (thema, platform): alleen chatrun heeft snapshots. Elke toepasser moet een snapshot nemen.
- **Geen globale standaardwaarden** over projecten heen (accounts, merkkleur, lettertype, UI-kit, taal).

## Voorgestelde volgorde van uitvoeren (opdracht 2)

1. **Gedeelde laag:** `studio/config.json` + reconciler + toepassers (Rust, `config.rs`), afleidregels op één plek (`derive.rs`: naam→bundle-ID, kleur→palet), snapshot per toepassing.
2. **Thema** aansluiten (hoogste frequentie, kleinste risico): kleur/lettertype/UI-kit/koppelen deterministisch; doel 1 handeling.
3. **Preflight-framework** + checklist erop (Publiceren eerst, daarna Aankopen/Data/Testrun).
4. **Achtergrondtaken** (voortgang, annuleren, gewone-taal-fouten) voor de overgebleven AI-taken met gestructureerde invoer.
5. Data & Auth, Instellingen, Assets, Talen, Aankopen op de configuratiemotor.
6. Globale standaardwaarden; meting en UX-tests (opdracht 3).

Niet-doelen / eerlijke grenzen: store-portals, accounts, betalingen en juridische goedkeuring zijn niet te automatiseren;
creatief werk (eigen schermen, eigen logica, logo-ontwerp) blijft AI, maar dan als achtergrondtaak met gestructureerde invoer
en zonder open vragen.

# End-to-end tests (browser, gesimuleerde backend)

Deze scripts draaien de echte UI in Chromium tegen de mock-backend (`src/lib/mock.ts`), zonder Tauri.

```bash
npm i -D playwright            # eenmalig; Chromium: npx playwright install chromium
npm run dev &                  # poort 1420
node e2e/sections-and-publish.cjs ./e2e-out   # tweede argument: map voor schermafbeeldingen
```

Elk script eindigt met een niet-nul exitcode als een controle mislukt. De fout "create project failed … flutter"
in `setup-and-projects.cjs` is het bewust nagebootste foutpad en wordt door dat script als console-fout geteld.

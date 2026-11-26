# Experiment: selectievak → widget via de Flutter widget inspector

Wegwerp-scripts (geen onderdeel van Studio) waarmee de koppeling is gemeten. Zie DECISIONS.md, "Experiment: vak → widget".

- `probe/`: klein testproject (knoppen, tekst, lijst, dialoog, gedeeld `PrimaryButton` in `lib/core/`).
- `chrome-wrap.sh`: start Chrome headless voor `flutter run -d chrome --no-web-resources-cdn --web-port=8123 --web-browser-debug-port=9333`.
- `exp.js`: 46 tikken op het web-voorbeeld, grondwaarheid uit de semantics-boom (onafhankelijk van de inspector). Resultaten: `results-web.json`.
- `t8.js`/`t9.js`: hot reload, regelverschuiving en hot restart.
- `desk.js`: Linux-desktop onder Xvfb met `xdotool`.
- `vm.js`: minimale VM-service-client (Node 22).

Paden en poorten zijn die van de sandbox; pas ze aan om ze te hergebruiken.

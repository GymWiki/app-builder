/// Studio draait de live preview met `--dart-define=STUDIO_PREVIEW=true`. Alleen dan is dit `true`; in een release-build is het altijd `false`.
///
/// In de preview moet elke pagina direct iets zinnigs tonen:
/// - een inlog- of omleidingsregel slaat zichzelf over: `if (kStudioPreview) return null;`
/// - een repository geeft voorbeelddata terug: `kStudioPreview ? sampleItems : await api.items()`
const bool kStudioPreview = bool.fromEnvironment('STUDIO_PREVIEW');

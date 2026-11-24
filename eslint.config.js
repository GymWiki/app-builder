import js from "@eslint/js";
import globals from "globals";
import reactHooks from "eslint-plugin-react-hooks";
import tseslint from "typescript-eslint";

export default tseslint.config(
  { ignores: ["dist", "src-tauri", "node_modules", "src/components/ui/**"] },
  js.configs.recommended,
  ...tseslint.configs.recommended,
  {
    files: ["src/**/*.{ts,tsx}"],
    languageOptions: { globals: globals.browser },
    plugins: { "react-hooks": reactHooks },
    rules: {
      ...reactHooks.configs.recommended.rules,
      "@typescript-eslint/no-unused-vars": ["error", { argsIgnorePattern: "^_" }],
      // Geen hardgecodeerde UI-tekst in JSX: alles via t().
      "no-restricted-syntax": [
        "error",
        { selector: "JSXText[value=/[A-Za-z]{3,}/]", message: "Hardcoded UI text: use t('…') from react-i18next." },
      ],
    },
  },
  // Het designsysteem-overzicht toont bewust Engelstalige voorbeeldlabels.
  { files: ["src/features/settings/design-page.tsx"], rules: { "no-restricted-syntax": "off" } },
);

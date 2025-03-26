import { defineConfig } from "eslint/config";
import globals from "globals";
import js from "@eslint/js";


export default defineConfig([
  { files: ["**/*.{js,mjs,cjs}"] },
  { files: ["**/*.{js,mjs,cjs}"], languageOptions: { globals: globals.browser } },
  { files: ["**/*.{js,mjs,cjs}"], plugins: { js }, extends: ["js/recommended"] },

  // Those don't play well with `k6`
  { files: ["**/*.{js,mjs,cjs}"], rules: { "no-unused-vars": "off" } },
  { files: ["**/*.{js,mjs,cjs}"], rules: { "no-undef": "off" } },
]);

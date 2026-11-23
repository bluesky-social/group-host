// The messages (messages/en.json) as functions, `#lib/paraglide`, each
// in the language a cookie names, else the one the browser asks for,
// else English: the plugin for the console's build and the shell's, which
// both read them. `npm run i18n` compiles the same, for the checks.

import { paraglideVitePlugin } from '@inlang/paraglide-js';
import { fileURLToPath } from 'node:url';

const here = (path: string) => fileURLToPath(new URL(path, import.meta.url));

export const paraglide = () =>
  paraglideVitePlugin({
    project: here('./project.inlang'),
    outdir: here('./src/lib/paraglide'),
    strategy: ['cookie', 'preferredLanguage', 'baseLocale']
  });

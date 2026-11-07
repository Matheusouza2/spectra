// Language source folder.
// Each language lives in its own directory (locales/<code>/<namespace>.json).
// To add a language: create the folder, add its namespace files and register it in LANGUAGES.
export const DEFAULT_LANGUAGE = 'en-US';

export const LANGUAGES = [
  { code: 'en-US', label: 'English', short: 'EN' },
  { code: 'pt-BR', label: 'Português (Brasil)', short: 'PT' },
];

const modules = import.meta.glob('./*/*.json', { eager: true });

export const messages = {};
for (const [path, mod] of Object.entries(modules)) {
  const [, lang, file] = path.match(/^\.\/([^/]+)\/([^/]+)\.json$/);
  messages[lang] ??= {};
  messages[lang][file] = mod.default ?? mod;
}

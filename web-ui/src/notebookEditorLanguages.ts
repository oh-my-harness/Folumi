import { LanguageDescription } from '@codemirror/language'

export const notebookEditorLanguages = [
  LanguageDescription.of({
    name: 'JavaScript',
    alias: ['js', 'jsx'],
    extensions: ['js', 'jsx', 'mjs', 'cjs'],
    load: () => import('@codemirror/lang-javascript').then(({ javascript }) => javascript({ jsx: true })),
  }),
  LanguageDescription.of({
    name: 'TypeScript',
    alias: ['ts', 'tsx'],
    extensions: ['ts', 'tsx', 'mts', 'cts'],
    load: () => import('@codemirror/lang-javascript').then(({ javascript }) => javascript({ jsx: true, typescript: true })),
  }),
  LanguageDescription.of({
    name: 'Python',
    alias: ['py'],
    extensions: ['py'],
    load: () => import('@codemirror/lang-python').then(({ python }) => python()),
  }),
  LanguageDescription.of({
    name: 'Rust',
    alias: ['rs'],
    extensions: ['rs'],
    load: () => import('@codemirror/lang-rust').then(({ rust }) => rust()),
  }),
  LanguageDescription.of({
    name: 'Markdown',
    alias: ['md'],
    extensions: ['md', 'markdown'],
    load: () => import('@codemirror/lang-markdown').then(({ markdown }) => markdown()),
  }),
  LanguageDescription.of({
    name: 'JSON',
    extensions: ['json', 'jsonc'],
    load: () => import('@codemirror/lang-json').then(({ json }) => json()),
  }),
  LanguageDescription.of({
    name: 'YAML',
    alias: ['yml'],
    extensions: ['yaml', 'yml'],
    load: () => import('@codemirror/lang-yaml').then(({ yaml }) => yaml()),
  }),
  LanguageDescription.of({
    name: 'SQL',
    extensions: ['sql'],
    load: () => import('@codemirror/lang-sql').then(({ sql }) => sql()),
  }),
  LanguageDescription.of({
    name: 'CSS',
    extensions: ['css'],
    load: () => import('@codemirror/lang-css').then(({ css }) => css()),
  }),
  LanguageDescription.of({
    name: 'HTML',
    extensions: ['html', 'htm'],
    load: () => import('@codemirror/lang-html').then(({ html }) => html()),
  }),
]

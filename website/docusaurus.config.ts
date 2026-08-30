// Docs site configuration for SeatTrellis (席序).
//
// The Markdown source of truth stays at the repository-root `docs/` directory
// (referenced by README/CHANGELOG relative links); this site config points the
// classic docs preset at that directory via `path: '../docs'`.

import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'SeatTrellis Documentation',
  tagline: 'A privacy-first, local-only classroom seating planner',
  favicon: 'assets/favicon.svg',

  url: 'https://frankfu916.github.io',
  baseUrl: '/seattrellis/',

  organizationName: 'FrankFu916',
  projectName: 'seattrellis',

  onBrokenLinks: 'throw',
  markdown: {
    hooks: {
      onBrokenMarkdownLinks: 'warn',
    },
    mermaid: false,
  },

  i18n: {
    // The Chinese docs currently live beside the English source files rather
    // than in Docusaurus's translated-content tree. Use the curated /zh/ page
    // until those files are ready for a real zh-Hans locale.
    defaultLocale: 'en',
    locales: ['en'],
    localeConfigs: {
      en: {
        label: 'English',
        htmlLang: 'en',
      },
    },
  },

  presets: [
    [
      'classic',
      {
        docs: {
          path: '../docs',
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          // Docs live at the repository-root docs/ directory (outside this
          // site dir), so the edit URL must be computed from the doc path
          // relative to docs/ — a plain editUrl prefix would produce
          // `edit/main/../docs/...` URLs that normalize to a 404 on GitHub.
          editUrl: ({ docPath }) =>
            `https://github.com/FrankFu916/seattrellis/edit/main/docs/${docPath}`,
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      } satisfies Preset.Options,
    ],
  ],

  plugins: ['./plugins/schemas-publish.mjs'],

  themes: [
    [
      // Local, offline-capable search (parity with the MkDocs `search`
      // plugin): the index is built at site-build time and queried fully
      // client-side — no external service, matching the privacy-first and
      // offline-by-default boundary. The optional `open-ask-ai` peer is
      // intentionally NOT installed (no AI features in the product UI).
      '@easyops-cn/docusaurus-search-local',
      {
        hashed: true,
        language: ['en', 'zh'],
        docsDir: ['../docs'],
        docsRouteBasePath: ['/'],
        indexBlog: false,
      },
    ],
  ],

  themeConfig: {
    image: 'assets/demo-seating.png',
    navbar: {
      title: 'SeatTrellis',
      logo: {
        alt: 'SeatTrellis',
        src: 'assets/logo.svg',
        width: 32,
        height: 32,
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docs',
          position: 'left',
          label: 'Docs',
        },
        {
          type: 'dropdown',
          label: 'Language',
          className: 'navbar-language',
          position: 'right',
          items: [
            {
              label: 'English',
              to: '/',
            },
            {
              label: '简体中文',
              to: '/zh/',
            },
          ],
        },
        {
          href: 'https://github.com/FrankFu916/seattrellis',
          label: 'GitHub',
          position: 'right',
        },
      ],
    },
    footer: {
      style: 'dark',
      copyright: `Copyright © ${new Date().getFullYear()} Frank Fu. Apache-2.0.`,
      links: [
        {
          title: 'Documentation',
          items: [
            { label: 'English', to: '/' },
            { label: 'Simplified Chinese', to: '/zh/' },
          ],
        },
        {
          title: 'Project',
          items: [
            {
              label: 'GitHub',
              href: 'https://github.com/FrankFu916/seattrellis',
            },
          ],
        },
      ],
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'bash', 'toml', 'json'],
    },
  },
};

export default config;

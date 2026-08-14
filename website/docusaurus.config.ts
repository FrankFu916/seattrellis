// Docs site configuration for SeatTrellis (席序).
//
// The Markdown source of truth stays at the repository-root `docs/` directory
// (referenced by README/CHANGELOG relative links); this site config points the
// classic docs preset at that directory via `path: '../docs'`.

import { themes as prismThemes } from 'prism-react-renderer';
import type { Config } from '@docusaurus/types';
import type * as Preset from '@docusaurus/preset-classic';

const config: Config = {
  title: 'SeatTrellis · 席序',
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
    defaultLocale: 'zh-Hans',
    locales: ['zh-Hans'],
  },

  presets: [
    [
      'classic',
      {
        docs: {
          path: '../docs',
          routeBasePath: '/',
          sidebarPath: './sidebars.ts',
          editUrl: 'https://github.com/FrankFu916/seattrellis/edit/main/',
        },
        blog: false,
        theme: {
          customCss: './src/css/custom.css',
        },
      },
    ] satisfies Preset.Options,
  ],

  plugins: ['./plugins/schemas-publish.mjs'],

  themeConfig: {
    image: 'assets/demo-seating.png',
    navbar: {
      title: 'SeatTrellis',
      logo: {
        alt: 'SeatTrellis',
        src: 'assets/favicon.svg',
      },
      items: [
        {
          type: 'docSidebar',
          sidebarId: 'docs',
          position: 'left',
          label: '文档',
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
    },
    prism: {
      theme: prismThemes.github,
      darkTheme: prismThemes.dracula,
      additionalLanguages: ['rust', 'bash', 'toml', 'json'],
    },
  },
};

export default config;

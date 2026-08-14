import type { SidebarsConfig } from '@docusaurus/plugin-content-docs';

/**
 * Documentation sidebar, mirroring the mkdocs.yml navigation. Internal
 * decision evidence (product-decisions, ADRs, audits, prototypes, ui-themes)
 * is intentionally NOT listed here — it lives in docs-internal/ and is never
 * published.
 */
const sidebars: SidebarsConfig = {
  docs: [
    {
      type: 'category',
      label: '快速开始',
      collapsible: true,
      items: ['quickstart.zh', 'quickstart.en'],
    },
    {
      type: 'category',
      label: 'CLI',
      collapsible: true,
      items: ['cli', 'versioning'],
    },
    {
      type: 'category',
      label: 'Web 端',
      collapsible: true,
      items: ['web.zh', 'web.en'],
    },
    {
      type: 'category',
      label: '输入格式',
      collapsible: true,
      items: ['input-format.zh', 'input-format.en'],
    },
    {
      type: 'category',
      label: '规则',
      collapsible: true,
      items: ['rules.zh', 'rules.en'],
    },
    {
      type: 'category',
      label: 'Presets',
      collapsible: true,
      items: ['presets'],
    },
    {
      type: 'category',
      label: 'Project 工作流',
      collapsible: true,
      items: ['project.zh'],
    },
    {
      type: 'category',
      label: '多方案与评分',
      collapsible: true,
      items: ['candidates', 'scoring'],
    },
    {
      type: 'category',
      label: '导出',
      collapsible: true,
      items: ['export.zh', 'font-strategy.zh'],
    },
    {
      type: 'category',
      label: '历史分析',
      collapsible: true,
      items: ['history', 'pair-history'],
    },
    {
      type: 'category',
      label: '隐私',
      collapsible: true,
      items: ['privacy'],
    },
    {
      type: 'category',
      label: '故障排除',
      collapsible: true,
      items: ['troubleshooting'],
    },
    {
      type: 'category',
      label: '开发者',
      collapsible: true,
      items: [
        'architecture',
        'roadmap',
        'testing',
        'benchmarks',
        'benchmark-baseline-v1.4',
        'native-core',
        'editor-protocol',
        'api',
        'publishing',
        'release-checklist',
        'development',
        'rust-migration',
      ],
    },
  ],
};

export default sidebars;

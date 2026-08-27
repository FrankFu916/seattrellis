import { existsSync } from 'node:fs';
import path from 'node:path';
import type { SidebarsConfig } from '@docusaurus/plugin-content-docs';

/**
 * The sidebar is the public English navigation contract. IDs intentionally
 * have no language suffix. During the docs rename, the source check lets the
 * site omit a section until its English canonical source exists; once the
 * docs agent uses names such as `quickstart.md`, the same IDs are selected.
 */
type DocsSection = {
  type: 'category';
  label: string;
  collapsible: true;
  items: string[];
};

const docsRoot = path.resolve(__dirname, '../docs');

function hasEnglishSource(id: string): boolean {
  return ['.md', '.mdx'].some((extension) =>
    [id, `${id}.en`].some((stem) =>
      existsSync(path.join(docsRoot, `${stem}${extension}`)),
    ),
  );
}

function availableEnglishDocs(ids: readonly string[]): string[] {
  return ids.filter(hasEnglishSource);
}

function section(label: string, ids: readonly string[]): DocsSection | undefined {
  const items = availableEnglishDocs(ids);
  return items.length > 0
    ? { type: 'category', label, collapsible: true, items }
    : undefined;
}

const docsSidebar: (string | DocsSection)[] = [
  ...availableEnglishDocs(['index']),
  section('Getting Started', ['quickstart']),
  section('CLI', ['cli', 'versioning']),
  section('Web Workbench', ['web']),
  section('Input & Rules', ['input-format', 'rules', 'presets']),
  section('Project Workflow', ['project']),
  section('Candidates & Scoring', ['candidates', 'scoring']),
  section('Export', ['export', 'font-strategy']),
  section('History & Rotation', ['history', 'pair-history']),
  section('Privacy', ['privacy']),
  section('Troubleshooting', ['troubleshooting']),
  section('Developer Reference', [
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
  ]),
].filter((item): item is string | DocsSection => item !== undefined);

const sidebars: SidebarsConfig = {
  docs: docsSidebar,
};

export default sidebars;

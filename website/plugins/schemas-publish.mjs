// Docusaurus plugin: publish the committed JSON Schemas at /schemas/ so the
// schema files' `$id` URLs resolve (replaces mkdocs_hooks.py on_post_build).
//
// Mirrors the MkDocs hook: copy repo-root `schemas/*.schema.json` into the
// build output's `schemas/` directory.

import path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

export default function schemasPublishPlugin() {
  return {
    name: 'schemas-publish',
    async postBuild({ outDir }) {
      const repoRoot = path.resolve(__dirname, '..', '..');
      const sourceDir = path.join(repoRoot, 'schemas');
      const destDir = path.join(outDir, 'schemas');
      fs.mkdirSync(destDir, { recursive: true });
      for (const name of fs.readdirSync(sourceDir)) {
        if (name.endsWith('.schema.json')) {
          fs.copyFileSync(path.join(sourceDir, name), path.join(destDir, name));
        }
      }
    },
  };
}

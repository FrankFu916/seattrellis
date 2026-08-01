# SeatTrellis Web Workbench

This directory contains the independent React workbench. It is intentionally
separate from the current Streamlit application so both experiences can coexist
during migration.

The browser workbench keeps the ordinary teacher flow simple: roster import,
custom or preset room selection, visual classroom editing, combined seating
preferences and common constraints, generation, manual adjustment, project
backup, and export. The
Generate step has a collapsed **Advanced settings** section for candidate count,
seed, time limit, backend selection, and complete custom rules JSON. The
Streamlit page and CLI remain supported for file-level configuration and
existing projects.

## Local preview

```bash
npm install
npm run dev
```

The development server forwards `/api/v1` to `http://127.0.0.1:8765`. When the
local SeatTrellis service is not running, the workbench uses a bundled demo
class automatically. A production build uses relative asset paths and can be
served from a Python wheel or desktop package:

```bash
npm run build
npm run preview
```

The generated `dist/` directory is copied into `src/seattrellis/web_static/`
when updating the bundled Python and desktop clients.

## Quality checks

```bash
npm run typecheck
npm test
```

All visible copy lives in `src/i18n/messages.ts`. Themes are CSS-token based and
live in `src/styles/tokens.css`; components do not contain theme-specific
styling.

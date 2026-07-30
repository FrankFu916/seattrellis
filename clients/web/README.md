# SeatTrellis Web Workbench

This directory contains the independent React workbench. It is intentionally
separate from the current Streamlit application so both experiences can coexist
during migration.

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

## Quality checks

```bash
npm run typecheck
npm test
```

All visible copy lives in `src/i18n/messages.ts`. Themes are CSS-token based and
live in `src/styles/tokens.css`; components do not contain theme-specific
styling.


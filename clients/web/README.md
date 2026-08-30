# SeatTrellis Web Workbench

This directory contains the interactive React 19 web workbench for [SeatTrellis (席序)](https://github.com/FrankFu916/seattrellis), designed for classroom seating management, real-time manual fine-tuning, and multi-format exports.

---

## 🚀 Features

- **Step-by-Step Seating Wizard**: Student roster upload (CSV/XLSX), classroom layout designer, rule & preset selection, solving, and exporting.
- **Interactive Seating Canvas**: Click-to-swap, move to empty desk, lock seats/students, atomic batch moves, and full undo/redo support.
- **Dual Export Templates**: One-click toggle between Teacher Internal Copy and Public Classroom Posting (with automated fail-closed privacy anonymization).
- **Multi-Period Rotation**: Schedule and preview future seating rotations with historical zone distribution and desk-mate repetition metrics.
- **Class Project Management**: Local workspace indexing, plan comparison, and `.seattrellis.zip` backup/restore.

---

## 💻 Local Development

### 1. Install Dependencies
```bash
npm install
```

### 2. Start Development Server
```bash
npm run dev
```
The dev server runs at `http://localhost:5173` and proxies API requests to `http://127.0.0.1:8765`. If the backend service is not running, the workbench automatically falls back to an offline demo classroom.

### 3. Build Production Bundle
```bash
npm run build
npm run preview
```
The compiled assets in `dist/` are embedded directly into the Rust server binary (`seattrellis-server` / `seattrellis_web`) at build time.

---

## 🧪 Quality Checks

```bash
# Typecheck TypeScript definitions
npm run typecheck

# Run Vitest test suite
npm test
```

- **Localization**: UI text strings are managed in `src/i18n/messages.ts` (supporting Simplified Chinese and English).
- **Design Tokens**: Theme variables and styling rules are centralized in `src/styles/tokens.css`.

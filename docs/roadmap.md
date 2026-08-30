# Product Roadmap

[English](roadmap.md) · [简体中文](roadmap.zh.md)

SeatTrellis 2.0 established the product's local-first foundation. This roadmap describes the next product investments and the privacy constraints that govern them; it is directional rather than a promise of release dates.

## Delivered in 2.0

- A native Rust solver shared by the CLI, local web service, and Tauri desktop app.
- A guided teacher workflow for roster import, classroom layout, rule configuration, candidate comparison, manual editing, and multi-period rotation.
- Eight local export formats: HTML, print HTML, SVG, PNG, PDF, XLSX, DOCX, and PPTX.
- Deterministic solving, structured diagnostics, project backups, and privacy-aware public exports.
- An English-first documentation site deployed as static content on GitHub Pages.

## Near-term priorities

1. **Advanced layout editing** — selection boxes, batch seat movement, better touch interactions, and guided irregular-layout creation.
2. **Change visualization** — seat-movement heatmaps, neighbor-history graphs, and clearer comparisons between candidates and periods.
3. **Web distribution proof of concept** — evaluate a browser-local WebAssembly execution path without weakening the project's privacy claims.
4. **Accessibility and responsive design** — keyboard coverage, screen-reader semantics, reduced-motion support, and improved tablet layouts.
5. **Signed desktop releases** — macOS notarization and Windows signing when the required publisher infrastructure is available.

## Web deployment and privacy architecture

The documentation site is fully static and can be hosted safely on GitHub Pages, Vercel, Netlify, or Cloudflare Pages. The application workbench is different: today it calls a loopback-only Rust API, so uploading the current `dist` directory to a static host would produce a UI without a solver.

### Preferred direction: browser-local WebAssembly

A WebAssembly edition is the best match for SeatTrellis's privacy model. The static UI and solver can be delivered by any of the hosts above while rosters and project files remain inside the user's browser.

The proof of concept should:

- expose a narrow, versioned solve/validate/export interface from Rust through WebAssembly;
- run expensive work in a Web Worker so the interface stays responsive;
- keep inputs in memory by default and make IndexedDB persistence explicit and opt-in;
- disable telemetry, remote logging, and third-party analytics;
- ship a strict Content Security Policy and automated tests that fail on unexpected network requests;
- document which export formats are browser-capable, since filesystem and native-font features need browser-specific replacements.

This path requires engineering work: several crates currently assume native threads, filesystem access, system fonts, or server APIs. A small end-to-end solver slice should be proven before promising full format parity.

### Alternative: ephemeral hosted compute

Vercel, Netlify, and Cloudflare can host a UI plus short-lived compute, but this changes the trust boundary because student data reaches a third-party server. GitHub Pages cannot provide this backend itself. Any hosted-compute design would require per-request isolation, authenticated and unguessable identifiers, memory-only processing, disabled body logging, strict size/time limits, immediate cleanup, and a public retention policy. “Delete after processing” reduces retention risk but is not equivalent to local-only processing.

For that reason, hosted compute is not the default roadmap direction. A self-hosted Rust service remains suitable for organizations that knowingly accept and control that boundary.

## Toolchain policy

The web application uses TypeScript 7 because its Vite toolchain supports it and the migration is verified by type checks, unit tests, and browser E2E tests. The Docusaurus documentation site remains on TypeScript 6 until its toolchain officially supports TypeScript 7. Upgrading the docs compiler alone offers little user-visible value, so compatibility is more important than version uniformity.

## Explicit non-goals

- A centralized student-data store or mandatory user-account system.
- Third-party advertising, behavioral analytics, or commercial telemetry.
- Claiming that server-side deletion provides the same privacy guarantee as browser-local computation.

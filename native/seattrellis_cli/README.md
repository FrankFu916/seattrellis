# seattrellis_cli

A single-file native command-line solver and exporter for
[SeatTrellis](https://github.com/FrankFu916/seattrellis) classroom seating. The
release binary is ~1.6 MB with no runtime dependencies.

This is the compact native CLI surface, not yet a drop-in replacement for the
Python CLI. It currently focuses on solving a versioned JSON problem and
exporting a solved plan; validation, history, project, schema migration, and
full candidate-report commands remain on the migration roadmap.

```
seattrellis_cli solve --problem problem.json [--seed N] [--output result.json]
seattrellis_cli export --problem problem.json --solution result.json --format svg|html|png|pdf --output plan.svg
```

- `solve` runs the cost-ranked solver from `seattrellis_core` and prints a
  feasibility/cost summary.
- `export` renders the solved plan as a self-contained SVG, inline HTML, PNG,
  or a hand-written single-page PDF.
- Colored help and results when the terminal supports it; plain text when
  piped.

Licensed under Apache-2.0.

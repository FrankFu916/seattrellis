# seattrellis_cli

A single-file command-line solver and exporter for [SeatTrellis](https://github.com/FrankFu916/seattrellis)
classroom seating. The release binary is ~1.6 MB with no runtime dependencies.

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

# Troubleshooting

**SeatTrellis v2.0.0 is released.**

## Run diagnostics first

```bash
seattrellis doctor
```

`doctor` prints the binary name, version, core API version, and a temporary
directory writability check. It exits with code `2` when the temporary directory
cannot be written. `seattrellis --version` prints only the version.

## Common problems

### The rules are infeasible

Run `validate` or `project-validate` and inspect fixed seats,
must/cannot-adjacent pairs, minimum distances, disabled seats, and the enabled
seat count. Soft objectives do not make a hard-rule validation fail. Exit code
`3` means `ProvenInfeasible`; exit code `5` means `Unknown` because the heuristic
search did not establish a result. They are not interchangeable.

### Excel cannot be read

Only `.xlsx` and `.xlsm` are supported. Save legacy `.xls` files as `.xlsx` or
CSV first. Excel import reads the first worksheet and rejects oversized,
encrypted, or formula-without-cached-value workbooks.

### Chinese text is missing in PDF or PNG

PDF and PNG rasterize text with a locally discovered system font. If no usable
CJK font can be loaded, the file is still produced but text is omitted and a
warning is emitted. Install a CJK font, for example:

```bash
# Debian/Ubuntu
sudo apt-get install fonts-noto-cjk

# CentOS/RHEL
sudo yum install google-noto-sans-cjk-fonts
```

See [Font strategy](font-strategy.md). HTML and print HTML use browser font
fallback instead.

### The web workbench will not start

`seattrellis_web` binds to `127.0.0.1:8765` by default. If that port is in use,
choose another local port:

```bash
seattrellis_web --port 8766 --open-browser
```

When running from a source checkout, build the embedded frontend first:

```bash
cd clients/web && npm ci && npm run build
```

Do not expose the service to a LAN or an untrusted network.

### Migration fails

Use `schema-migrate` or the Project panel for supported v1 roster, layout, and
project inputs. A preview validates before writing, and replacement creates a
backup. Unknown artifact kinds or newer schema versions are rejected; the
original file is not silently changed.

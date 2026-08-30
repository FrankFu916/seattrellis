# Security Policy / 安全策略与漏洞通报

SeatTrellis values the privacy and security of educational data above all else.

---

## 🔒 1. Reporting Security Vulnerabilities

Please **do not disclose security vulnerabilities or paste sensitive classroom information in public GitHub Issues or Pull Requests**.

If you discover a security vulnerability, privacy leak, or potential vector of concern, please report it privately to the maintainers via the repository owner's security contact channels. If private contact info is unavailable, open a minimal public issue requesting a private security contact without disclosing vulnerability details.

---

## 🛡️ 2. Strict Prohibition on Real Student Data

To protect minor and student privacy, **never include real student personal information in any public contributions**:
- Real student names, student IDs, phone numbers, or addresses;
- Academic test scores, rankings, vision records, health accommodations, or behavioral notes;
- Real school, teacher, or parent contact details;
- Snapshots, CSV/Excel rosters, or export files derived from actual school rosters.

All sample files in `examples/` and test fixtures in `fixtures/` must remain **100% fictional**.

---

## 💻 3. Local-First Security Architecture

- **Offline Processing**: SeatTrellis operates entirely on the user's local machine and never sends classroom data to remote cloud servers.
- **Local Network Boundary**: The background web service binds strictly to `127.0.0.1` and requires dynamic 256-bit session token authentication for all API operations.
- **Private Data Storage**: Keep real rosters, history files, and outputs in `.gitignore`-protected directories (such as `private/`, `data/`, or `outputs/`).

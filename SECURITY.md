# Security Policy

## Reporting Security Issues

Please do not report security or privacy issues by pasting sensitive classroom data into a public GitHub Issue.

If you find a vulnerability or privacy problem, contact the maintainer privately through the repository owner's preferred contact channel. If private contact information is not available yet, open a minimal public Issue that says you need a private security contact, without including sensitive data.

## Sensitive Data

Do not include any of the following in public Issues, Pull Requests, screenshots, logs, fixtures, or examples:

- real student names or IDs;
- grades, rankings, vision records, health notes, or seating preferences;
- class names, school names, teacher names, or parent contact information;
- JSON snapshots generated from real classroom data;
- Excel, CSV, PNG, HTML, or other exports generated from real classroom data.

## Local-First Design

SeatTrellis processes data locally by default and does not upload classroom data to a cloud service. Users are responsible for keeping real data in ignored private folders such as `private/`, `data/`, `outputs/`, `exports/`, or `snapshots/`.

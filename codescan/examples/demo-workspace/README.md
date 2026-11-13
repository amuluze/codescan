# demo-workspace

An intentionally vulnerable sample workspace used to produce the screenshots in the repository README.

- It is **not** a real project and must never be scanned by CI or used as a baseline.
- The repository excludes `examples/` from its own scans via the root `.codescanignore`, so these findings do not pollute the project's gate.

Files here trigger representative findings across the three dimensions: security (dynamic execution, shell injection, weak hash, hardcoded credential), business logic (missing ownership check), and quality (long function, deep nesting, TODO debt).

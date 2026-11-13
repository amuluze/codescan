# website/ conventions

Static site for CodeScan. See `website/README.md` for layout and deployment. This file supplements
`../AGENTS.md`; the root file owns shared SDD workflow, product boundaries and validation routing.

- **No build step.** Plain HTML/CSS/JS only; do not introduce a bundler or framework without an
  explicit decision in `.specs/tasks/`.
- **No external requests.** No CDNs, no web fonts, no analytics. The site must render offline.
- **Facts come from the implementation.** Rule counts, language coverage and capability claims must
  match `codescan/crates/engine/rules/*.yaml` and `codescan/apps/desktop/src/`; re-check them when the engine changes.
- **Assets are copies.** `website/assets/brand/` and `website/assets/img/` mirror
  `codescan/assets/brand/` and `codescan/assets/screenshots/`; update both.
- **Theming through variables.** Use the CSS custom properties in `:root`; keep the light-theme
  override block in sync.
- **Accessibility.** Semantic landmarks, labelled navigation, alt text on images, sufficient
  contrast in both colour schemes.

# website/ conventions

Static site for CodeScan. See `website/README.md` for layout and deployment.

- **No build step.** Plain HTML/CSS/JS only; do not introduce a bundler or framework without an
  explicit decision in `.specs/tasks/`.
- **No external requests.** No CDNs, no web fonts, no analytics. The site must render offline.
- **Facts come from the implementation.** Rule counts, language coverage and capability claims must
  match `crates/engine/rules/*.yaml` and `apps/desktop/src/`; re-check them when the engine changes.
- **Assets are copies.** `assets/brand/` and `assets/img/` mirror `assets/brand/` and
  `assets/screenshots/` at the repository root; update both.
- **Theming through variables.** Use the CSS custom properties in `:root`; keep the light-theme
  override block in sync.
- **Accessibility.** Semantic landmarks, labelled navigation, alt text on images, sufficient
  contrast in both colour schemes.

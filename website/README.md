# CodeScan website

Static site for [CodeScan](https://github.com/amuluze/codescan). No build step, no dependencies —
plain HTML, CSS and one small script.

## Local preview

```bash
cd website
python3 -m http.server 8000
# open http://localhost:8000
```

## Layout

```text
website/
├── index.html            # single page: hero, why, features, coverage, screens, get started,
│                         # architecture, privacy, open source
├── assets/
│   ├── brand/            # logo.svg / icon.svg (copies of codescan/assets/brand)
│   ├── css/site.css      # styles (dark first, light theme via prefers-color-scheme)
│   ├── img/              # scan.png / findings.png (copies of codescan/assets/screenshots)
│   └── js/
│       ├── i18n.js       # en/zh copy dictionary
│       └── site.js       # language + theme switching, mobile nav, copy buttons
└── .nojekyll             # keep GitHub Pages from running Jekyll
```

## Languages

The page ships English and Chinese copy in one document:

- every text node carries a `data-i18n="key"` attribute; images use `data-i18n-alt="key"`;
- `assets/js/i18n.js` holds the `en` and `zh` dictionaries — one key per entry, same order as the page;
- on load the language is resolved as **`?lang=` → `localStorage` → browser language** (`zh*` → Chinese, otherwise English);
- the header button switches language explicitly and persists the choice;
- `<html lang>`, the document title and the meta description follow the active language.

To add a language: add a `xx` dictionary in `i18n.js` and extend the resolver in `site.js`. To change
copy, edit the dictionary only — the markup stays language-neutral.

URL helpers: `?lang=zh`, `?lang=en`, `?theme=dark`, `?theme=light`.

## Deploying to GitHub Pages

1. Push `website/` to the repository.
2. *Settings → Pages → Build and deployment → Source: **Deploy from a branch***, branch `main`, folder `/website`.
3. The site is served from `https://amuluze.github.io/codescan/` (custom domain optional).

`.nojekyll` is required so Pages serves the files as-is.

## Updating assets

Brand and screenshot assets are copied from the CodeScan workspace (`codescan/assets/`):

```bash
cp ../codescan/assets/brand/logo.svg ../codescan/assets/brand/icon.svg assets/brand/
cp ../codescan/assets/screenshots/scan.png ../codescan/assets/screenshots/findings.png assets/img/
```

Keep the site copy consistent with the implementation — the numbers in the coverage section
(rule counts per language and dimension) come from `codescan/crates/engine/rules/*.yaml`.

## Conventions

- English only; the product UI is Chinese, and the site says so explicitly.
- Dark-first CSS with a `prefers-color-scheme: light` fallback; colours come from CSS variables.
- No tracking, no external fonts, no CDN dependencies — the site must work offline.

# Bitácora · website

Static landing page published to GitHub Pages by `.github/workflows/pages.yml` on every push to
`main` that touches `website/`. Plain HTML and CSS, no build step, no external requests.

Source of the design: the `bitacora-design-system` repository (`website/`). Its `design/` canvas
files are not copied here.

```
website/
├── index.html            the page
├── css/tokens.css        copy of the design-system tokens (generated there, do not edit here)
├── css/styles.css        layout and components, colours only via tokens
├── js/theme.js           light/dark: follows the system, toggle remembered in localStorage
├── fonts/                Atkinson Hyperlegible Next/Mono + Literata (SIL OFL 1.1, see OFL.txt)
└── assets/brand/         logo files (SVG)
```

Run locally: `python3 -m http.server -d website 8080`.

Download cards point at the latest GitHub release, the changelog at `CHANGELOG.md` on `main`.
Update the version in the hero (`hero__meta`) when a release is published.

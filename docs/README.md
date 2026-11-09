# pkgr docs

Astro + Starlight site. Dark by default, one theme file (`src/styles/theme.css`).

```bash
npm install
npm run dev      # http://localhost:4321/pkgr/
npm run build    # also fails on broken internal links
```

- Edit site facts in `src/site.config.mjs`; use `{{repo}}`, `{{releases}}`, `{{version}}`, `{{name}}` in Markdown.
- Sidebar groups auto-generate from the folders in `src/content/docs/`.
- Root deployment (custom domain): `DOCS_BASE=/ DOCS_SITE=https://example.com npm run build`.
- Publishing: `.github/workflows/docs.yml` deploys to GitHub Pages (Settings > Pages > Source: GitHub Actions).
  When this docs folder is the whole project's `docs/`, the workflow lives at the project root `.github/workflows/`.

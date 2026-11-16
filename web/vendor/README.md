Vendored browser assets for the standalone web UI and `dist/web-site/` bundle.

Why these files are checked in:
- `web/index.html` should work from `/web/` and from `dist/web-site/` without runtime CDN access.
- The Playwright smoke test asserts that the page does not fetch Tailwind or KaTeX from third-party CDNs.

Current upstream sources:
- Tailwind utilities are compiled by the pinned npm dependency into `web/styles.css`; no Tailwind runtime ships to the browser.
  Run `npm ci && npm run build:css` after changing classes or `tailwind.config.cjs`.
- `katex/`: downloaded from `https://cdn.jsdelivr.net/npm/katex@0.16.21/dist/` on 2026-03-17

When updating:
1. Replace the vendored files from the new upstream release.
2. Keep the relative paths in `web/index.html` and `scripts/build_web_site.sh` aligned.
3. Re-run `npm run build:web:site` and `npm run test:web:smoke`.

# Landing page

Static landing page for the **monkey** Monkey C toolchain, deployed to GitHub
Pages at <https://hieudoanm.github.io/monkey/>.

Next.js 16 (App Router) with `output: "export"` — the build produces plain
static HTML in `out/`, which CI uploads via `actions/upload-pages-artifact`
on pushes to `master`.

## Stack

- Next.js 16 + React 19, App Router, static export (`trailingSlash: true`).
- Tailwind CSS v4 + DaisyUI 5 (custom light `monkey` theme in
  `src/app/globals.css`).
- `marked` renders the Markdown docs under `docs/` at `/docs/`.

## Commands

```bash
pnpm install
pnpm run dev      # dev server
pnpm run build    # static export → out/
pnpm run lint
pnpm run typecheck
```

## Structure

- `src/app/` — routes: home (`page.tsx`), `/docs/[[...slug]]`, plus shared
  `layout.tsx` (navbar/footer/metadata) and `globals.css`.
- `src/components/molecules/` — shared components (`Navbar`, `Hero`,
  `Features`, `CTA`, `Footer`, `SectionHead`, `Terminal`, `DocsSearch`).
- `src/lib/docs.ts` — Markdown → HTML pipeline and doc index for `/docs/`.
- `docs/` — documentation sources (Markdown) rendered by `/docs/`.
- `public/` — `favicon.*`, `logo.*`, `og.png`, `robots.txt`, `sitemap.xml`,
  `humans.txt`, `llms.txt`.

## Deployment

`basePath`/`assetPrefix` are `/monkey` in production builds (`next.config.ts`).
Raw `<img src>` paths must include the base path — see the `LOGO` constant in
`src/app/layout.tsx`.

See `AGENTS.md` in this directory for agent-facing conventions.

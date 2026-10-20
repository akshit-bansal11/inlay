# Inlay website

Two routes: a landing page at `/`, and the whole documentation on one page at `/docs`.

**Live at [inlay.akshitbansal.dev](https://inlay.akshitbansal.dev).** Deployed to Vercel from the repository root; see *Hosting* below.

It is **not** part of the app's quality gate: `scripts/check.ps1` checks only the Rust workspace. The site's own gate and build run in `.github/workflows/site.yml` on every push and pull request that touches `site/`.

The layout, type scale, components and motion are those of Edgepad's site, copied and re-coloured. Only the words and the palette are Inlay's.

## Running it

```bash
cd site
npm install
npm run dev        # http://localhost:3000
```

## The quality gate

One command, three tools, in this order. It exits non-zero on any finding.

```bash
npm run check      # Biome (format + import order + lint) -> ESLint -> tsc
npm run check:ci   # the same, non-mutating: fails on unformatted code
```

`check` writes, because fixing your own formatting is useful. `check:ci` must not write to a branch, so it fails instead of quietly reformatting a pull request. That is the same split `scripts/check.ps1` uses for the app.

| Tool | Owns | Command |
| --- | --- | --- |
| **Biome** | Formatting, import order, general JS/TS lint | `biome check --write .` |
| **ESLint** | React hooks, JSX, accessibility, Next.js rules | `eslint .` |
| **tsc** | Type checking | `tsc --noEmit` |

Individually: `npm run format`, `npm run lint`, `npm run typecheck`.

**The split is deliberate and the overlap is switched off.** Biome formats and organises imports; ESLint ships no stylistic rules, so no `eslint-config-prettier` layer is needed. Biome's two React-hook rules (`useExhaustiveDependencies`, `useHookAtTopLevel`) are turned **off** in `biome.json` because `eslint-config-next` already carries `eslint-plugin-react-hooks`.

Two rules in `biome.json` encode project rules rather than defaults: `noExplicitAny` is an error, and `noConsole` is an error that allows `warn` and `error` only.

**Accessibility is checked twice, on purpose.** Biome has a11y rules and `eslint-config-next` brings `jsx-a11y`. They overlap, so some findings are reported by both.

**Stylelint is not used.** With Tailwind 4 nearly all styling lives in `className` attributes; the site has one CSS file, so Stylelint would guard almost nothing. Biome's CSS linter is disabled, because Tailwind 4's `@theme`, `@custom-variant` and `@layer` at-rules are not something an untested CSS parser should be let loose on.

Node 24 or later.

### About `.editorconfig`

`site/.editorconfig` adds to the repository root one and does not replace it. The root sets `indent_size = 4` for `[*]`, which is right for Rust and wrong here. Biome reads editorconfig, so without that local override it would reformat the entire site to 4-space indents on its first run.

```bash
npm run build      # next build, kept out of the gate
```

## Structure

```
app/
  layout.tsx          fonts, metadata, theme provider
  page.tsx            the landing page: hero, widgets, how it works, limits, get it
  docs/page.tsx       the documentation: sidebar layout, composes the four section files
  globals.css         the palette and the two font variables
  icon.svg            the favicon: the mark, in its fixed brand colours
  robots.ts, sitemap.ts   written to robots.txt and sitemap.xml at build
lib/
  nav.ts              the table of contents; every id must exist as a section anchor
  utils.ts            cn()
components/
  sections/           the documentation itself, in four files
  ui/                 shadcn primitives and Magic UI components
  section.tsx         Section, Sub, P, Note, C
  toc.tsx             sidebar with an IntersectionObserver scroll-spy
  flow-diagram.tsx    the two-crate architecture map
  demo-figure.tsx     the hero recording, and its still frame for reduced motion
  mark.tsx            the logo, inline, in its fixed brand colours
  site-header.tsx     shared; `variant` picks the landing menu or the table of contents
  site-footer.tsx     shared
public/
  demo.gif            a copy of assets/demo.gif
  demo-poster.png     its first frame
  social-preview.png  a copy of assets/social-preview.png
```

## Design notes

Calm, rounded, flat. No gradients anywhere: not on a fill, a border, a hover or a mask.

**Palette.** Ink `#15151C`, Bone `#F3EFE6` and one accent, Orange `#FF6B2C`. Light is Bone with Ink text; dark is Ink with Bone text. Orange is the fill in both themes (buttons, the filled badge, step numbers, selection), always with Ink on it. As text it is too weak on Bone (2.5:1), so orange text, links, icons and the focus ring use `--orange-text`: `#B33A0B` in light, `#FF6B2C` itself in dark. In classes that is `bg-primary` for the fill and `text-primary-text` for the letters. The neutral tints the layout needs (card, line, faint, dim, accent-soft) are mixes of Ink, Bone and Orange, and the mixes are written out in `app/globals.css` next to the contrast ratio of every text pair, each computed at 4.5:1 or better. The values are mapped onto shadcn's CSS variables, so components installed with the CLI pick them up. Cobalt `#3355FF` and Sun `#FFC93C` belong to the mark and are not used on the page.

**The mark.** Four flat pieces set into one tile, one for each widget: `assets/inlay.svg`. `components/mark.tsx` and `app/icon.svg` are the same drawing, in fixed brand colours that do not follow the theme.

**Type.** Lato in 400, 700 and 900, sentence case everywhere, no letter-spaced capitals. JetBrains Mono only for what really is code: commands, config keys, file paths. Both are downloaded by `next/font` at build and served from this origin.

**Motion.** Magic UI's BlurFade for section entrances (a short lift and unblur, once), and a border that turns orange under the pointer on the widget cards. The BorderBeam, the shine on the hero pill and MagicCard's spotlight were taken out because each is a gradient; their files in `components/ui/` are no longer imported. Under `prefers-reduced-motion` the entrances stop, **in CSS** (`motion-reduce:` utilities), never with `useReducedMotion`: the server cannot know the preference, so a hook renders different markup on the client, and React does not repair a mismatched `style` on hydration. The hero recording is a GIF, which cannot be paused, so under the same preference its first frame is shown instead. The hero headline and the docs title have no entrance at all, so the first paint never waits for JavaScript.

**Theme toggle.** `next-themes` with `attribute="class"`, three states (light / dark / system). The dark variant follows the class rather than the media query, so the toggle wins over the OS in both directions.

## Versions

**Versions in `package.json` are pinned exactly**, and are the ones Edgepad's site is pinned to. Two pins are deliberately behind `latest`, and both are load-bearing:

- **`eslint` is pinned to `9.39.5`, not `10.x`.** The `eslint-plugin-react` that `eslint-config-next@16.3.6` bundles calls a context API that ESLint 10 removed, and every lint run dies before reporting anything.
- **`typescript` is pinned to `5.9.3`, not `7.x`.** 5.9.3 is what this code was verified against.

`components.json` registers the `@magicui` namespace, so `npx shadcn@latest add @magicui/<name>` works from here. Installed components are owned code: each was trimmed or fixed after install, and the reason is in its comments.

## Content sources

Everything on both pages traces to something in this repository:

| Section | Source |
| --- | --- |
| What Inlay is, install, using it, edit layout, troubleshooting | `README.md` |
| Settings | `README.md`, and the rows of `crates/app/src/settings.rs` |
| `config.toml` keys, defaults and clamps | `crates/core/src/config.rs` |
| Clock formats | `crates/core/src/timefmt.rs` |
| Hotkeys | `crates/app/src/app.rs` |
| Lyrics sources, architecture, the loop, drawing, tests | `docs/ARCHITECTURE.md`, `crates/core/src/lyricsplus.rs` |
| Privacy and security | `SECURITY.md` |
| Repository layout, prerequisites, gate, contributing, releases | `README.md`, `CONTRIBUTING.md`, `scripts/check.ps1` |
| Known limits | `README.md`, `SECURITY.md` |
| Version history | `CHANGELOG.md` |

**Nothing is invented to fill a section.** The only measured number on the site is the one the repository's README states: memory use of about 54 MB. There is no CPU figure, no benchmark against Rainmeter and no testimonial, because none exists in the repository.

The tables on `/docs` are typed by hand from those files, not generated from them. When a config key, a hotkey or a settings row changes, the page has to be changed with it.

## Hosting

Live at [inlay.akshitbansal.dev](https://inlay.akshitbansal.dev), on Vercel. The Vercel project's name was not rechecked and may still say `deskbeat`. `cleanUrls` in the root `vercel.json` is what makes `/docs` serve `docs.html` from the export.

The site is a **static export** (`output: "export"` in `next.config.ts`): every page is prerendered and nothing is read at request time, so it ships as plain files with no Next.js runtime and no serverless functions.

It deploys from the **repository root**, not from `site/`, so the one `vercel.json` there can run the build inside `site/` and point Vercel at `site/out`. `.vercelignore` keeps the app's source out of the upload.

**Response headers** come from `vercel.json` too, since a static export has no server of its own to send them: a Content-Security-Policy, `nosniff`, a referrer policy and a Permissions-Policy that switches off the device APIs the site never asks for. The CSP allows only this origin for everything, and nothing may frame the site. Two parts of it are loose on purpose. `script-src` allows `'unsafe-inline'` because the export carries inline scripts (next-themes' no-flash theme script and the React Server Components payload) whose content changes with every edit to a page, so hashes would have to be regenerated on every build and a nonce needs a server. `style-src` allows it because React writes `style` attributes (the entrance animations, Radix's accordion heights). No script comes from anywhere but this origin, and the site fetches nothing from any other. A new external resource (a font, an embed, an analytics script) has to be added to the policy or the browser will block it.

```bash
vercel deploy --prod     # from the repository root
npm run preview          # or serve the export locally from site/
```

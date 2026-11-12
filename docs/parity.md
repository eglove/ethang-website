# Parity with the old site

The rewrite is only finished if every public page of the old Astro site still
exists and still says what it said. This file records what was checked, how it
was checked, and the differences that were chosen rather than missed.

The reference is `apps/ethang-monorepo/apps/ethang-astro` (read-only) and the
`dist/` it produced. `tests/parity.rs` holds the machine-checkable half of this
document; the numbers below come from comparing the two output trees.

## Routes

`ported` means the page exists with its content. `dropped` means the user
removed it from scope. `added` means this build has it and the old one did not.

| Old route | New route | Status |
| --- | --- | --- |
| `/` | `/` | ported |
| `/blog` | `/blog` | ported |
| `/blog/page/<n>` | `/blog/page/<n>` | ported (3 pages, 2-4) |
| `/blog/<slug>` (37 posts) | `/blog/<slug>` | ported |
| `/learn` | `/learn` | ported |
| `/tips` | `/tips` | ported |
| `/tips/scroll-containers` | `/tips/scroll-containers` | ported |
| `/tips/scrollbar-gutter` | `/tips/scrollbar-gutter` | ported |
| `/courses` | `/courses` | ported as a 301 to `/learn` |
| (no custom 404) | `/404.html` | added |
| `/rss` | `/blog/feed.xml` | reimplemented as a static RSS 2.0 feed |
| `/applications` | - | dropped |
| `/applications/<id>/resume` | - | dropped |
| `/login` | - | dropped |

### The dropped routes

All four were server-rendered or private. `/applications` and
`/applications/<id>/resume` were the job-application tracker behind auth, and
`/login` only existed to reach it. Nothing links to them, `tests/parity.rs`
asserts the directories are absent, and the sitemap never listed them.

`/rss` is the exception: it was dropped for want of a feed generator, and the
feed now exists. It is published at `/blog/feed.xml` rather than `/rss` because
that is where the blog's own pages advertise it, and a feed is a property of the
blog rather than a route of its own. `tests/parity.rs` still asserts that `/rss`
itself is absent.

### The unpublished drafts

The old content directory holds 39 post directories, but two carry only an
`images/` folder and no `index.mdx` - `the-harness-is-the-orchestrator` and
`three-channels-for-reasoning`. They were never published, so they were not
migrated and no page exists for either. `tests/parity.rs` asserts that.

### `/courses`

The old site redirected `/courses` to `/learn` with a `_redirects` rule that
the host answers with a 301, so the same file is emitted and Cloudflare's
static assets serve it the same way. Both spellings are listed because the old
file listed both. It is deliberately absent from the sitemap: a redirect is not
a destination, and `/learn` is the URL worth indexing.

## What was measured

Comparing the two `dist/` trees, route by route:

- **Routes.** 47 HTML pages in each tree, and every old route has a counterpart.
  The only differences are the deliberate ones above.
- **Content.** Article prose matches the old site's word for word, at a mean of
  99.3% by word coverage across the 42 article pages. The residual is not lost
  prose: it is the old site's nav (which carried `RSS` and `Login`) and the
  hidden TeX source KaTeX embeds in its `annotation` element, which MathML does
  not carry. The two tips pages read lowest for the same reason - they are the
  shortest pages, so the nav is the largest share of their text.
- **Code blocks.** Byte-identical to the old output, fence for fence.
- **Links.** Every `href` and `src` that points inside the site resolves to a
  file in `dist/` (443 targets across 48 pages).
- **Math.** 24 rendered expressions on `the-blowup-half`, the same count as the
  old site's KaTeX output.

## Deliberate rendering differences

Two are structural, and neither changes what a reader sees as text.

**Math is MathML, not KaTeX.** The old site rendered TeX with KaTeX, which
ships HTML and CSS to reproduce the layout. `latex2mathml` emits MathML, which
the browser renders natively with no stylesheet and no script. The formulas are
the same formulas; the markup that draws them is not.

**Mermaid diagrams are the committed SVGs, not build-time renders.** The old
build ran `rehype-mermaid`, which needed a Playwright Chromium install at build
time. The SVGs that render produced are committed under `static/diagrams/` and
copied into `dist/`, so the build has no browser dependency and the diagrams are
byte-identical to what the old site served. The trade is that editing a diagram
means re-rendering it by hand rather than changing the fence.

One further difference is deliberate and visible: the old nav carried `RSS` and
`Login`. `Login` is gone with its route, and `RSS` is not in the nav - the feed
is advertised in `<head>` as `rel="alternate" type="application/rss+xml"`, which
is what a reader actually looks for.

## Additions since parity

Three things exist that the old site had no equivalent of, none of which
changes a page a reader sees:

- **`/llms.txt` and `/llms-full.txt`**, following <https://llmstxt.org/>: the
  site's pages as an index of markdown files, and the same file with every body
  inlined.
- **A markdown twin for every destination** - `/index.md`,
  `/blog/<slug>/index.md`, `/tips/<slug>/index.md`, `/learn/index.md`, and the
  two indexes - advertised from each page as `rel="alternate"
  type="text/markdown"` and covered by `rel="describedby"` pointing at
  `llms.txt`.
- **Inlined, pruned, minified CSS.** `static/styles/site.css` is a build input
  and is never published. Each page carries a `<style>` element holding only the
  rules whose selectors can match that page. `tests/css_pruning.rs` proves the
  pruned sheet matches exactly what the full sheet matches, element for element.

## How to re-run the check

```
cargo run --release -- build --out dist
cargo test --test parity
```

`tests/parity.rs` reads the committed `dist/`, not a temporary build, so a stale
output tree fails the suite instead of a fresh one quietly passing it. The route
and slug lists are frozen in the test on purpose: a test that regenerated them
from the old repo would agree with any future deletion.
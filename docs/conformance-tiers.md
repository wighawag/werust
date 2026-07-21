# Native-renderer conformance tiers — the pinned page + WPT checklist

This is the operational CHECKLIST for the native renderer's conformance target:
a **tiered capability ladder T0..T3**. It is the reference a build spec (or a
reviewer) reads to answer, concretely, **"which pages and which WPT bar define
each tier?"** (Ported in substance from the wezig project; this repo is the Rust
successor. The rationale for a tiered ladder belongs in a short ADR alongside
this doc.)

The target is **a real general web browser** (see the thesis ADR), NOT "good
enough for on-chain / dapp frontends." Every tier therefore has, as its
compatibility floor, **normal server-served general-web pages** — the same pages
an incumbent browser renders — AND, because verifiable / content-addressed
static content is where the thesis lands earliest, a **content-addressed
(`ipfs://`) static page** at the SAME tier. A tier is not "reached" until BOTH
land.

## How to read a tier

Each tier below gives two things, and both are load-bearing:

1. **A page checklist** — a concrete, enumerable list of representative pages the
   renderer MUST render correctly to claim the tier. "Render correctly" means:
   the page's intended visual layout and text are produced by the native renderer
   path (not the webview backend), free of the diagnostics that would mark an
   unsupported construct, and stable against the tier's golden references. The
   checklist is the PRIMARY, human-legible driver of the roadmap: it says what a
   user can actually open.
2. **A WPT-subset bar** — the specific [web-platform-tests](https://web-platform-tests.org/)
   area(s) and a pass-rate threshold on that subset. This is the **objective
   secondary regression meter** (see "The role of WPT %" below): it is how we
   detect regressions and measure progress *within* a tier objectively, NOT how
   we decide what to build next.

The pages are named by CATEGORY plus at least one concrete, pinnable exemplar so
a build spec can freeze an exact fixture set (a specific commit / snapshot /
CID). Exemplars are representative, not exhaustive; a build spec MAY add pages to
a tier's checklist but MUST NOT remove the categories pinned here without a new ADR.

---

## T0 — Fixed v0 subset (the ladder's floor)

**Capability:** a deliberately small, fixed HTML/CSS subset behind swappable
seams. A naive subset tokenizer + allowlist tree builder, a real cascade on a
handful of properties, block/inline flow, software text. This tier is the
ladder's floor and the anchor the higher tiers extend, NOT an aspiration. (In the
Rust successor it is the first thing built — the v0 subset the wezig Zig arm
already reached — assembled where possible from the pure-Rust stack rather than
from scratch.)

**Page checklist (what T0 renders):**

- [ ] **Server-web floor:** an authored static HTML fragment using only the v0
      element allowlist (`html/head/body`, `div`, `p`, `h1`–`h6`, `ul/ol/li`,
      `span/a/strong/em/b/i`, `br`) with `<style>` / inline `style` restricted to
      a small supported property set and selector set — the committed golden
      fixtures. This is the compatibility floor at T0: a real, if tiny,
      server-served document.
- [ ] **Content-addressed floor:** the SAME class of authored subset fragment
      fetched over `ipfs://` (or, before networking exists, loaded through the
      content-addressed resolution seam as an authored fixture) and rendered
      identically — proving the content-addressed path renders v0 content at
      parity with the server path.

**WPT-subset bar:** none required at T0. T0 predates a WHATWG-conformant parser
and a CSS-conformant engine, so a public WPT pass-rate is not a meaningful meter
for a fixed private subset. The regression guard at T0 is the golden-image suite
+ the subset-doc-drift guard, not WPT. WPT bars begin at T1, where a real parser
exists to run them against.

---

## T1 — Real static documents (real parse + core CSS)

**Capability:** a real WHATWG-algorithm HTML parser (replacing the subset
tokenizer behind the `Tokenizer | TreeBuilder` seam) and a core CSS engine — the
common box-model, colour, typography, and normal-flow properties that a
hand-written or lightly-templated static page uses — producing correct **static
block/inline layout of REAL documents**, with real shaping for Latin/LTR text. No
floats/flex/grid/tables yet (that is T2); no JS (that is T3). This is the first
tier that opens pages authored for the real web rather than for the subset. (In
the Rust arm, this is where the pure-Rust stack — html5ever for parse, stylo for
cascade, parley/cosmic-text for shaping — is expected to pay off, and reaching
T1 by standing on it is the project's first real experiment vs the Zig arm.)

**Page checklist (what T1 must render):**

- [ ] **Server-web floor — a real hand-authored article/doc page:** a
      content-first static page served over HTTP(S), e.g. a single
      [MDN](https://developer.mozilla.org/) article page or a
      [https://motherfuckingwebsite.com/](https://motherfuckingwebsite.com/)-class
      minimal semantic-HTML page — real headings, paragraphs, lists, links,
      inline emphasis, a stylesheet using core CSS. Text must be properly shaped.
- [ ] **Server-web floor — a plain server-rendered blog/news post:** a second,
      independently-authored static page (e.g. a static-site-generator blog post)
      so the tier is not tuned to one exemplar.
- [ ] **Content-addressed floor — an `ipfs://` static site:** a real
      content-addressed static site fetched by CID and rendered at parity with
      the server path — e.g. an IPFS-hosted static docs/landing page (a
      Jekyll/Hugo-class site pinned to a specific CID). This is where the thesis
      lands FIRST: a verifiable, content-addressed static document the native
      renderer opens as a first-class page, not a novelty.

**WPT-subset bar:** **≥ 90 %** on the HTML-parsing tree-construction subset
(`html/syntax/parsing/`, the `html5lib`-derived tree-construction tests) **and**
**≥ 70 %** on core CSS static-layout areas — `css/CSS2/normal-flow/`,
`css/css-box/`, `css/css-color/`, `css/css-fonts/`, `css/css-text/`. Complex-script
/ bidi text subsets are explicitly EXCLUDED from the T1 bar (deferred with T2
shaping). These percentages are the objective regression meter for T1, not its
definition — the page checklist is.

---

## T2 — Full static layout (floats / flex / grid / tables + real shaping)

**Capability:** the full static-layout feature set — floats and `clear`,
flexbox, CSS grid, tables, and out-of-flow positioning (relative / absolute /
fixed / sticky) — plus real text shaping beyond Latin/LTR (complex scripts, bidi,
full kerning). This is "renders MOST static real pages the way an incumbent
does": still no JavaScript (T3), but the layout engine is now general rather than
block/inline-only. (In the Rust arm, taffy covers flex/grid; parley/swash cover
complex-script + bidi shaping.)

**Page checklist (what T2 must render):**

- [ ] **Server-web floor — a modern CSS-layout marketing/landing page:** a real
      static page whose layout depends on flexbox and/or grid — e.g. a product
      landing page or a documentation site with a grid/flex shell (a
      [https://web.dev/](https://web.dev/)-class or framework-docs static page).
      Must lay out correctly, including responsive/wrapping behaviour at the test
      viewport.
- [ ] **Server-web floor — a table + float classic page:** a real page exercising
      tables and floats (e.g. a Wikipedia article page — infobox floats, content
      tables — served over HTTPS). This is the "old web still works" guarantee.
- [ ] **Server-web floor — a complex-script / bidi page:** a real page with
      non-Latin, shaped, and/or right-to-left text (e.g. an Arabic or Devanagari
      Wikipedia article) rendering with correct shaping and bidi ordering.
- [ ] **Content-addressed floor — an `ipfs://` static site using modern layout:**
      a content-addressed static site whose layout uses flex/grid/tables — e.g. an
      IPFS-hosted static app frontend or docs site pinned to a CID — rendered at
      parity with the server path.

**WPT-subset bar:** **≥ 85 %** across the static-layout areas —
`css/css-flexbox/`, `css/css-grid/`, `css/css-tables/` (and `css/CSS2/floats/`,
`css/css-position/`) — **and** **≥ 80 %** on the text-shaping / bidi subsets
(`css/css-text/`, `css/css-writing-modes/`, the relevant `i18n` text tests). As
at every tier, this is the regression meter, not the roadmap.

---

## T3 — Interactive sites (JS + networking + dynamic DOM)

**Capability:** a JavaScript runtime behind the `ScriptEngine` seam (a bound
engine first — SpiderMonkey leant), real networking (the bound HTTP+TLS stack),
and a dynamic DOM: DOM APIs, events, `fetch`/`XHR`, timers, and script-driven
re-layout. This is "renders and runs interactive real sites," the top pinned rung.
(Note: the JS engine is BOUND in both the Zig and Rust arms — a pure-Rust engine
is an aspirational later swap-in, never day-one — so this tier is a wash on the
language axis; it is the renderer, not JS, where the Rust experiment is decided.)

**Page checklist (what T3 must render + run):**

- [ ] **Server-web floor — a mainstream JS-driven app page:** a real interactive
      site whose content is script-rendered and network-fed — e.g. a
      client-rendered SPA view or a page that fetches and renders data on load.
      DOM mutations, event handling, and `fetch`-driven updates must work.
- [ ] **Server-web floor — a form + dynamic-interaction page:** a real page with
      forms, client-side validation, and event-driven DOM updates, proving input
      and event plumbing end-to-end.
- [ ] **Content-addressed floor — an `ipfs://` interactive dapp/app frontend:** a
      real content-addressed interactive frontend fetched by CID and run — e.g.
      an IPFS-hosted SPA (the class of "verifiable frontend" the thesis targets),
      with its JS executing and its `fetch`/provider calls working over the native
      networking path. This is the thesis's interactive endpoint: a verifiable,
      content-addressed *application*, not just a document.

**WPT-subset bar:** **≥ 75 %** across the DOM / HTML-scripting / fetch areas —
`dom/`, `html/dom/`, `html/semantics/scripting-1/`, `fetch/` (and the relevant
`XMLHttpRequest/` tests) — measured with the pinned bound `ScriptEngine`. This
bar moves with the bound engine's own conformance and is, again, the objective
regression meter, not the definition of the tier.

---

## The role of WPT % (objective secondary meter, NOT the roadmap driver)

The web-platform-tests pass rate on each tier's subset is the **objective,
secondary REGRESSION METER**. It answers, mechanically and without judgement,
*"did a change move conformance backward, and by how much?"* and gives an
at-a-glance, comparable-over-time number for how complete a tier's implementation
is.

It is explicitly **NOT the roadmap driver.** What we build next is decided by the
**page checklists** above — concrete, representative real pages a user opens,
chosen to advance the general-browser goal and to land the verifiable /
content-addressed thesis early. We do NOT pick the next feature by "which WPT
directory has the lowest percentage," because:

- WPT coverage is uneven and includes deep edge cases with near-zero real-world
  page impact; chasing the number optimises for the test suite, not for the pages
  users actually open.
- The tiers are a *capability* ladder (can a real page of this class render at
  all?), which a raw percentage cannot express.
- The thesis (verifiable, content-addressed static → interactive content) is a
  page-shaped goal, not a WPT-directory-shaped one; only the checklist can pin
  "an `ipfs://` static site renders" as a first-class requirement.

So: **the page checklist defines and drives each tier; the WPT bar measures and
guards it.** A tier is "reached" when its full page checklist (server-web floor
AND content-addressed floor) renders correctly; the WPT bar is the objective
regression gate that keeps it reached. A rising WPT % with an unmet checklist is
NOT tier progress; a met checklist with the WPT bar below threshold means there
is a regression to fix, not a tier to re-scope.

## Cross-references

- **The thesis ADR** (`docs/adr/0001-general-browser-for-a-post-trusted-server-web.md`)
  — the general-browser thesis these tiers are anchored on.
- **The seam ADRs** — the `Tokenizer | TreeBuilder`, `PaintBackend`, `Renderer`,
  `Fetcher`, and `ScriptEngine` seams the higher tiers grow by swapping mature
  backends in at.
- **The wezig project** — the Zig arm this Rust successor runs as an experiment
  against; the tiers are shared so the two arms aim at the same bar.

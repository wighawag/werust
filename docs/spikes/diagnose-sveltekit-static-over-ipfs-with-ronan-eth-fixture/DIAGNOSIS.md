# Spike: why the ronan.eth blog shows a SvelteKit "500" over `ipfs://` (and the fix)

Task: `diagnose-sveltekit-static-over-ipfs-with-ronan-eth-fixture`
Spec: `ens-to-ipfs-resolution-phase1-rpc-skeleton`
Source finding: `work/notes/observations/field-test-v0.2.4-spa-clientrouting-eth-path-blank-links-2026-07-23.md` (finding D, desktop half)

SCOPE: the DESKTOP blog-data-over-`ipfs://` failure only. The MOBILE no-navigation half of finding D is PARKED for a human device re-test (see "Parked follow-on" below), NOT diagnosed here.

## Symptom

Desktop, v0.2.4, on `ronan.eth` (a SvelteKit `@sveltejs/adapter-static` prerendered site): clicking "blog" reaches the blog page but shows NO posts and prints a "500 error". There is no server, so this is NOT a real HTTP 500: it is SvelteKit's client-side error boundary rendering because its route-data load failed. Clicking "portfolio" works.

## What SvelteKit actually requests (primary-source evidence)

On a client-side navigation, SvelteKit's router does NOT do a full page load; it fetches the target route's serialized data as a `__data.json` sibling and hydrates. Two facts from the SvelteKit runtime source pin the exact URL:

1. The data path is `add_data_suffix(pathname)`. From `packages/kit/src/runtime/pathname.js`:

   ```js
   const DATA_SUFFIX = '/__data.json';
   export function add_data_suffix(pathname) {
     if (pathname.endsWith('.html')) return pathname.replace(/\.html$/, HTML_DATA_SUFFIX);
     return pathname.replace(/\/$/, '') + DATA_SUFFIX;
   }
   ```

   With `trailingSlash = 'always'` the blog route pathname is `/blog/`, so the data path is `/blog` + `/__data.json` = **`/blog/__data.json`**.

2. The client ALWAYS appends an invalidation query param to that fetch (`INVALIDATED_PARAM`, imported in `packages/kit/src/runtime/client/client.js`; visible in the wild as `?x-sveltekit-invalidated=...`, e.g. sveltejs/kit issue #11625 titled "Failure to fetch `/__data.json?x-sveltekit-invalidated`").

So the EXACT URL werust's `ipfs://` handler receives on the blog client-nav is:

```
ipfs://<rootcid>/blog/__data.json?x-sveltekit-invalidated=01
```

## What werust did with it (root cause)

`crates/werust-core/src/ipfs.rs::parse_ipfs_uri` split the URI into `cid` + `path` by cutting at the first `/` and taking the WHOLE remainder as the path, with NO handling of a query string or fragment. So the parsed path was the literal:

```
/blog/__data.json?x-sveltekit-invalidated=01
```

That path is then resolved into the verified UnixFS DAG by `crates/fetcher/src/retriever.rs::resolve_in_dag`, which splits on `/` and matches each segment against a directory entry by name. The last segment is the literal string `__data.json?x-sveltekit-invalidated=01`, which matches NO directory entry (the real entry is named `__data.json`). The retrieval fails closed with `PathNotFound`, the `ipfs://` load fails, the client `fetch` for the route data rejects, and SvelteKit renders its error boundary = the "500", with no posts.

Reproduced headlessly (pre-fix) at the seam:

```
resolve_ipfs_request("ipfs://<cid>/blog/__data.json?x-sveltekit-invalidated=01")
  => Err(Backend("ipfs:// content-addressed load failed: path not found in dag:
                  /blog/__data.json?x-sveltekit-invalidated=01"))
```

The query string is a REQUEST modifier, never part of the content-addressed DAG path; a fragment (`#...`) is the same. Neither can appear in a CID or a static file path, so leaking either into the resolved path can only break resolution.

## Why blog failed but portfolio "worked"

The bug is not blog-specific: it breaks EVERY SvelteKit client-nav `__data.json` fetch, because the invalidation query is always appended. The observed portfolio-works / blog-fails asymmetry is a symptom-ordering artefact, not a second root cause:

- The initial full-page load of `ronan.eth/` renders the prerendered `index.html` (the home/portfolio surface) WITHOUT a client-nav `__data.json` fetch, so that content shows regardless of this bug.
- The failure is triggered by the CLIENT-SIDE navigation's `__data.json` fetch. The blog is a list route the human navigated to client-side, so its `/blog/__data.json?...` fetch hit the bug and rendered the error boundary.

The deterministic, provable defect is the query/fragment leak; fixing it resolves the whole SvelteKit-`__data.json`-over-`ipfs://` class. (An observation note records the residual "confirm portfolio's exact client-nav path on a live device" question so it is not lost.)

## The fix

`crates/werust-core/src/ipfs.rs::parse_ipfs_uri` now STRIPS a trailing fragment (`#...`) and then a query string (`?...`) from the `ipfs://` URI before taking the `<cid>[/path]`, so the DAG path is the clean `/blog/__data.json`. Nothing else changes: the CID is still validated by the retriever, every block is still hash-verified, and a genuinely-missing resource still fails closed with `PathNotFound` (the fix is correct resolution, not a bypass).

## Verification / fail-closed unchanged

- Every block is still hash-verified in `resolve_in_dag` / `CarBlockStore::read_and_verify`; this change only cleans the request path, it touches no verification.
- A genuinely-missing nested resource still fails closed: the regression fixture asserts `/blog/does-not-exist.json` returns `PathNotFound`.
- The existing tamper / incomplete-CAR / budget / path-not-found tests are unchanged and green.

## Tests (test-first, network-isolated, ride `cargo test`)

- `crates/werust-core/src/ipfs.rs`:
  - `strips_a_query_string_from_the_resolved_dag_path`
  - `strips_a_fragment_from_the_resolved_dag_path`
  - `a_sveltekit_data_fetch_with_the_invalidated_query_resolves_the_nested_data` (the end-to-end seam regression)
- `crates/fetcher/src/retriever.rs`:
  - `a_sveltekit_adapter_static_build_serves_the_nested_page_and_its_data_json` (the committed, network-isolated regression fixture: a minimal adapter-static build shape - root `index.html` + `_app/` + nested `blog/` with `index.html` + `__data.json` - synthesized as a real dag-pb/UnixFS DAG + CAR offline, asserting werust resolves + serves the nested page AND its `__data.json`, and that a missing nested resource still fails closed).

## Parked follow-on (OUT of scope, needs a device)

The MOBILE half of finding D - clicking blog OR portfolio does NOTHING on Android (no navigation at all) - is a SEPARATE symptom from this desktop data-fetch bug. It needs on-device diagnosis (is the SPA client nav even proceeding on the Android WebView; possible interaction with the ANR-fix executor serialisation). It is left PARKED for a human device re-test and is not attempted here.

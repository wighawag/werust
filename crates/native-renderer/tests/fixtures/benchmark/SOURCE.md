# Benchmark-harness fixtures — provenance + pinning

The native-renderer benchmark harness (task
`native-renderer-benchmark-harness-capability-and-trust-hooks`, spec stories 20 + 21;
`docs/conformance-tiers.md`; the exploration spec
`rust-successor-native-renderer-architecture-benchmark`) SCORES a candidate
native-renderer path on capability + trust-hooks + the vs-wezig meter, and emits ONE
structured, comparable, reproducible report the exploration spec decides the
architecture from. The harness must run **hermetically and reproducibly** under
`verify` (`cargo test`, offline), so every input it reads is pinned here.

## What the harness reuses (not duplicated here)

- **Capability — page checklist:** the harness renders the pinned T1 server-floor
  pages through the native path. Those pages are already pinned as committed
  snapshots under `../t1-server-floor/` (`article.html`, `blog-post.html`; provenance
  in that directory's `SOURCE.md`). The harness reuses them rather than re-pinning a
  second copy, so there is one source of truth for "the T1 pages a candidate renders".
- **Capability — WPT subsets:** the harness runs the two pinned WPT subsets under
  `../t1-wpt/` via the reused `wpt_meter` engine (tree-construction `.dat` cases +
  the core-CSS computed-value cases; provenance in that directory's `SOURCE.md`).
- **Trust hooks:** the harness reuses the `Renderer` seam's own `renderer::qualify`
  gate (provider injection + `ipfs://` scheme) as its pass/fail trust-hook check — no
  fixture; it is a pure function of the backend's declared `TrustHooks`.

## What is pinned in THIS directory

- **`vs-wezig.txt`** — the vs-wezig meter's pinned arm signals (effort, code volume,
  DOM object-graph friction) for the Rust arm and the wezig (Zig) arm at the same
  rung. The harness STRUCTURES the comparison and puts these on the shared ladder; it
  does NOT compute them from a source tree. They are recorded evidence, pinned here so
  the meter is reproducible and re-runnable. Updating a number is a fixture edit, not
  a code change — this file is the single source the harness reads for the wezig-arm
  comparison. See that file's header for the field format and its provenance/status
  note.

## Why the wezig-arm figures are pinned, not fetched or computed

wezig is a SEPARATE project (the Zig control arm); its source is not in this repo, and
the effort/volume figures are build-history evidence, not something derivable from the
werust tree. Computing them here would be fiction; fetching wezig at test time would
make the meter non-hermetic under `verify` and non-reproducible. So the comparison is
pinned as recorded evidence, exactly as the T1 page snapshots and WPT subsets are — the
harness's job is to put the recorded signals on the shared ladder in a comparable,
reproducible shape, which the exploration spec then reads.

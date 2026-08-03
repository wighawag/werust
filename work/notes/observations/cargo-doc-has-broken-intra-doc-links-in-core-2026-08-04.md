# `cargo doc` reports broken intra-doc links in `werust-core` (2026-08-04)

Spotted while checking that a new module's rustdoc links resolved (task `shortcut-resolution-in-core-and-the-gtk-edge`): `cargo doc -p werust-core --no-deps` emits pre-existing `rustdoc::broken_intra_doc_links` and "links to private item" warnings, e.g. `Cid::to_string` in `crates/werust-core/src/contenthash.rs:38` and `ProtoCode::display_name` / `rpc_endpoint` links in `contenthash.rs` / `ethereum.rs`.

Not investigated and not in this task's scope. Worth knowing that the `verify` gate (`fmt && clippy && build && test`) never runs `cargo doc`, so these stay invisible to it; if the docs are meant to be navigable, a `cargo doc` leg (or `-D rustdoc::broken_intra_doc_links`) would catch them.

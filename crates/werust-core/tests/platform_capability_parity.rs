//! Platform-capability parity guard (task `platform-capability-parity-guard`,
//! spec `ens-to-ipfs-resolution-phase1-rpc-skeleton`,
//! `docs/adr/0005-platform-capability-parity-guard.md`).
//!
//! THE PROBLEM THIS GUARD EXISTS FOR: werust ships three contexts (desktop
//! WebKitGTK, iOS WKWebView, Android System WebView), each an OS edge over the
//! shared `werust-core` seams. A cross-cutting user-facing CAPABILITY can
//! silently ship on ONE platform only when a seam method is no-op'd on a backend
//! and nothing flags it. That is exactly what hid the mobile `ipfs://` gap:
//! `register_scheme_handler` was an empty `{}` on both mobile backends, the whole
//! capability shipped desktop-only, and the release (a tag push through the
//! `fmt && clippy && build && test` gate) looked green.
//!
//! THE FIX (the settled design, ADR-0005): a checked-in capability matrix
//! (`docs/platform-capability-matrix.toml`) — one row per capability, a cell per
//! platform, each cell explicitly `implemented` / `stubbed`(+linked task) /
//! `n-a`(+reason) — PLUS a no-silent-no-op-seam rule expressed through it: a
//! `stubbed` cell (the matrix face of a no-op'd seam method) MUST name a
//! follow-on task that really exists in `work/tasks/{backlog,ready,done}/`. This
//! test is that guard. It is a plain workspace `cargo test`, so it rides the
//! existing `verify` gate with NO CI change: a tag can never ship a
//! silently-one-platform feature, because an untracked cross-platform gap reds
//! the gate right here.
//!
//! `werust-core` hosts it because it is the ONE shared crate every OS edge sits
//! over (the seams live at/under it) and it carries no GTK/SDK deps, so the guard
//! runs inside the pure-Rust gate with no extra toolchain — the same reason the
//! sibling `release_plumbing_shape.rs` test lives here.
//!
//! Acceptance criteria mapped to assertions below:
//! 1. A checked-in matrix lists each capability x {desktop, iOS, Android} with an
//!    explicit `implemented` / `stubbed`(+task) / `n-a`(+reason) state
//!    (`the_real_matrix_is_well_formed_and_covers_every_platform`).
//! 2. The guard FAILS when a cell is `stubbed`/unimplemented WITHOUT a resolvable
//!    linked task, or when a cell is missing
//!    (`an_untracked_stub_fails_the_guard`, `a_missing_cell_fails_the_guard`,
//!    `a_stub_pointing_at_a_nonexistent_task_fails_the_guard`).
//! 3. The matrix is SEEDED with current reality; the once-known mobile `ipfs://`
//!    gap is now CLOSED (implemented on every platform), and the remaining gaps
//!    are tracked (not hidden)
//!    (`the_real_matrix_passes_because_gaps_are_tracked`,
//!    `the_mobile_ipfs_gap_is_now_implemented_on_every_platform`).
//! 4. The no-op-seam rule: an unmarked silent no-op reds the gate; the current
//!    mobile no-ops are marked+linked
//!    (`no_op_seam_methods_on_mobile_are_represented_as_tracked_stubs`).
//! 5. The guard runs inside the gate (this file is a plain `cargo test`).
//! 6. Tests cover the guard itself: a fixture with an untracked stub FAILS, a
//!    fully-implemented-or-tracked fixture PASSES, network-isolated (the
//!    `fixture` tests below parse in-memory TOML — no filesystem task lookup, no
//!    network).

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

// --- The guard model: parse the matrix + validate it against the task pool. ---

/// One capability x platform cell state.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CellState {
    Implemented,
    /// A known gap, carrying the follow-on task slug that must resolve.
    Stubbed {
        task: String,
    },
    /// Genuinely not applicable, carrying the reason.
    NotApplicable {
        reason: String,
    },
}

/// A parsed matrix: the platform list + one row per capability, each row a map
/// of platform -> cell (or a missing cell, kept as `None`, so the guard can red
/// on omission).
#[derive(Debug)]
struct Matrix {
    platforms: Vec<String>,
    capabilities: Vec<Capability>,
}

#[derive(Debug)]
struct Capability {
    name: String,
    /// Parallel to `Matrix::platforms`: the cell for each platform, `None` if the
    /// row omitted that platform (an error the guard reports).
    cells: Vec<(String, Option<CellState>)>,
}

/// A validation error the guard surfaces (each one reds the gate). Kept as a
/// typed list so the tests can assert the guard fails for the RIGHT reason, not
/// just that it failed.
#[derive(Debug, PartialEq, Eq)]
enum GuardError {
    /// A capability row did not give a cell for this platform (gap by omission).
    MissingCell {
        capability: String,
        platform: String,
    },
    /// A `stubbed` cell whose linked task does not resolve to a real task file.
    UnresolvedStubTask {
        capability: String,
        platform: String,
        task: String,
    },
    /// A `stubbed` cell with no `task` at all (a bare untracked stub).
    UntrackedStub {
        capability: String,
        platform: String,
    },
    /// An `n-a` cell with no `reason`.
    NaWithoutReason {
        capability: String,
        platform: String,
    },
    /// The `platforms` list was empty or a capability listed a platform not in it.
    UnknownPlatform {
        capability: String,
        platform: String,
    },
}

/// Parse the matrix TOML text into the guard model. A parse-level defect (not a
/// policy violation) is returned as `Err(String)` so a broken file is loud.
fn parse_matrix(text: &str) -> Result<Matrix, String> {
    let doc: toml::Value = toml::from_str(text).map_err(|e| format!("parse matrix TOML: {e}"))?;

    let platforms: Vec<String> = doc
        .get("platforms")
        .and_then(toml::Value::as_array)
        .ok_or("matrix must declare a `platforms` array")?
        .iter()
        .map(|v| {
            v.as_str()
                .map(str::to_string)
                .ok_or_else(|| "every `platforms` entry must be a string".to_string())
        })
        .collect::<Result<_, _>>()?;

    let rows = doc
        .get("capability")
        .and_then(toml::Value::as_array)
        .ok_or("matrix must declare at least one `[[capability]]`")?;

    let mut capabilities = Vec::new();
    for row in rows {
        let name = row
            .get("name")
            .and_then(toml::Value::as_str)
            .ok_or("every `[[capability]]` needs a `name`")?
            .to_string();

        // Read a cell for EVERY declared platform; a missing key stays `None` so
        // the guard reds on omission rather than silently skipping it.
        let mut cells = Vec::with_capacity(platforms.len());
        for platform in &platforms {
            let cell = match row.get(platform) {
                None => None,
                Some(v) => Some(
                    parse_cell(v)
                        .map_err(|e| format!("capability `{name}` platform `{platform}`: {e}"))?,
                ),
            };
            cells.push((platform.clone(), cell));
        }
        capabilities.push(Capability { name, cells });
    }

    Ok(Matrix {
        platforms,
        capabilities,
    })
}

/// Parse a single cell table into a `CellState`, or a parse error string.
fn parse_cell(v: &toml::Value) -> Result<CellState, String> {
    let tbl = v
        .as_table()
        .ok_or("a cell must be a table (e.g. `{ state = \"implemented\" }`)")?;
    let state = tbl
        .get("state")
        .and_then(toml::Value::as_str)
        .ok_or("a cell must have a `state`")?;
    match state {
        "implemented" => Ok(CellState::Implemented),
        "stubbed" => {
            let task = tbl
                .get("task")
                .and_then(toml::Value::as_str)
                .ok_or("a `stubbed` cell must carry a `task` slug")?
                .to_string();
            Ok(CellState::Stubbed { task })
        }
        "n-a" => {
            let reason = tbl
                .get("reason")
                .and_then(toml::Value::as_str)
                .ok_or("an `n-a` cell must carry a `reason`")?
                .to_string();
            Ok(CellState::NotApplicable { reason })
        }
        other => Err(format!(
            "unknown cell state `{other}` (want implemented/stubbed/n-a)"
        )),
    }
}

/// Validate the matrix against a task-slug resolver. `task_exists(slug)` returns
/// whether a task with that slug exists in the work board — injected so the
/// fixture tests can run WITHOUT touching the filesystem (network-isolated).
fn validate(matrix: &Matrix, task_exists: &dyn Fn(&str) -> bool) -> Vec<GuardError> {
    let mut errors = Vec::new();
    let known: BTreeSet<&str> = matrix.platforms.iter().map(String::as_str).collect();

    for cap in &matrix.capabilities {
        for (platform, cell) in &cap.cells {
            if !known.contains(platform.as_str()) {
                errors.push(GuardError::UnknownPlatform {
                    capability: cap.name.clone(),
                    platform: platform.clone(),
                });
            }
            match cell {
                None => errors.push(GuardError::MissingCell {
                    capability: cap.name.clone(),
                    platform: platform.clone(),
                }),
                Some(CellState::Implemented) => {}
                Some(CellState::Stubbed { task }) => {
                    if task.is_empty() {
                        errors.push(GuardError::UntrackedStub {
                            capability: cap.name.clone(),
                            platform: platform.clone(),
                        });
                    } else if !task_exists(task) {
                        errors.push(GuardError::UnresolvedStubTask {
                            capability: cap.name.clone(),
                            platform: platform.clone(),
                            task: task.clone(),
                        });
                    }
                }
                Some(CellState::NotApplicable { reason }) => {
                    if reason.trim().is_empty() {
                        errors.push(GuardError::NaWithoutReason {
                            capability: cap.name.clone(),
                            platform: platform.clone(),
                        });
                    }
                }
            }
        }
    }
    errors
}

// --- Real-repo wiring: locate the matrix + resolve task slugs on disk. ---

/// The workspace root: this crate lives at `crates/werust-core`.
fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("resolve workspace root from crates/werust-core")
}

fn load_real_matrix() -> Matrix {
    let path = repo_root().join("docs/platform-capability-matrix.toml");
    let text =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    parse_matrix(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// A `stubbed` cell resolves iff a task `<slug>.md` exists in ANY of the work
/// board's status folders (`backlog`/`ready`/`done`). This is the "linked
/// follow-on task references those slugs" contract: the guard reds if the slug
/// names no real tracked task.
fn real_task_exists(slug: &str) -> bool {
    let tasks = repo_root().join("work/tasks");
    ["backlog", "ready", "done"]
        .iter()
        .any(|folder| tasks.join(folder).join(format!("{slug}.md")).is_file())
}

// --- Acceptance: the REAL matrix is well-formed and green because gaps are tracked. ---

#[test]
fn the_real_matrix_is_well_formed_and_covers_every_platform() {
    // Criterion 1: the checked-in matrix declares the three contexts and gives an
    // explicit, well-formed cell for every capability x platform (no omission).
    let matrix = load_real_matrix();
    let platforms: BTreeSet<&str> = matrix.platforms.iter().map(String::as_str).collect();
    for expected in ["desktop", "ios", "android"] {
        assert!(
            platforms.contains(expected),
            "the matrix must cover the `{expected}` context; platforms = {:?}",
            matrix.platforms
        );
    }
    assert!(
        !matrix.capabilities.is_empty(),
        "the matrix must list at least one capability"
    );
    // Every row must give a cell for every declared platform (adding a platform
    // or capability FORCES a cell — no silent-by-omission gap).
    for cap in &matrix.capabilities {
        for (platform, cell) in &cap.cells {
            assert!(
                cell.is_some(),
                "capability `{}` is missing a cell for platform `{platform}` \
                 (adding a platform forces a cell in every row)",
                cap.name
            );
        }
    }
}

#[test]
fn the_real_matrix_passes_because_gaps_are_tracked() {
    // Criteria 2+3: the REAL matrix, validated against the REAL work board, has
    // NO guard errors — it is green today ONLY because every gap is a `stubbed`
    // cell linked to a task that actually exists, not because gaps are hidden.
    let matrix = load_real_matrix();
    let errors = validate(&matrix, &real_task_exists);
    assert!(
        errors.is_empty(),
        "the platform-capability parity guard is RED: {errors:#?}\n\
         Every capability must be `implemented` on all shipped contexts, or a \
         `stubbed`/`n-a` cell that names a real follow-on task / reason. Fix the \
         matrix at docs/platform-capability-matrix.toml (or land the capability)."
    );
}

#[test]
fn the_mobile_ipfs_gap_is_now_implemented_on_every_platform() {
    // Criterion 3 (the motivating gap, now CLOSED): `ipfs://` render is
    // implemented on ALL three contexts. Desktop intercepts via WebKitGTK
    // `install_ipfs`; both mobile edges now intercept too (task
    // `mobile-ipfs-scheme-interception-ios-and-android`) — the mobile
    // `register_scheme_handler` no-op is gone (it now stores + dispatches the
    // handler through the SAME core resolve path), and the OS edge drives it
    // (Android `shouldInterceptRequest`, iOS `WKURLSchemeHandler`). This pins the
    // resolution so a regression that re-stubs a mobile edge (or drops the row)
    // breaks this test.
    let matrix = load_real_matrix();
    let ipfs = matrix
        .capabilities
        .iter()
        .find(|c| c.name == "ipfs-render")
        .expect("the matrix must carry the `ipfs-render` capability (the motivating gap)");
    let cell = |platform: &str| {
        ipfs.cells
            .iter()
            .find(|(p, _)| p == platform)
            .and_then(|(_, c)| c.clone())
            .unwrap_or_else(|| panic!("ipfs-render must have a `{platform}` cell"))
    };
    for platform in ["desktop", "ios", "android"] {
        assert_eq!(
            cell(platform),
            CellState::Implemented,
            "`ipfs://` render must be implemented on {platform} (desktop via install_ipfs, \
             mobile via the real register_scheme_handler + OS-edge interception)"
        );
    }
}

#[test]
fn no_op_seam_methods_on_mobile_are_represented_as_tracked_stubs() {
    // Criterion 4 (the no-silent-no-op-seam rule, expressed through the matrix):
    // the seam methods that are STILL empty `{}` no-ops on the mobile backends
    // (`register_script_message_handler` + `inject_script` -> eip1193-provider;
    // the default `trust_posture` -> trust-indicator) MUST each show up as a
    // marked + task-linked `stubbed` cell on mobile, never as a silent
    // `implemented`. If any is flipped to `implemented` while the backend still
    // no-ops it, or dropped, this reds. (`register_scheme_handler` -> ipfs-render
    // is NO LONGER a no-op — it is implemented on mobile now, asserted by
    // `the_mobile_ipfs_gap_is_now_implemented_on_every_platform` — so it is not in
    // this list anymore.)
    let matrix = load_real_matrix();
    for (capability, platform) in [
        ("eip1193-provider", "ios"),
        ("eip1193-provider", "android"),
        ("trust-indicator", "ios"),
        ("trust-indicator", "android"),
    ] {
        let cap = matrix
            .capabilities
            .iter()
            .find(|c| c.name == capability)
            .unwrap_or_else(|| panic!("matrix must carry `{capability}`"));
        let cell = cap
            .cells
            .iter()
            .find(|(p, _)| p == platform)
            .and_then(|(_, c)| c.clone())
            .unwrap_or_else(|| panic!("`{capability}` must have a `{platform}` cell"));
        match cell {
            CellState::Stubbed { task } => assert!(
                real_task_exists(&task),
                "`{capability}` on {platform} is a stub linked to `{task}`, which must be a real task"
            ),
            other => panic!(
                "`{capability}` on {platform} is a mobile seam no-op today, so it MUST be a \
                 tracked `stubbed` cell, not {other:?} — do not mark a silent no-op implemented"
            ),
        }
    }
}

// --- Guard-itself tests: fixtures, network-isolated (in-memory TOML, injected resolver). ---

/// A resolver where a fixed set of slugs "exist" — no filesystem, no network.
fn fixture_resolver(existing: &'static [&'static str]) -> impl Fn(&str) -> bool {
    move |slug: &str| existing.contains(&slug)
}

#[test]
fn a_fully_implemented_or_tracked_fixture_passes() {
    // Criterion 6 (PASS case): a matrix where every cell is implemented, or a
    // stub linked to a task the resolver knows, or an n-a with a reason, has NO
    // guard errors.
    let text = r#"
        platforms = ["desktop", "ios", "android"]

        [[capability]]
        name = "address-bar"
        desktop = { state = "implemented" }
        ios = { state = "implemented" }
        android = { state = "implemented" }

        [[capability]]
        name = "ipfs-render"
        desktop = { state = "implemented" }
        ios = { state = "stubbed", task = "wire-mobile-ipfs" }
        android = { state = "n-a", reason = "no webview on this hypothetical edge" }
    "#;
    let matrix = parse_matrix(text).expect("fixture parses");
    let errors = validate(&matrix, &fixture_resolver(&["wire-mobile-ipfs"]));
    assert!(
        errors.is_empty(),
        "a fully-tracked fixture must PASS; got {errors:#?}"
    );
}

#[test]
fn an_untracked_stub_fails_the_guard() {
    // Criterion 2/6 (FAIL case, the core one): a bare `stubbed` cell with no
    // `task` is an UNTRACKED gap and MUST red the guard.
    let text = r#"
        platforms = ["desktop", "ios"]

        [[capability]]
        name = "ipfs-render"
        desktop = { state = "implemented" }
        ios = { state = "stubbed" }
    "#;
    // A cell with `state = "stubbed"` but no `task` is a parse error (a stub MUST
    // carry a task field), which is itself the guard refusing a bare stub.
    let parsed = parse_matrix(text);
    assert!(
        parsed.is_err(),
        "a `stubbed` cell with no `task` must be rejected (a bare untracked stub cannot be expressed)"
    );
}

#[test]
fn a_stub_pointing_at_a_nonexistent_task_fails_the_guard() {
    // Criterion 2/6 (FAIL case): a `stubbed` cell whose linked task does NOT
    // resolve to a real task file reds the guard — a dangling link is as bad as
    // no link.
    let text = r#"
        platforms = ["desktop", "ios"]

        [[capability]]
        name = "ipfs-render"
        desktop = { state = "implemented" }
        ios = { state = "stubbed", task = "a-task-that-does-not-exist" }
    "#;
    let matrix = parse_matrix(text).expect("fixture parses");
    let errors = validate(&matrix, &fixture_resolver(&["some-other-task"]));
    assert_eq!(
        errors,
        vec![GuardError::UnresolvedStubTask {
            capability: "ipfs-render".to_string(),
            platform: "ios".to_string(),
            task: "a-task-that-does-not-exist".to_string(),
        }],
        "a stub whose task does not exist must red the guard"
    );
}

#[test]
fn a_missing_cell_fails_the_guard() {
    // Criterion 2/6 (FAIL case): a capability that omits a platform's cell is a
    // gap-by-omission and MUST red — adding a platform forces a cell in every row.
    let text = r#"
        platforms = ["desktop", "ios", "android"]

        [[capability]]
        name = "ipfs-render"
        desktop = { state = "implemented" }
        ios = { state = "implemented" }
        # android cell deliberately omitted
    "#;
    let matrix = parse_matrix(text).expect("fixture parses");
    let errors = validate(&matrix, &fixture_resolver(&[]));
    assert_eq!(
        errors,
        vec![GuardError::MissingCell {
            capability: "ipfs-render".to_string(),
            platform: "android".to_string(),
        }],
        "an omitted platform cell must red the guard (no silent-by-omission gap)"
    );
}

#[test]
fn an_na_cell_without_a_reason_fails_the_guard() {
    // Criterion 1/6: `n-a` must carry a reason, so "not applicable" is never a
    // silent escape hatch. (Expressed as a parse error: an `n-a` cell without a
    // `reason` field is rejected.)
    let text = r#"
        platforms = ["desktop"]

        [[capability]]
        name = "ipfs-render"
        desktop = { state = "n-a" }
    "#;
    assert!(
        parse_matrix(text).is_err(),
        "an `n-a` cell with no `reason` must be rejected"
    );
}

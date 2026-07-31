# werust

A from-scratch, general-purpose web browser in **Rust** for a "post-trusted-server" web: native `ipfs://` resolution, a native Ethereum (EIP-1193) provider and ENS-name resolution, privacy-protecting, local-first, with full compatibility for the normal server web. The domain language, conventions, and architecture entry point live in [CONTEXT.md](CONTEXT.md); decisions in `docs/adr/`; the work system in `work/`.

## Command line

`werust` with no argument (or `werust <url>`) opens the browser window, as always. A few verb-first subcommands run HEADLESSLY instead — no GTK window, no display needed — so a resolution can be scripted or debugged over ssh:

```
werust resolve <ens-name>   # print the ipfs://<cid> the browser would load for the name
werust resolve --json <n>   # the same facts as one JSON object:
                            #   {"name":…,"kind":…,"reference":…,"cid":…,"mutable":…,"pointer":…}
werust version              # print the version banner (also --version, -V)
werust --help               # the usage message (also -h)
```

`resolve` prints the reference on stdout and exits 0; a resolution failure prints the reason on stderr and exits 1. It performs the FULL resolution the browser performs, through the same core path: a name whose ENS contenthash is a MUTABLE `ipns-ns` pointer is followed through its CLIENT-VERIFIED IPNS record to the CID it points at right now, so what `resolve` prints is what the GUI would load. Following is not flattening — the mutable name is still a mutable name (`docs/adr/0006`): the human form notes it on stderr (stdout stays the bare `ipfs://<cid>` a script can pin) and `--json` carries both the followed `pointer` and the resolved `cid` (`docs/spikes/cli-resolve-follows-mutable-names-to-the-cid/DECISIONS.md`). Any other first argument is still treated as a URL to open in the GUI, so nothing that launched the browser before does anything different now.

## Development

Standard cargo workflow: `cargo build`, `cargo test` (the `verify` gate additionally runs `cargo fmt --check` and `cargo clippy`). Desktop builds need the WebKitGTK 6.0 system dev packages; see `.github/workflows/release.yml` for the exact list.

### The macOS shell (`werust-macos`)

On a Mac, `cargo run -p werust-macos [url]` opens the AppKit window over the `WKWebView` backend: the same URL bar, nav controls, trust indicator, error surface, in-URL-bar load progress, ⋮ menu and Console/Network debug view the GTK shell has, painted from the SAME `werust-core` derivation. It is a separate binary because `werust` itself is bound to GTK/WebKitGTK. What CI proves about it, what still awaits a human on a Mac, and the manual verification steps are in [`docs/spikes/macos-appkit-window-and-chrome/README.md`](docs/spikes/macos-appkit-window-and-chrome/README.md). To package that shell as an app bundle (locally, exactly as the release does), run `crates/werust-macos/bundle-app.sh`.

### The macOS release artifact (`Werust.app`, UNSIGNED)

Every tagged release attaches `Werust-macos-universal-unsigned.app.zip`: a `Werust.app` bundle around one UNIVERSAL binary, `lipo`'d from the `x86_64-apple-darwin` and `aarch64-apple-darwin` builds, so a single download runs natively on both Intel and Apple Silicon. Its `CFBundleVersion` is the same version string the ⋮ menu inside it reports.

It is **unsigned and unnotarized** (no Apple Developer account is involved), so Gatekeeper refuses a plain double-click the first time. Open it either way:

- double-click `Werust.app` once and let it be blocked, then open **System Settings -> Privacy & Security**, find the message naming `Werust.app` near the bottom, and click **Open Anyway**; or
- clear the quarantine flag first: `xattr -d com.apple.quarantine /path/to/Werust.app` (this one works on every macOS version, including from a terminal over ssh).

On **macOS 14 (Sonoma) and older** there is a third way: right-click (or Control-click) `Werust.app`, choose **Open**, then **Open** again in the dialog. **macOS 15 (Sequoia) removed that bypass** for apps that are not signed and notarized, so on a current Mac it silently does nothing useful and the two options above are the ones that work.

Signing and notarization are a deliberate FOLLOW-ON, the macOS analogue of the landed `android-apk-signing` leg, and when it comes it should copy that leg's pattern: gate on a secrets-presence flag, no-op gracefully without it, name the artifacts honestly. Until then this build is for developers and early testers, not for general distribution. Decisions and what still awaits a human on a Mac: [`docs/spikes/macos-release-packaging-leg/README.md`](docs/spikes/macos-release-packaging-leg/README.md).

### The Windows shell (`werust-windows`)

On Windows, `cargo run -p werust-windows [url]` opens the Win32 window over the Edge **WebView2** backend: the same URL bar, nav controls, trust indicator, error banner, in-URL-bar load progress, ⋮ menu and Console/Network debug view the GTK and AppKit shells have, painted from the SAME `werust-core` derivation (and the same shared `desktop-paint` palette the macOS window uses). It is a separate binary because `werust` itself is bound to GTK/WebKitGTK. It needs the **Microsoft Edge WebView2 Runtime**, which ships with Windows 11 and is present on most Windows 10 machines; without it werust exits with a message naming the runtime and pointing at its download rather than crashing. `ipfs://` pages get a REAL `ipfs://<cid>` tuple origin here, measured on a Windows runner rather than assumed. What CI proves about it, what still awaits a human on a Windows box, and the manual verification steps are in [`docs/spikes/windows-win32-window-and-chrome/README.md`](docs/spikes/windows-win32-window-and-chrome/README.md).

There is **no Windows release artifact yet**: nothing is signed, nothing is packaged, there is no installer and no `.zip` on a Release, so the only way to run it today is to build it from source as above. The chrome is also classic-styled and not DPI-aware until the application manifest lands with that packaging work (task `windows-release-packaging-leg`).

### Private Ethereum RPC endpoint (`WERUST_RPC_URL`)

ENS resolution (`ronan.eth` -> IPFS) goes through a trusted JSON-RPC endpoint whose public, keyless default is `https://mainnet.infura.io/v3/9aa3d95b3bc440fa88ea12eaa4456161`. To use a private endpoint (your own node, e.g. the `https://your-private-rpc.example.com/` shape) WITHOUT committing its URL:

1. Copy `.env.example` to `.env` (git-ignored) and set `WERUST_RPC_URL`.
2. Run via `source .env && cargo run` (or a `direnv`-style loader — your choice; werust deliberately ships NO runtime `.env` loader).

The variable is read ONCE at session construction, so a change takes effect on relaunch. An unset or empty value falls back to the public default, so a fresh build works with no configuration. The release workflow passes an optional `WERUST_RPC_URL` repository secret through the same name. NEVER commit a real RPC URL to any tracked file.

---
title: "The goreleaser leg is NOT idempotent on re-run, so the v0.2.9 release is red on a 422 already_exists"
date: 2026-07-31
status: open
---

Noticed by the conductor while driving `macos-release-packaging-leg`: I checked the release workflow's recent history to see whether the new macOS leg had any precedent to match, and found the last REAL release red.

`v0.2.9` has two runs, [30464954386](https://github.com/wighawag/werust/actions/runs/30464954386) and [30465110439](https://github.com/wighawag/werust/actions/runs/30465110439), and both ended in `goreleaser failure` while `verify`, `android-apk` and `ios-simulator-app` all succeeded. The failure is not a build failure:

```
upload failed  error=POST .../releases/361853856/assets?name=checksums.txt:
               422 Validation Failed [{Resource:ReleaseAsset Field:name Code:already_exists}]
upload failed  error=POST .../releases/361853856/assets?name=werust_0.2.9_linux_amd64.tar.gz:
               422 Validation Failed [{Resource:ReleaseAsset Field:name Code:already_exists}]
⨯ release failed after 43s
```

GoReleaser refuses to overwrite an asset that is already attached, so the first attempt got far enough to upload the tarball and checksums, and every re-run after that is guaranteed to fail on the assets its own predecessor uploaded. The tag's Release page presumably HAS its Linux artifacts; the workflow says the release failed. That is the worst combination, because the red is now the normal state and stops being read.

**Why this is worth a note rather than a shrug:** this repo already made the OTHER legs idempotent on purpose. The mobile legs each do an idempotent `gh release create` then `gh release upload`, and the new `macos-desktop-app` leg copies them, precisely so a re-run or a decoupled leg cannot fail on state a sibling created. The desktop Linux leg is the one that did not get that treatment, and it is the one that is red. The fix is presumably GoReleaser's own overwrite/replace-existing-artifacts behaviour or a `gh release upload --clobber`-shaped step, but the choice belongs to whoever owns the release path.

**Also worth knowing:** the same pipeline is GREEN on the `workflow_dispatch` dry run. I fired [30594040744](https://github.com/wighawag/werust/actions/runs/30594040744) on `main` to measure the new macOS leg, and all five jobs succeeded (`verify`, `goreleaser`, `android-apk`, `ios-simulator-app`, `macos-desktop-app`), producing four artifacts including a 4.2 MB `werust-macos-desktop-app`. So the dry-run path proves the build; only the tag path's asset upload is broken, and only on a second attempt.

Not fixed here (out of the scope of the task being driven, and it is the release path's own decision). Recorded so the next person cutting a tag does not read a red release run as "the macOS leg broke it".

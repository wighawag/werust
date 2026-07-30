# A disposable GitHub repo `wighawag/werust-windows-origin-probe-scratch` is left over and should be deleted (2026-07-30)

Task `windows-ipfs-origin-probe-on-ci` had to MEASURE on a `windows-latest` runner, but a worker may not push to `werust` (the runner owns every git-state transition), so the probe was measured in a throwaway repo, `wighawag/werust-windows-origin-probe-scratch`. It has served its purpose: the measurement is recorded in `docs/spikes/windows-ipfs-origin-probe-on-ci/` and the probe now ships in `werust` itself with its own `windows-origin-probe` workflow.

It could not be deleted from here (the available token has `repo` + `workflow` but not `delete_repo`), so it was made private, archived, and had Actions and issues disabled. **A human should delete it**; nothing depends on it.

The general signal worth keeping: a task whose deliverable is a CI MEASUREMENT is awkward under the work/ contract, because the worker cannot reach CI on the repo it is working in. The scratch-repo detour worked but leaves litter. Worth deciding once, properly, how such tasks should get their measurement.

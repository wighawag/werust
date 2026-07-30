#!/usr/bin/env python3
"""Measure GApplication single-instance behaviour for a PAIR of werust app ids.

The question this answers: when release B is launched while release A is
already running, does B get its OWN process (primary) or is its session handed
to the running A (remote)? That hand-off is the "stale process trap" behind the
task `versioned-gtk-app-id-and-stale-process-detection`, where the old process
answers with ITS compiled-in behaviour (old RPC endpoint, old constants).

It is HEADLESS on purpose: it registers plain `Gio.Application`s on the session
bus, no GTK and no window, so it isolates the ONE rule that decides the
hand-off (D-Bus well-known-name ownership, keyed on the application id) from
everything else werust does. Running the two real binaries is the end-to-end
confirmation (see README.md); this is the cheap, repeatable check.

Usage (needs python3-gi and a session bus):

    ./app-id-uniqueness.py <running-id> <launched-id>

    ./app-id-uniqueness.py com.github.wighawag.werust com.github.wighawag.werust
    ./app-id-uniqueness.py com.github.wighawag.werust.v0_2_8 \
                           com.github.wighawag.werust.v0_2_9

Recorded output for both of those is in README.md.
"""
import subprocess
import sys

import gi

gi.require_version("Gio", "2.0")
from gi.repository import GLib, Gio  # noqa: E402


def register(app_id):
    """Register on the session bus; report whether THIS process is the primary
    instance or a remote one (a running instance already owns the id).

    The application is returned alongside the verdict because dropping it
    releases the bus name.
    """
    app = Gio.Application(
        application_id=app_id, flags=Gio.ApplicationFlags.DEFAULT_FLAGS
    )
    app.register(None)
    return app, ("remote" if app.get_is_remote() else "primary")


def hold(app_id, seconds):
    """Stand in for the ALREADY-RUNNING release: own the id and stay alive."""
    app, state = register(app_id)
    print(state, flush=True)
    if state != "primary":
        return 2
    # A real primary instance runs a main loop, so it can ANSWER the D-Bus
    # activation a same-id launch sends it. That answer IS the hand-off.
    loop = GLib.MainLoop()
    GLib.timeout_add_seconds(int(float(seconds)), loop.quit)
    loop.run()
    del app
    return 0


def main():
    if sys.argv[1] == "hold":
        return hold(sys.argv[2], sys.argv[3])
    if sys.argv[1] == "probe":
        print(register(sys.argv[2])[1])
        return 0

    running, launched = sys.argv[1], sys.argv[2]
    holder = subprocess.Popen(
        [sys.executable, __file__, "hold", running, "10"],
        stdout=subprocess.PIPE,
        text=True,
    )
    assert (
        holder.stdout.readline().strip() == "primary"
    ), "the already-running release must own its id"
    # Probe from SEPARATE processes: two applications inside one process share a
    # bus connection, which is not the situation being measured.
    for label, app_id in (("same version", running), ("new version", launched)):
        probe = subprocess.run(
            [sys.executable, __file__, "probe", app_id],
            capture_output=True,
            text=True,
        )
        print(f"{label:13} {app_id:38} -> {probe.stdout.strip()}")
    holder.kill()
    return 0


sys.exit(main())

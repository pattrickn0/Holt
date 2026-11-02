---
title: HOLT_SMOKE screenshots on Linux/Wayland need the X11 backend
summary: On a Wayland session the window never draws and HOLT_SMOKE hangs; run with WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11 (and a timeout).
touches: [crates/app/src/smoke.rs, .claude/skills/holt-ui/SKILL.md]
---
`HOLT_SMOKE=<dir> ./target/debug/holt` only saves `screen.png` after the first frames are drawn. Under a GNOME Wayland
session started from a sandboxed shell the window is never presented, so the app sits at `read_command;` forever and
no screenshot appears.

**Why:** no frame callbacks arrive for a window the compositor never maps; the smoke timer is armed from the draw loop.
**How to apply:** `timeout 40 env WAYLAND_DISPLAY= WINIT_UNIX_BACKEND=x11 HOLT_HOME=… HOLT_SMOKE=… HOLT_DEBUG_UI=… ./target/debug/holt`
(Xwayland on `DISPLAY=:0` is enough, no xvfb needed). The window may open larger than a requested `windowWidth` below ~820 px.
Never `pkill holt`: the user's own Holt runs under the same name; stop only the process you started.

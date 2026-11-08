---
title: A window that says it is visible can still get no frame
summary: The Deck work must not depend on RedrawRequested; Background keeps a 150 ms watchdog that runs it when the requested frame does not come.
touches: [crates/app/src/app.rs]
---
`hidden()` only sees what the windowing system reports (minimized, invisible, zero area). A window can report
itself visible and still receive no frame: occluded, on another GNOME workspace, or minimized on a backend that does
not report it (Wayland reports neither `is_minimized` nor reliably `is_visible`). On 2026-11-01 that stalled the app
for an hour: `CoreEvent::Deliver` arrived (the brief of a task started with `ask-agent --task new --start`), the loop
called `request_redraw`, no `RedrawRequested` ever came, and `Deck::work` never ran — so no tab opened and
`POST /activate` seemed to do nothing.

**Why:** the whole delivery, queue, timer and tray logic lived in the frame; hiding the window worked because
`set_visible(false)`/parking is detected, but being *occluded* is not.

**How to apply:** anything that changes state must go through `App::request_work` (never a bare `request_redraw`),
and a visible window that does not draw is covered by `Background::pending` + `FRAME_WATCHDOG` (150 ms): the work
runs in `about_to_wait` and the frame, when it finally comes, clears it. Deadlines (`ControlFlow::WaitUntil` →
`ResumeTimeReached`) also call `request_work`, not `request_redraw`. Keep the unit tests of `Background` updated;
the real behavior cannot be reproduced in a test (it needs a window manager).

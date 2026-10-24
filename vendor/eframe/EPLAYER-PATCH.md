# EPlayer local eframe 0.31.1 patch

The crate is copied from the pinned Cargo registry source. That published crate omitted license text files. LICENSE-MIT and LICENSE-APACHE were retrieved from https://github.com/emilk/egui/tree/0.31.1 on 2026-10-23 and are included by the portable packaging script, together with this modification notice.

`src/native/run.rs` services scheduled redraws directly for hidden Windows windows instead of relying on `Window::request_redraw`, which does not produce a paint event for the hidden root. The normal event-loop context and result handling are preserved. Visible windows retain upstream behavior.

Reproduction: play local audio with an immediate Mini viewport, hide the root to the tray, and observe the Mini and lyric clock stop updating and queued restore actions not being handled. The GUI diagnostic log also stops at the last visible root update. This patch must be verified with continued hidden-root updates, FFT gating, child controls and tray/hotkey restoration; audio-only tests do not cover it.

Keep this patch scoped to the pinned version. Re-evaluate it before upgrading eframe.

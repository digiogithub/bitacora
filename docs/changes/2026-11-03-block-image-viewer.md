---
created_at: 2026-11-03T13:25:17.918557108Z
updated_at: 2026-11-03T13:25:22.39905117Z
tags:
    - change
    - feature
    - app
---
Feature: full-window viewer for block images. Follows [[block-editor]] and [[2026-10-30-block-image-row-height]].

Hover on a local block image shows a maximize button; click opens an overlay covering the app (image fitted to 92% of the window, never upscaled). Wheel zooms around the pointer (0.1x-20x), drag pans, +/-/0 and double-click zoom/reset, Escape or close button closes and restores focus.

Files: crates/bitacora-app/src/views/image_viewer.rs (new: ImageViewer, ImageViewerSlot, open_image, displayed_size), views/block_view.rs (BlockImage hover group + button), views/workspace.rs (image_viewer field, slot global, render, close_topmost_overlay), views/dims.rs (VIEWER_BACKDROP, ON_IMAGE_*), ui/mod.rs (canvas re-exports ScrollDelta, TouchPhase), assets/locales/*.yml (image_viewer.*), docs/design/block-editor.md §8.

Verified: cargo test -p bitacora-app 700 passed (new image_viewer_zooms_with_the_wheel_and_closes_on_escape, large_images_fit_the_viewport_and_small_ones_stay_natural); clippy -D warnings clean. No real-window check (no GUI session on host).
---
created_at: 2026-10-30T14:59:49.009421466Z
updated_at: 2026-10-30T14:59:49.009421466Z
tags:
    - change
    - fix
    - app
---
# Fix: blank space under capped block images

Follows [[block-editor]].

- **Symptom:** a block with an image wider than 640px left blank space below it, as tall as the image's natural height minus its scaled height.
- **Cause:** GPUI `img` turns an `auto` height into the natural height. `max_w(640)` capped only the width, so the list row was measured with the natural height. Reproduced with a 1024x1024 PNG: row 1024px for a 640px image.
- **Change:** `crates/bitacora-app/src/views/block_view.rs`: new `BlockImage` (`RenderOnce`) reads the decoded size through `window.use_asset::<ImgResourceLoader>` and sets both width and height from the new `fitted_image_size` (block `{:width :height}`, else natural size, aspect ratio kept, scaled to 640px, no upscaling). `image_element` builds it. SVGs are divided by `SMOOTH_SVG_SCALE_FACTOR`, because the public `RenderImage::size` is the rasterized size. While the image loads, the old sizing applies. `crates/bitacora-app/src/ui/mod.rs`: `text_edit` re-exports `ImgResourceLoader`, `Resource`, `SMOOTH_SVG_SCALE_FACTOR`.
- **Known limit:** the cap stays a fixed 640px, not relative to the pane, as before.
- **Verification:** `look_tests::a_capped_image_takes_only_its_scaled_height` (real `gpui::list`; failed with 1024px before the fix, 640px after) and `image_size_keeps_the_aspect_ratio_within_the_cap`. `cargo test -p bitacora-app --lib`: 698 passed. `cargo clippy -p bitacora-app --all-targets -- -D warnings` clean. Not checked in a real window (no GUI session on the host).

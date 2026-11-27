# Meteor Blitz 1.2.3

Fixes macOS multi-monitor / HiDPI launch window sizing when a high resolution preset (such as `3840x2160`) is saved.

## Fixed
- **macOS window sizing & aspect clamping (`src/gfx.rs`):**
  - `set_window_resolution` now queries the window's actual `NSScreen` (instead of always `NSScreen::mainScreen`), converts to points using that display's `backingScaleFactor`, clamps uniformly to `visibleFrame` while preserving the requested aspect ratio (preventing `3840x2160` on a `1×` 1080p monitor from becoming an oversized `3840×1050` window with massive left pillarbox and right-edge clipping), and centres it immediately with `animate: false`.
  - `Canvas::placement()` and `window_view_w()` now read live `miniquad::window::screen_size() / dpi_scale()` directly so they always agree with `set_default_camera()`'s projection matrix on frame 1 and across monitor/DPI switches.
  - `Resolution: Window` now sizes the offscreen canvas from physical pixels (`screen_size()`) rather than logical points, rendering at true native Retina resolution.

## Verification
- `cargo test`: **39 / 39 passed** (0 warnings on native and `wasm32-unknown-unknown`).
- Verified live window bounds on a `1920×1080` `1×` display with `MB_RESOLUTION=3840x2160`: `1810×1050` (exact 16:9 content, centred) and on a 5K `2×` display: exact `3840×2160` physical backbuffer.

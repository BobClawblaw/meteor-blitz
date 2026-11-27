# Meteor Blitz 1.2.0 — Graphics Quality Overhaul

A complete visual overhaul of the 8× art assets, lighting, bloom, planet shaders, and combat effects, while keeping the original simulation, tuning, and classic 480×320 fallback intact.

Read the full interactive before/after technical review on GitHub Pages: **https://bobclawblaw.github.io/meteor-blitz/review.html**

## Highlights

- **Edge-bled Real-ESRGAN 8× art & HD mipmaps.** Replaced the previous `xBR 4× → xBR 2×` chain (which caused onion-ring alpha banding and painterly smearing on pre-rendered 3D sprites and chrome UI) with edge-bled two-pass `realesrgan-x4plus` for sprites, `realesrgan-x4plus-anime` for UI/font, SVG-direct 128×128 for the GitHub mark, and hybrid alpha. Added trilinear mipmaps and parallel PNG predecoding for HD sprites/UI, plus smooth linear HD typography across the HUD and pause menu.
- **5-level pyramid bloom & ACES filmic tone mapping.** Replaced the single 1/4-res blur (and fixed a `u8` strength clamping bug) with a 5-mip progressive downsample/upsample bloom pyramid composited inside the post shader so shockwaves bend both the scene and glow. Added calibrated Narkowicz ACES tone mapping (preventing flat white explosion clipping) and 1-LSB dithering against dark-gradient banding.
- **Planet atmosphere, cloud shadows, molten lava & parallax starfield.** Added parallax cloud drop shadows on Worlds 1 & 3, sunlit + dynamic ocean speculars, animated pulsing incandescent magma fissures on World 2, deeper crater relief on Worlds 4 & 5, per-world atmospheric Fresnel limb haze, dynamic light scattering through clouds, and a 3-layer procedural twinkling parallax starfield in space and menus.
- **3D parallax ground shadows & per-pixel PBR sprite lighting.** Entities on planet worlds cast soft parallax drop shadows onto the surface below. Upgraded sprite lighting from flat center-point tinting to per-pixel 3D dome + luminance-bump normals shaded by key light, hemisphere fill, and all 12 dynamic point lights with Blinn-Phong speculars, Fresnel rim highlights, crystalline ice glints, metallic craft sheen, and molten glowing fissures on fire rocks.
- **Real-time 2D Navier-Stokes fluid gas & volumetric smoke.** Explosions, ship thrusters, boosts, flamethrower streams, bullet impacts, and the gravity gun now drive a live 128×128 toroidal fluid simulation (`src/gas.rs`) with vorticity confinement and incompressible pressure projection, rendered at 4K through a dual-phase flow-advected FBM volumetric shader with 3D self-shadowing and illumination from all 12 dynamic lights. Flying through smoke clouds physically parts and swirls them in your ship's wake, while white-hot sparks and debris chips advect through the live fluid eddies. Enabled by default (toggleable via Options > Particle style).
- **Procedural energy shaders & combat polish.** Added a spherical rotating hexagonal Fresnel energy shield shader, pulsing cyan-violet gravity gun tractor-beam shader, continuous tapered two-layer plasma engine ribbon, weapon-coloured muzzle flashes, and cooling molten-ember cores on fresh scorch marks.

All shader and lighting upgrades are live in the browser version as well (**https://bobclawblaw.github.io/meteor-blitz/**).

## Downloads

- **macOS** (`MeteorBlitz-1.2.0-macos.dmg`): universal app for Apple Silicon and Intel, macOS 11 or later. Open the DMG and drag Meteor Blitz to Applications. The app is signed ad hoc only, so on first launch right-click the app and choose Open, or allow it under System Settings > Privacy & Security.

Settings, bindings, stats and scores carry over from earlier versions.

The original game and assets are © Alley Labs. This is a free, non-commercial fan port.

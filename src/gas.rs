//! High-resolution GPU toroidal Navier-Stokes fluid-gas queue (`Gas Physics`
//! particle style).
//!
//! `GasQueue` stays pure CPU/data so gameplay simulation and unit tests run
//! cleanly without a GL context; `gfx::GpuFluid` consumes the queued splats
//! each frame and steps a 256x256 velocity/pressure + 512x512 MacCormack
//! dye/plasma Navier-Stokes solver entirely on the GPU in `RGBA16F` textures
//! with hardware toroidal wrapping (`GL_REPEAT`) matching the 1024x1024 arena.

use macroquad::prelude::*;

/// Arena size in world units (matches `play::WORLD`).
pub const WORLD: f32 = 1024.0;
const INV_WORLD: f32 = 1.0 / WORLD;

/// Maximum splats queued per frame so extreme bomb chains stay bounded.
const MAX_SPLATS: usize = 96;

/// One GPU fluid impulse + HDR plasma/smoke injection in toroidal UV space.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct FluidSplat {
    /// Centre in normalized toroidal UV `[0, 1)`.
    pub pos: Vec2,
    /// Gaussian radius in UV units (`world_radius / 1024.0`).
    pub radius: f32,
    /// Directional velocity impulse in UV/s.
    pub force: Vec2,
    /// Rotational vortex spin around `pos` (UV/s at 1 radius).
    pub swirl: f32,
    /// HDR plasma/gas emission colour (values > 1.0 bloom intensely).
    pub color: Vec3,
    /// Optical smoke/vapour density contribution.
    pub smoke: f32,
}

#[inline]
fn to_uv(pos: Vec2) -> Vec2 {
    vec2(
        (pos.x * INV_WORLD).rem_euclid(1.0),
        (pos.y * INV_WORLD).rem_euclid(1.0),
    )
}

#[inline]
fn wrap_delta_world(from: Vec2, to: Vec2) -> Vec2 {
    let mut d = to - from;
    if d.x > WORLD * 0.5 {
        d.x -= WORLD;
    } else if d.x < -WORLD * 0.5 {
        d.x += WORLD;
    }
    if d.y > WORLD * 0.5 {
        d.y -= WORLD;
    } else if d.y < -WORLD * 0.5 {
        d.y += WORLD;
    }
    d
}

pub struct GasQueue {
    splats: Vec<FluidSplat>,
    pending_steps: u32,
    needs_clear: bool,
    active_timer: f32,
    seed: u32,
}

impl GasQueue {
    pub fn new() -> GasQueue {
        GasQueue {
            splats: Vec::with_capacity(64),
            pending_steps: 0,
            needs_clear: true,
            active_timer: 0.0,
            seed: 0x6c8e_9cf5,
        }
    }

    pub fn clear(&mut self) {
        self.splats.clear();
        self.pending_steps = 0;
        self.needs_clear = true;
        self.active_timer = 0.0;
    }

    #[allow(dead_code)]
    pub fn is_active(&self) -> bool {
        self.active_timer > 0.0 || !self.splats.is_empty()
    }

    pub fn step(&mut self, dt: f32) {
        if self.active_timer > 0.0 || !self.splats.is_empty() {
            self.pending_steps = (self.pending_steps + 1).min(3);
            self.active_timer = (self.active_timer - dt).max(0.0);
        }
    }

    /// Consumes queued splats, pending simulation steps, and clear flag for
    /// `gfx::GpuFluid`.
    pub fn take(&mut self) -> (bool, u32, Vec<FluidSplat>) {
        let clear = self.needs_clear;
        self.needs_clear = false;
        let steps = self.pending_steps;
        self.pending_steps = 0;
        let splats = std::mem::take(&mut self.splats);
        (clear, steps, splats)
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    fn range(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * self.rand()
    }

    fn push(&mut self, splat: FluidSplat) {
        if self.splats.len() < MAX_SPLATS {
            self.splats.push(splat);
        }
        self.active_timer = self.active_timer.max(8.5);
    }

    /// Asymmetric multi-jet + vortex-dipole explosion: launches narrow,
    /// high-shear plasma jets with alternating swirl so blasts immediately
    /// roll into curling tendrils and Kelvin-Helmholtz eddies instead of
    /// static round splotches.
    pub fn blast(
        &mut self,
        pos: Vec2,
        shot_vel: Vec2,
        radius: f32,
        tint: Vec3,
        tint_amount: f32,
        smoke_opacity: f32,
        heat: f32,
    ) {
        let r = radius.clamp(14.0, 140.0);
        let jets = ((6.0 + r * 0.05) as usize).clamp(6, 10);
        let base_ang = self.rand() * std::f32::consts::TAU;
        let carry = (shot_vel * 0.35).clamp_length_max(460.0);
        // Rich saturated base colour (keeps delicate translucent curls instead of clipping solid white).
        let fire_core = vec3(1.15, 0.42, 0.08);
        let plasma_col = fire_core.lerp(tint * 1.05, (tint_amount * 0.88).clamp(0.0, 0.94)) * (0.55 + 0.55 * heat);

        for k in 0..jets {
            let frac = k as f32 / jets as f32;
            let ang = base_ang + frac * std::f32::consts::TAU + self.range(-0.28, 0.28);
            let dir = Vec2::from_angle(ang);
            // High-speed outward jets + alternating dipole spin immediately stretch
            // into sweeping, curling Kelvin-Helmholtz plasma ribbons.
            let jet_speed = self.range(520.0, 1080.0) * (0.72 + r / 110.0);
            let vel = dir * jet_speed + carry;
            let offset = dir * self.range(4.0, r * 0.38);
            let jet_r = self.range(3.6, 7.2) * (0.75 + r / 140.0);
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            let swirl = sign * self.range(1.15, 2.25) * (0.8 + r / 120.0);
            let col_var = self.range(0.70, 1.35);
            let smoke_amt = smoke_opacity * self.range(0.32, 0.65);
            self.push(FluidSplat {
                pos: to_uv(pos + offset),
                radius: jet_r * INV_WORLD,
                force: vel * INV_WORLD,
                swirl,
                color: plasma_col * col_var,
                smoke: smoke_amt,
            });
        }
    }

    /// Tight directional plasma jet for weapon impacts & ricochets.
    pub fn impact(&mut self, pos: Vec2, shot_vel: Vec2, tint: Vec3, tint_amount: f32, heat: f32) {
        let speed = shot_vel.length().max(180.0);
        let dir = if speed > 1.0 { shot_vel / speed } else { vec2(0.0, -1.0) };
        let fire = vec3(1.15, 0.48, 0.12);
        let col = fire.lerp(tint * 1.15, (tint_amount * 0.9).clamp(0.0, 0.95)) * (0.55 + 0.45 * heat);
        for k in 0..2 {
            let spread = if k == 0 { self.range(-0.45, -0.08) } else { self.range(0.08, 0.45) };
            let v = Vec2::from_angle(spread).rotate(dir) * speed.min(720.0) * 0.95;
            let sign = if k == 0 { 1.0 } else { -1.0 };
            let rad = self.range(3.2, 5.4) * INV_WORLD;
            let swirl = sign * self.range(0.75, 1.45);
            self.push(FluidSplat {
                pos: to_uv(pos + dir * 3.0),
                radius: rad,
                force: v * INV_WORLD,
                swirl,
                color: col * 0.85,
                smoke: 0.25,
            });
        }
    }

    /// Razor-thin luminous plasma wake left behind flying shots (especially
    /// flamethrower streams and ionized laser bolts).
    pub fn bullet_wake(&mut self, pos: Vec2, vel: Vec2, color: Vec3, radius_world: f32, smoke: f32) {
        let sign = if self.rand() < 0.5 { 1.0 } else { -1.0 };
        let force = vel.clamp_length_max(680.0) * 0.72;
        let swirl = sign * self.range(0.45, 0.95);
        self.push(FluidSplat {
            pos: to_uv(pos),
            radius: radius_world.clamp(2.6, 8.5) * INV_WORLD,
            force: force * INV_WORLD,
            swirl,
            color: color * 0.45,
            smoke: smoke * 0.7,
        });
    }

    /// Continuous ship thruster plasma stream + aerodynamic wake that carves
    /// swirling Karman vortices whenever the ship flies through gas clouds.
    pub fn ship_exhaust(&mut self, pos: Vec2, ship_vel: Vec2, thrust_dir: Vec2, boosting: bool) {
        let speed = ship_vel.length();
        let jet_speed = if boosting { 640.0 } else { 320.0 + speed * 0.65 };
        let exhaust_col = if boosting {
            vec3(0.22, 1.15, 1.05)
        } else {
            vec3(0.10, 0.72, 0.48)
        };
        let tail = pos - thrust_dir * 10.5;
        let jitter = self.range(-0.22, 0.22);
        let jet_v = Vec2::from_angle(jitter).rotate(-thrust_dir) * jet_speed;
        let swirl = self.range(-0.85, 0.85);
        self.push(FluidSplat {
            pos: to_uv(tail),
            radius: (if boosting { 4.8 } else { 3.2 }) * INV_WORLD,
            force: jet_v * INV_WORLD,
            swirl,
            color: exhaust_col,
            smoke: if boosting { 0.22 } else { 0.12 },
        });
        // Pure velocity slipstream wake around the hull (adds zero dye, only
        // curls and parts existing explosion clouds as you weave through them).
        if speed > 30.0 {
            self.push(FluidSplat {
                pos: to_uv(pos),
                radius: 14.0 * INV_WORLD,
                force: (ship_vel * 1.15).clamp_length_max(540.0) * INV_WORLD,
                swirl: 0.0,
                color: Vec3::ZERO,
                smoke: 0.0,
            });
        }
    }

    /// Inward spiral accretion vortex for the gravity gun: pulls nearby
    /// luminous gas and smoke into a swirling vortex around the tractor beam.
    pub fn vortex(&mut self, center: Vec2, ship_pos: Vec2, radius: f32) {
        let to_ship = wrap_delta_world(center, ship_pos).normalize_or_zero();
        let base = self.rand() * std::f32::consts::TAU;
        for k in 0..3 {
            let ang = base + k as f32 * (std::f32::consts::TAU / 3.0);
            let arm = Vec2::from_angle(ang);
            let p = center + arm * (radius * 0.42);
            let tang = vec2(-arm.y, arm.x);
            let force = tang * 260.0 - arm * 120.0 + to_ship * 90.0;
            self.push(FluidSplat {
                pos: to_uv(p),
                radius: 12.0 * INV_WORLD,
                force: force * INV_WORLD,
                swirl: 0.95,
                color: vec3(0.18, 0.95, 0.55) * 0.45,
                smoke: 0.08,
            });
        }
    }

    /// Expanding toroidal vortex ring for bombs and boss deaths: blasts all
    /// existing gas on screen outward into a curling ring nebula.
    pub fn shock_ring(&mut self, pos: Vec2, radius: f32, color: Vec3) {
        let segs = 12;
        let base = self.rand() * std::f32::consts::TAU;
        for k in 0..segs {
            let ang = base + (k as f32 / segs as f32) * std::f32::consts::TAU;
            let dir = Vec2::from_angle(ang);
            let p = pos + dir * (radius * 0.25);
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            let col_var = self.range(0.85, 1.25);
            self.push(FluidSplat {
                pos: to_uv(p),
                radius: 11.0 * INV_WORLD,
                force: (dir * 620.0) * INV_WORLD,
                swirl: sign * 1.25,
                color: color * col_var,
                smoke: 0.55,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blast_generates_asymmetric_vortex_jets_and_drains_cleanly() {
        let mut q = GasQueue::new();
        q.blast(
            vec2(100.0, 200.0),
            vec2(400.0, 0.0),
            60.0,
            vec3(1.0, 0.55, 0.2),
            0.25,
            0.85,
            1.0,
        );
        assert!(q.is_active());
        q.step(1.0 / 60.0);
        let (clear, steps, splats) = q.take();
        assert!(clear);
        assert_eq!(steps, 1);
        assert!(splats.len() >= 5, "multi-jet blast emits multiple jets");
        // Both positive and negative vortex dipoles should exist so blasts curl.
        assert!(splats.iter().any(|s| s.swirl > 0.0));
        assert!(splats.iter().any(|s| s.swirl < 0.0));
        assert!(splats.iter().all(|s| s.pos.x >= 0.0 && s.pos.x < 1.0 && s.pos.y >= 0.0 && s.pos.y < 1.0));
    }
}

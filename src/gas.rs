//! High-resolution GPU toroidal Navier-Stokes fluid-gas queue (`Gas Physics`
//! particle style).
//!
//! `GasQueue` stays pure CPU/data so gameplay simulation and unit tests run
//! cleanly without a GL context; `gfx::GpuFluid` consumes the queued splats
//! each frame and steps a 256x256 velocity/pressure + 512x512 MacCormack
//! luminous plasma/gas Navier-Stokes solver entirely on the GPU in `RGBA16F`
//! textures with hardware `GL_REPEAT` toroidal wrapping matching the 1024x1024
//! arena.

use macroquad::prelude::*;

/// Arena size in world units (matches `play::WORLD`).
pub const WORLD: f32 = 1024.0;
const INV_WORLD: f32 = 1.0 / WORLD;

/// Maximum splats queued per frame so extreme bomb chains stay bounded.
const MAX_SPLATS: usize = 96;

/// One GPU fluid impulse + HDR luminous gas injection in toroidal UV space.
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
    /// HDR luminous gas/plasma colour (values > 1.0 bloom intensely).
    pub color: Vec3,
    /// Energy weight (decays with `color` so curls vanish cleanly with zero
    /// lingering muddy grey splotches).
    pub energy: f32,
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
        self.active_timer = self.active_timer.max(6.5);
    }

    /// Asymmetric multi-jet + vortex-dipole explosion: launches narrow,
    /// high-shear plasma jets with alternating swirl so blasts immediately
    /// roll into curling luminous tendrils instead of round splotches.
    pub fn blast(
        &mut self,
        pos: Vec2,
        shot_vel: Vec2,
        radius: f32,
        tint: Vec3,
        tint_amount: f32,
        heat: f32,
    ) {
        let r = radius.clamp(14.0, 140.0);
        let jets = ((6.0 + r * 0.05) as usize).clamp(6, 10);
        let base_ang = self.rand() * std::f32::consts::TAU;
        let carry = (shot_vel * 0.32).clamp_length_max(420.0);
        let fire_core = vec3(1.25, 0.46, 0.09);
        let plasma_col =
            fire_core.lerp(tint * 1.15, (tint_amount * 0.88).clamp(0.0, 0.94)) * (0.55 + 0.55 * heat);

        for k in 0..jets {
            let frac = k as f32 / jets as f32;
            let ang = base_ang + frac * std::f32::consts::TAU + self.range(-0.28, 0.28);
            let dir = Vec2::from_angle(ang);
            let jet_speed = self.range(540.0, 1100.0) * (0.72 + r / 110.0);
            let vel = dir * jet_speed + carry;
            let offset = dir * self.range(4.0, r * 0.36);
            let jet_r = self.range(3.4, 6.8) * (0.75 + r / 140.0);
            let sign = if k % 2 == 0 { 1.0 } else { -1.0 };
            let swirl = sign * self.range(1.25, 2.35) * (0.8 + r / 120.0);
            let col_var = self.range(0.75, 1.35);
            let energy = self.range(0.7, 1.1) * heat.max(0.4);
            self.push(FluidSplat {
                pos: to_uv(pos + offset),
                radius: jet_r * INV_WORLD,
                force: vel * INV_WORLD,
                swirl,
                color: plasma_col * col_var,
                energy,
            });
        }
    }

    /// Tight directional twin-vortex plasma jet for weapon impacts.
    pub fn impact(&mut self, pos: Vec2, shot_vel: Vec2, tint: Vec3, tint_amount: f32, heat: f32) {
        let speed = shot_vel.length().max(180.0);
        let dir = if speed > 1.0 { shot_vel / speed } else { vec2(0.0, -1.0) };
        let fire = vec3(1.2, 0.5, 0.12);
        let col = fire.lerp(tint * 1.2, (tint_amount * 0.9).clamp(0.0, 0.95)) * (0.55 + 0.45 * heat);
        for k in 0..2 {
            let spread = if k == 0 { self.range(-0.42, -0.08) } else { self.range(0.08, 0.42) };
            let v = Vec2::from_angle(spread).rotate(dir) * speed.min(720.0) * 0.9;
            let sign = if k == 0 { 1.0 } else { -1.0 };
            let rad = self.range(3.0, 5.0) * INV_WORLD;
            let swirl = sign * self.range(0.8, 1.45);
            self.push(FluidSplat {
                pos: to_uv(pos + dir * 2.5),
                radius: rad,
                force: v * INV_WORLD,
                swirl,
                color: col * 0.85,
                energy: 0.7,
            });
        }
    }

    /// Subtle curling heat-wisp stream for the flamethrower weapon. Because
    /// 20-40 flame projectiles are in flight every tick, we sample a fraction
    /// per tick with low dye intensity so the gas complements the flame
    /// sprites instead of overwhelming the screen.
    pub fn flame_stream(&mut self, prev: Vec2, pos: Vec2, vel: Vec2) {
        if self.rand() > 0.28 {
            return;
        }
        let delta = wrap_delta_world(prev, pos);
        let p = prev + delta * self.rand();
        let force = vel.clamp_length_max(520.0) * 0.36;
        let sign = if self.rand() < 0.5 { 1.0 } else { -1.0 };
        let swirl = sign * self.range(0.35, 0.72);
        let col = vec3(0.24, self.range(0.075, 0.13), 0.016);
        self.push(FluidSplat {
            pos: to_uv(p),
            radius: 3.1 * INV_WORLD,
            force: force * INV_WORLD,
            swirl,
            color: col,
            energy: 0.20,
        });
    }

    /// Subtle impact lick for flamethrower hits (50-100 hits/s on a rock, so
    /// kept far lighter than single-shot cannon/laser impacts).
    pub fn flame_impact(&mut self, pos: Vec2, shot_vel: Vec2, tint: Vec3, tint_amount: f32) {
        if self.rand() > 0.35 {
            return;
        }
        let speed = shot_vel.length().max(160.0);
        let dir = if speed > 1.0 { shot_vel / speed } else { vec2(0.0, -1.0) };
        let fire = vec3(0.34, 0.13, 0.03);
        let col = fire.lerp(tint * 0.36, (tint_amount * 0.85).clamp(0.0, 0.9));
        let spread = self.range(-0.35, 0.35);
        let v = Vec2::from_angle(spread).rotate(dir) * speed.min(480.0) * 0.55;
        let sign = if self.rand() < 0.5 { 1.0 } else { -1.0 };
        let swirl = sign * self.range(0.45, 0.90);
        self.push(FluidSplat {
            pos: to_uv(pos + dir * 2.0),
            radius: 3.2 * INV_WORLD,
            force: v * INV_WORLD,
            swirl,
            color: col,
            energy: 0.24,
        });
    }

    /// Pure velocity-only supersonic wake for fast cannon/laser shots: slices
    /// swirling eddies through existing explosion clouds without dropping dots
    /// in empty space.
    pub fn bullet_wake_vel(&mut self, pos: Vec2, vel: Vec2) {
        if !self.is_active() {
            return;
        }
        let force = vel.clamp_length_max(560.0) * 0.45;
        self.push(FluidSplat {
            pos: to_uv(pos),
            radius: 5.0 * INV_WORLD,
            force: force * INV_WORLD,
            swirl: 0.0,
            color: Vec3::ZERO,
            energy: 0.0,
        });
    }

    /// Ship tail thruster exhaust: strictly emitted behind the ship's nozzle
    /// (`-nose_dir`), interpolated smoothly along the step, shooting cleanly
    /// backward away from the hull (never forward past the nose).
    pub fn ship_exhaust(
        &mut self,
        prev_pos: Vec2,
        pos: Vec2,
        nose_dir: Vec2,
        speed: f32,
        boosting: bool,
    ) {
        let delta = wrap_delta_world(prev_pos, pos);
        let substeps = ((delta.length() / 3.5).ceil() as usize).clamp(1, 3);
        let tail_offset = -nose_dir * 12.5;
        let jet_speed = if boosting { 480.0 } else { 240.0 + speed * 0.35 };
        let base_col = if boosting {
            vec3(0.24, 1.05, 0.95)
        } else {
            vec3(0.10, 0.68, 0.45)
        } * (1.0 / substeps as f32);
        let rad = (if boosting { 3.8 } else { 2.6 }) * INV_WORLD;

        for i in 0..substeps {
            let t = (i as f32 + 0.5) / substeps as f32;
            let p = prev_pos + delta * t + tail_offset;
            let jitter = self.range(-0.14, 0.14);
            let jet_v = Vec2::from_angle(jitter).rotate(-nose_dir) * jet_speed;
            let swirl = self.range(-0.55, 0.55);
            self.push(FluidSplat {
                pos: to_uv(p),
                radius: rad,
                force: jet_v * INV_WORLD,
                swirl,
                color: base_col,
                energy: (if boosting { 0.8 } else { 0.5 }) / substeps as f32,
            });
        }
    }

    /// Inward spiral accretion vortex for the gravity gun: pulls nearby
    /// luminous gas into a swirling vortex around the tractor beam.
    pub fn vortex(&mut self, center: Vec2, ship_pos: Vec2, radius: f32) {
        let to_ship = wrap_delta_world(center, ship_pos).normalize_or_zero();
        let base = self.rand() * std::f32::consts::TAU;
        for k in 0..3 {
            let ang = base + k as f32 * (std::f32::consts::TAU / 3.0);
            let arm = Vec2::from_angle(ang);
            let p = center + arm * (radius * 0.42);
            let tang = vec2(-arm.y, arm.x);
            let force = tang * 320.0 - arm * 140.0 + to_ship * 110.0;
            self.push(FluidSplat {
                pos: to_uv(p),
                radius: 9.0 * INV_WORLD,
                force: force * INV_WORLD,
                swirl: 1.15,
                color: vec3(0.10, 0.65, 0.85) * 0.28,
                energy: 0.32,
            });
        }
    }

    /// Expanding toroidal vortex ring for bombs and boss deaths.
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
                radius: 8.0 * INV_WORLD,
                force: (dir * 780.0) * INV_WORLD,
                swirl: sign * 1.45,
                color: color * col_var,
                energy: 1.0,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ship_exhaust_shoots_strictly_backward_from_tail() {
        let mut q = GasQueue::new();
        let nose = vec2(0.0, -1.0); // pointing up (-Y)
        q.ship_exhaust(vec2(500.0, 500.0), vec2(500.0, 496.0), nose, 240.0, false);
        let (_, _, splats) = q.take();
        assert!(!splats.is_empty());
        for s in &splats {
            // Tail is at y > 500 (below the ship), and force pushes further +Y (downward, away from nose).
            assert!(s.pos.y * WORLD > 505.0, "exhaust must spawn behind tail: {:?}", s.pos);
            assert!(s.force.y > 0.0, "exhaust must push away from tail, not forward: {:?}", s.force);
        }
    }

    #[test]
    fn blast_generates_asymmetric_vortex_jets() {
        let mut q = GasQueue::new();
        q.blast(vec2(100.0, 200.0), vec2(400.0, 0.0), 60.0, vec3(1.0, 0.55, 0.2), 0.25, 1.0);
        q.step(1.0 / 60.0);
        let (clear, steps, splats) = q.take();
        assert!(clear);
        assert_eq!(steps, 1);
        assert!(splats.len() >= 6);
        assert!(splats.iter().any(|s| s.swirl > 0.0));
        assert!(splats.iter().any(|s| s.swirl < 0.0));
    }
}

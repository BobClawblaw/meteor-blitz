//! Real-time 2D toroidal Eulerian fluid gas simulation (Jos Stam Stable Fluids
//! with vorticity confinement and dual-phase flow rendering).
//!
//! The simulation grid wraps periodically in both X and Y, matching Meteor
//! Blitz's 1024x1024 toroidal world 1-to-1 with zero boundary seams.
//! Explosions, ship exhaust, weapons, shockwaves, and the gravity gun inject
//! velocity, vorticity, coloured vapour density, and incandescent heat into
//! the field; moving hulls and shots physically wake and swirl existing smoke.

use macroquad::prelude::*;

/// Grid cells per side (128x128 = 16,384 cells; 8 world units per cell).
pub const N: usize = 128;
const CELLS: usize = N * N;
const WORLD: f32 = 1024.0;
const INV_CELL: f32 = N as f32 / WORLD;
const CELL_SIZE: f32 = WORLD / N as f32;

/// Vorticity confinement strength: amplifies swirling Kelvin-Helmholtz eddies
/// so blasts roll into curling smoke tendrils instead of diffusing flat.
const VORTICITY: f32 = 17.5;
/// Pressure projection iterations (6 before + 4 after advection = sharp,
/// divergence-free vortex rings at ~0.3 ms total CPU in release builds).
const PROJ_ITERS_PRE: usize = 6;
const PROJ_ITERS_POST: usize = 4;
/// Clamp grid velocity (cells/s) to keep semi-Lagrangian advection smooth.
const MAX_VEL: f32 = 85.0;

#[inline]
fn wrap_coord(v: f32) -> f32 {
    let u = v * INV_CELL;
    u.rem_euclid(N as f32)
}

#[inline]
fn wrap_delta(d: f32) -> f32 {
    let half = N as f32 * 0.5;
    let mut r = d;
    if r > half {
        r -= N as f32;
    } else if r < -half {
        r += N as f32;
    }
    r
}

/// Blackbody fire ramp (temperature 0..1 -> crimson, orange, yellow, white-hot).
pub fn blackbody(t: f32) -> Vec3 {
    const STOPS: [(f32, Vec3); 5] = [
        (0.0, Vec3::new(0.28, 0.03, 0.01)),
        (0.28, Vec3::new(0.95, 0.16, 0.02)),
        (0.55, Vec3::new(1.0, 0.52, 0.08)),
        (0.80, Vec3::new(1.0, 0.85, 0.36)),
        (1.0, Vec3::new(1.0, 0.97, 0.88)),
    ];
    let t = t.clamp(0.0, 1.0);
    let i = STOPS.iter().rposition(|s| s.0 <= t).unwrap_or(0).min(3);
    let (a, b) = (STOPS[i], STOPS[i + 1]);
    a.1.lerp(b.1, (t - a.0) / (b.0 - a.0))
}

pub struct GasField {
    vx: Vec<f32>,
    vy: Vec<f32>,
    vx0: Vec<f32>,
    vy0: Vec<f32>,
    div: Vec<f32>,
    p: Vec<f32>,
    curl: Vec<f32>,
    /// Premultiplied gas/smoke RGB dye.
    r: Vec<f32>,
    g: Vec<f32>,
    b: Vec<f32>,
    r0: Vec<f32>,
    g0: Vec<f32>,
    b0: Vec<f32>,
    /// Optical vapour/smoke density.
    density: Vec<f32>,
    d0: Vec<f32>,
    /// Incandescent temperature (0..2.5), cooling rapidly into smoke.
    heat: Vec<f32>,
    h0: Vec<f32>,
    /// Tint colour of incandescent gas (blended with blackbody for cyan ice/laser plasma).
    tint_r: Vec<f32>,
    tint_g: Vec<f32>,
    tint_b: Vec<f32>,
    tr0: Vec<f32>,
    tg0: Vec<f32>,
    tb0: Vec<f32>,
    tint_w: Vec<f32>,
    tw0: Vec<f32>,
    /// True while any cell has visible density, heat, or velocity.
    active: bool,
    seed: u32,
}

impl GasField {
    pub fn new() -> GasField {
        GasField {
            vx: vec![0.0; CELLS],
            vy: vec![0.0; CELLS],
            vx0: vec![0.0; CELLS],
            vy0: vec![0.0; CELLS],
            div: vec![0.0; CELLS],
            p: vec![0.0; CELLS],
            curl: vec![0.0; CELLS],
            r: vec![0.0; CELLS],
            g: vec![0.0; CELLS],
            b: vec![0.0; CELLS],
            r0: vec![0.0; CELLS],
            g0: vec![0.0; CELLS],
            b0: vec![0.0; CELLS],
            density: vec![0.0; CELLS],
            d0: vec![0.0; CELLS],
            heat: vec![0.0; CELLS],
            h0: vec![0.0; CELLS],
            tint_r: vec![1.0; CELLS],
            tint_g: vec![1.0; CELLS],
            tint_b: vec![1.0; CELLS],
            tr0: vec![1.0; CELLS],
            tg0: vec![1.0; CELLS],
            tb0: vec![1.0; CELLS],
            tint_w: vec![0.0; CELLS],
            tw0: vec![0.0; CELLS],
            active: false,
            seed: 0x6c8e_9cf5,
        }
    }

    pub fn clear(&mut self) {
        self.vx.fill(0.0);
        self.vy.fill(0.0);
        self.r.fill(0.0);
        self.g.fill(0.0);
        self.b.fill(0.0);
        self.density.fill(0.0);
        self.heat.fill(0.0);
        self.tint_w.fill(0.0);
        self.active = false;
    }

    pub fn is_active(&self) -> bool {
        self.active
    }

    fn rand(&mut self) -> f32 {
        self.seed ^= self.seed << 13;
        self.seed ^= self.seed >> 17;
        self.seed ^= self.seed << 5;
        (self.seed >> 8) as f32 / (1u32 << 24) as f32
    }

    /// Injects an explosion: radial blast pressure, inherited shot velocity,
    /// swirling dipole eddies (so clouds curl asymmetrically instead of forming
    /// plain circles), coloured smoke/vapour, and incandescent plasma heat.
    pub fn blast(
        &mut self,
        pos: Vec2,
        shot_vel: Vec2,
        radius: f32,
        smoke_col: Vec3,
        smoke_density: f32,
        heat_tint: Vec3,
        tint_amount: f32,
        heat_strength: f32,
    ) {
        self.active = true;
        let cx = wrap_coord(pos.x);
        let cy = wrap_coord(pos.y);
        let rad = (radius * INV_CELL).clamp(1.6, 22.0);
        let ir = rad.ceil() as i32;
        let carry = (shot_vel * 0.28).clamp_length_max(420.0) * INV_CELL;
        let swirl_sign = if self.rand() < 0.5 { 1.0 } else { -1.0 };
        let phase = self.rand() * std::f32::consts::TAU;
        let radial_push = (18.0 + radius * 0.34).min(56.0);

        let ix0 = cx.floor() as i32;
        let iy0 = cy.floor() as i32;
        for dy in -ir..=ir {
            let gy = (iy0 + dy).rem_euclid(N as i32) as usize;
            let ry = wrap_delta(gy as f32 + 0.5 - cy);
            for dx in -ir..=ir {
                let gx = (ix0 + dx).rem_euclid(N as i32) as usize;
                let rx = wrap_delta(gx as f32 + 0.5 - cx);
                let dist = (rx * rx + ry * ry).sqrt();
                if dist >= rad {
                    continue;
                }
                let q = 1.0 - dist / rad;
                let w = q * q;
                let i = gy * N + gx;
                // Radial pressure + dipole curl so the plume rolls into organic billows.
                let inv_d = if dist > 0.05 { 1.0 / dist } else { 0.0 };
                let (nx, ny) = (rx * inv_d, ry * inv_d);
                let ang = ry.atan2(rx);
                let lobe = 0.72 + 0.42 * (ang * 3.0 + phase).sin() + 0.24 * (ang * 5.0 - phase * 1.7).cos();
                let swirl = swirl_sign * (0.55 + 0.45 * (ang * 2.0 + phase).cos()) * radial_push * 0.52;
                let add_vx = (nx * radial_push * lobe - ny * swirl) * w + carry.x * w;
                let add_vy = (ny * radial_push * lobe + nx * swirl) * w + carry.y * w;
                self.vx[i] = (self.vx[i] + add_vx).clamp(-MAX_VEL, MAX_VEL);
                self.vy[i] = (self.vy[i] + add_vy).clamp(-MAX_VEL, MAX_VEL);

                let d_add = smoke_density * (0.75 + 0.45 * lobe) * w * 1.15;
                self.density[i] = (self.density[i] + d_add).min(2.2);
                self.r[i] = (self.r[i] + smoke_col.x * d_add).min(2.2);
                self.g[i] = (self.g[i] + smoke_col.y * d_add).min(2.2);
                self.b[i] = (self.b[i] + smoke_col.z * d_add).min(2.2);

                // Hot inner fireball core with turbulent lobes.
                let core = (1.0 - dist / (rad * 0.85)).max(0.0);
                let h_add = heat_strength * core * core * (0.85 + 0.35 * lobe) * 1.85;
                if h_add > 0.005 {
                    let prev_h = self.heat[i];
                    let next_h = (prev_h + h_add).min(2.6);
                    let mix_k = h_add / next_h.max(1e-4);
                    self.heat[i] = next_h;
                    self.tint_r[i] = self.tint_r[i] + (heat_tint.x - self.tint_r[i]) * mix_k;
                    self.tint_g[i] = self.tint_g[i] + (heat_tint.y - self.tint_g[i]) * mix_k;
                    self.tint_b[i] = self.tint_b[i] + (heat_tint.z - self.tint_b[i]) * mix_k;
                    self.tint_w[i] = (self.tint_w[i] + (tint_amount - self.tint_w[i]) * mix_k).clamp(0.0, 1.0);
                }
            }
        }
    }

    /// Injects a directional stream of hot gas & vapour (ship afterburner,
    /// flamethrower jet, boost plume, bullet impact spray).
    pub fn jet(
        &mut self,
        pos: Vec2,
        vel: Vec2,
        radius: f32,
        smoke_col: Vec3,
        density: f32,
        heat_tint: Vec3,
        tint_amount: f32,
        heat: f32,
    ) {
        self.active = true;
        let cx = wrap_coord(pos.x);
        let cy = wrap_coord(pos.y);
        let rad = (radius * INV_CELL).clamp(1.1, 8.0);
        let ir = rad.ceil() as i32;
        let v = (vel * INV_CELL).clamp_length_max(MAX_VEL * 0.75);
        let ix0 = cx.floor() as i32;
        let iy0 = cy.floor() as i32;
        for dy in -ir..=ir {
            let gy = (iy0 + dy).rem_euclid(N as i32) as usize;
            let ry = wrap_delta(gy as f32 + 0.5 - cy);
            for dx in -ir..=ir {
                let gx = (ix0 + dx).rem_euclid(N as i32) as usize;
                let rx = wrap_delta(gx as f32 + 0.5 - cx);
                let dist = (rx * rx + ry * ry).sqrt();
                if dist >= rad {
                    continue;
                }
                let q = 1.0 - dist / rad;
                let w = q * q;
                let i = gy * N + gx;
                self.vx[i] = (self.vx[i] + v.x * w * 0.65).clamp(-MAX_VEL, MAX_VEL);
                self.vy[i] = (self.vy[i] + v.y * w * 0.65).clamp(-MAX_VEL, MAX_VEL);
                if density > 0.0 {
                    let d_add = density * w;
                    self.density[i] = (self.density[i] + d_add).min(2.0);
                    self.r[i] = (self.r[i] + smoke_col.x * d_add).min(2.0);
                    self.g[i] = (self.g[i] + smoke_col.y * d_add).min(2.0);
                    self.b[i] = (self.b[i] + smoke_col.z * d_add).min(2.0);
                }
                if heat > 0.0 {
                    let h_add = heat * w;
                    let next_h = (self.heat[i] + h_add).min(2.4);
                    let mix_k = h_add / next_h.max(1e-4);
                    self.heat[i] = next_h;
                    self.tint_r[i] = self.tint_r[i] + (heat_tint.x - self.tint_r[i]) * mix_k;
                    self.tint_g[i] = self.tint_g[i] + (heat_tint.y - self.tint_g[i]) * mix_k;
                    self.tint_b[i] = self.tint_b[i] + (heat_tint.z - self.tint_b[i]) * mix_k;
                    self.tint_w[i] = (self.tint_w[i] + (tint_amount - self.tint_w[i]) * mix_k).clamp(0.0, 1.0);
                }
            }
        }
    }

    /// Stirs existing gas without adding dye (ship hull, flying bullets, and
    /// tumbling asteroids physically part and swirl lingering smoke).
    pub fn wake(&mut self, pos: Vec2, vel: Vec2, radius: f32, strength: f32) {
        if !self.active || vel.length_squared() < 4.0 {
            return;
        }
        let cx = wrap_coord(pos.x);
        let cy = wrap_coord(pos.y);
        let rad = (radius * INV_CELL).clamp(1.2, 10.0);
        let ir = rad.ceil() as i32;
        let v = (vel * INV_CELL * strength).clamp_length_max(MAX_VEL * 0.55);
        let ix0 = cx.floor() as i32;
        let iy0 = cy.floor() as i32;
        for dy in -ir..=ir {
            let gy = (iy0 + dy).rem_euclid(N as i32) as usize;
            let ry = wrap_delta(gy as f32 + 0.5 - cy);
            for dx in -ir..=ir {
                let gx = (ix0 + dx).rem_euclid(N as i32) as usize;
                let rx = wrap_delta(gx as f32 + 0.5 - cx);
                let dist = (rx * rx + ry * ry).sqrt();
                if dist >= rad {
                    continue;
                }
                let q = 1.0 - dist / rad;
                let w = q * q;
                let i = gy * N + gx;
                self.vx[i] = (self.vx[i] + v.x * w).clamp(-MAX_VEL, MAX_VEL);
                self.vy[i] = (self.vy[i] + v.y * w).clamp(-MAX_VEL, MAX_VEL);
            }
        }
    }

    /// Gravity gun tractor suction: draws and swirls nearby gas toward the ship.
    pub fn vortex(&mut self, from: Vec2, toward: Vec2, radius: f32, pull: f32) {
        if !self.active {
            return;
        }
        let dir = (toward - from).normalize_or_zero();
        let side = dir.perp();
        let cx = wrap_coord(from.x);
        let cy = wrap_coord(from.y);
        let rad = (radius * INV_CELL).clamp(2.0, 12.0);
        let ir = rad.ceil() as i32;
        let ix0 = cx.floor() as i32;
        let iy0 = cy.floor() as i32;
        for dy in -ir..=ir {
            let gy = (iy0 + dy).rem_euclid(N as i32) as usize;
            let ry = wrap_delta(gy as f32 + 0.5 - cy);
            for dx in -ir..=ir {
                let gx = (ix0 + dx).rem_euclid(N as i32) as usize;
                let rx = wrap_delta(gx as f32 + 0.5 - cx);
                let dist = (rx * rx + ry * ry).sqrt();
                if dist >= rad {
                    continue;
                }
                let w = (1.0 - dist / rad).powi(2);
                let i = gy * N + gx;
                let v = (dir * pull + side * (pull * 0.45)) * INV_CELL * w;
                self.vx[i] = (self.vx[i] + v.x).clamp(-MAX_VEL, MAX_VEL);
                self.vy[i] = (self.vy[i] + v.y).clamp(-MAX_VEL, MAX_VEL);
            }
        }
    }

    /// Bilinearly samples the fluid's live velocity (in world units/s) at `pos`.
    pub fn sample_velocity(&self, pos: Vec2) -> Vec2 {
        if !self.active {
            return Vec2::ZERO;
        }
        let gx = wrap_coord(pos.x) - 0.5;
        let gy = wrap_coord(pos.y) - 0.5;
        let x0 = gx.floor() as i32;
        let y0 = gy.floor() as i32;
        let sx = gx - x0 as f32;
        let sy = gy - y0 as f32;
        let xa = x0.rem_euclid(N as i32) as usize;
        let xb = (xa + 1) & (N - 1);
        let ya = y0.rem_euclid(N as i32) as usize;
        let yb = (ya + 1) & (N - 1);
        let bilerp = |f: &[f32]| {
            let v00 = f[ya * N + xa];
            let v10 = f[ya * N + xb];
            let v01 = f[yb * N + xa];
            let v11 = f[yb * N + xb];
            (v00 * (1.0 - sx) + v10 * sx) * (1.0 - sy) + (v01 * (1.0 - sx) + v11 * sx) * sy
        };
        vec2(bilerp(&self.vx), bilerp(&self.vy)) * CELL_SIZE
    }

    /// Advances the toroidal fluid by `dt` seconds (60 Hz fixed step).
    pub fn update(&mut self, dt: f32) {
        if !self.active {
            return;
        }
        // 1. Vorticity confinement: restore and curl swirling eddies.
        for y in 0..N {
            let ym = (y + N - 1) & (N - 1);
            let yp = (y + 1) & (N - 1);
            let row = y * N;
            for x in 0..N {
                let xm = (x + N - 1) & (N - 1);
                let xp = (x + 1) & (N - 1);
                let dvydx = self.vy[row + xp] - self.vy[row + xm];
                let dvxdy = self.vx[yp * N + x] - self.vx[ym * N + x];
                self.curl[row + x] = 0.5 * (dvydx - dvxdy);
            }
        }
        for y in 0..N {
            let ym = (y + N - 1) & (N - 1);
            let yp = (y + 1) & (N - 1);
            let row = y * N;
            for x in 0..N {
                let xm = (x + N - 1) & (N - 1);
                let xp = (x + 1) & (N - 1);
                let gx = self.curl[row + xp].abs() - self.curl[row + xm].abs();
                let gy = self.curl[yp * N + x].abs() - self.curl[ym * N + x].abs();
                let len = (gx * gx + gy * gy).sqrt() + 1e-5;
                let nx = gx / len;
                let ny = gy / len;
                let w = self.curl[row + x];
                let i = row + x;
                self.vx[i] = (self.vx[i] + ny * w * VORTICITY * dt).clamp(-MAX_VEL, MAX_VEL);
                self.vy[i] = (self.vy[i] - nx * w * VORTICITY * dt).clamp(-MAX_VEL, MAX_VEL);
            }
        }

        // 2. Pressure projection (incompressibility -> crisp vortex rings).
        self.project(PROJ_ITERS_PRE);

        // 3. Semi-Lagrangian velocity advection + gentle drag.
        let v_keep = (-0.65 * dt).exp();
        self.vx0.copy_from_slice(&self.vx);
        self.vy0.copy_from_slice(&self.vy);
        for y in 0..N {
            let row = y * N;
            for x in 0..N {
                let i = row + x;
                let bx = x as f32 - self.vx0[i] * dt;
                let by = y as f32 - self.vy0[i] * dt;
                self.vx[i] = sample_periodic(&self.vx0, bx, by) * v_keep;
                self.vy[i] = sample_periodic(&self.vy0, bx, by) * v_keep;
            }
        }

        // 4. Second projection so advected flow stays divergence-free.
        self.project(PROJ_ITERS_POST);

        // 5. Advect smoke dye, optical density, temperature, and plasma tint.
        let d_keep = (-0.42 * dt).exp();
        let h_keep = (-2.15 * dt).exp();
        self.r0.copy_from_slice(&self.r);
        self.g0.copy_from_slice(&self.g);
        self.b0.copy_from_slice(&self.b);
        self.d0.copy_from_slice(&self.density);
        self.h0.copy_from_slice(&self.heat);
        self.tr0.copy_from_slice(&self.tint_r);
        self.tg0.copy_from_slice(&self.tint_g);
        self.tb0.copy_from_slice(&self.tint_b);
        self.tw0.copy_from_slice(&self.tint_w);

        let mut energy = 0.0f32;
        for y in 0..N {
            let row = y * N;
            for x in 0..N {
                let i = row + x;
                let bx = x as f32 - self.vx[i] * dt;
                let by = y as f32 - self.vy[i] * dt;
                // Compute bilinear weights once for all scalar channels!
                let x0 = bx.floor() as i32;
                let y0 = by.floor() as i32;
                let sx = bx - x0 as f32;
                let sy = by - y0 as f32;
                let xa = x0.rem_euclid(N as i32) as usize;
                let xb = (xa + 1) & (N - 1);
                let ya = y0.rem_euclid(N as i32) as usize;
                let yb = (ya + 1) & (N - 1);
                let i00 = ya * N + xa;
                let i10 = ya * N + xb;
                let i01 = yb * N + xa;
                let i11 = yb * N + xb;
                let w00 = (1.0 - sx) * (1.0 - sy);
                let w10 = sx * (1.0 - sy);
                let w01 = (1.0 - sx) * sy;
                let w11 = sx * sy;

                let bi = |f: &[f32]| f[i00] * w00 + f[i10] * w10 + f[i01] * w01 + f[i11] * w11;
                let d = bi(&self.d0) * d_keep;
                let h = bi(&self.h0) * h_keep;
                self.r[i] = bi(&self.r0) * d_keep;
                self.g[i] = bi(&self.g0) * d_keep;
                self.b[i] = bi(&self.b0) * d_keep;
                self.density[i] = d;
                self.heat[i] = h;
                if h > 0.004 {
                    self.tint_r[i] = bi(&self.tr0);
                    self.tint_g[i] = bi(&self.tg0);
                    self.tint_b[i] = bi(&self.tb0);
                    self.tint_w[i] = bi(&self.tw0);
                }
                energy = energy.max(d).max(h);
            }
        }
        if energy < 0.002 {
            self.clear();
        }
    }

    fn project(&mut self, iters: usize) {
        for y in 0..N {
            let ym = (y + N - 1) & (N - 1);
            let yp = (y + 1) & (N - 1);
            let row = y * N;
            for x in 0..N {
                let xm = (x + N - 1) & (N - 1);
                let xp = (x + 1) & (N - 1);
                let i = row + x;
                self.div[i] = -0.5
                    * (self.vx[row + xp] - self.vx[row + xm]
                        + self.vy[yp * N + x]
                        - self.vy[ym * N + x]);
                self.p[i] = 0.0;
            }
        }
        for _ in 0..iters {
            for y in 0..N {
                let ym = (y + N - 1) & (N - 1);
                let yp = (y + 1) & (N - 1);
                let row = y * N;
                for x in 0..N {
                    let xm = (x + N - 1) & (N - 1);
                    let xp = (x + 1) & (N - 1);
                    let i = row + x;
                    self.p[i] = (self.div[i]
                        + self.p[row + xm]
                        + self.p[row + xp]
                        + self.p[ym * N + x]
                        + self.p[yp * N + x])
                        * 0.25;
                }
            }
        }
        for y in 0..N {
            let ym = (y + N - 1) & (N - 1);
            let yp = (y + 1) & (N - 1);
            let row = y * N;
            for x in 0..N {
                let xm = (x + N - 1) & (N - 1);
                let xp = (x + 1) & (N - 1);
                let i = row + x;
                self.vx[i] -= 0.5 * (self.p[row + xp] - self.p[row + xm]);
                self.vy[i] -= 0.5 * (self.p[yp * N + x] - self.p[ym * N + x]);
            }
        }
    }

    /// Packs the fluid state into three 128x128 RGBA8 buffers for GPU upload:
    /// - `dye`: premultiplied smoke RGB + optical density in alpha
    /// - `heat_vel`: temperature, encoded velocity (vx, vy), and curl magnitude
    /// - `glow`: incandescent blackbody/tinted plasma emission for pyramid bloom
    pub fn pack(&self, dye: &mut [u8], heat_vel: &mut [u8], glow: &mut [u8]) {
        let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        for i in 0..CELLS {
            let o = i * 4;
            let d = self.density[i];
            let h = self.heat[i];
            // Un-premultiply smoke colour softly so thin wispy edges keep colour.
            let inv_d = if d > 0.01 { 1.0 / d } else { 0.0 };
            dye[o] = to_u8(self.r[i] * inv_d);
            dye[o + 1] = to_u8(self.g[i] * inv_d);
            dye[o + 2] = to_u8(self.b[i] * inv_d);
            dye[o + 3] = to_u8(1.0 - (-d * 1.15).exp());

            // Encode temperature (0..1.8 -> 0..255), velocity (-40..40 -> 0..255), curl.
            let h_norm = (h / 1.8).clamp(0.0, 1.0);
            heat_vel[o] = to_u8(h_norm);
            heat_vel[o + 1] = to_u8(self.vx[i] / 80.0 + 0.5);
            heat_vel[o + 2] = to_u8(self.vy[i] / 80.0 + 0.5);
            heat_vel[o + 3] = to_u8((self.curl[i].abs() / 16.0).clamp(0.0, 1.0));

            if h > 0.04 {
                let bb = blackbody((h * 0.72).clamp(0.0, 1.0));
                let tint = vec3(self.tint_r[i], self.tint_g[i], self.tint_b[i]);
                let col = bb.lerp(tint, self.tint_w[i] * 0.65);
                let em = (h * 0.95).clamp(0.0, 1.0);
                glow[o] = to_u8(col.x * em);
                glow[o + 1] = to_u8(col.y * em);
                glow[o + 2] = to_u8(col.z * em);
                glow[o + 3] = to_u8(em);
            } else {
                glow[o] = 0;
                glow[o + 1] = 0;
                glow[o + 2] = 0;
                glow[o + 3] = 0;
            }
        }
    }
}

#[inline]
fn sample_periodic(f: &[f32], x: f32, y: f32) -> f32 {
    let x0 = x.floor() as i32;
    let y0 = y.floor() as i32;
    let sx = x - x0 as f32;
    let sy = y - y0 as f32;
    let xa = x0.rem_euclid(N as i32) as usize;
    let xb = (xa + 1) & (N - 1);
    let ya = y0.rem_euclid(N as i32) as usize;
    let yb = (ya + 1) & (N - 1);
    let v00 = f[ya * N + xa];
    let v10 = f[ya * N + xb];
    let v01 = f[yb * N + xa];
    let v11 = f[yb * N + xb];
    (v00 * (1.0 - sx) + v10 * sx) * (1.0 - sy) + (v01 * (1.0 - sx) + v11 * sx) * sy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blast_injects_swirling_gas_and_cools_into_smoke() {
        let mut g = GasField::new();
        g.blast(
            vec2(100.0, 200.0),
            vec2(400.0, 0.0),
            60.0,
            vec3(0.45, 0.42, 0.40),
            0.85,
            vec3(1.0, 0.55, 0.2),
            0.2,
            1.0,
        );
        assert!(g.is_active());
        let peak_heat_0 = g.heat.iter().copied().fold(0.0f32, f32::max);
        assert!(peak_heat_0 > 0.8);
        for _ in 0..30 {
            g.update(1.0 / 60.0);
        }
        let peak_heat_30 = g.heat.iter().copied().fold(0.0f32, f32::max);
        let peak_dens_30 = g.density.iter().copied().fold(0.0f32, f32::max);
        assert!(peak_heat_30 < peak_heat_0 * 0.6, "fire cools over time");
        assert!(peak_dens_30 > 0.15, "smoke plume lingers after blast");
        // Verify velocity is finite and periodic wrapping has no NaNs.
        assert!(g.vx.iter().all(|v| v.is_finite()));
        assert!(g.vy.iter().all(|v| v.is_finite()));
        let v = g.sample_velocity(vec2(120.0, 200.0));
        assert!(v.length() > 1.0);
    }
}

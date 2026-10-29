//! Pre-baked particle effects (`SpriteSystem`, `PointSystem`, `LineSystem`,
//! `DiamondSystem`, `RingSystem` and their managers), played from the
//! original's `systems.dat`.
//!
//! Every effect is a fixed animation: per frame, a slice of points, lines or
//! triangles relative to where it was started. Each update shows the next
//! frame. Each set has a pool; starting an effect when the pool is busy
//! restarts the oldest one (`SpriteSystemSet::start`).

use macroquad::prelude::*;

use crate::gfx::{MAX_BATCH_INDICES, MAX_BATCH_VERTICES};

/// Effect families, in the order `systems.dat` stores their managers.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Sprite,
    Point,
    Line,
    Diamond,
    Ring,
}

/// Sets per manager (SpriteSystemManager::load etc.).
const SETS: [(Kind, usize); 5] = [
    (Kind::Sprite, 10),
    (Kind::Point, 19),
    (Kind::Line, 11),
    (Kind::Diamond, 5),
    (Kind::Ring, 9),
];

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn i32(&mut self) -> i32 {
        let v = i32::from_le_bytes(self.data[self.at..self.at + 4].try_into().unwrap());
        self.at += 4;
        v
    }
    fn i16(&mut self) -> i16 {
        let v = i16::from_le_bytes([self.data[self.at], self.data[self.at + 1]]);
        self.at += 2;
        v
    }
    fn u8(&mut self) -> u8 {
        self.at += 1;
        self.data[self.at - 1]
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.i32() as u32)
    }
    fn i16s(&mut self, n: usize) -> Vec<i16> {
        (0..n).map(|_| self.i16()).collect()
    }
    fn colors(&mut self, n: usize) -> Vec<[u8; 4]> {
        (0..n)
            .map(|_| [self.u8(), self.u8(), self.u8(), self.u8()])
            .collect()
    }
    fn xy(&mut self, n: usize) -> Vec<Vec2> {
        (0..n)
            .map(|_| vec2(self.i16() as f32, self.i16() as f32))
            .collect()
    }
}

/// One baked animation.
enum Anim {
    /// Point sprites: per point x, y (i16) and size (f32).
    Points {
        counts: Vec<i16>,
        offsets: Vec<i16>,
        points: Vec<(Vec2, f32)>,
        colors: Vec<[u8; 4]>,
    },
    /// Line segments: two vertices per line.
    Lines {
        counts: Vec<i16>,
        offsets: Vec<i16>,
        verts: Vec<Vec2>,
        colors: Vec<[u8; 4]>,
    },
    /// Diamonds: five vertices each, per-frame triangle lists.
    Diamonds {
        counts: Vec<i16>,
        offsets: Vec<i16>,
        verts: Vec<Vec2>,
        colors: Vec<[u8; 4]>,
        indices: Vec<Vec<u16>>,
    },
    /// Rings: two vertices per segment per frame, per-frame triangle lists.
    Rings {
        frames: usize,
        verts: Vec<Vec2>,
        colors: Vec<[u8; 4]>,
        indices: Vec<Vec<u16>>,
    },
}

impl Anim {
    fn frames(&self) -> usize {
        match self {
            Anim::Points { counts, .. }
            | Anim::Lines { counts, .. }
            | Anim::Diamonds { counts, .. } => counts.len(),
            Anim::Rings { frames, .. } => *frames,
        }
    }

    fn read(kind: Kind, r: &mut Reader) -> Anim {
        match kind {
            Kind::Sprite | Kind::Point => {
                let _ = r.i16();
                let frames = r.i16() as usize;
                let n = r.i16() as usize;
                if kind == Kind::Point {
                    let _ = (r.u8(), r.u8());
                }
                let counts = r.i16s(frames);
                let offsets = r.i16s(frames);
                let points = (0..n)
                    .map(|_| (vec2(r.i16() as f32, r.i16() as f32), r.f32()))
                    .collect();
                let colors = r.colors(n);
                Anim::Points {
                    counts,
                    offsets,
                    points,
                    colors,
                }
            }
            Kind::Line => {
                let _ = r.i16();
                let frames = r.i16() as usize;
                let n = r.i16() as usize;
                let _ = r.u8();
                let counts = r.i16s(frames);
                let offsets = r.i16s(frames);
                let verts = r.xy(n * 2);
                let colors = r.colors(n * 2);
                Anim::Lines {
                    counts,
                    offsets,
                    verts,
                    colors,
                }
            }
            Kind::Diamond => {
                let _ = r.i16();
                let frames = r.i16() as usize;
                let n = r.i16() as usize;
                let verts = r.xy(n * 5);
                let colors = r.colors(n * 5);
                let counts = r.i16s(frames);
                let offsets = r.i16s(frames);
                let indices = counts
                    .iter()
                    .map(|&c| (0..c as usize * 12).map(|_| r.i16() as u16).collect())
                    .collect();
                Anim::Diamonds {
                    counts,
                    offsets,
                    verts,
                    colors,
                    indices,
                }
            }
            Kind::Ring => {
                let frames = r.i16() as usize;
                let segments = r.i16() as usize;
                let verts = r.xy(frames * segments * 2);
                let colors = r.colors(frames * segments * 2);
                let indices = (0..frames)
                    .map(|_| (0..segments * 6).map(|_| r.i16() as u16).collect())
                    .collect();
                Anim::Rings {
                    frames,
                    verts,
                    colors,
                    indices,
                }
            }
        }
    }
}

struct Set {
    pool: usize,
    variants: Vec<Anim>,
}

struct Running {
    set: usize,
    variant: usize,
    pos: Vec2,
    angle: f32,
    reverse: bool,
    frame: usize,
    /// Handle for effects that follow something (LineSystem pointers).
    id: u64,
}

pub struct Effects {
    sets: Vec<(Kind, Vec<Set>)>,
    running: Vec<Vec<Vec<Running>>>,
    /// Round-robin over each set's stored variants.
    next_variant: Vec<Vec<usize>>,
    /// FlashManager: (colour, seconds left, alpha speed).
    flashes: Vec<(Color, f32, f32)>,
    next_id: u64,
}

// FlashManager: flashes last 0.5 s; alpha falls at 3/s, damped each update.
const FLASH_SECONDS: f32 = 0.5;
const FLASH_ALPHA_SPEED: f32 = 3.0;
const FLASH_DAMPING: f32 = 1.0;

impl Effects {
    pub fn load(data: &[u8]) -> Effects {
        let mut r = Reader { data, at: 0 };
        let mut sets = Vec::new();
        for (kind, count) in SETS {
            let mut list = Vec::new();
            for _ in 0..count {
                let pool = r.i32() as usize;
                let stored = r.i32() as usize;
                if kind == Kind::Sprite {
                    let _ = r.i32();
                }
                let variants = (0..stored).map(|_| Anim::read(kind, &mut r)).collect();
                list.push(Set { pool, variants });
            }
            sets.push((kind, list));
        }
        assert_eq!(r.at, data.len(), "systems.dat not fully parsed");
        let running = sets
            .iter()
            .map(|(_, list)| list.iter().map(|_| Vec::new()).collect())
            .collect();
        let next_variant = sets.iter().map(|(_, list)| vec![0; list.len()]).collect();
        Effects {
            sets,
            running,
            next_variant,
            flashes: Vec::new(),
            next_id: 1,
        }
    }

    fn manager(kind: Kind) -> usize {
        SETS.iter().position(|(k, _)| *k == kind).unwrap()
    }

    /// `<Kind>SystemManager::start(set, x, y, angle, reverse)`. Returns a
    /// handle for `follow` and `is_running`, or 0 when nothing started.
    pub fn start(&mut self, kind: Kind, set: usize, pos: Vec2, angle: f32, reverse: bool) -> u64 {
        let m = Effects::manager(kind);
        let Some(info) = self.sets[m].1.get(set) else {
            return 0;
        };
        if info.variants.is_empty() {
            return 0;
        }
        let id = self.next_id;
        self.next_id += 1;
        let running = &mut self.running[m][set];
        let variant = self.next_variant[m][set] % info.variants.len();
        self.next_variant[m][set] += 1;
        let effect = Running {
            set,
            variant,
            pos,
            angle,
            reverse,
            frame: 0,
            id,
        };
        if running.len() < info.pool {
            running.push(effect);
        } else if let Some(oldest) = running.iter_mut().max_by_key(|e| e.frame) {
            *oldest = effect;
        }
        id
    }

    fn find(&mut self, id: u64) -> Option<&mut Running> {
        self.running.iter_mut().flatten().flatten().find(|e| e.id == id)
    }

    /// Moves a running effect (the original writes the system's x, y).
    pub fn follow(&mut self, id: u64, pos: Vec2) {
        if let Some(e) = self.find(id) {
            e.pos = pos;
        }
    }

    /// `LineSystem::isActive` for a handle from `start`.
    pub fn is_running(&mut self, id: u64) -> bool {
        id != 0 && self.find(id).is_some()
    }

    /// FlashManager::flash.
    pub fn flash(&mut self, color: Color) {
        self.flashes.push((color, FLASH_SECONDS, FLASH_ALPHA_SPEED));
    }

    /// One frame of every running effect; finished ones are dropped.
    pub fn update(&mut self, dt: f32) {
        for (m, (_, list)) in self.sets.iter().enumerate() {
            for (s, set) in list.iter().enumerate() {
                let running = &mut self.running[m][s];
                for e in running.iter_mut() {
                    e.frame += 1;
                }
                running.retain(|e| e.frame < set.variants[e.variant].frames());
            }
        }
        for f in &mut self.flashes {
            f.1 -= 1.0 / 60.0;
            f.0.a -= dt * f.2;
            f.2 *= FLASH_DAMPING / (FLASH_DAMPING + 1.0 / 60.0);
        }
        self.flashes.retain(|f| f.1 > 0.0);
    }

    #[allow(dead_code)] // used once restart lands
    pub fn clear(&mut self) {
        for list in &mut self.running {
            for r in list {
                r.clear();
            }
        }
        self.flashes.clear();
    }

    /// GameScreen::drawParticleSystems: diamonds, points, sprites, lines,
    /// rings. `to_screen` maps world positions to the canvas.
    pub fn draw(
        &self,
        canvas: &crate::gfx::Canvas,
        sprite: &Texture2D,
        z: f32,
        to_screen: impl Fn(Vec2) -> Vec2,
    ) {
        for kind in [
            Kind::Diamond,
            Kind::Point,
            Kind::Sprite,
            Kind::Line,
            Kind::Ring,
        ] {
            let m = Effects::manager(kind);
            let texture = if kind == Kind::Sprite {
                Some(sprite)
            } else {
                None
            };
            let additive = kind != Kind::Diamond;
            let mut verts: Vec<Vertex> = Vec::new();
            let mut indices: Vec<u16> = Vec::new();
            let mut one_verts: Vec<Vertex> = Vec::new();
            let mut one_indices: Vec<u16> = Vec::new();
            for running in &self.running[m] {
                for e in running {
                    let anim = &self.sets[m].1[e.set].variants[e.variant];
                    let origin = to_screen(e.pos);
                    let (s, c) = e.angle.to_radians().sin_cos();
                    let place = |p: Vec2| origin + vec2(p.x * c - p.y * s, p.x * s + p.y * c);
                    let frame = if e.reverse {
                        anim.frames() - 1 - e.frame
                    } else {
                        e.frame
                    };
                    one_verts.clear();
                    one_indices.clear();
                    emit(anim, frame, &place, z, &mut one_verts, &mut one_indices);
                    if verts.len() + one_verts.len() > MAX_BATCH_VERTICES
                        || indices.len() + one_indices.len() > MAX_BATCH_INDICES
                    {
                        canvas.draw_triangles(
                            std::mem::take(&mut verts),
                            std::mem::take(&mut indices),
                            texture,
                            additive,
                        );
                    }
                    let base = verts.len() as u16;
                    verts.extend_from_slice(&one_verts);
                    indices.extend(one_indices.iter().map(|&k| base + k));
                }
            }
            canvas.draw_triangles(verts, indices, texture, additive);
        }
        // FlashManager::draw: full-screen colour at depth 0.
        let (x0, x1) = (-crate::gfx::edge(), crate::gfx::CANVAS_W + crate::gfx::edge());
        for (color, _, _) in &self.flashes {
            canvas.draw_triangles(
                vec![
                    Vertex::new(x0, 0.0, 0.0, 0.0, 0.0, *color),
                    Vertex::new(x1, 0.0, 0.0, 1.0, 0.0, *color),
                    Vertex::new(x1, crate::gfx::CANVAS_H, 0.0, 1.0, 1.0, *color),
                    Vertex::new(x0, crate::gfx::CANVAS_H, 0.0, 0.0, 1.0, *color),
                ],
                vec![0, 1, 2, 0, 2, 3],
                None,
                false,
            );
        }
    }
}

fn color(c: [u8; 4]) -> Color {
    Color::from_rgba(c[0], c[1], c[2], c[3])
}

/// Append one frame of an animation as triangles.
fn emit(
    anim: &Anim,
    frame: usize,
    place: &dyn Fn(Vec2) -> Vec2,
    z: f32,
    verts: &mut Vec<Vertex>,
    indices: &mut Vec<u16>,
) {
    let push_quad = |verts: &mut Vec<Vertex>,
                     indices: &mut Vec<u16>,
                     corners: [Vec2; 4],
                     uvs: [Vec2; 4],
                     col: Color| {
        let base = verts.len() as u16;
        for (p, uv) in corners.iter().zip(uvs) {
            verts.push(Vertex::new(p.x, p.y, z, uv.x, uv.y, col));
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };
    let unit_uv = [
        vec2(0.0, 0.0),
        vec2(1.0, 0.0),
        vec2(1.0, 1.0),
        vec2(0.0, 1.0),
    ];
    match anim {
        Anim::Points {
            counts,
            offsets,
            points,
            colors,
        } => {
            let start = offsets[frame] as usize;
            for i in start..start + counts[frame] as usize {
                let (p, size) = points[i];
                let centre = place(p);
                let h = size * 0.5;
                let corners = [
                    centre + vec2(-h, -h),
                    centre + vec2(h, -h),
                    centre + vec2(h, h),
                    centre + vec2(-h, h),
                ];
                push_quad(verts, indices, corners, unit_uv, color(colors[i]));
            }
        }
        Anim::Lines {
            counts,
            offsets,
            verts: line_verts,
            colors,
        } => {
            let start = offsets[frame] as usize;
            for l in 0..counts[frame] as usize {
                let (i, j) = (start + l * 2, start + l * 2 + 1);
                let (a, b) = (place(line_verts[i]), place(line_verts[j]));
                // One-unit-wide quad standing in for GL_LINES.
                let n = (b - a).perp().normalize_or_zero() * 0.5;
                let base = verts.len() as u16;
                for (p, ci) in [(a - n, i), (b - n, j), (b + n, j), (a + n, i)] {
                    verts.push(Vertex::new(p.x, p.y, z, 0.0, 0.0, color(colors[ci])));
                }
                indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
            }
        }
        Anim::Diamonds {
            offsets,
            verts: d_verts,
            colors,
            indices: frame_indices,
            ..
        } => {
            let start = offsets[frame] as usize;
            let base = verts.len() as u16;
            let used = frame_indices[frame]
                .iter()
                .map(|&k| k as usize)
                .max()
                .map_or(0, |m| m + 1);
            for k in 0..used {
                let p = place(d_verts[start + k]);
                verts.push(Vertex::new(p.x, p.y, z, 0.0, 0.0, color(colors[start + k])));
            }
            indices.extend(frame_indices[frame].iter().map(|&k| base + k));
        }
        Anim::Rings {
            verts: r_verts,
            colors,
            indices: frame_indices,
            ..
        } => {
            let list = &frame_indices[frame];
            let lo = list.iter().copied().min().unwrap_or(0) as usize;
            let hi = list.iter().copied().max().unwrap_or(0) as usize;
            let base = verts.len() as u16;
            for k in lo..=hi.min(r_verts.len() - 1) {
                let p = place(r_verts[k]);
                verts.push(Vertex::new(p.x, p.y, z, 0.0, 0.0, color(colors[k])));
            }
            indices.extend(list.iter().map(|&k| base + (k as usize - lo) as u16));
        }
    }
}

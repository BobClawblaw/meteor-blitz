//! High-resolution GPU toroidal Navier-Stokes fluid solver (`Gas Physics`).
//!
//! Runs completely on the GPU using `miniquad` `RGBA16F` ping-pong render
//! targets with hardware `GL_REPEAT` toroidal wrapping matching Meteor Blitz's
//! 1024x1024 arena:
//! - Velocity / curl / divergence / warm-started pressure: `256x256` `RGBA16F`
//! - Luminous gas / HDR plasma: `512x512` `RGBA16F` with 2nd-order clamped
//!   MacCormack advection for crisp curling filaments and zero muddy grey
//!   splotches.

use macroquad::prelude::*;
use crate::gas::FluidSplat;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;
    use macroquad::miniquad::{
        Bindings, BufferId, BufferLayout, BufferSource, BufferType, BufferUsage, FilterMode,
        MipmapFilterMode, PassAction, Pipeline, PipelineParams, RenderPass, ShaderMeta,
        ShaderSource, TextureFormat, TextureId, TextureKind, TextureParams, TextureWrap,
        UniformBlockLayout, UniformDesc, UniformType, UniformsSource, VertexAttribute,
        VertexFormat,
    };

    const VEL_N: u32 = 256;
    const DYE_N: u32 = 512;
    const BATCH: usize = 16;
    const PRESSURE_ITERS: usize = 16;
    const DT: f32 = 1.0 / 60.0;
    const VORTICITY_STRENGTH: f32 = 62.0;

    const QUAD_VERT: &str = r#"#version 100
attribute vec2 pos;
varying highp vec2 uv;
void main() {
    uv = pos * 0.5 + 0.5;
    gl_Position = vec4(pos, 0.0, 1.0);
}"#;

    const SPLAT_VEL_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Src;
uniform float Count;
uniform vec4 PosRadSwirl[16];
uniform vec4 ForcePad[16];
void main() {
    vec2 v = texture2D(Src, uv).xy;
    for (int i = 0; i < 16; i++) {
        if (float(i) >= Count) break;
        vec4 prs = PosRadSwirl[i];
        vec2 d = fract(uv - prs.xy + 0.5) - 0.5;
        float r = max(prs.z, 0.001);
        float d2 = dot(d, d) / (r * r);
        if (d2 < 9.0) {
            float g = exp(-d2);
            vec2 tang = vec2(-d.y, d.x) / r;
            v += (ForcePad[i].xy + tang * prs.w) * g;
        }
    }
    v = clamp(v, vec2(-1.8), vec2(1.8));
    gl_FragColor = vec4(v, 0.0, 1.0);
}"#;

    const SPLAT_DYE_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Src;
uniform float Count;
uniform vec4 PosRadSwirl[16];
uniform vec4 ColEnergy[16];
void main() {
    vec4 cur = texture2D(Src, uv);
    for (int i = 0; i < 16; i++) {
        if (float(i) >= Count) break;
        vec4 prs = PosRadSwirl[i];
        vec2 d = fract(uv - prs.xy + 0.5) - 0.5;
        float r = max(prs.z, 0.001);
        float d2 = dot(d, d) / (r * r);
        if (d2 < 9.0) {
            float g = exp(-d2 * 1.4);
            cur.rgb += ColEnergy[i].rgb * g;
            cur.a += ColEnergy[i].a * g;
        }
    }
    cur.rgb = min(cur.rgb, vec3(5.0));
    cur.a = min(cur.a, 4.0);
    gl_FragColor = cur;
}"#;

    const CURL_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Vel;
uniform vec2 Texel;
void main() {
    float vL = texture2D(Vel, uv - vec2(Texel.x, 0.0)).y;
    float vR = texture2D(Vel, uv + vec2(Texel.x, 0.0)).y;
    float vB = texture2D(Vel, uv - vec2(0.0, Texel.y)).x;
    float vT = texture2D(Vel, uv + vec2(0.0, Texel.y)).x;
    float curl = 0.5 * ((vR - vL) - (vT - vB));
    gl_FragColor = vec4(curl, 0.0, 0.0, 1.0);
}"#;

    const VORTICITY_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Vel;
uniform sampler2D Curl;
uniform vec2 Texel;
uniform float Strength;
uniform float Dt;
void main() {
    float L = abs(texture2D(Curl, uv - vec2(Texel.x, 0.0)).x);
    float R = abs(texture2D(Curl, uv + vec2(Texel.x, 0.0)).x);
    float B = abs(texture2D(Curl, uv - vec2(0.0, Texel.y)).x);
    float T = abs(texture2D(Curl, uv + vec2(0.0, Texel.y)).x);
    float C = texture2D(Curl, uv).x;
    vec2 grad = 0.5 * vec2(R - L, T - B);
    float len = length(grad) + 1e-5;
    vec2 n = grad / len;
    vec2 force = vec2(n.y, -n.x) * C * Strength;
    vec2 v = texture2D(Vel, uv).xy + force * Dt;
    v = clamp(v, vec2(-1.8), vec2(1.8));
    gl_FragColor = vec4(v, 0.0, 1.0);
}"#;

    const DIV_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Vel;
uniform vec2 Texel;
void main() {
    float L = texture2D(Vel, uv - vec2(Texel.x, 0.0)).x;
    float R = texture2D(Vel, uv + vec2(Texel.x, 0.0)).x;
    float B = texture2D(Vel, uv - vec2(0.0, Texel.y)).y;
    float T = texture2D(Vel, uv + vec2(0.0, Texel.y)).y;
    float div = 0.5 * ((R - L) + (T - B));
    gl_FragColor = vec4(div, 0.0, 0.0, 1.0);
}"#;

    const SCALE_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Src;
uniform float Scale;
void main() {
    gl_FragColor = texture2D(Src, uv) * Scale;
}"#;

    const PRESSURE_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Pressure;
uniform sampler2D Div;
uniform vec2 Texel;
void main() {
    float L = texture2D(Pressure, uv - vec2(Texel.x, 0.0)).x;
    float R = texture2D(Pressure, uv + vec2(Texel.x, 0.0)).x;
    float B = texture2D(Pressure, uv - vec2(0.0, Texel.y)).x;
    float T = texture2D(Pressure, uv + vec2(0.0, Texel.y)).x;
    float div = texture2D(Div, uv).x;
    float p = (L + R + B + T - div) * 0.25;
    gl_FragColor = vec4(p, 0.0, 0.0, 1.0);
}"#;

    const GRAD_SUB_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Vel;
uniform sampler2D Pressure;
uniform vec2 Texel;
void main() {
    float L = texture2D(Pressure, uv - vec2(Texel.x, 0.0)).x;
    float R = texture2D(Pressure, uv + vec2(Texel.x, 0.0)).x;
    float B = texture2D(Pressure, uv - vec2(0.0, Texel.y)).x;
    float T = texture2D(Pressure, uv + vec2(0.0, Texel.y)).x;
    vec2 v = texture2D(Vel, uv).xy - 0.5 * vec2(R - L, T - B);
    gl_FragColor = vec4(v, 0.0, 1.0);
}"#;

    const ADVECT_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Vel;
uniform sampler2D Src;
uniform float Dt;
uniform float Decay;
void main() {
    vec2 v = texture2D(Vel, uv).xy;
    vec2 prev = uv - v * Dt;
    gl_FragColor = texture2D(Src, prev) * Decay;
}"#;

    /// 2nd-order clamped MacCormack advection for razor-sharp curling plasma
    /// filaments. Colour and energy decay together so wisps dissolve cleanly
    /// into space with zero lingering muddy grey splotches.
    const MACCORMACK_FRAG: &str = r#"#version 100
precision highp float;
varying highp vec2 uv;
uniform sampler2D Vel;
uniform sampler2D Src;
uniform sampler2D Hat;
uniform vec2 Texel;
uniform float Dt;
void main() {
    vec2 v = texture2D(Vel, uv).xy;
    vec2 back_uv = uv - v * Dt;
    vec4 phi_hat = texture2D(Hat, uv);
    vec4 phi_n = texture2D(Src, uv);
    vec4 phi_rev = texture2D(Hat, uv + v * Dt);
    vec4 corr = phi_hat + 0.5 * (phi_n - phi_rev);

    vec2 c_uv = (floor(back_uv / Texel - 0.5) + 0.5) * Texel;
    vec4 s00 = texture2D(Src, c_uv);
    vec4 s10 = texture2D(Src, c_uv + vec2(Texel.x, 0.0));
    vec4 s01 = texture2D(Src, c_uv + vec2(0.0, Texel.y));
    vec4 s11 = texture2D(Src, c_uv + Texel);
    vec4 lo = min(min(s00, s10), min(s01, s11));
    vec4 hi = max(max(s00, s10), max(s01, s11));
    vec4 out_val = clamp(corr, lo, hi);

    float lum = max(out_val.r, max(out_val.g, out_val.b));
    float decay = mix(0.976, 0.958, clamp(lum * 0.5, 0.0, 1.0));
    out_val *= decay;
    if (lum < 0.004) {
        out_val = vec4(0.0);
    }
    gl_FragColor = out_val;
}"#;

    #[repr(C)]
    struct SplatVelUniforms {
        count: f32,
        pos_rad_swirl: [[f32; 4]; BATCH],
        force_pad: [[f32; 4]; BATCH],
    }

    #[repr(C)]
    struct SplatDyeUniforms {
        count: f32,
        pos_rad_swirl: [[f32; 4]; BATCH],
        col_energy: [[f32; 4]; BATCH],
    }

    #[repr(C)]
    struct TexelUniform {
        texel: [f32; 2],
    }

    #[repr(C)]
    struct VortUniform {
        texel: [f32; 2],
        strength: f32,
        dt: f32,
    }

    #[repr(C)]
    struct ScaleUniform {
        scale: f32,
    }

    #[repr(C)]
    struct AdvectUniform {
        dt: f32,
        decay: f32,
    }

    #[repr(C)]
    struct MacUniform {
        texel: [f32; 2],
        dt: f32,
    }

    struct Target {
        tex: TextureId,
        mq: Texture2D,
        pass: RenderPass,
    }

    fn make_target(ctx: &mut dyn macroquad::miniquad::RenderingBackend, n: u32) -> Target {
        let tex = ctx.new_render_texture(TextureParams {
            kind: TextureKind::Texture2D,
            format: TextureFormat::RGBA16F,
            wrap: TextureWrap::Repeat,
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::None,
            width: n,
            height: n,
            allocate_mipmaps: false,
            sample_count: 1,
        });
        let pass = ctx.new_render_pass(tex, None);
        let mq = Texture2D::from_miniquad_texture(tex);
        Target { tex, mq, pass }
    }

    fn make_pipe(
        ctx: &mut dyn macroquad::miniquad::RenderingBackend,
        frag: &str,
        images: &[&str],
        uniforms: Vec<UniformDesc>,
    ) -> Pipeline {
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl {
                    vertex: QUAD_VERT,
                    fragment: frag,
                },
                ShaderMeta {
                    images: images.iter().map(|s| (*s).to_owned()).collect(),
                    uniforms: UniformBlockLayout { uniforms },
                },
            )
            .expect("gpu fluid shader");
        ctx.new_pipeline(
            &[BufferLayout::default()],
            &[VertexAttribute::new("pos", VertexFormat::Float2)],
            shader,
            PipelineParams::default(),
        )
    }

    pub struct GpuFluid {
        vbuf: BufferId,
        ibuf: BufferId,
        vel: [Target; 2],
        vel_i: usize,
        curl: Target,
        div: Target,
        press: [Target; 2],
        press_i: usize,
        dye: [Target; 2],
        dye_hat: Target,
        dye_i: usize,
        splat_vel: Pipeline,
        splat_dye: Pipeline,
        curl_pipe: Pipeline,
        vort_pipe: Pipeline,
        div_pipe: Pipeline,
        scale_pipe: Pipeline,
        press_pipe: Pipeline,
        grad_pipe: Pipeline,
        advect_pipe: Pipeline,
        mac_pipe: Pipeline,
        active: bool,
    }

    impl GpuFluid {
        pub fn new() -> GpuFluid {
            let gl = unsafe { get_internal_gl() };
            let ctx = &mut *gl.quad_context;

            let verts: [f32; 8] = [-1.0, -1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0];
            let inds: [u16; 6] = [0, 1, 2, 0, 2, 3];
            let vbuf = ctx.new_buffer(
                BufferType::VertexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&verts),
            );
            let ibuf = ctx.new_buffer(
                BufferType::IndexBuffer,
                BufferUsage::Immutable,
                BufferSource::slice(&inds),
            );

            let vel = [make_target(ctx, VEL_N), make_target(ctx, VEL_N)];
            let curl = make_target(ctx, VEL_N);
            let div = make_target(ctx, VEL_N);
            let press = [make_target(ctx, VEL_N), make_target(ctx, VEL_N)];
            let dye = [make_target(ctx, DYE_N), make_target(ctx, DYE_N)];
            let dye_hat = make_target(ctx, DYE_N);

            let splat_vel = make_pipe(
                ctx,
                SPLAT_VEL_FRAG,
                &["Src"],
                vec![
                    UniformDesc::new("Count", UniformType::Float1),
                    UniformDesc::new("PosRadSwirl", UniformType::Float4).array(BATCH),
                    UniformDesc::new("ForcePad", UniformType::Float4).array(BATCH),
                ],
            );
            let splat_dye = make_pipe(
                ctx,
                SPLAT_DYE_FRAG,
                &["Src"],
                vec![
                    UniformDesc::new("Count", UniformType::Float1),
                    UniformDesc::new("PosRadSwirl", UniformType::Float4).array(BATCH),
                    UniformDesc::new("ColEnergy", UniformType::Float4).array(BATCH),
                ],
            );
            let curl_pipe = make_pipe(
                ctx,
                CURL_FRAG,
                &["Vel"],
                vec![UniformDesc::new("Texel", UniformType::Float2)],
            );
            let vort_pipe = make_pipe(
                ctx,
                VORTICITY_FRAG,
                &["Vel", "Curl"],
                vec![
                    UniformDesc::new("Texel", UniformType::Float2),
                    UniformDesc::new("Strength", UniformType::Float1),
                    UniformDesc::new("Dt", UniformType::Float1),
                ],
            );
            let div_pipe = make_pipe(
                ctx,
                DIV_FRAG,
                &["Vel"],
                vec![UniformDesc::new("Texel", UniformType::Float2)],
            );
            let scale_pipe = make_pipe(
                ctx,
                SCALE_FRAG,
                &["Src"],
                vec![UniformDesc::new("Scale", UniformType::Float1)],
            );
            let press_pipe = make_pipe(
                ctx,
                PRESSURE_FRAG,
                &["Pressure", "Div"],
                vec![UniformDesc::new("Texel", UniformType::Float2)],
            );
            let grad_pipe = make_pipe(
                ctx,
                GRAD_SUB_FRAG,
                &["Vel", "Pressure"],
                vec![UniformDesc::new("Texel", UniformType::Float2)],
            );
            let advect_pipe = make_pipe(
                ctx,
                ADVECT_FRAG,
                &["Vel", "Src"],
                vec![
                    UniformDesc::new("Dt", UniformType::Float1),
                    UniformDesc::new("Decay", UniformType::Float1),
                ],
            );
            let mac_pipe = make_pipe(
                ctx,
                MACCORMACK_FRAG,
                &["Vel", "Src", "Hat"],
                vec![
                    UniformDesc::new("Texel", UniformType::Float2),
                    UniformDesc::new("Dt", UniformType::Float1),
                ],
            );

            let mut f = GpuFluid {
                vbuf,
                ibuf,
                vel,
                vel_i: 0,
                curl,
                div,
                press,
                press_i: 0,
                dye,
                dye_hat,
                dye_i: 0,
                splat_vel,
                splat_dye,
                curl_pipe,
                vort_pipe,
                div_pipe,
                scale_pipe,
                press_pipe,
                grad_pipe,
                advect_pipe,
                mac_pipe,
                active: false,
            };
            f.clear_gpu(ctx);
            f
        }

        fn clear_pass(ctx: &mut dyn macroquad::miniquad::RenderingBackend, pass: RenderPass) {
            ctx.begin_pass(Some(pass), PassAction::clear_color(0.0, 0.0, 0.0, 0.0));
            ctx.end_render_pass();
        }

        fn clear_gpu(&mut self, ctx: &mut dyn macroquad::miniquad::RenderingBackend) {
            for t in &self.vel {
                Self::clear_pass(ctx, t.pass);
            }
            Self::clear_pass(ctx, self.curl.pass);
            Self::clear_pass(ctx, self.div.pass);
            for t in &self.press {
                Self::clear_pass(ctx, t.pass);
            }
            for t in &self.dye {
                Self::clear_pass(ctx, t.pass);
            }
            Self::clear_pass(ctx, self.dye_hat.pass);
            self.active = false;
        }

        fn run_pass<U>(
            ctx: &mut dyn macroquad::miniquad::RenderingBackend,
            vbuf: BufferId,
            ibuf: BufferId,
            dst: RenderPass,
            pipe: &Pipeline,
            images: Vec<TextureId>,
            uniforms: &U,
        ) {
            ctx.begin_pass(Some(dst), PassAction::Nothing);
            ctx.apply_pipeline(pipe);
            ctx.apply_bindings(&Bindings {
                vertex_buffers: vec![vbuf],
                index_buffer: ibuf,
                images,
            });
            ctx.apply_uniforms(UniformsSource::table(uniforms));
            ctx.draw(0, 6, 1);
            ctx.end_render_pass();
        }

        pub fn is_active(&self) -> bool {
            self.active
        }

        pub fn dye_texture(&self) -> &Texture2D {
            &self.dye[self.dye_i].mq
        }

        pub fn step(&mut self, clear: bool, steps: u32, splats: &[FluidSplat]) {
            if !clear && steps == 0 && splats.is_empty() {
                return;
            }
            let mut gl = unsafe { get_internal_gl() };
            gl.flush();
            let ctx = &mut *gl.quad_context;

            if clear {
                self.clear_gpu(ctx);
            }

            if !splats.is_empty() {
                self.active = true;
                for chunk in splats.chunks(BATCH) {
                    let mut prs = [[0.0f32; 4]; BATCH];
                    let mut fp = [[0.0f32; 4]; BATCH];
                    let mut ce = [[0.0f32; 4]; BATCH];
                    let mut any_dye = false;
                    for (i, s) in chunk.iter().enumerate() {
                        prs[i] = [s.pos.x, s.pos.y, s.radius, s.swirl];
                        fp[i] = [s.force.x, s.force.y, 0.0, 0.0];
                        ce[i] = [s.color.x, s.color.y, s.color.z, s.energy];
                        if s.color.length_squared() > 1e-6 || s.energy > 1e-6 {
                            any_dye = true;
                        }
                    }
                    let vel_u = SplatVelUniforms {
                        count: chunk.len() as f32,
                        pos_rad_swirl: prs,
                        force_pad: fp,
                    };
                    let next_v = 1 - self.vel_i;
                    Self::run_pass(
                        ctx,
                        self.vbuf,
                        self.ibuf,
                        self.vel[next_v].pass,
                        &self.splat_vel,
                        vec![self.vel[self.vel_i].tex],
                        &vel_u,
                    );
                    self.vel_i = next_v;

                    if any_dye {
                        let dye_u = SplatDyeUniforms {
                            count: chunk.len() as f32,
                            pos_rad_swirl: prs,
                            col_energy: ce,
                        };
                        let next_d = 1 - self.dye_i;
                        Self::run_pass(
                            ctx,
                            self.vbuf,
                            self.ibuf,
                            self.dye[next_d].pass,
                            &self.splat_dye,
                            vec![self.dye[self.dye_i].tex],
                            &dye_u,
                        );
                        self.dye_i = next_d;
                    }
                }
            }

            if !self.active || steps == 0 {
                return;
            }

            let vel_texel = [1.0 / VEL_N as f32, 1.0 / VEL_N as f32];
            let dye_texel = [1.0 / DYE_N as f32, 1.0 / DYE_N as f32];

            for _ in 0..steps {
                Self::run_pass(
                    ctx,
                    self.vbuf,
                    self.ibuf,
                    self.curl.pass,
                    &self.curl_pipe,
                    vec![self.vel[self.vel_i].tex],
                    &TexelUniform { texel: vel_texel },
                );

                let next_v = 1 - self.vel_i;
                Self::run_pass(
                    ctx,
                    self.vbuf,
                    self.ibuf,
                    self.vel[next_v].pass,
                    &self.vort_pipe,
                    vec![self.vel[self.vel_i].tex, self.curl.tex],
                    &VortUniform {
                        texel: vel_texel,
                        strength: VORTICITY_STRENGTH,
                        dt: DT,
                    },
                );
                self.vel_i = next_v;

                Self::run_pass(
                    ctx,
                    self.vbuf,
                    self.ibuf,
                    self.div.pass,
                    &self.div_pipe,
                    vec![self.vel[self.vel_i].tex],
                    &TexelUniform { texel: vel_texel },
                );

                let next_p = 1 - self.press_i;
                Self::run_pass(
                    ctx,
                    self.vbuf,
                    self.ibuf,
                    self.press[next_p].pass,
                    &self.scale_pipe,
                    vec![self.press[self.press_i].tex],
                    &ScaleUniform { scale: 0.8 },
                );
                self.press_i = next_p;
                for _ in 0..PRESSURE_ITERS {
                    let np = 1 - self.press_i;
                    Self::run_pass(
                        ctx,
                        self.vbuf,
                        self.ibuf,
                        self.press[np].pass,
                        &self.press_pipe,
                        vec![self.press[self.press_i].tex, self.div.tex],
                        &TexelUniform { texel: vel_texel },
                    );
                    self.press_i = np;
                }

                let next_v = 1 - self.vel_i;
                Self::run_pass(
                    ctx,
                    self.vbuf,
                    self.ibuf,
                    self.vel[next_v].pass,
                    &self.grad_pipe,
                    vec![self.vel[self.vel_i].tex, self.press[self.press_i].tex],
                    &TexelUniform { texel: vel_texel },
                );
                self.vel_i = next_v;

                let next_v = 1 - self.vel_i;
                Self::run_pass(
                    ctx,
                    self.vbuf,
                    self.ibuf,
                    self.vel[next_v].pass,
                    &self.advect_pipe,
                    vec![self.vel[self.vel_i].tex, self.vel[self.vel_i].tex],
                    &AdvectUniform { dt: DT, decay: 0.990 },
                );
                self.vel_i = next_v;

                Self::run_pass(
                    ctx,
                    self.vbuf,
                    self.ibuf,
                    self.dye_hat.pass,
                    &self.advect_pipe,
                    vec![self.vel[self.vel_i].tex, self.dye[self.dye_i].tex],
                    &AdvectUniform { dt: DT, decay: 1.0 },
                );
                let next_d = 1 - self.dye_i;
                Self::run_pass(
                    ctx,
                    self.vbuf,
                    self.ibuf,
                    self.dye[next_d].pass,
                    &self.mac_pipe,
                    vec![
                        self.vel[self.vel_i].tex,
                        self.dye[self.dye_i].tex,
                        self.dye_hat.tex,
                    ],
                    &MacUniform {
                        texel: dye_texel,
                        dt: DT,
                    },
                );
                self.dye_i = next_d;
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    pub struct GpuFluid {
        dummy: Texture2D,
    }
    impl GpuFluid {
        pub fn new() -> GpuFluid {
            GpuFluid {
                dummy: Texture2D::from_rgba8(1, 1, &[0, 0, 0, 0]),
            }
        }
        pub fn is_active(&self) -> bool {
            false
        }
        pub fn dye_texture(&self) -> &Texture2D {
            &self.dummy
        }
        pub fn step(&mut self, _clear: bool, _steps: u32, _splats: &[FluidSplat]) {}
    }
}

pub use imp::GpuFluid;

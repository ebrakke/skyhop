//! Static GPU buffers + a tiny lit/fogged shader. Everything in the world is
//! uploaded once at startup, so a frame is just a few dozen draw calls.

use crate::flight::Aircraft;
use crate::airliner;
use crate::ground;
use crate::lights::Lamp;
use crate::weather::Sky;
use crate::mesh::{self, MeshData};
use crate::scenery;
use crate::world::{World, LIGHTHOUSE};
use macroquad::math::{vec3, Mat4, Quat, Vec3, Vec4};
use macroquad::miniquad::*;

/// Fixed sun used to bake fine relief shading into the ground colour map.
pub const BAKE_SUN: Vec3 = vec3(-0.45, 0.62, 0.64);

// The ground shader maps world x/z onto the island colour map.
const _: () = assert!(crate::world::HALF == 12_000.0);

pub struct GpuMesh {
    bind: Bindings,
    n: i32,
}

impl GpuMesh {
    fn with_images(mut self, images: &[TextureId]) -> Self {
        self.bind.images = images.to_vec();
        self
    }
}

fn upload(ctx: &mut dyn RenderingBackend, m: &MeshData) -> GpuMesh {
    let mut m2;
    let m = if m.i.is_empty() {
        m2 = MeshData::default();
        m2.tri(Vec3::ZERO, Vec3::ZERO, Vec3::ZERO, [0.0; 4], None);
        &m2
    } else {
        m
    };
    let vb = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(&m.v));
    let ib = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&m.i));
    GpuMesh { bind: Bindings { vertex_buffers: vec![vb], index_buffer: ib, images: vec![] }, n: m.i.len() as i32 }
}

fn texture(ctx: &mut dyn RenderingBackend, size: usize, bytes: &[u8], wrap: TextureWrap) -> TextureId {
    let t = ctx.new_texture_from_data_and_format(
        bytes,
        TextureParams {
            width: size as u32,
            height: size as u32,
            wrap,
            min_filter: FilterMode::Linear,
            mag_filter: FilterMode::Linear,
            mipmap_filter: MipmapFilterMode::Linear,
            allocate_mipmaps: true,
            ..Default::default()
        },
    );
    ctx.texture_generate_mipmaps(t);
    t
}

#[repr(C)]
#[derive(Clone, Copy)]
struct WorldUniforms {
    mvp: [f32; 16],   // the sky uses this slot for the inverse view-projection
    model: [f32; 16],
    cam: [f32; 4],    // xyz eye, w time
    sun: [f32; 4],    // xyz direction to the main light, w ambient
    fog: [f32; 4],    // rgb, w density
    tint: [f32; 4],   // multiplies vertex colour
    params: [f32; 4], // x 1 water / 2 cloud deck, y emissive, z min ambient, w depth bias
    light_col: [f32; 4], // rgb, w night
    amb_sky: [f32; 4],
    amb_gnd: [f32; 4],
    sun_dir: [f32; 4], // the sun itself (haze glow, disc), w overcast
    lamp: [f32; 4],    // landing light position, w intensity
    lamp_dir: [f32; 4],
    zenith: [f32; 4],  // rgb, w stars
}

/// Shared by every fragment shader: sun or moon light with a sky/ground ambient, the aircraft's
/// landing light, and haze that is thick low down and thin at altitude, warm toward the sun.
macro_rules! atmosphere_glsl {
    () => {
        r#"
uniform vec4 sun;
uniform vec4 fog;
uniform vec4 light_col;
uniform vec4 amb_sky;
uniform vec4 amb_gnd;
uniform vec4 sun_dir;
uniform vec4 lamp;
uniform vec4 lamp_dir;
vec3 haze(vec3 dir) {
    vec3 S = normalize(sun_dir.xyz);
    float s = max(dot(dir, S), 0.0);
    float glow = pow(s, 6.0) * 0.55 * (1.0 - sun_dir.w) * smoothstep(-0.15, 0.05, S.y);
    vec3 c = mix(fog.rgb, vec3(1.0, 0.86, 0.68) * (0.4 + 0.6 * (1.0 - light_col.w)), glow);
    return c * (1.0 - 0.06 * clamp(dir.y * 3.0, 0.0, 1.0));
}
vec3 fogged(vec3 col, vec3 eye, vec3 wpos) {
    vec3 d = wpos - eye;
    float dist = length(d);
    float H = 1300.0;
    float e0 = exp(-max(eye.y, 0.0) / H);
    float avg = abs(d.y) > 1.0 ? H * (e0 - exp(-max(wpos.y, 0.0) / H)) / d.y : e0;
    float f = 1.0 - exp(-fog.w * dist * max(avg, 0.0));
    return mix(col, haze(d / max(dist, 0.001)), clamp(f, 0.0, 1.0));
}
vec3 landing_light(vec3 p, vec3 n) {
    if (lamp.w <= 0.0) return vec3(0.0);
    vec3 d = p - lamp.xyz;
    float dist = length(d);
    d /= max(dist, 0.01);
    float cone = smoothstep(0.94, 0.99, dot(d, lamp_dir.xyz));
    float fall = lamp.w / (1.0 + dist * dist / 40000.0);
    return vec3(1.0, 0.94, 0.82) * cone * fall * (0.25 + 0.75 * max(dot(n, -d), 0.0));
}
vec3 lit(vec3 base, vec3 n, float amb_min, vec3 wpos) {
    vec3 L = normalize(sun.xyz);
    float diff = max(dot(n, L), 0.0);
    float amb = max(sun.w, amb_min * (1.0 - 0.9 * light_col.w));
    vec3 ambient = mix(amb_gnd.rgb, amb_sky.rgb, 0.5 + 0.5 * n.y);
    return base * (ambient * amb + light_col.rgb * diff * (1.0 - min(amb, 0.6)) * 1.25 + landing_light(wpos, n));
}
float hash2(vec2 p) {
    return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453);
}
float vnoise(vec2 p) {
    vec2 i = floor(p);
    vec2 f = fract(p);
    f = f * f * (3.0 - 2.0 * f);
    return mix(mix(hash2(i), hash2(i + vec2(1.0, 0.0)), f.x), mix(hash2(i + vec2(0.0, 1.0)), hash2(i + vec2(1.0, 1.0)), f.x), f.y);
}
"#
    };
}

const WORLD_VS: &str = r#"#version 100
attribute vec3 in_pos;
attribute vec3 in_nrm;
attribute vec4 in_col;
uniform mat4 mvp;
uniform mat4 model;
uniform highp vec4 params;
varying highp vec3 v_wpos;
varying mediump vec3 v_nrm;
varying lowp vec4 v_col;
void main() {
    vec4 wp = model * vec4(in_pos, 1.0);
    v_wpos = wp.xyz;
    v_nrm = (model * vec4(in_nrm, 0.0)).xyz;
    v_col = in_col;
    gl_Position = mvp * vec4(in_pos, 1.0);
    gl_Position.z -= params.w * gl_Position.w;
}"#;

const WORLD_FS: &str = concat!(
    r#"#version 100
precision highp float;
varying highp vec3 v_wpos;
varying mediump vec3 v_nrm;
varying lowp vec4 v_col;
uniform vec4 cam;
uniform vec4 tint;
uniform vec4 params;
"#,
    atmosphere_glsl!(),
    r#"
void main() {
    vec3 n = normalize(v_nrm);
    vec3 V = normalize(cam.xyz - v_wpos);
    vec4 base = v_col * tint;
    if (params.x > 1.5) {
        // cloud deck: billowy coverage from layered noise, drifting slowly
        vec2 q = v_wpos.xz / 1400.0 + vec2(cam.w * 0.002, 0.0);
        float c = vnoise(q) * 0.5 + vnoise(q * 2.1) * 0.25 + vnoise(q * 4.3) * 0.15 + vnoise(q * 9.1) * 0.1;
        base.a *= smoothstep(0.25, 0.55, c);
        base.rgb *= 0.8 + 0.3 * c;
    }
    if (dot(n, V) < 0.0) n = -n;
    vec3 col = lit(base.rgb, n, params.z, v_wpos);
    col = mix(col, base.rgb, params.y);
    gl_FragColor = vec4(fogged(col, cam.xyz, v_wpos), base.a);
}"#
);

/// Terrain and sea: the baked colour map, detail texture up close, shallows and surf.
const GROUND_FS: &str = concat!(
    r#"#version 100
precision highp float;
varying highp vec3 v_wpos;
varying mediump vec3 v_nrm;
varying lowp vec4 v_col;
uniform vec4 cam;
uniform vec4 tint;
uniform vec4 params;
uniform sampler2D colormap;
uniform sampler2D detail;
"#,
    atmosphere_glsl!(),
    r#"
void main() {
    vec3 V = cam.xyz - v_wpos;
    float dist = length(V);
    V /= max(dist, 0.001);
    vec2 p = v_wpos.xz;
    vec4 cm = texture2D(colormap, (p + 12000.0) / 24000.0);
    vec3 col;
    if (params.x > 0.5) {
        float t = cam.w;
        // ripples fade out with distance before they can alias into moire
        float w = 1.0 - smoothstep(300.0, 2500.0, dist);
        vec3 n = normalize(vec3(
            (sin(p.x * 0.050 + t * 1.3) * 0.05 + sin((p.x + p.y) * 0.11 + t * 2.1) * 0.03) * w,
            1.0,
            (cos(p.y * 0.047 + t * 1.1) * 0.05 + cos((p.x - p.y) * 0.09 - t * 1.7) * 0.03) * w));
        float depth = cm.a;
        vec3 water = mix(vec3(0.20, 0.58, 0.60), vec3(0.05, 0.22, 0.36), smoothstep(0.0, 0.6, depth));
        col = lit(water, n, 0.5, v_wpos);
        vec3 L = normalize(sun.xyz);
        vec3 H = normalize(L + V);
        float spec = pow(max(dot(n, H), 0.0), 160.0);
        float fres = pow(1.0 - max(dot(n, V), 0.0), 4.0);
        col = mix(col, haze(reflect(-V, n)), fres * 0.8) + light_col.rgb * spec * 1.4;
        // surf where the sea meets the beach, rolling in
        float shore = 1.0 - smoothstep(0.0, 0.06, depth);
        float wave = smoothstep(0.55, 1.0, sin(depth * 140.0 - t * 1.6 + sin(p.x * 0.01) * 2.0));
        float brightness = 0.15 + 0.85 * (1.0 - light_col.w);
        col = mix(col, vec3(0.95, 0.97, 0.98) * brightness, shore * (0.35 + 0.5 * wave) * (1.0 - smoothstep(800.0, 4000.0, dist)));
    } else {
        vec3 base = cm.rgb;
        float d1 = texture2D(detail, p / 48.0).r;
        float d2 = texture2D(detail, p / 9.0).g;
        float d3 = texture2D(detail, p / 2.2).b;
        base *= 1.0 + (d1 - 0.5) * 0.45 * (1.0 - smoothstep(600.0, 2500.0, dist))
                    + (d2 - 0.5) * 0.4 * (1.0 - smoothstep(80.0, 400.0, dist))
                    + (d3 - 0.5) * 0.3 * (1.0 - smoothstep(12.0, 70.0, dist));
        col = lit(base * tint.rgb, normalize(v_nrm), params.z, v_wpos);
    }
    gl_FragColor = vec4(fogged(col, cam.xyz, v_wpos), 1.0);
}"#
);

const SKY_VS: &str = r#"#version 100
attribute vec2 in_pos;
uniform mat4 mvp;
varying highp vec3 v_dir;
void main() {
    gl_Position = vec4(in_pos, 0.0, 1.0);
    vec4 far_p = mvp * vec4(in_pos, 1.0, 1.0);
    vec4 near_p = mvp * vec4(in_pos, -1.0, 1.0);
    v_dir = far_p.xyz / far_p.w - near_p.xyz / near_p.w;
}"#;

const SKY_FS: &str = concat!(
    r#"#version 100
precision highp float;
varying highp vec3 v_dir;
uniform vec4 zenith;
"#,
    atmosphere_glsl!(),
    r#"
void main() {
    vec3 d = normalize(v_dir);
    float h = d.y;
    vec3 col = mix(haze(d), zenith.rgb, pow(clamp(h, 0.0, 1.0), 0.5));
    col = mix(col, haze(vec3(d.x, 0.0, d.z)) * 0.93, clamp(-h * 6.0, 0.0, 1.0));
    vec3 S = normalize(sun_dir.xyz);
    float s = max(dot(d, S), 0.0);
    float shown = (1.0 - sun_dir.w) * smoothstep(-0.03, 0.02, S.y);
    col += vec3(1.0, 0.92, 0.75) * (pow(s, 1200.0) * 6.0 + pow(s, 60.0) * 0.25 + pow(s, 8.0) * 0.08) * shown;
    if (zenith.w > 0.01 && h > 0.0) {
        // stars: one in a few hundred cells, twinkling gently
        vec3 q = d * 260.0;
        vec3 cell = floor(q);
        float r = fract(sin(dot(cell, vec3(12.9898, 78.233, 37.719))) * 43758.5453);
        float core = smoothstep(0.42, 0.0, length(fract(q) - 0.5));
        float star = step(0.992, r) * core * (0.6 + 0.4 * sin(r * 900.0));
        col += vec3(0.9, 0.95, 1.0) * star * zenith.w * smoothstep(0.0, 0.25, h);
        // and a moon
        vec3 M = normalize(vec3(-0.3, 0.75, 0.55));
        float m = dot(d, M);
        col += vec3(0.9, 0.92, 1.0) * (smoothstep(0.99985, 0.9999, m) * 1.2 + pow(max(m, 0.0), 300.0) * 0.08) * zenith.w;
    }
    gl_FragColor = vec4(col, 1.0);
}"#
);

fn world_meta() -> ShaderMeta {
    ShaderMeta {
        images: vec![],
        uniforms: UniformBlockLayout {
            uniforms: ["mvp", "model"]
                .iter()
                .map(|n| UniformDesc::new(n, UniformType::Mat4))
                .chain(
                    ["cam", "sun", "fog", "tint", "params", "light_col", "amb_sky", "amb_gnd", "sun_dir", "lamp", "lamp_dir", "zenith"]
                        .iter()
                        .map(|n| UniformDesc::new(n, UniformType::Float4)),
                )
                .collect(),
        },
    }
}

fn ground_meta() -> ShaderMeta {
    ShaderMeta { images: vec!["colormap".to_string(), "detail".to_string()], uniforms: world_meta().uniforms }
}

pub struct View {
    pub view_proj: Mat4,
    pub eye: Vec3,
    pub forward: Vec3,
    pub right: Vec3,
    pub up: Vec3,
    /// Vertical field of view, radians.
    pub fov: f32,
}

pub struct RingDraw {
    pub pos: Vec3,
    pub normal: Vec3,
    pub radius: f32,
    pub color: [f32; 4],
}

/// A soft billboard: smoke, dust.
pub struct Puff {
    pub pos: Vec3,
    pub size: f32,
    pub color: [f32; 4],
}

pub struct Scene<'a> {
    pub ac: &'a Aircraft,
    /// Lights that move or change this frame (the aircraft's, the PAPI).
    pub lamps: &'a [Lamp],
    pub sky: &'a Sky,
    /// Landing light: position and beam direction, when it's on.
    pub lamp: Option<(Vec3, Vec3)>,
    /// Fraction of the cumulus field to draw, and how far to raise it (m).
    pub cumulus: (f32, f32),
    pub puffs: &'a [Puff],
    pub show_fuselage: bool,
    /// Surface wind (world velocity of the air) for windsocks and turbines.
    pub wind: Vec3,
    /// Other aircraft flying around.
    pub traffic: &'a [Aircraft],
    pub rings: Vec<RingDraw>,
    pub papi: Vec<(Vec3, bool)>, // position, white?
    pub time: f32,
    pub shadow_alpha: f32,
    pub shadow_ground: f32,
}

struct GpuPart {
    mesh: GpuMesh,
    pivot: Vec3,
    axis: Vec3,
    drive: mesh::Drive,
    seen: mesh::Seen,
}

struct GpuModel {
    parts: Vec<GpuPart>,
    prop_disc: Option<(Vec3, f32)>,
}

fn upload_model(ctx: &mut dyn RenderingBackend, m: mesh::Model) -> GpuModel {
    GpuModel {
        parts: m.parts.into_iter().map(|p| GpuPart { mesh: upload(ctx, &p.mesh), pivot: p.pivot, axis: p.axis, drive: p.drive, seen: p.seen }).collect(),
        prop_disc: m.prop_disc,
    }
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct LightVertex {
    pos: [f32; 3],
    uv: [f32; 2],
    col: [f32; 4],
}

const LIGHT_VS: &str = r#"#version 100
attribute vec3 in_pos;
attribute vec2 in_uv;
attribute vec4 in_col;
uniform mat4 mvp;
varying mediump vec2 v_uv;
varying lowp vec4 v_col;
void main() {
    v_uv = in_uv;
    v_col = in_col;
    gl_Position = mvp * vec4(in_pos, 1.0);
}"#;

const LIGHT_FS: &str = r#"#version 100
precision mediump float;
varying mediump vec2 v_uv;
varying lowp vec4 v_col;
void main() {
    float r2 = dot(v_uv, v_uv);
    float a = exp(-r2 * 9.0) * 1.2 + exp(-r2 * 2.2) * 0.22;
    gl_FragColor = vec4(v_col.rgb * a * v_col.a, 1.0);
}"#;

/// All the glowing lamps in one streamed vertex buffer and one draw call.
struct LightLayer {
    pipe: Pipeline,
    bind: Bindings,
    cap: usize,
    lamps: Vec<Lamp>,
}

const MAX_LAMPS: usize = 6000;

impl LightLayer {
    fn new(ctx: &mut dyn RenderingBackend, lamps: Vec<Lamp>) -> Self {
        let shader = ctx
            .new_shader(
                ShaderSource::Glsl { vertex: LIGHT_VS, fragment: LIGHT_FS },
                ShaderMeta { images: vec![], uniforms: UniformBlockLayout { uniforms: vec![UniformDesc::new("mvp", UniformType::Mat4)] } },
            )
            .unwrap_or_else(|e| panic!("light shader: {e:?}"));
        let additive = Some(BlendState::new(Equation::Add, BlendFactor::One, BlendFactor::One));
        let pipe = ctx.new_pipeline(
            &[BufferLayout::default()],
            &[VertexAttribute::new("in_pos", VertexFormat::Float3), VertexAttribute::new("in_uv", VertexFormat::Float2), VertexAttribute::new("in_col", VertexFormat::Float4)],
            shader,
            PipelineParams { depth_test: Comparison::LessOrEqual, depth_write: false, color_blend: additive, ..Default::default() },
        );
        let vb = ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Stream, BufferSource::empty::<LightVertex>(MAX_LAMPS * 4));
        let idx: Vec<u32> = (0..MAX_LAMPS as u32).flat_map(|q| [q * 4, q * 4 + 1, q * 4 + 2, q * 4, q * 4 + 2, q * 4 + 3]).collect();
        let ib = ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&idx));
        LightLayer { pipe, bind: Bindings { vertex_buffers: vec![vb], index_buffer: ib, images: vec![] }, cap: MAX_LAMPS, lamps }
    }

    fn draw(&self, ctx: &mut dyn RenderingBackend, view: &View, sky: &Sky, extra: &[Lamp], time: f32) {
        let (_, sh) = window::screen_size();
        // world size of one pixel at unit distance; far lamps stay about 3 px across
        let px = 2.0 * (view.fov * 0.5).tan() / sh.max(1.0) * window::dpi_scale();
        let mut verts: Vec<LightVertex> = Vec::with_capacity((self.lamps.len() + extra.len()) * 4);
        for l in self.lamps.iter().chain(extra) {
            if verts.len() / 4 >= self.cap {
                break;
            }
            let strength = if l.day { 1.0 } else { sky.lamps };
            if strength < 0.01 {
                continue;
            }
            let to = l.pos - view.eye;
            let dist = to.length();
            if dist < 0.5 || to.dot(view.forward) < 0.0 || dist > 20_000.0 {
                continue;
            }
            let mut a = strength * l.level(time);
            if let Some(f) = l.facing {
                a *= ((-to / dist).dot(f.normalize()) * 4.0 + 0.2).clamp(0.0, 1.0);
            }
            a *= (-sky.haze * dist * 0.35).exp();
            if a < 0.01 {
                continue;
            }
            let s = l.size.max(dist * px * 2.0);
            // far away a lamp is a point: shrink its share so a runway seen end-on doesn't
            // pile up into one blinding blob
            a *= (l.size * 3.0 / s).clamp(0.3, 1.0);
            let (r, u) = (view.right * s, view.up * s);
            let col = [l.col[0], l.col[1], l.col[2], a];
            for (c, uv) in [(-r - u, [-1.0, -1.0]), (r - u, [1.0, -1.0]), (r + u, [1.0, 1.0]), (-r + u, [-1.0, 1.0])] {
                let p = l.pos + c;
                verts.push(LightVertex { pos: [p.x, p.y, p.z], uv, col });
            }
        }
        if verts.is_empty() {
            return;
        }
        ctx.buffer_update(self.bind.vertex_buffers[0], BufferSource::slice(&verts));
        ctx.apply_pipeline(&self.pipe);
        ctx.apply_bindings(&self.bind);
        ctx.apply_uniforms(UniformsSource::table(&view.view_proj.to_cols_array()));
        ctx.draw(0, (verts.len() / 4 * 6) as i32, 1);
    }
}

pub struct Renderer {
    opaque: Pipeline,
    ground: Pipeline,
    blend: Pipeline,
    shadow: Pipeline,
    sky: Pipeline,
    sky_quad: Bindings,
    terrain: GpuMesh,
    water: GpuMesh,
    trees: GpuMesh,
    buildings: GpuMesh,
    clouds: GpuMesh,
    runway: GpuMesh,
    markings: GpuMesh,
    ring: GpuMesh,
    cube: GpuMesh,
    disc: GpuMesh,
    puff: GpuMesh,
    deck: GpuMesh,
    lights: LightLayer,
    pavement: GpuMesh,
    props: GpuMesh,
    sock: GpuMesh,
    nacelle: GpuMesh,
    rotor: GpuMesh,
    beam: GpuMesh,
    boats: [GpuMesh; 2],
    turbines: Vec<Vec3>,
    lighthouse: Vec3,
    pier: (Vec3, Vec3),
    parked: Vec<Aircraft>,
    /// Indexed by `Craft`.
    models: [GpuModel; 4],
}

impl Renderer {
    pub fn new(ctx: &mut dyn RenderingBackend, world: &World) -> Self {
        let shader = ctx
            .new_shader(ShaderSource::Glsl { vertex: WORLD_VS, fragment: WORLD_FS }, world_meta())
            .unwrap_or_else(|e| panic!("world shader: {e:?}"));
        let attrs = [
            VertexAttribute::new("in_pos", VertexFormat::Float3),
            VertexAttribute::new("in_nrm", VertexFormat::Float3),
            VertexAttribute::new("in_col", VertexFormat::Float4),
        ];
        let layout = [BufferLayout::default()];
        let opaque = ctx.new_pipeline(
            &layout,
            &attrs,
            shader,
            PipelineParams { depth_test: Comparison::LessOrEqual, depth_write: true, ..Default::default() },
        );
        let alpha = Some(BlendState::new(
            Equation::Add,
            BlendFactor::Value(BlendValue::SourceAlpha),
            BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
        ));
        let blend = ctx.new_pipeline(
            &layout,
            &attrs,
            shader,
            PipelineParams { depth_test: Comparison::LessOrEqual, depth_write: false, color_blend: alpha, ..Default::default() },
        );
        // Depth write + strict Less means overlapping shadow triangles darken only once.
        let shadow = ctx.new_pipeline(
            &layout,
            &attrs,
            shader,
            PipelineParams { depth_test: Comparison::Less, depth_write: true, color_blend: alpha, ..Default::default() },
        );

        let ground_shader = ctx
            .new_shader(ShaderSource::Glsl { vertex: WORLD_VS, fragment: GROUND_FS }, ground_meta())
            .unwrap_or_else(|e| panic!("ground shader: {e:?}"));
        let ground = ctx.new_pipeline(
            &layout,
            &attrs,
            ground_shader,
            PipelineParams { depth_test: Comparison::LessOrEqual, depth_write: true, ..Default::default() },
        );
        let ground_tex = [
            texture(ctx, ground::COLORMAP, &ground::colormap(world), TextureWrap::Clamp),
            texture(ctx, ground::DETAIL, &ground::detail(), TextureWrap::Repeat),
        ];

        let sky_shader = ctx
            .new_shader(ShaderSource::Glsl { vertex: SKY_VS, fragment: SKY_FS }, world_meta())
            .unwrap_or_else(|e| panic!("sky shader: {e:?}"));
        let sky = ctx.new_pipeline(
            &layout,
            &[VertexAttribute::new("in_pos", VertexFormat::Float2)],
            sky_shader,
            PipelineParams::default(),
        );
        let tri: [f32; 6] = [-1.0, -1.0, 3.0, -1.0, -1.0, 3.0];
        let idx: [u16; 3] = [0, 1, 2];
        let sky_quad = Bindings {
            vertex_buffers: vec![ctx.new_buffer(BufferType::VertexBuffer, BufferUsage::Immutable, BufferSource::slice(&tri))],
            index_buffer: ctx.new_buffer(BufferType::IndexBuffer, BufferUsage::Immutable, BufferSource::slice(&idx)),
            images: vec![],
        };

        let (nacelle, rotor) = scenery::turbine_head();
        let models = [upload_model(ctx, mesh::trainer()), upload_model(ctx, airliner::model()), upload_model(ctx, crate::fighter::model()), upload_model(ctx, crate::glider::model())];

        Renderer {
            opaque,
            ground,
            blend,
            shadow,
            sky,
            sky_quad,
            terrain: upload(ctx, &mesh::terrain(&world.terrain)).with_images(&ground_tex),
            water: upload(ctx, &mesh::water()).with_images(&ground_tex),
            trees: upload(ctx, &mesh::trees(world)),
            buildings: upload(ctx, &mesh::buildings(world)),
            clouds: upload(ctx, &mesh::clouds(world)),
            runway: upload(ctx, &mesh::runway()),
            markings: upload(ctx, &mesh::markings()),
            ring: upload(ctx, &mesh::torus(0.1)),
            cube: upload(ctx, &mesh::unit_cube()),
            disc: upload(ctx, &mesh::disc(0.95)),
            puff: upload(ctx, &mesh::puff()),
            deck: upload(ctx, &mesh::deck()),
            lights: LightLayer::new(ctx, crate::lights::static_lamps(world)),
            pavement: upload(ctx, &scenery::pavement()),
            props: upload(ctx, &scenery::props(world)),
            sock: upload(ctx, &scenery::sock()),
            nacelle: upload(ctx, &nacelle),
            rotor: upload(ctx, &rotor),
            beam: upload(ctx, &scenery::beam()),
            boats: [upload(ctx, &scenery::sailboat()), upload(ctx, &scenery::ferry())],
            turbines: world.turbines.clone(),
            lighthouse: vec3(LIGHTHOUSE.0, world.terrain.height(LIGHTHOUSE.0, LIGHTHOUSE.1) + 32.5, LIGHTHOUSE.1),
            pier: scenery::pier(&world.terrain),
            parked: scenery::parked()
                .into_iter()
                .map(|(craft, pos, hdg)| {
                    let mut a = Aircraft::on_ground_at(craft, &world.terrain, pos.x, pos.z, hdg);
                    a.pos.y = pos.y;
                    a
                })
                .collect(),
            models,
        }
    }

    pub fn draw(&self, ctx: &mut dyn RenderingBackend, view: &View, scene: &Scene) {
        let sky = scene.sky;
        let f4 = |v: Vec3, w: f32| [v.x, v.y, v.z, w];
        let (lamp, lamp_dir) = match scene.lamp {
            Some((p, d)) => (f4(p, 1.6 * (0.25 + 0.75 * sky.night)), f4(d, 0.0)),
            None => ([0.0; 4], [0.0; 4]),
        };
        let frame = WorldUniforms {
            mvp: [0.0; 16],
            model: [0.0; 16],
            cam: [view.eye.x, view.eye.y, view.eye.z, scene.time],
            sun: f4(sky.light, sky.ambient),
            fog: f4(sky.fog, sky.haze),
            tint: [1.0; 4],
            params: [0.0; 4],
            light_col: f4(sky.light_col, sky.night),
            amb_sky: f4(sky.amb_sky, 0.0),
            amb_gnd: f4(sky.amb_ground, 0.0),
            sun_dir: f4(sky.sun, sky.overcast),
            lamp,
            lamp_dir,
            zenith: f4(sky.zenith, sky.night * (1.0 - sky.overcast)),
        };
        ctx.begin_default_pass(PassAction::Clear { color: Some((sky.fog.x, sky.fog.y, sky.fog.z, 1.0)), depth: Some(1.0), stencil: None });

        // sky
        ctx.apply_pipeline(&self.sky);
        ctx.apply_bindings(&self.sky_quad);
        ctx.apply_uniforms(UniformsSource::table(&WorldUniforms { mvp: view.view_proj.inverse().to_cols_array(), ..frame }));
        ctx.draw(0, 3, 1);
        let sun = sky.light;

        let d = Draw { ctx, view, frame };
        let mut d = d;
        let white = [1.0; 4];
        let none = [0.0; 4];

        d.ctx.apply_pipeline(&self.ground);
        d.mesh(&self.terrain, Mat4::IDENTITY, white, none);
        d.mesh(&self.water, Mat4::IDENTITY, white, [1.0, 0.0, 0.0, 0.0]);
        d.ctx.apply_pipeline(&self.opaque);
        // Runway paint sits 4-8 cm above flat ground; a small depth bias stops it shimmering far away.
        d.mesh(&self.runway, Mat4::IDENTITY, white, [0.0, 0.0, 0.0, 6e-6]);
        d.mesh(&self.markings, Mat4::IDENTITY, white, [0.0, 0.0, 0.0, 1.4e-5]);
        d.mesh(&self.pavement, Mat4::IDENTITY, white, [0.0, 0.0, 0.0, 6e-6]);
        d.mesh(&self.props, Mat4::IDENTITY, white, none);
        // parked aircraft and traffic
        for a in self.parked.iter().chain(scene.traffic) {
            for (mesh, m) in self.plane_parts(a, false, false) {
                d.mesh(mesh, m, white, none);
            }
        }
        // turbines face into the wind and turn with it
        let w = scene.wind;
        let into = if w.length() > 0.3 { -w.normalize() } else { vec3(1.0, 0.0, 0.0) };
        let yaw = Quat::from_rotation_y(-into.x.atan2(-into.z));
        let spin = scene.time * (0.4 + w.length() * 0.12);
        for (k, p) in self.turbines.iter().enumerate() {
            let hub = Mat4::from_rotation_translation(yaw, *p + vec3(0.0, 82.0, 0.0));
            d.mesh(&self.nacelle, hub, white, none);
            d.mesh(&self.rotor, hub * Mat4::from_translation(vec3(0.0, 0.0, -2.6)) * Mat4::from_rotation_z(spin + k as f32 * 0.7), white, none);
        }
        // windsocks stream out with the wind, hang limp in a calm
        let speed = (w.length() / (15.0 * crate::flight::KT)).min(1.0);
        let along = if w.length() > 0.2 { w.normalize() } else { vec3(0.0, 0.0, 1.0) };
        for (k, (top, _)) in scenery::windsocks().iter().enumerate() {
            let flap = 0.06 * (scene.time * 3.0 + k as f32).sin() * speed;
            let droop = (1.0 - speed) * 1.35 + flap;
            let dir = (along * droop.cos() - Vec3::Y * droop.sin()).normalize();
            let rot = Quat::from_rotation_arc(Vec3::Z, dir);
            d.mesh(&self.sock, Mat4::from_rotation_translation(rot, *top), white, none);
        }
        // boats
        for (kind, m) in scenery::boats(scene.time, self.pier.0, self.pier.1) {
            d.mesh(&self.boats[kind], m, white, none);
        }
        d.mesh(&self.trees, Mat4::IDENTITY, white, none);
        d.mesh(&self.buildings, Mat4::IDENTITY, white, none);
        let (cover, lift) = scene.cumulus;
        if cover > 0.01 {
            let n = ((self.clouds.n as f32 * cover) as i32 / 3) * 3;
            d.mesh_part(&self.clouds, Mat4::from_translation(vec3(0.0, lift, 0.0)), white, [0.0, 0.0, 0.85, 0.0], n);
        }

        for r in &scene.rings {
            let m = Mat4::from_scale_rotation_translation(Vec3::splat(r.radius), Quat::from_rotation_arc(Vec3::Z, r.normal), r.pos);
            d.mesh(&self.ring, m, r.color, [0.0, 0.45, 0.0, 0.0]);
        }
        for (p, white_light) in &scene.papi {
            let scale = ((*p - view.eye).length() / 1500.0).clamp(1.0, 2.5);
            let m = Mat4::from_scale_rotation_translation(vec3(2.4, 1.0, 2.4) * scale, Quat::IDENTITY, *p);
            let c = if *white_light { [1.0, 1.0, 0.95, 1.0] } else { [1.0, 0.12, 0.08, 1.0] };
            d.mesh(&self.cube, m, c, [0.0, 1.0, 0.0, 0.0]);
        }

        // aircraft
        let ac = scene.ac;
        // (the landing light shines ahead of the aircraft, not on it)
        let lamp = d.frame.lamp;
        d.frame.lamp[3] = 0.0;
        let parts = self.plane_parts(ac, !scene.show_fuselage, false);
        for (mesh, m) in &parts {
            if !std::ptr::eq(*mesh, self.burner_mesh(ac)) {
                d.mesh(mesh, *m, white, none);
            }
        }
        d.frame.lamp = lamp;
        if let Some((mesh, m)) = parts.iter().find(|(mesh, _)| std::ptr::eq(*mesh, self.burner_mesh(ac))) {
            d.ctx.apply_pipeline(&self.blend);
            d.mesh(mesh, *m, [1.0, 1.0, 1.0, ac.burner], [0.0, 1.0, 0.0, 0.0]);
            d.ctx.apply_pipeline(&self.opaque);
        }

        // shadow, projected onto the ground plane under the aircraft along the sun direction
        let shadow_alpha = scene.shadow_alpha * (1.0 - sky.overcast) * (1.0 - sky.night) * ((sky.light.y - 0.08) * 6.0).clamp(0.0, 1.0);
        if shadow_alpha > 0.01 {
            let l = sun;
            let (kx, kz, g) = (l.x / l.y, l.z / l.y, scene.shadow_ground + 0.1);
            let proj = Mat4::from_cols(
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::new(-kx, 0.0, -kz, 0.0),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
                Vec4::new(kx * g, g, kz * g, 1.0),
            );
            d.ctx.apply_pipeline(&self.shadow);
            for (mesh, m) in self.plane_parts(ac, false, true) {
                d.mesh(mesh, proj * m, [0.0, 0.0, 0.0, shadow_alpha], [0.0, 1.0, 0.0, 2e-5]);
            }
        }

        // spinning prop disc
        d.ctx.apply_pipeline(&self.blend);
        if let Some((c, r)) = self.models[ac.craft as usize].prop_disc {
            let disc_m = Mat4::from_rotation_translation(ac.rot, ac.pos) * Mat4::from_scale_rotation_translation(Vec3::splat(r / 0.95), Quat::IDENTITY, c);
            d.mesh(&self.disc, disc_m, [0.85, 0.85, 0.85, if scene.show_fuselage { 0.10 + 0.12 * ac.ctl.throttle } else { 0.04 }], [0.0, 0.4, 0.0, 0.0]);
        }

        // smoke and dust, farthest first so they layer properly
        let mut order: Vec<&Puff> = scene.puffs.iter().collect();
        order.sort_by(|a, b| (b.pos - view.eye).length_squared().total_cmp(&(a.pos - view.eye).length_squared()));
        for p in order {
            let facing = Quat::from_rotation_arc(Vec3::Z, (view.eye - p.pos).normalize_or_zero());
            let m = Mat4::from_scale_rotation_translation(Vec3::splat(p.size), facing, p.pos);
            d.mesh(&self.puff, m, p.color, [0.0, 0.55, 1.0, 0.0]);
        }

        // the lighthouse sweeps two beams round every twelve seconds
        if sky.lamps > 0.15 {
            d.ctx.apply_pipeline(&self.blend);
            let a = scene.time * std::f32::consts::TAU / 12.0;
            for k in 0..2 {
                let m = Mat4::from_rotation_translation(Quat::from_rotation_y(a + k as f32 * std::f32::consts::PI), self.lighthouse);
                d.mesh(&self.beam, m, [1.0, 1.0, 1.0, 0.16 * sky.lamps], [0.0, 1.0, 0.0, 0.0]);
            }
        }
        self.lights.draw(d.ctx, view, sky, scene.lamps, scene.time);

        // overcast deck: a lit top and a darker underside a few hundred metres apart
        if let Some(base) = sky.deck {
            let grey = [0.78, 0.8, 0.84, 0.97];
            let top = [1.0, 1.0, 1.0, 0.97];
            // draw the far side first
            let (first, second) = if view.eye.y > base + 150.0 { ((base, grey), (base + 300.0, top)) } else { ((base + 300.0, top), (base, grey)) };
            for (y, c) in [first, second] {
                let m = Mat4::from_translation(vec3(view.eye.x, y, view.eye.z));
                d.mesh(&self.deck, m, c, [2.0, 0.0, 0.75, 0.0]);
            }
        }

        d.ctx.end_render_pass();
    }

    /// The afterburner flame's mesh, if this aircraft has one.
    fn burner_mesh(&self, ac: &Aircraft) -> *const GpuMesh {
        self.models[ac.craft as usize].parts.iter().find(|p| p.drive == mesh::Drive::Burner).map_or(std::ptr::null(), |p| &p.mesh as *const GpuMesh)
    }

    /// The aircraft's parts posed for this frame: from the cockpit, or (for the shadow) only
    /// the solid outside parts.
    fn plane_parts(&self, ac: &Aircraft, cockpit: bool, shadow: bool) -> Vec<(&GpuMesh, Mat4)> {
        use mesh::{Drive, Seen};
        let base = Mat4::from_rotation_translation(ac.rot, ac.pos);
        let mut v = Vec::new();
        for p in &self.models[ac.craft as usize].parts {
            let show = match p.seen {
                Seen::Always => true,
                Seen::Outside => !cockpit,
                Seen::Cockpit => cockpit && !shadow,
            };
            if !show || (shadow && p.drive == Drive::Spin) {
                continue;
            }
            let hinge = |angle: f32| base * Mat4::from_translation(p.pivot) * Mat4::from_axis_angle(p.axis, angle) * Mat4::from_translation(-p.pivot);
            let m = match p.drive {
                Drive::Fixed => base,
                Drive::Flap => hinge(ac.flap_deg.to_radians()),
                Drive::Aileron(sign) => hinge(sign * ac.ail),
                Drive::Elevator => hinge(-ac.elev),
                Drive::Rudder => hinge(ac.rud),
                Drive::Spin => hinge(ac.prop_angle),
                Drive::Spoiler => hinge(-ac.spoiler * 0.85),
                Drive::Gear => {
                    if ac.gear < 0.03 {
                        continue;
                    }
                    base * Mat4::from_translation(vec3(0.0, (1.0 - ac.gear) * 1.4, 0.0))
                }
                Drive::Reverser => base * Mat4::from_translation(vec3(0.0, 0.0, ac.reverser * 0.9)),
                Drive::Burner => {
                    if ac.burner < 0.02 || shadow || cockpit {
                        continue;
                    }
                    let flicker = 1.0 + 0.06 * (ac.prop_angle * 7.0).sin();
                    base * Mat4::from_translation(p.pivot) * Mat4::from_scale(vec3(1.0, 1.0, (0.35 + 0.65 * ac.burner) * flicker)) * Mat4::from_translation(-p.pivot)
                }
            };
            v.push((&p.mesh, m));
        }
        v
    }
}

struct Draw<'a> {
    ctx: &'a mut dyn RenderingBackend,
    view: &'a View,
    frame: WorldUniforms,
}

impl Draw<'_> {
    fn mesh(&mut self, m: &GpuMesh, model: Mat4, tint: [f32; 4], params: [f32; 4]) {
        self.mesh_part(m, model, tint, params, m.n);
    }

    /// Draw only the first `count` indices (the cumulus field is thinned this way).
    fn mesh_part(&mut self, m: &GpuMesh, model: Mat4, tint: [f32; 4], params: [f32; 4], count: i32) {
        self.ctx.apply_bindings(&m.bind);
        self.ctx.apply_uniforms(UniformsSource::table(&WorldUniforms {
            mvp: (self.view.view_proj * model).to_cols_array(),
            model: model.to_cols_array(),
            tint,
            params,
            ..self.frame
        }));
        self.ctx.draw(0, count, 1);
    }
}

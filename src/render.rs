//! Static GPU buffers + a tiny lit/fogged shader. Everything in the world is
//! uploaded once at startup, so a frame is just a few dozen draw calls.

use crate::flight::Aircraft;
use crate::mesh::{self, MeshData};
use crate::world::World;
use macroquad::math::{vec3, Mat4, Quat, Vec3, Vec4};
use macroquad::miniquad::*;

pub const SUN: Vec3 = vec3(-0.36, 0.78, 0.5);
pub const FOG: [f32; 3] = [0.73, 0.81, 0.89];
const ZENITH: [f32; 3] = [0.24, 0.47, 0.86];
const FOG_DENSITY: f32 = 1.35e-4;

pub struct GpuMesh {
    bind: Bindings,
    n: i32,
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

#[repr(C)]
struct WorldUniforms {
    mvp: [f32; 16],
    model: [f32; 16],
    cam: [f32; 4],    // xyz eye, w time
    sun: [f32; 4],    // xyz dir, w ambient
    fog: [f32; 4],    // rgb, w density
    tint: [f32; 4],   // multiplies vertex colour
    params: [f32; 4], // x water, y emissive, z min ambient, w depth bias
}

#[repr(C)]
struct SkyUniforms {
    inv_vp: [f32; 16],
    sun: [f32; 4],
    fog: [f32; 4],
    zenith: [f32; 4],
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

const WORLD_FS: &str = r#"#version 100
precision highp float;
varying highp vec3 v_wpos;
varying mediump vec3 v_nrm;
varying lowp vec4 v_col;
uniform vec4 cam;
uniform vec4 sun;
uniform vec4 fog;
uniform vec4 tint;
uniform vec4 params;
void main() {
    vec3 n = normalize(v_nrm);
    vec3 L = normalize(sun.xyz);
    vec3 V = cam.xyz - v_wpos;
    float dist = length(V);
    V /= max(dist, 0.001);
    vec4 base = v_col * tint;
    if (params.x > 0.5) {
        float t = cam.w;
        vec2 p = v_wpos.xz;
        n = normalize(vec3(
            sin(p.x * 0.050 + t * 1.3) * 0.05 + sin((p.x + p.y) * 0.11 + t * 2.1) * 0.03,
            1.0,
            cos(p.y * 0.047 + t * 1.1) * 0.05 + cos((p.x - p.y) * 0.09 - t * 1.7) * 0.03));
    }
    if (dot(n, V) < 0.0 && params.x < 0.5) n = -n;
    float diff = max(dot(n, L), 0.0);
    float amb = max(sun.w, params.z);
    float hemi = 0.7 + 0.3 * n.y;
    vec3 col = base.rgb * (amb * hemi + diff * (1.0 - amb));
    if (params.x > 0.5) {
        vec3 H = normalize(L + V);
        float spec = pow(max(dot(n, H), 0.0), 140.0);
        float fres = pow(1.0 - max(dot(n, V), 0.0), 4.0);
        col = mix(col, fog.rgb, fres * 0.75) + vec3(1.0, 0.95, 0.8) * spec * 1.4;
    }
    col = mix(col, base.rgb, params.y);
    float fd = dist * fog.w;
    float f = 1.0 - exp(-fd * fd);
    col = mix(col, fog.rgb, clamp(f, 0.0, 1.0));
    gl_FragColor = vec4(col, base.a);
}"#;

const SKY_VS: &str = r#"#version 100
attribute vec2 in_pos;
uniform mat4 inv_vp;
varying highp vec3 v_dir;
void main() {
    gl_Position = vec4(in_pos, 0.0, 1.0);
    vec4 far_p = inv_vp * vec4(in_pos, 1.0, 1.0);
    vec4 near_p = inv_vp * vec4(in_pos, -1.0, 1.0);
    v_dir = far_p.xyz / far_p.w - near_p.xyz / near_p.w;
}"#;

const SKY_FS: &str = r#"#version 100
precision highp float;
varying highp vec3 v_dir;
uniform vec4 sun;
uniform vec4 fog;
uniform vec4 zenith;
void main() {
    vec3 d = normalize(v_dir);
    float h = d.y;
    vec3 col = mix(fog.rgb, zenith.rgb, pow(clamp(h, 0.0, 1.0), 0.55));
    col = mix(col, fog.rgb * 0.92, clamp(-h * 4.0, 0.0, 1.0));
    float s = max(dot(d, normalize(sun.xyz)), 0.0);
    col += vec3(1.0, 0.92, 0.75) * (pow(s, 900.0) * 4.0 + pow(s, 24.0) * 0.18);
    gl_FragColor = vec4(col, 1.0);
}"#;

fn world_meta() -> ShaderMeta {
    ShaderMeta {
        images: vec![],
        uniforms: UniformBlockLayout {
            uniforms: vec![
                UniformDesc::new("mvp", UniformType::Mat4),
                UniformDesc::new("model", UniformType::Mat4),
                UniformDesc::new("cam", UniformType::Float4),
                UniformDesc::new("sun", UniformType::Float4),
                UniformDesc::new("fog", UniformType::Float4),
                UniformDesc::new("tint", UniformType::Float4),
                UniformDesc::new("params", UniformType::Float4),
            ],
        },
    }
}

fn sky_meta() -> ShaderMeta {
    ShaderMeta {
        images: vec![],
        uniforms: UniformBlockLayout {
            uniforms: vec![
                UniformDesc::new("inv_vp", UniformType::Mat4),
                UniformDesc::new("sun", UniformType::Float4),
                UniformDesc::new("fog", UniformType::Float4),
                UniformDesc::new("zenith", UniformType::Float4),
            ],
        },
    }
}

pub struct View {
    pub view_proj: Mat4,
    pub eye: Vec3,
}

pub struct RingDraw {
    pub pos: Vec3,
    pub normal: Vec3,
    pub radius: f32,
    pub color: [f32; 4],
}

pub struct Scene<'a> {
    pub ac: &'a Aircraft,
    pub show_fuselage: bool,
    pub rings: Vec<RingDraw>,
    pub papi: Vec<(Vec3, bool)>, // position, white?
    pub time: f32,
    pub shadow_alpha: f32,
    pub shadow_ground: f32,
}

struct Plane {
    fuselage: GpuMesh,
    cowl: GpuMesh,
    wings: GpuMesh,
    flap_l: GpuMesh,
    flap_r: GpuMesh,
    ail_l: GpuMesh,
    ail_r: GpuMesh,
    elevator: GpuMesh,
    rudder: GpuMesh,
    prop: GpuMesh,
}

pub struct Renderer {
    opaque: Pipeline,
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
    plane: Plane,
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

        let sky_shader = ctx
            .new_shader(ShaderSource::Glsl { vertex: SKY_VS, fragment: SKY_FS }, sky_meta())
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

        let p = mesh::plane();
        let plane = Plane {
            fuselage: upload(ctx, &p.fuselage),
            cowl: upload(ctx, &p.cowl),
            wings: upload(ctx, &p.wings),
            flap_l: upload(ctx, &p.flap_l),
            flap_r: upload(ctx, &p.flap_r),
            ail_l: upload(ctx, &p.ail_l),
            ail_r: upload(ctx, &p.ail_r),
            elevator: upload(ctx, &p.elevator),
            rudder: upload(ctx, &p.rudder),
            prop: upload(ctx, &p.prop),
        };

        Renderer {
            opaque,
            blend,
            shadow,
            sky,
            sky_quad,
            terrain: upload(ctx, &mesh::terrain(&world.terrain)),
            water: upload(ctx, &mesh::water()),
            trees: upload(ctx, &mesh::trees(world)),
            buildings: upload(ctx, &mesh::buildings(world)),
            clouds: upload(ctx, &mesh::clouds(world)),
            runway: upload(ctx, &mesh::runway()),
            markings: upload(ctx, &mesh::markings()),
            ring: upload(ctx, &mesh::torus(0.1)),
            cube: upload(ctx, &mesh::unit_cube()),
            disc: upload(ctx, &mesh::disc(0.95)),
            plane,
        }
    }

    pub fn draw(&self, ctx: &mut dyn RenderingBackend, view: &View, scene: &Scene) {
        ctx.begin_default_pass(PassAction::Clear { color: Some((FOG[0], FOG[1], FOG[2], 1.0)), depth: Some(1.0), stencil: None });

        // sky
        ctx.apply_pipeline(&self.sky);
        ctx.apply_bindings(&self.sky_quad);
        let sun = SUN.normalize();
        ctx.apply_uniforms(UniformsSource::table(&SkyUniforms {
            inv_vp: view.view_proj.inverse().to_cols_array(),
            sun: [sun.x, sun.y, sun.z, 0.0],
            fog: [FOG[0], FOG[1], FOG[2], 0.0],
            zenith: [ZENITH[0], ZENITH[1], ZENITH[2], 0.0],
        }));
        ctx.draw(0, 3, 1);

        let d = Draw { ctx, view, time: scene.time };
        let mut d = d;
        let white = [1.0; 4];
        let none = [0.0; 4];

        d.ctx.apply_pipeline(&self.opaque);
        d.mesh(&self.terrain, Mat4::IDENTITY, white, none);
        d.mesh(&self.water, Mat4::IDENTITY, white, [1.0, 0.0, 0.0, 0.0]);
        // Runway paint sits 4-8 cm above flat ground; a small depth bias stops it shimmering far away.
        d.mesh(&self.runway, Mat4::IDENTITY, white, [0.0, 0.0, 0.0, 6e-6]);
        d.mesh(&self.markings, Mat4::IDENTITY, white, [0.0, 0.0, 0.0, 1.4e-5]);
        d.mesh(&self.trees, Mat4::IDENTITY, white, none);
        d.mesh(&self.buildings, Mat4::IDENTITY, white, none);
        d.mesh(&self.clouds, Mat4::IDENTITY, white, [0.0, 0.0, 0.72, 0.0]);

        for r in &scene.rings {
            let m = Mat4::from_scale_rotation_translation(Vec3::splat(r.radius), Quat::from_rotation_arc(Vec3::Z, r.normal), r.pos);
            d.mesh(&self.ring, m, r.color, [0.0, 0.45, 0.0, 0.0]);
        }
        for (p, white_light) in &scene.papi {
            let scale = ((*p - view.eye).length() / 300.0).max(1.0);
            let m = Mat4::from_scale_rotation_translation(vec3(2.4, 1.0, 2.4) * scale, Quat::IDENTITY, *p);
            let c = if *white_light { [1.0, 1.0, 0.95, 1.0] } else { [1.0, 0.12, 0.08, 1.0] };
            d.mesh(&self.cube, m, c, [0.0, 1.0, 0.0, 0.0]);
        }

        // aircraft
        let ac = scene.ac;
        let parts = self.plane_parts(ac, scene.show_fuselage);
        for (mesh, m) in &parts {
            d.mesh(mesh, *m, white, none);
        }

        // shadow, projected onto the ground plane under the aircraft along the sun direction
        if scene.shadow_alpha > 0.01 {
            let l = sun;
            let (kx, kz, g) = (l.x / l.y, l.z / l.y, scene.shadow_ground + 0.1);
            let proj = Mat4::from_cols(
                Vec4::new(1.0, 0.0, 0.0, 0.0),
                Vec4::new(-kx, 0.0, -kz, 0.0),
                Vec4::new(0.0, 0.0, 1.0, 0.0),
                Vec4::new(kx * g, g, kz * g, 1.0),
            );
            d.ctx.apply_pipeline(&self.shadow);
            let all = self.plane_parts(ac, true);
            for (mesh, m) in all.iter().filter(|(mesh, _)| !std::ptr::eq(*mesh, &self.plane.prop)) {
                d.mesh(mesh, proj * *m, [0.0, 0.0, 0.0, scene.shadow_alpha], [0.0, 1.0, 0.0, 2e-5]);
            }
        }

        // spinning prop disc
        d.ctx.apply_pipeline(&self.blend);
        let disc_m = Mat4::from_rotation_translation(ac.rot, ac.pos) * Mat4::from_translation(vec3(0.0, -0.05, -2.8));
        d.mesh(&self.disc, disc_m, [0.85, 0.85, 0.85, if scene.show_fuselage { 0.10 + 0.12 * ac.ctl.throttle } else { 0.04 }], [0.0, 0.4, 0.0, 0.0]);

        d.ctx.end_render_pass();
    }

    fn plane_parts(&self, ac: &Aircraft, fuselage: bool) -> Vec<(&GpuMesh, Mat4)> {
        let base = Mat4::from_rotation_translation(ac.rot, ac.pos);
        let hinge = |pivot: Vec3, axis: Vec3, angle: f32| {
            base * Mat4::from_translation(pivot) * Mat4::from_axis_angle(axis, angle) * Mat4::from_translation(-pivot)
        };
        let flap = ac.flap_deg.to_radians();
        let fp = mesh::FLAP_PIVOT;
        let ap = mesh::AIL_PIVOT;
        let p = &self.plane;
        let mut v = vec![
            (&p.cowl, base),
            (&p.wings, base),
            (&p.flap_l, hinge(vec3(-fp.x, fp.y, fp.z), Vec3::X, flap)),
            (&p.flap_r, hinge(fp, Vec3::X, flap)),
            (&p.ail_l, hinge(vec3(-ap.x, ap.y, ap.z), Vec3::X, ac.ail)),
            (&p.ail_r, hinge(ap, Vec3::X, -ac.ail)),
        ];
        if fuselage {
            v.push((&p.prop, hinge(mesh::PROP_PIVOT, Vec3::Z, ac.prop_angle)));
            v.push((&p.fuselage, base));
            v.push((&p.elevator, hinge(mesh::ELEV_PIVOT, Vec3::X, -ac.elev)));
            v.push((&p.rudder, hinge(mesh::RUD_PIVOT, Vec3::Y, ac.rud)));
        }
        v
    }
}

struct Draw<'a> {
    ctx: &'a mut dyn RenderingBackend,
    view: &'a View,
    time: f32,
}

impl Draw<'_> {
    fn mesh(&mut self, m: &GpuMesh, model: Mat4, tint: [f32; 4], params: [f32; 4]) {
        let sun = SUN.normalize();
        self.ctx.apply_bindings(&m.bind);
        self.ctx.apply_uniforms(UniformsSource::table(&WorldUniforms {
            mvp: (self.view.view_proj * model).to_cols_array(),
            model: model.to_cols_array(),
            cam: [self.view.eye.x, self.view.eye.y, self.view.eye.z, self.time],
            sun: [sun.x, sun.y, sun.z, 0.42],
            fog: [FOG[0], FOG[1], FOG[2], FOG_DENSITY],
            tint,
            params,
        }));
        self.ctx.draw(0, m.n, 1);
    }
}

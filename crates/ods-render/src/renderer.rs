//! The wgpu renderer: chunk meshes with a material palette and simple sun
//! lighting, plus a translucent overlay pass for fog of war and selection.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context, Result};
use bytemuck::{Pod, Zeroable};
use glam::{IVec3, Mat4};
use ods_voxel::MeshData;
use wgpu::util::DeviceExt;
use winit::window::Window;

use crate::{GpuMesh, Vertex, upload_mesh};

const VOXEL_SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, sun: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

// Entries 16+ are EMISSIVE: they ignore the sun and pulse on the clock.
var<private> PALETTE: array<vec3<f32>, 22> = array<vec3<f32>, 22>(
    vec3<f32>(1.0, 0.0, 1.0),    // 0: unused (empty)
    vec3<f32>(0.33, 0.34, 0.27), // 1: ground: dark field-green-grey
    vec3<f32>(0.40, 0.24, 0.18), // 2: chapel brick, soot-darkened
    vec3<f32>(0.27, 0.25, 0.22), // 3: rubble
    vec3<f32>(0.24, 0.40, 0.26), // 4: rift obelisk
    vec3<f32>(0.30, 0.19, 0.10), // 5: door timber
    vec3<f32>(0.50, 0.13, 0.10), // 6: fuel cask
    vec3<f32>(0.80, 0.48, 0.08), // 7: brimstone pool
    vec3<f32>(0.42, 0.13, 0.24), // 8: nest flesh
    vec3<f32>(0.09, 0.08, 0.12), // 9: obsidian
    vec3<f32>(0.62, 0.53, 0.33), // 10: desert sand, dusk-dulled
    vec3<f32>(0.72, 0.76, 0.82), // 11: snow and ice
    vec3<f32>(0.15, 0.28, 0.12), // 12: foliage, deep
    vec3<f32>(0.28, 0.19, 0.10), // 13: tree trunk
    vec3<f32>(0.34, 0.04, 0.04), // 14: spilled blood
    vec3<f32>(0.46, 0.08, 0.10), // 15: viscera
    vec3<f32>(0.95, 0.12, 0.10), // 16: sigil crimson (summoning circles, runes)
    vec3<f32>(0.15, 0.85, 0.75), // 17: witchfire teal (the Order's wards)
    vec3<f32>(0.70, 0.20, 0.85), // 18: corruption glow (the obelisk's veins)
    vec3<f32>(0.20, 0.36, 0.12), // 19: grass tuft
    vec3<f32>(0.58, 0.10, 0.08), // 20: wildflower red
    vec3<f32>(0.52, 0.58, 0.68)  // 21: frost glint / scree
);

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) material: u32,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) @interpolate(flat) material: u32,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.material = in.material;
    out.world = in.position;
    return out;
}

// Sprite-era shading: one brightness per face direction, no gradients.
fn face_shade(n: vec3<f32>) -> f32 {
    if n.z > 0.5 { return 1.0; }        // tops catch the light
    if n.z < -0.5 { return 0.30; }      // undersides are pits
    if abs(n.y) > 0.5 { return 0.68; }  // one flank in half-light
    return 0.52;                        // the other in shadow
}

// A cheap integer hash for per-voxel value jitter.
fn voxel_hash(cell: vec3<i32>) -> f32 {
    var h: u32 = u32(cell.x) * 374761393u + u32(cell.y) * 668265263u + u32(cell.z) * 2147483647u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    return f32((h >> 8u) & 255u) / 255.0;
}

// 4x4 Bayer matrix, normalized: ordered dithering between quantized bands.
fn bayer(frag: vec2<f32>) -> f32 {
    let x = i32(frag.x) & 3;
    let y = i32(frag.y) & 3;
    var m = array<i32, 16>(0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5);
    return f32(m[y * 4 + x]) / 16.0 - 0.5;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let base = PALETTE[min(in.material, 21u)];
    if in.material >= 16u && in.material <= 18u {
        // Occult light: full-bright, breathing on the clock. Unlit by sun,
        // so sigils burn brightest exactly where the night is darkest.
        let pulse = 0.75 + 0.35 * sin(camera.sun.w * 3.2 + f32(in.material) * 1.9);
        return vec4<f32>(base * pulse, 1.0);
    }
    let n = normalize(in.normal);
    // Greedy meshing merges faces: recover WHICH voxel this fragment sits
    // on by stepping half a voxel against the face normal, then jitter its
    // value so big surfaces read as tiled texture, not slab.
    let cell = vec3<i32>(floor(in.world - n * 0.5));
    let jitter = 0.86 + 0.22 * voxel_hash(cell);
    // Face-quantized light, with the sun deciding only how hard the
    // contrast bites (flat per face — no smooth gradients anywhere).
    let sun_bite = 0.7 + 0.3 * max(dot(n, camera.sun.xyz), 0.0);
    var color = base * face_shade(n) * sun_bite * jitter;
    // Crush to banded levels with ordered dithering: the 1994 finish.
    let levels = 6.0;
    let d = bayer(in.clip.xy) / levels;
    color = floor((color + d) * levels + 0.5) / levels;
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
"#;

const OVERLAY_SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, sun: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) color: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.color = in.color;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    return in.color;
}
"#;

const LIT_SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, sun: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.color = in.color;
    return out;
}

fn lit_face_shade(n: vec3<f32>) -> f32 {
    if n.z > 0.5 { return 1.0; }
    if n.z < -0.5 { return 0.32; }
    if abs(n.y) > 0.5 { return 0.70; }
    return 0.54;
}

fn lit_bayer(frag: vec2<f32>) -> f32 {
    let x = i32(frag.x) & 3;
    let y = i32(frag.y) & 3;
    var m = array<i32, 16>(0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5);
    return f32(m[y * 4 + x]) / 16.0 - 0.5;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    let sun_bite = 0.7 + 0.3 * max(dot(n, camera.sun.xyz), 0.0);
    var color = in.color.rgb * lit_face_shade(n) * sun_bite;
    let levels = 6.0;
    let d = lit_bayer(in.clip.xy) / levels;
    color = floor((color + d) * levels + 0.5) / levels;
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), in.color.a);
}
"#;

const GLOBE_SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, sun: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsIn {
    @location(0) position: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};
struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.color = in.color;
    return out;
}

fn globe_bayer(frag: vec2<f32>) -> f32 {
    let x = i32(frag.x) & 3;
    let y = i32(frag.y) & 3;
    var m = array<i32, 16>(0, 8, 2, 10, 12, 4, 14, 6, 3, 11, 1, 9, 15, 7, 13, 5);
    return f32(m[y * 4 + x]) / 16.0 - 0.5;
}

// The 1994 planet: flat saturated color, a mapmaker's graticule over
// everything, and a terminator that falls like a knife — day on one side,
// night on the other, dithered only along the blade itself.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let n = normalize(in.normal);
    var color = in.color.rgb;

    // Hairline meridians and parallels every 30 degrees.
    let lat = degrees(asin(clamp(n.z, -1.0, 1.0)));
    let lon = degrees(atan2(n.y, n.x));
    let dlat = abs(lat - round(lat / 30.0) * 30.0);
    let dlon = abs(lon - round(lon / 30.0) * 30.0);
    let lon_tol = 0.30 / max(cos(radians(lat)), 0.05);
    if dlat < 0.30 || dlon < lon_tol {
        color = mix(color, vec3<f32>(0.30, 0.44, 0.58), 0.40);
    }

    let l = dot(n, camera.sun.xyz);
    let day = step(0.0, l + globe_bayer(in.clip.xy) * 0.08);
    color = color * mix(0.28, 1.0, day);

    let levels = 7.0;
    let d = globe_bayer(in.clip.xy) / levels;
    color = floor((color + d) * levels + 0.5) / levels;
    return vec4<f32>(clamp(color, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
"#;

const STARFIELD_SHADER: &str = r#"
struct Camera { view_proj: mat4x4<f32>, sun: vec4<f32> };
@group(0) @binding(0) var<uniform> camera: Camera;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    var out: VsOut;
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    out.clip = vec4<f32>(x, y, 1.0, 1.0);
    return out;
}

fn star_hash(p: vec2<i32>) -> f32 {
    var h: u32 = u32(p.x) * 374761393u + u32(p.y) * 668265263u;
    h = (h ^ (h >> 13u)) * 1274126177u;
    return f32((h >> 8u) & 65535u) / 65535.0;
}

fn noise2(p: vec2<f32>) -> f32 {
    let i = vec2<i32>(floor(p));
    let f = fract(p);
    let a = star_hash(i);
    let b = star_hash(i + vec2<i32>(1, 0));
    let c = star_hash(i + vec2<i32>(0, 1));
    let d = star_hash(i + vec2<i32>(1, 1));
    let u = f * f * (3.0 - 2.0 * f);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// The void behind the world is not black: a violet nebula breathes there,
// and the stars mind their own business.
@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let px = in.clip.xy;
    let p = px * 0.006;
    var neb = noise2(p) * 0.55;
    neb += noise2(p * 2.3 + vec2<f32>(19.7, 7.3)) * 0.30;
    neb += noise2(p * 5.1 + vec2<f32>(3.1, 41.9)) * 0.15;
    neb = pow(max(neb - 0.35, 0.0) * 1.6, 1.6);
    var color = vec3<f32>(0.015, 0.010, 0.030);
    color += vec3<f32>(0.16, 0.05, 0.24) * neb;

    let cell = vec2<i32>(floor(px / 3.0));
    let h = star_hash(cell);
    if h > 0.982 {
        let tw = 0.7 + 0.3 * sin(camera.sun.w * (1.0 + fract(h * 57.0) * 3.0) + h * 40.0);
        let bright = (h - 0.982) / 0.018;
        color += vec3<f32>(0.8, 0.85, 1.0) * bright * tw;
    }
    return vec4<f32>(color, 1.0);
}
"#;

/// Opaque lit vertex with a free RGBA color — the Geoscape globe and its
/// markers use this (the voxel palette is too small for a planet).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct LitVertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub color: [f32; 4],
}

impl LitVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];

    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<LitVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &Self::ATTRIBUTES,
    };
}

/// Vertex for the translucent overlay pass (fog, selection, markers).
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct OverlayVertex {
    pub position: [f32; 3],
    pub color: [f32; 4],
}

impl OverlayVertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 2] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];

    const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<OverlayVertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &Self::ATTRIBUTES,
    };
}

const BLIT_SHADER: &str = r#"
@group(0) @binding(0) var scene_tex: texture_2d<f32>;
@group(0) @binding(1) var scene_smp: sampler;
// x: pixel scale, y: CRT flag, z/w: screen size.
@group(0) @binding(2) var<uniform> params: vec4<f32>;

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_main(@builtin(vertex_index) i: u32) -> VsOut {
    // One triangle over the whole screen.
    var out: VsOut;
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    out.clip = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, 1.0 - (y + 1.0) * 0.5);
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    var color = textureSample(scene_tex, scene_smp, in.uv).rgb;
    if params.y > 0.5 {
        // The tube: a dark scanline per virtual pixel row, a whisper of
        // phosphor mask, and corners that fall away.
        let scale = max(i32(params.x), 1);
        if i32(in.clip.y) % scale == 0 {
            color *= 0.80;
        }
        if i32(in.clip.x) % 3 == 0 {
            color *= 0.94;
        }
        let centered = in.uv - vec2<f32>(0.5, 0.5);
        color *= 1.0 - dot(centered, centered) * 0.45;
    }
    return vec4<f32>(color, 1.0);
}
"#;

/// Default virtual-pixel size: the world renders at 1/scale resolution and
/// upscales with hard nearest-neighbor pixels — 1994 in the cheapest honest
/// way. The UI paints at full resolution on top.
const PIXEL_SCALE: u32 = 3;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct CameraUniform {
    view_proj: [[f32; 4]; 4],
    sun: [f32; 4],
}

/// One frame's worth of egui output, ready to paint over the 3D scene.
pub struct UiFrame {
    pub textures_delta: egui::TexturesDelta,
    pub primitives: Vec<egui::ClippedPrimitive>,
    pub pixels_per_point: f32,
}

pub struct Renderer {
    surface: wgpu::Surface<'static>,
    device: wgpu::Device,
    queue: wgpu::Queue,
    config: wgpu::SurfaceConfiguration,
    depth_view: wgpu::TextureView,
    /// The low-resolution canvas the world is painted on.
    scene_view: wgpu::TextureView,
    blit_pipeline: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    blit_bind_group: wgpu::BindGroup,
    blit_sampler: wgpu::Sampler,
    blit_params: wgpu::Buffer,
    /// Virtual pixel size (1..=4) and the CRT dressing toggle.
    pixel_scale: u32,
    crt: bool,
    voxel_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    lit_pipeline: wgpu::RenderPipeline,
    globe_pipeline: wgpu::RenderPipeline,
    starfield_pipeline: wgpu::RenderPipeline,
    chunk_meshes: HashMap<IVec3, GpuMesh>,
    unit_mesh: Option<GpuMesh>,
    overlay_mesh: Option<GpuMesh>,
    globe_mesh: Option<GpuMesh>,
    marker_mesh: Option<GpuMesh>,
    figure_mesh: Option<GpuMesh>,
    fx_mesh: Option<GpuMesh>,
    ui_renderer: egui_wgpu::Renderer,
}

impl Renderer {
    pub fn new(window: Arc<Window>) -> Result<Self> {
        let size = window.inner_size();
        let instance = wgpu::Instance::default();
        let surface = instance.create_surface(window)?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("ods-device"),
            ..Default::default()
        }))?;

        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .context("surface is not supported by the adapter")?;
        surface.configure(&device, &config);
        let (scene_view, depth_view) = create_scene_targets(&device, &config, PIXEL_SCALE);

        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("blit-sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let blit_params = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blit-params"),
            contents: bytemuck::bytes_of(&[PIXEL_SCALE as f32, 0.0f32, 0.0, 0.0]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("blit-layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 2,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });
        let blit_bind_group =
            create_blit_bind(&device, &blit_layout, &scene_view, &blit_sampler, &blit_params);
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blit-shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_SHADER.into()),
        });
        let blit_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("blit-pipeline-layout"),
                bind_group_layouts: &[&blit_layout],
                push_constant_ranges: &[],
            });
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blit-pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            multiview: None,
            cache: None,
        });

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("camera"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_proj: Mat4::IDENTITY.to_cols_array_2d(),
                sun: [0.35, 0.5, 0.8, 0.0],
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let camera_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("camera-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("camera-bind"),
            layout: &camera_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ods-pipeline-layout"),
            bind_group_layouts: &[&camera_layout],
            push_constant_ranges: &[],
        });

        let voxel_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("voxel-shader"),
            source: wgpu::ShaderSource::Wgsl(VOXEL_SHADER.into()),
        });
        let overlay_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("overlay-shader"),
            source: wgpu::ShaderSource::Wgsl(OVERLAY_SHADER.into()),
        });

        let voxel_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("voxel-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &voxel_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[Vertex::LAYOUT],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &voxel_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            multiview: None,
            cache: None,
        });

        let overlay_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("overlay-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &overlay_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[OverlayVertex::LAYOUT],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::LessEqual,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &overlay_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: config.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        let lit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("lit-shader"),
            source: wgpu::ShaderSource::Wgsl(LIT_SHADER.into()),
        });
        let lit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("lit-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &lit_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[LitVertex::LAYOUT],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &lit_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            multiview: None,
            cache: None,
        });

        let globe_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("globe-shader"),
            source: wgpu::ShaderSource::Wgsl(GLOBE_SHADER.into()),
        });
        let globe_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("globe-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &globe_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[LitVertex::LAYOUT],
            },
            primitive: wgpu::PrimitiveState {
                cull_mode: Some(wgpu::Face::Back),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: true,
                depth_compare: wgpu::CompareFunction::Less,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &globe_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            multiview: None,
            cache: None,
        });

        let starfield_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("starfield-shader"),
            source: wgpu::ShaderSource::Wgsl(STARFIELD_SHADER.into()),
        });
        let starfield_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("starfield-pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &starfield_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: Default::default(),
            depth_stencil: Some(wgpu::DepthStencilState {
                format: wgpu::TextureFormat::Depth32Float,
                depth_write_enabled: false,
                depth_compare: wgpu::CompareFunction::Always,
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &starfield_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(config.format.into())],
            }),
            multiview: None,
            cache: None,
        });

        let ui_renderer = egui_wgpu::Renderer::new(
            &device,
            config.format,
            egui_wgpu::RendererOptions::default(),
        );

        Ok(Self {
            surface,
            device,
            queue,
            config,
            depth_view,
            scene_view,
            blit_pipeline,
            blit_layout,
            blit_bind_group,
            blit_sampler,
            blit_params,
            pixel_scale: PIXEL_SCALE,
            crt: false,
            voxel_pipeline,
            overlay_pipeline,
            camera_buffer,
            camera_bind_group,
            lit_pipeline,
            globe_pipeline,
            starfield_pipeline,
            chunk_meshes: HashMap::new(),
            unit_mesh: None,
            overlay_mesh: None,
            globe_mesh: None,
            marker_mesh: None,
            figure_mesh: None,
            fx_mesh: None,
            ui_renderer,
        })
    }

    /// Drop all scene geometry (between battles / on returning to menus).
    pub fn clear_scene(&mut self) {
        self.chunk_meshes.clear();
        self.unit_mesh = None;
        self.overlay_mesh = None;
        self.globe_mesh = None;
        self.marker_mesh = None;
        self.figure_mesh = None;
        self.fx_mesh = None;
    }

    fn upload_lit(&self, vertices: &[LitVertex], indices: &[u32]) -> Option<GpuMesh> {
        if indices.is_empty() {
            return None;
        }
        let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lit-vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("lit-indices"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        Some(GpuMesh { vertices: vb, indices: ib, index_count: indices.len() as u32 })
    }

    /// Install the Geoscape globe (rebuilt only when its look changes).
    pub fn set_globe(&mut self, vertices: &[LitVertex], indices: &[u32]) {
        self.globe_mesh = self.upload_lit(vertices, indices);
    }

    /// Install the globe's surface markers (rifts, nests, the chapterhouse).
    pub fn set_markers(&mut self, vertices: &[LitVertex], indices: &[u32]) {
        self.marker_mesh = self.upload_lit(vertices, indices);
    }

    /// Install the battle's unit figures (body-part voxel models).
    pub fn set_figures(&mut self, vertices: &[LitVertex], indices: &[u32]) {
        self.figure_mesh = self.upload_lit(vertices, indices);
    }

    /// Install transient battle effects (tracers, blasts) — translucent.
    pub fn set_fx(&mut self, vertices: &[OverlayVertex], indices: &[u32]) {
        self.fx_mesh = if indices.is_empty() {
            None
        } else {
            let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fx-vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("fx-indices"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            Some(GpuMesh { vertices: vb, indices: ib, index_count: indices.len() as u32 })
        };
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.rebuild_scene_targets();
    }

    fn rebuild_scene_targets(&mut self) {
        let (scene_view, depth_view) =
            create_scene_targets(&self.device, &self.config, self.pixel_scale);
        self.scene_view = scene_view;
        self.depth_view = depth_view;
        self.blit_bind_group = create_blit_bind(
            &self.device,
            &self.blit_layout,
            &self.scene_view,
            &self.blit_sampler,
            &self.blit_params,
        );
        self.write_blit_params();
    }

    fn write_blit_params(&self) {
        self.queue.write_buffer(
            &self.blit_params,
            0,
            bytemuck::bytes_of(&[
                self.pixel_scale as f32,
                if self.crt { 1.0f32 } else { 0.0 },
                self.config.width as f32,
                self.config.height as f32,
            ]),
        );
    }

    pub fn pixel_scale(&self) -> u32 {
        self.pixel_scale
    }

    pub fn crt(&self) -> bool {
        self.crt
    }

    /// Change the virtual pixel size (1 = sharp, 4 = chunky).
    pub fn set_pixel_scale(&mut self, scale: u32) {
        let scale = scale.clamp(1, 4);
        if scale != self.pixel_scale {
            self.pixel_scale = scale;
            self.rebuild_scene_targets();
        }
    }

    /// Dress the upscale as a tube: scanlines, mask, corner falloff.
    pub fn set_crt(&mut self, on: bool) {
        if on != self.crt {
            self.crt = on;
            self.write_blit_params();
        }
    }

    pub fn aspect(&self) -> f32 {
        self.config.width as f32 / self.config.height.max(1) as f32
    }

    pub fn size(&self) -> (f32, f32) {
        (self.config.width as f32, self.config.height as f32)
    }

    /// Upload the camera. `clock` (seconds, wrapping is fine) drives the
    /// pulse of emissive materials and rides in the sun vector's w lane.
    pub fn set_camera(&mut self, view_proj: Mat4, sun: glam::Vec3, clock: f32) {
        let sun = sun.normalize_or(glam::Vec3::Z);
        self.queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_proj: view_proj.to_cols_array_2d(),
                sun: [sun.x, sun.y, sun.z, clock],
            }),
        );
    }

    /// Install or replace the mesh for a chunk; empty meshes remove it.
    pub fn upsert_chunk(&mut self, coord: IVec3, mesh: &MeshData) {
        if mesh.is_empty() {
            self.chunk_meshes.remove(&coord);
        } else {
            self.chunk_meshes.insert(coord, upload_mesh(&self.device, mesh));
        }
    }

    /// Replace the dynamic mesh that draws units (built per state change).
    pub fn set_units(&mut self, mesh: &MeshData) {
        self.unit_mesh = if mesh.is_empty() {
            None
        } else {
            Some(upload_mesh(&self.device, mesh))
        };
    }

    /// Replace the translucent overlay geometry (fog, selection).
    pub fn set_overlay(&mut self, vertices: &[OverlayVertex], indices: &[u32]) {
        self.overlay_mesh = if indices.is_empty() {
            None
        } else {
            let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("overlay-vertices"),
                contents: bytemuck::cast_slice(vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("overlay-indices"),
                contents: bytemuck::cast_slice(indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            Some(GpuMesh {
                vertices: vb,
                indices: ib,
                index_count: indices.len() as u32,
            })
        };
    }

    pub fn render(&mut self, ui: Option<UiFrame>) -> Result<()> {
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&self.device, &self.config);
                self.surface.get_current_texture()?
            }
            Err(e) => return Err(e.into()),
        };
        let view = frame.texture.create_view(&Default::default());
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });

        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: ui.as_ref().map_or(1.0, |u| u.pixels_per_point),
        };
        if let Some(ui) = &ui {
            for (id, delta) in &ui.textures_delta.set {
                self.ui_renderer
                    .update_texture(&self.device, &self.queue, *id, delta);
            }
            self.ui_renderer.update_buffers(
                &self.device,
                &self.queue,
                &mut encoder,
                &ui.primitives,
                &screen,
            );
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.scene_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.03,
                            g: 0.03,
                            b: 0.05,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Space first: the nebula backdrop paints behind the globe.
            if self.globe_mesh.is_some() {
                pass.set_pipeline(&self.starfield_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.draw(0..3, 0..1);
            }

            pass.set_pipeline(&self.voxel_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            for mesh in self.chunk_meshes.values().chain(self.unit_mesh.iter()) {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }

            // The planet gets its own 1994 treatment: flat color, graticule,
            // and the knife-edge terminator.
            if let Some(mesh) = &self.globe_mesh {
                pass.set_pipeline(&self.globe_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }

            let lit_meshes: Vec<&GpuMesh> = self
                .marker_mesh
                .iter()
                .chain(self.figure_mesh.iter())
                .collect();
            if !lit_meshes.is_empty() {
                pass.set_pipeline(&self.lit_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                for mesh in lit_meshes {
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
            }

            let overlays: Vec<&GpuMesh> =
                self.overlay_mesh.iter().chain(self.fx_mesh.iter()).collect();
            if !overlays.is_empty() {
                pass.set_pipeline(&self.overlay_pipeline);
                pass.set_bind_group(0, &self.camera_bind_group, &[]);
                for mesh in overlays {
                    pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                    pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                    pass.draw_indexed(0..mesh.index_count, 0, 0..1);
                }
            }
        }

        // The low-res world lands on the swapchain as hard, honest pixels.
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("blit-pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(&self.blit_pipeline);
            pass.set_bind_group(0, &self.blit_bind_group, &[]);
            pass.draw(0..3, 0..1);
        }

        // UI paints in its own pass (no depth buffer), over the scene.
        if let Some(ui) = &ui {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("ui-pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Load,
                            store: wgpu::StoreOp::Store,
                        },
                        depth_slice: None,
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                })
                .forget_lifetime();
            self.ui_renderer.render(&mut pass, &ui.primitives, &screen);
        }

        self.queue.submit([encoder.finish()]);
        frame.present();

        if let Some(ui) = ui {
            for id in &ui.textures_delta.free {
                self.ui_renderer.free_texture(id);
            }
        }
        Ok(())
    }
}

/// The pixel canvas: a color target at 1/PIXEL_SCALE resolution and its
/// matching depth buffer.
fn create_scene_targets(
    device: &wgpu::Device,
    config: &wgpu::SurfaceConfiguration,
    pixel_scale: u32,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let size = wgpu::Extent3d {
        width: (config.width / pixel_scale).max(1),
        height: (config.height / pixel_scale).max(1),
        depth_or_array_layers: 1,
    };
    let color = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("scene-lowres"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: config.format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        })
        .create_view(&Default::default());
    let depth = device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("scene-depth"),
            size,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default());
    (color, depth)
}

fn create_blit_bind(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    scene: &wgpu::TextureView,
    sampler: &wgpu::Sampler,
    params: &wgpu::Buffer,
) -> wgpu::BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("blit-bind"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::TextureView(scene),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: params.as_entire_binding(),
            },
        ],
    })
}


#[cfg(test)]
mod shader_tests {
    /// WGSL only fails at runtime on the player's machine — so parse and
    /// validate every shader here, where CI can catch it.
    #[test]
    fn all_shaders_parse_and_validate() {
        for (name, src) in [
            ("voxel", super::VOXEL_SHADER),
            ("overlay", super::OVERLAY_SHADER),
            ("lit", super::LIT_SHADER),
            ("blit", super::BLIT_SHADER),
            ("globe", super::GLOBE_SHADER),
            ("starfield", super::STARFIELD_SHADER),
        ] {
            let module = naga::front::wgsl::parse_str(src)
                .unwrap_or_else(|e| panic!("{name} shader fails to parse: {e}"));
            naga::valid::Validator::new(
                naga::valid::ValidationFlags::all(),
                naga::valid::Capabilities::all(),
            )
            .validate(&module)
            .unwrap_or_else(|e| panic!("{name} shader fails to validate: {e:?}"));
        }
    }
}

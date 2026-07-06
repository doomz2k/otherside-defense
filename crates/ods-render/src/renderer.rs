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
var<private> PALETTE: array<vec3<f32>, 19> = array<vec3<f32>, 19>(
    vec3<f32>(1.0, 0.0, 1.0),    // 0: unused (empty)
    vec3<f32>(0.42, 0.41, 0.38), // 1: ground stone
    vec3<f32>(0.45, 0.27, 0.22), // 2: chapel brick
    vec3<f32>(0.33, 0.31, 0.27), // 3: rubble
    vec3<f32>(0.30, 0.45, 0.30), // 4: rift obelisk
    vec3<f32>(0.35, 0.22, 0.12), // 5: door timber
    vec3<f32>(0.55, 0.15, 0.12), // 6: fuel cask
    vec3<f32>(0.85, 0.55, 0.10), // 7: brimstone pool
    vec3<f32>(0.48, 0.16, 0.28), // 8: nest flesh
    vec3<f32>(0.10, 0.09, 0.13), // 9: obsidian
    vec3<f32>(0.78, 0.68, 0.45), // 10: desert sand
    vec3<f32>(0.85, 0.88, 0.92), // 11: snow and ice
    vec3<f32>(0.20, 0.36, 0.16), // 12: foliage
    vec3<f32>(0.34, 0.23, 0.13), // 13: tree trunk
    vec3<f32>(0.38, 0.05, 0.05), // 14: spilled blood
    vec3<f32>(0.52, 0.10, 0.12), // 15: viscera
    vec3<f32>(0.95, 0.12, 0.10), // 16: sigil crimson (summoning circles, runes)
    vec3<f32>(0.15, 0.85, 0.75), // 17: witchfire teal (the Order's wards)
    vec3<f32>(0.70, 0.20, 0.85)  // 18: corruption glow (the obelisk's veins)
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
};

@vertex
fn vs_main(in: VsIn) -> VsOut {
    var out: VsOut;
    out.clip = camera.view_proj * vec4<f32>(in.position, 1.0);
    out.normal = in.normal;
    out.material = in.material;
    return out;
}

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let base = PALETTE[min(in.material, 18u)];
    if in.material >= 16u {
        // Occult light: full-bright, breathing on the clock. Unlit by sun,
        // so sigils burn brightest exactly where the night is darkest.
        let pulse = 0.75 + 0.35 * sin(camera.sun.w * 3.2 + f32(in.material) * 1.9);
        return vec4<f32>(base * pulse, 1.0);
    }
    let ndl = max(dot(normalize(in.normal), camera.sun.xyz), 0.0);
    let lit = base * (0.35 + 0.65 * ndl);
    return vec4<f32>(lit, 1.0);
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

@fragment
fn fs_main(in: VsOut) -> @location(0) vec4<f32> {
    let ndl = max(dot(normalize(in.normal), camera.sun.xyz), 0.0);
    let lit = in.color.rgb * (0.22 + 0.78 * ndl);
    return vec4<f32>(lit, in.color.a);
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
    voxel_pipeline: wgpu::RenderPipeline,
    overlay_pipeline: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    lit_pipeline: wgpu::RenderPipeline,
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
        let depth_view = create_depth(&device, &config);

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
            voxel_pipeline,
            overlay_pipeline,
            camera_buffer,
            camera_bind_group,
            lit_pipeline,
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
        self.depth_view = create_depth(&self.device, &self.config);
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
                    view: &view,
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

            pass.set_pipeline(&self.voxel_pipeline);
            pass.set_bind_group(0, &self.camera_bind_group, &[]);
            for mesh in self.chunk_meshes.values().chain(self.unit_mesh.iter()) {
                pass.set_vertex_buffer(0, mesh.vertices.slice(..));
                pass.set_index_buffer(mesh.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..mesh.index_count, 0, 0..1);
            }

            let lit_meshes: Vec<&GpuMesh> = self
                .globe_mesh
                .iter()
                .chain(self.marker_mesh.iter())
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

fn create_depth(device: &wgpu::Device, config: &wgpu::SurfaceConfiguration) -> wgpu::TextureView {
    device
        .create_texture(&wgpu::TextureDescriptor {
            label: Some("depth"),
            size: wgpu::Extent3d {
                width: config.width,
                height: config.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Depth32Float,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .create_view(&Default::default())
}

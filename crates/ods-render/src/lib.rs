//! wgpu presentation layer.
//!
//! Consumes sim state and mesh data from the other crates; must never feed
//! anything back into the rules.

mod camera;
mod renderer;

pub use camera::OrbitCamera;
pub use renderer::{OverlayVertex, Renderer};

use bytemuck::{Pod, Zeroable};
use ods_voxel::MeshData;
use wgpu::util::DeviceExt;

/// GPU vertex for chunk meshes.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub material: u32,
}

impl Vertex {
    const ATTRIBUTES: [wgpu::VertexAttribute; 3] =
        wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Uint32];

    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as wgpu::BufferAddress,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &Self::ATTRIBUTES,
    };
}

/// Interleave `MeshData`'s parallel arrays into GPU vertices.
pub fn interleave(mesh: &MeshData) -> Vec<Vertex> {
    (0..mesh.positions.len())
        .map(|i| Vertex {
            position: mesh.positions[i],
            normal: mesh.normals[i],
            material: mesh.materials[i],
        })
        .collect()
}

/// A chunk mesh resident on the GPU.
pub struct GpuMesh {
    pub vertices: wgpu::Buffer,
    pub indices: wgpu::Buffer,
    pub index_count: u32,
}

pub fn upload_mesh(device: &wgpu::Device, mesh: &MeshData) -> GpuMesh {
    let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("chunk-vertices"),
        contents: bytemuck::cast_slice(&interleave(mesh)),
        usage: wgpu::BufferUsages::VERTEX,
    });
    let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("chunk-indices"),
        contents: bytemuck::cast_slice(&mesh.indices),
        usage: wgpu::BufferUsages::INDEX,
    });
    GpuMesh {
        vertices,
        indices,
        index_count: mesh.indices.len() as u32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interleave_preserves_order_and_length() {
        let mesh = MeshData {
            positions: vec![[0.0, 1.0, 2.0], [3.0, 4.0, 5.0]],
            normals: vec![[0.0, 1.0, 0.0], [0.0, -1.0, 0.0]],
            materials: vec![7, 9],
            indices: vec![0, 1, 0],
        };
        let verts = interleave(&mesh);
        assert_eq!(verts.len(), 2);
        assert_eq!(verts[1].position, [3.0, 4.0, 5.0]);
        assert_eq!(verts[1].material, 9);
    }
}

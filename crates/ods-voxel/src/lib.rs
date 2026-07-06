//! Voxel storage, meshing, and spatial queries for the Battlescape.
//!
//! This crate owns the *fine* representation of the battlefield: raw material
//! occupancy in chunks, destruction, and ray queries. The coarse gameplay tile
//! grid (`ods-sim`) is derived from this data and never the other way around.
//! See `docs/design/tech-stack.md`.

mod chunk;
mod mesh;
mod world;

pub use chunk::{CHUNK_SIZE, Chunk, Voxel};
pub use mesh::{MeshData, mesh_chunk, mesh_chunk_capped};
pub use world::{RayHit, VoxelWorld};

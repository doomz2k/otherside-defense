//! Otherside Defense — application shell.
//!
//! The winit window and wgpu renderer arrive with milestone M0. Until then
//! this binary is a headless smoke test of the voxel core: build a wall,
//! check line of fire, breach it, check again. Runs anywhere, including CI.

use glam::{IVec3, Vec3};
use ods_voxel::{VoxelWorld, Voxel, mesh_chunk};

const STONE: Voxel = Voxel(1);

fn main() {
    let mut world = VoxelWorld::new();

    // A courtyard wall: 48 x 24 voxels, 3 thick (3m x 1.5m at 16 voxels/m).
    world.fill_box(IVec3::new(0, 0, 0), IVec3::new(48, 24, 3), STONE);
    report(&world, "wall built");

    let muzzle = Vec3::new(24.0, 8.0, -10.0);
    let hit = world.raycast(muzzle, Vec3::Z, 100.0);
    println!(
        "shot at the wall: {}",
        match hit {
            Some(h) => format!("blocked by voxel {} at {:.1}m", h.voxel, h.distance / 16.0),
            None => "clean through?!".to_string(),
        }
    );

    let destroyed = world.carve_sphere(Vec3::new(24.0, 8.0, 1.5), 6.0);
    println!("breaching charge detonated: {destroyed} voxels destroyed");
    report(&world, "after breach");

    let hit = world.raycast(muzzle, Vec3::Z, 100.0);
    println!(
        "shot through the breach: {}",
        match hit {
            Some(h) => format!("still blocked by voxel {}", h.voxel),
            None => "clean through — line of fire is open".to_string(),
        }
    );
}

fn report(world: &VoxelWorld, label: &str) {
    let coords = world.chunk_coords();
    let (mut quads, mut verts) = (0usize, 0usize);
    for &c in &coords {
        let mesh = mesh_chunk(world, c);
        quads += mesh.quad_count();
        verts += mesh.positions.len();
    }
    println!(
        "{label}: {} chunks, {quads} quads, {verts} vertices",
        coords.len()
    );
}

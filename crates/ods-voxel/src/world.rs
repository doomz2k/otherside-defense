use std::collections::hash_map::Entry;
use std::collections::{HashMap, HashSet};

use glam::{IVec3, Vec3};

use crate::chunk::{CHUNK_SIZE, Chunk, Voxel};

/// Sparse voxel world: a map of chunk coordinates to dense chunks.
///
/// Mutations mark affected chunks dirty so the renderer knows what to re-mesh;
/// mutations on a chunk border also dirty the neighbor, because border faces
/// depend on the voxels on both sides.
#[derive(Default)]
pub struct VoxelWorld {
    chunks: HashMap<IVec3, Chunk>,
    dirty: HashSet<IVec3>,
}

/// Result of a [`VoxelWorld::raycast`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayHit {
    /// The solid voxel that was hit.
    pub voxel: IVec3,
    pub material: Voxel,
    /// Distance along the ray to the entry point.
    pub distance: f32,
    /// World-space entry point.
    pub position: Vec3,
    /// Face normal of the hit (zero if the ray started inside a solid voxel).
    pub normal: IVec3,
}

impl VoxelWorld {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn chunk_coord(p: IVec3) -> IVec3 {
        IVec3::new(
            p.x.div_euclid(CHUNK_SIZE),
            p.y.div_euclid(CHUNK_SIZE),
            p.z.div_euclid(CHUNK_SIZE),
        )
    }

    fn local_coord(p: IVec3) -> IVec3 {
        IVec3::new(
            p.x.rem_euclid(CHUNK_SIZE),
            p.y.rem_euclid(CHUNK_SIZE),
            p.z.rem_euclid(CHUNK_SIZE),
        )
    }

    /// Material at `p`; unallocated space reads as empty.
    pub fn voxel(&self, p: IVec3) -> Voxel {
        self.chunks
            .get(&Self::chunk_coord(p))
            .map_or(Voxel::EMPTY, |c| c.get(Self::local_coord(p)))
    }

    pub fn set_voxel(&mut self, p: IVec3, voxel: Voxel) {
        let cc = Self::chunk_coord(p);
        let chunk = match self.chunks.entry(cc) {
            Entry::Occupied(e) => e.into_mut(),
            Entry::Vacant(_) if !voxel.is_solid() => {
                return; // clearing air in unallocated space is a no-op
            }
            Entry::Vacant(e) => e.insert(Chunk::new()),
        };
        let local = Self::local_coord(p);
        chunk.set(local, voxel);
        self.mark_dirty_around(cc, local);
    }

    fn mark_dirty_around(&mut self, cc: IVec3, local: IVec3) {
        self.dirty.insert(cc);
        for axis in 0..3 {
            let mut neighbor = cc;
            if local[axis] == 0 {
                neighbor[axis] -= 1;
            } else if local[axis] == CHUNK_SIZE - 1 {
                neighbor[axis] += 1;
            } else {
                continue;
            }
            if self.chunks.contains_key(&neighbor) {
                self.dirty.insert(neighbor);
            }
        }
    }

    /// Fill the box `[min, max)` with `voxel`.
    pub fn fill_box(&mut self, min: IVec3, max: IVec3, voxel: Voxel) {
        for z in min.z..max.z {
            for y in min.y..max.y {
                for x in min.x..max.x {
                    self.set_voxel(IVec3::new(x, y, z), voxel);
                }
            }
        }
    }

    /// Destroy all solid voxels whose centers lie within `radius` of `center`.
    /// Returns the number of voxels destroyed.
    pub fn carve_sphere(&mut self, center: Vec3, radius: f32) -> usize {
        let min = (center - radius).floor().as_ivec3();
        let max = (center + radius).floor().as_ivec3();
        let r2 = radius * radius;
        let mut destroyed = 0;
        for z in min.z..=max.z {
            for y in min.y..=max.y {
                for x in min.x..=max.x {
                    let p = IVec3::new(x, y, z);
                    let voxel_center = p.as_vec3() + 0.5;
                    if (voxel_center - center).length_squared() <= r2 && self.voxel(p).is_solid() {
                        self.set_voxel(p, Voxel::EMPTY);
                        destroyed += 1;
                    }
                }
            }
        }
        destroyed
    }

    /// March a ray through the grid (Amanatides & Woo DDA) and return the
    /// first solid voxel within `max_dist`, if any.
    pub fn raycast(&self, origin: Vec3, dir: Vec3, max_dist: f32) -> Option<RayHit> {
        let dir = dir.normalize();
        let mut voxel = origin.floor().as_ivec3();

        let start = self.voxel(voxel);
        if start.is_solid() {
            return Some(RayHit {
                voxel,
                material: start,
                distance: 0.0,
                position: origin,
                normal: IVec3::ZERO,
            });
        }

        let step = IVec3::new(
            if dir.x > 0.0 { 1 } else { -1 },
            if dir.y > 0.0 { 1 } else { -1 },
            if dir.z > 0.0 { 1 } else { -1 },
        );
        let boundary_t = |o: f32, d: f32, v: i32, s: i32| -> f32 {
            if d == 0.0 {
                f32::INFINITY
            } else if s > 0 {
                ((v + 1) as f32 - o) / d
            } else {
                (v as f32 - o) / d
            }
        };
        let mut t_max = Vec3::new(
            boundary_t(origin.x, dir.x, voxel.x, step.x),
            boundary_t(origin.y, dir.y, voxel.y, step.y),
            boundary_t(origin.z, dir.z, voxel.z, step.z),
        );
        let t_delta = Vec3::new(1.0 / dir.x.abs(), 1.0 / dir.y.abs(), 1.0 / dir.z.abs());

        loop {
            let axis = if t_max.x <= t_max.y && t_max.x <= t_max.z {
                0
            } else if t_max.y <= t_max.z {
                1
            } else {
                2
            };
            let t = t_max[axis];
            if t > max_dist {
                return None;
            }
            voxel[axis] += step[axis];
            t_max[axis] += t_delta[axis];

            let material = self.voxel(voxel);
            if material.is_solid() {
                let mut normal = IVec3::ZERO;
                normal[axis] = -step[axis];
                return Some(RayHit {
                    voxel,
                    material,
                    distance: t,
                    position: origin + dir * t,
                    normal,
                });
            }
        }
    }

    /// Drain the set of chunks whose meshes are stale, in deterministic order.
    pub fn take_dirty_chunks(&mut self) -> Vec<IVec3> {
        let mut dirty: Vec<IVec3> = self.dirty.drain().collect();
        dirty.sort_unstable_by_key(|c| (c.x, c.y, c.z));
        dirty
    }

    /// All allocated chunk coordinates, in deterministic order.
    pub fn chunk_coords(&self) -> Vec<IVec3> {
        let mut coords: Vec<IVec3> = self.chunks.keys().copied().collect();
        coords.sort_unstable_by_key(|c| (c.x, c.y, c.z));
        coords
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STONE: Voxel = Voxel(1);

    #[test]
    fn voxel_roundtrip_across_chunks() {
        let mut world = VoxelWorld::new();
        let points = [
            IVec3::new(0, 0, 0),
            IVec3::new(31, 31, 31),
            IVec3::new(32, 0, 0),
            IVec3::new(-1, -5, 10),
            IVec3::new(-33, 100, -64),
        ];
        for p in points {
            world.set_voxel(p, STONE);
        }
        for p in points {
            assert_eq!(world.voxel(p), STONE, "at {p}");
        }
        assert_eq!(world.voxel(IVec3::new(1, 0, 0)), Voxel::EMPTY);
        assert_eq!(world.voxel(IVec3::new(500, 500, 500)), Voxel::EMPTY);
    }

    #[test]
    fn border_writes_dirty_existing_neighbors() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(31, 5, 5), STONE); // chunk (0,0,0)
        world.set_voxel(IVec3::new(32, 5, 5), STONE); // chunk (1,0,0)
        world.take_dirty_chunks();

        // Writing on the shared border must dirty both chunks.
        world.set_voxel(IVec3::new(32, 5, 5), Voxel::EMPTY);
        assert_eq!(
            world.take_dirty_chunks(),
            vec![IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]
        );
    }

    #[test]
    fn carve_sphere_destroys_and_reports() {
        let mut world = VoxelWorld::new();
        world.fill_box(IVec3::new(0, 0, 0), IVec3::new(10, 10, 10), STONE);
        world.take_dirty_chunks();

        let destroyed = world.carve_sphere(Vec3::new(5.0, 5.0, 5.0), 3.0);
        assert!(destroyed > 0);
        // The center voxel is gone, corners of the box are not.
        assert_eq!(world.voxel(IVec3::new(5, 5, 5)), Voxel::EMPTY);
        assert_eq!(world.voxel(IVec3::new(0, 0, 0)), STONE);
        // Carving twice destroys nothing new.
        assert_eq!(world.carve_sphere(Vec3::new(5.0, 5.0, 5.0), 3.0), 0);
    }

    #[test]
    fn raycast_hits_expected_face() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(5, 0, 0), STONE);

        let hit = world
            .raycast(Vec3::new(0.0, 0.5, 0.5), Vec3::X, 100.0)
            .expect("should hit");
        assert_eq!(hit.voxel, IVec3::new(5, 0, 0));
        assert_eq!(hit.normal, IVec3::new(-1, 0, 0));
        assert!((hit.distance - 5.0).abs() < 1e-4);

        // Away from the voxel: no hit. Toward it but too short: no hit.
        assert_eq!(world.raycast(Vec3::new(0.0, 0.5, 0.5), -Vec3::X, 100.0), None);
        assert_eq!(world.raycast(Vec3::new(0.0, 0.5, 0.5), Vec3::X, 4.0), None);
    }

    #[test]
    fn raycast_from_inside_solid() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(2, 2, 2), STONE);
        let hit = world
            .raycast(Vec3::new(2.5, 2.5, 2.5), Vec3::X, 10.0)
            .expect("should hit immediately");
        assert_eq!(hit.distance, 0.0);
        assert_eq!(hit.voxel, IVec3::new(2, 2, 2));
    }

    #[test]
    fn raycast_diagonal_across_chunks() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(40, 40, 40), STONE);
        let dir = Vec3::ONE.normalize();
        let hit = world
            .raycast(Vec3::new(0.2, 0.2, 0.2), dir, 200.0)
            .expect("should hit");
        assert_eq!(hit.voxel, IVec3::new(40, 40, 40));
    }

    #[test]
    fn breach_opens_line_of_fire() {
        let mut world = VoxelWorld::new();
        // A wall in the XY plane, 3 voxels thick in Z.
        world.fill_box(IVec3::new(0, 0, 0), IVec3::new(48, 24, 3), STONE);

        let origin = Vec3::new(24.0, 8.0, -10.0);
        assert!(world.raycast(origin, Vec3::Z, 100.0).is_some(), "wall blocks");

        let destroyed = world.carve_sphere(Vec3::new(24.0, 8.0, 1.5), 6.0);
        assert!(destroyed > 0);
        assert_eq!(
            world.raycast(origin, Vec3::Z, 100.0),
            None,
            "shot passes through the breach"
        );
    }
}

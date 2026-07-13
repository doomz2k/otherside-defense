use glam::IVec3;

use crate::chunk::CHUNK_SIZE;
use crate::world::VoxelWorld;

/// Renderer-agnostic mesh: parallel per-vertex arrays plus triangle indices.
/// Quads are emitted as 4 vertices + 6 indices, counter-clockwise when viewed
/// from the direction the face normal points.
#[derive(Default, Debug)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    /// Material id per vertex (uniform across each quad).
    pub materials: Vec<u32>,
    /// Baked ambient occlusion per vertex, 0..=1 (1 = fully open corner).
    pub aos: Vec<f32>,
    pub indices: Vec<u32>,
}

impl MeshData {
    pub fn is_empty(&self) -> bool {
        self.indices.is_empty()
    }

    pub fn quad_count(&self) -> usize {
        self.indices.len() / 6
    }

    /// Append an axis-aligned box (used for unit markers and debug shapes).
    /// Winding matches `mesh_chunk`: CCW seen from outside.
    pub fn push_box(&mut self, min: glam::Vec3, max: glam::Vec3, material: u8) {
        for d in 0..3 {
            let u = (d + 1) % 3;
            let v = (d + 2) % 3;
            for front in [true, false] {
                let plane = if front { max[d] } else { min[d] };
                let corner = |cu: f32, cv: f32| -> [f32; 3] {
                    let mut p = [0.0f32; 3];
                    p[d] = plane;
                    p[u] = cu;
                    p[v] = cv;
                    p
                };
                let (p00, p10, p11, p01) = (
                    corner(min[u], min[v]),
                    corner(max[u], min[v]),
                    corner(max[u], max[v]),
                    corner(min[u], max[v]),
                );
                let mut normal = [0.0f32; 3];
                normal[d] = if front { 1.0 } else { -1.0 };
                let first = self.positions.len() as u32;
                let quad = if front {
                    [p00, p10, p11, p01]
                } else {
                    [p00, p01, p11, p10]
                };
                for p in quad {
                    self.positions.push(p);
                    self.normals.push(normal);
                    self.materials.push(material as u32);
                    self.aos.push(1.0);
                }
                self.indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
            }
        }
    }
}

/// AO for one face: probe the eight neighbors of the face's air cell and
/// grade each of its four corners. Returned in [p00, p10, p11, p01] order.
fn face_ao(
    air: IVec3,
    u: usize,
    v: usize,
    solid: &impl Fn(IVec3) -> bool,
) -> [u8; 4] {
    let probe = |su: i32, sv: i32| -> bool {
        let mut p = air;
        p[u] += su;
        p[v] += sv;
        solid(p)
    };
    let corner = |su: i32, sv: i32| -> u8 {
        corner_ao(probe(su, 0), probe(0, sv), probe(su, sv))
    };
    [corner(-1, -1), corner(1, -1), corner(1, 1), corner(-1, 1)]
}

/// Corner AO levels, 0..=3: 3 is a fully open corner, 0 a pinched one.
/// The classic rule: two perpendicular side occluders pinch the corner
/// completely; otherwise every occluding neighbor costs a level.
fn corner_ao(side1: bool, side2: bool, corner: bool) -> u8 {
    if side1 && side2 {
        0
    } else {
        3 - (side1 as u8 + side2 as u8 + corner as u8)
    }
}

/// The brightness a baked AO level maps to.
fn ao_level(level: u8) -> f32 {
    0.55 + 0.15 * level as f32
}

/// Greedy-mesh one chunk.
///
/// Neighbor voxels are sampled through `world`, so faces against adjacent
/// chunks are culled correctly. Each boundary face is owned by the chunk that
/// contains its solid voxel, so meshing two adjacent chunks never emits the
/// same face twice.
pub fn mesh_chunk(world: &VoxelWorld, chunk: IVec3) -> MeshData {
    mesh_chunk_capped(world, chunk, None)
}

/// Like [`mesh_chunk`], but treats every voxel at world-z >= `cap` as empty —
/// the renderer's floor-cutaway view.
pub fn mesh_chunk_capped(world: &VoxelWorld, chunk: IVec3, cap: Option<i32>) -> MeshData {
    let origin = chunk * CHUNK_SIZE;
    let n = CHUNK_SIZE as usize;
    let mut mesh = MeshData::default();

    // A mask cell holds (material, is_front_face, corner AO levels) for a
    // face lying on the current slice plane, or None where no face is
    // needed. AO rides in the mask so cells only merge when their baked
    // shading agrees — the greedy pass stays correct.
    let mut mask: Vec<Option<(u8, bool, [u8; 4])>> = vec![None; n * n];

    for d in 0..3 {
        let u = (d + 1) % 3;
        let v = (d + 2) % 3;

        for slice in 0..=CHUNK_SIZE {
            // Build the face mask for this slice plane by comparing the two
            // voxels that meet at it.
            for j in 0..CHUNK_SIZE {
                for i in 0..CHUNK_SIZE {
                    let mut pa = IVec3::ZERO;
                    pa[d] = slice - 1;
                    pa[u] = i;
                    pa[v] = j;
                    let mut pb = pa;
                    pb[d] = slice;

                    let sample = |p: IVec3| {
                        let v = world.voxel(p);
                        match cap {
                            Some(cap) if p.z >= cap => crate::chunk::Voxel::EMPTY,
                            _ => v,
                        }
                    };
                    let solid = |p: IVec3| sample(origin + p).is_solid();
                    let a = sample(origin + pa);
                    let b = sample(origin + pb);
                    mask[(j * CHUNK_SIZE + i) as usize] = match (a.is_solid(), b.is_solid()) {
                        // Front face (+d) of voxel `a` — only if `a` is ours.
                        (true, false) if slice > 0 => {
                            Some((a.0, true, face_ao(pb, u, v, &solid)))
                        }
                        // Back face (-d) of voxel `b` — only if `b` is ours.
                        (false, true) if slice < CHUNK_SIZE => {
                            Some((b.0, false, face_ao(pa, u, v, &solid)))
                        }
                        _ => None,
                    };
                }
            }

            // Greedily merge equal mask cells into maximal rectangles.
            let mut emit = |i: usize,
                            j: usize,
                            w: usize,
                            h: usize,
                            (material, front, ao): (u8, bool, [u8; 4])| {
                let mut base = IVec3::ZERO;
                base[d] = slice;
                base[u] = i as i32;
                base[v] = j as i32;
                let corner = |du: i32, dv: i32| -> [f32; 3] {
                    let mut p = base;
                    p[u] += du;
                    p[v] += dv;
                    (origin + p).as_vec3().to_array()
                };
                let (p00, p10, p11, p01) = (
                    corner(0, 0),
                    corner(w as i32, 0),
                    corner(w as i32, h as i32),
                    corner(0, h as i32),
                );

                let mut normal = [0.0f32; 3];
                normal[d] = if front { 1.0 } else { -1.0 };

                let first = mesh.positions.len() as u32;
                // AO corner order matches [p00, p10, p11, p01].
                let (quad, ao4) = if front {
                    ([p00, p10, p11, p01], [ao[0], ao[1], ao[2], ao[3]])
                } else {
                    ([p00, p01, p11, p10], [ao[0], ao[3], ao[2], ao[1]])
                };
                for (p, a) in quad.into_iter().zip(ao4) {
                    mesh.positions.push(p);
                    mesh.normals.push(normal);
                    mesh.materials.push(material as u32);
                    mesh.aos.push(ao_level(a));
                }
                mesh.indices
                    .extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
            };

            for j in 0..n {
                let mut i = 0;
                while i < n {
                    let Some(face) = mask[j * n + i] else {
                        i += 1;
                        continue;
                    };
                    let mut w = 1;
                    while i + w < n && mask[j * n + i + w] == Some(face) {
                        w += 1;
                    }
                    let mut h = 1;
                    'grow: while j + h < n {
                        for k in 0..w {
                            if mask[(j + h) * n + i + k] != Some(face) {
                                break 'grow;
                            }
                        }
                        h += 1;
                    }

                    emit(i, j, w, h, face);
                    for row in j..j + h {
                        for col in i..i + w {
                            mask[row * n + col] = None;
                        }
                    }
                    i += w;
                }
            }
        }
    }

    mesh
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chunk::Voxel;
    use glam::Vec3;

    const STONE: Voxel = Voxel(1);
    const IRON: Voxel = Voxel(2);

    fn cross(a: Vec3, b: Vec3) -> Vec3 {
        a.cross(b)
    }

    #[test]
    fn single_voxel_is_six_quads() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(3, 4, 5), STONE);
        let mesh = mesh_chunk(&world, IVec3::ZERO);
        assert_eq!(mesh.quad_count(), 6);
        assert_eq!(mesh.positions.len(), 24);
        assert_eq!(mesh.indices.len(), 36);
    }

    #[test]
    fn winding_matches_normals() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(3, 4, 5), STONE);
        let mesh = mesh_chunk(&world, IVec3::ZERO);
        for quad in 0..mesh.quad_count() {
            let i0 = mesh.indices[quad * 6] as usize;
            let i1 = mesh.indices[quad * 6 + 1] as usize;
            let i2 = mesh.indices[quad * 6 + 2] as usize;
            let p0 = Vec3::from(mesh.positions[i0]);
            let p1 = Vec3::from(mesh.positions[i1]);
            let p2 = Vec3::from(mesh.positions[i2]);
            let face_normal = cross(p1 - p0, p2 - p0);
            let stored = Vec3::from(mesh.normals[i0]);
            assert!(
                face_normal.dot(stored) > 0.0,
                "quad {quad}: winding disagrees with normal {stored}"
            );
        }
    }

    #[test]
    fn coplanar_same_material_merges() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(0, 0, 0), STONE);
        world.set_voxel(IVec3::new(1, 0, 0), STONE);
        // A 2x1x1 bar still meshes to exactly 6 quads.
        assert_eq!(mesh_chunk(&world, IVec3::ZERO).quad_count(), 6);
    }

    #[test]
    fn different_materials_do_not_merge() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(0, 0, 0), STONE);
        world.set_voxel(IVec3::new(1, 0, 0), IRON);
        // Shared face is culled; the remaining 5 faces of each voxel cannot
        // merge across the material change.
        assert_eq!(mesh_chunk(&world, IVec3::ZERO).quad_count(), 10);
    }

    #[test]
    fn solid_chunk_is_six_quads() {
        let mut world = VoxelWorld::new();
        world.fill_box(IVec3::ZERO, IVec3::splat(CHUNK_SIZE), STONE);
        assert_eq!(mesh_chunk(&world, IVec3::ZERO).quad_count(), 6);
    }

    #[test]
    fn no_duplicate_faces_between_chunks() {
        let mut world = VoxelWorld::new();
        // A 64x1x1 bar spanning two chunks along X.
        world.fill_box(IVec3::new(0, 0, 0), IVec3::new(64, 1, 1), STONE);
        let total: usize = [IVec3::new(0, 0, 0), IVec3::new(1, 0, 0)]
            .iter()
            .map(|&c| mesh_chunk(&world, c).quad_count())
            .sum();
        // Per chunk: 4 long side faces + 1 end cap = 5. The seam at x=32 must
        // produce no faces at all.
        assert_eq!(total, 10);
    }

    #[test]
    fn empty_chunk_is_empty_mesh() {
        let world = VoxelWorld::new();
        assert!(mesh_chunk(&world, IVec3::ZERO).is_empty());
    }

    #[test]
    fn walls_pinch_the_floor_corners_they_touch() {
        let mut world = VoxelWorld::new();
        // A 5x5 floor slab with a single wall voxel standing mid-slab.
        world.fill_box(IVec3::ZERO, IVec3::new(5, 5, 1), STONE);
        world.set_voxel(IVec3::new(2, 2, 1), IRON);
        let mesh = mesh_chunk(&world, IVec3::ZERO);
        assert_eq!(mesh.aos.len(), mesh.positions.len());

        // Floor vertices under the open sky stay fully lit; the ones that
        // meet the wall's foot are occluded.
        let mut open = false;
        let mut pinched = false;
        for i in 0..mesh.positions.len() {
            // Upward floor faces only.
            if mesh.normals[i] != [0.0, 0.0, 1.0] {
                continue;
            }
            let [x, y, _] = mesh.positions[i];
            let near_wall = (2.0..=3.0).contains(&x) && (2.0..=3.0).contains(&y);
            if near_wall && mesh.aos[i] < 0.99 {
                pinched = true;
            }
            if !near_wall && mesh.aos[i] > 0.99 {
                open = true;
            }
        }
        assert!(pinched, "the wall's foot must darken the floor");
        assert!(open, "open floor must stay bright");
    }

    #[test]
    fn a_lone_voxel_has_fully_open_corners() {
        let mut world = VoxelWorld::new();
        world.set_voxel(IVec3::new(3, 4, 5), STONE);
        let mesh = mesh_chunk(&world, IVec3::ZERO);
        assert!(mesh.aos.iter().all(|&a| a > 0.99), "nothing occludes it");
    }
}

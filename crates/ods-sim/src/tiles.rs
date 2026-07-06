//! The coarse gameplay grid, derived from voxel occupancy.
//!
//! One tile is a `TILE_VOXELS`³ block. The tile layer owns everything the
//! player must read at a glance — walkability, pathfinding, TU costs — and is
//! re-derived from the voxel world whenever destruction changes it.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};

use glam::IVec3;
use ods_voxel::VoxelWorld;

use crate::TILE_VOXELS;

/// TU cost of one orthogonal step (X-COM's classic 4).
pub const MOVE_COST_ORTHO: i32 = 4;
/// TU cost of one diagonal step (X-COM's classic 6).
pub const MOVE_COST_DIAG: i32 = 6;

/// Voxels with local z >= this height block a tile (torso/head space).
/// Anything lower is floor or step-over rubble.
const HEADROOM_Z: i32 = 4;
/// Minimum solid voxels in the floor slab for a tile to support a unit
/// (one full 16x16 layer's worth).
const FLOOR_SUPPORT_MIN: i32 = TILE_VOXELS * TILE_VOXELS;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum TileKind {
    Blocked,
    Open,
    /// Waist-high climbable mass (rubble, steps): passable, and the only
    /// place a unit can transition between z-levels.
    Ramp,
}

pub struct TileMap {
    min: IVec3,
    size: IVec3,
    kinds: Vec<TileKind>,
}

impl TileMap {
    /// Derive the grid for tiles in `[min, min + size)` (tile coordinates).
    pub fn derive(world: &VoxelWorld, min: IVec3, size: IVec3) -> Self {
        let mut map = Self {
            min,
            size,
            kinds: vec![TileKind::Blocked; (size.x * size.y * size.z) as usize],
        };
        for z in 0..size.z {
            for y in 0..size.y {
                for x in 0..size.x {
                    let tile = min + IVec3::new(x, y, z);
                    let idx = map.index(tile).expect("in bounds by construction");
                    map.kinds[idx] = derive_tile(world, tile);
                }
            }
        }
        map
    }

    fn index(&self, tile: IVec3) -> Option<usize> {
        let rel = tile - self.min;
        if rel.min_element() < 0
            || rel.x >= self.size.x
            || rel.y >= self.size.y
            || rel.z >= self.size.z
        {
            return None;
        }
        Some(((rel.z * self.size.y + rel.y) * self.size.x + rel.x) as usize)
    }

    pub fn bounds(&self) -> (IVec3, IVec3) {
        (self.min, self.min + self.size)
    }

    pub fn is_walkable(&self, tile: IVec3) -> bool {
        self.index(tile)
            .is_some_and(|i| self.kinds[i] != TileKind::Blocked)
    }

    /// Ramp tiles are where units can climb between z-levels.
    pub fn is_ramp(&self, tile: IVec3) -> bool {
        self.index(tile).is_some_and(|i| self.kinds[i] == TileKind::Ramp)
    }

    /// Re-derive all tiles overlapping the voxel-space AABB `[vmin, vmax]`.
    /// Call after destruction; the AABB is the affected voxel region.
    pub fn rederive_region(&mut self, world: &VoxelWorld, vmin: IVec3, vmax: IVec3) {
        let tmin = crate::voxel_to_tile(vmin);
        let tmax = crate::voxel_to_tile(vmax);
        for z in tmin.z..=tmax.z {
            for y in tmin.y..=tmax.y {
                for x in tmin.x..=tmax.x {
                    let tile = IVec3::new(x, y, z);
                    if let Some(idx) = self.index(tile) {
                        self.kinds[idx] = derive_tile(world, tile);
                    }
                }
            }
        }
    }

    /// A* path from `from` to `to` on this z-level, avoiding `blocked` tiles
    /// (typically tiles occupied by living units). Returns the sequence of
    /// tiles stepped onto (excluding `from`, including `to`), or None.
    pub fn path(&self, from: IVec3, to: IVec3, blocked: &HashSet<IVec3>) -> Option<Vec<IVec3>> {
        if from == to || !self.is_walkable(to) || blocked.contains(&to) {
            return None;
        }
        let h = |t: IVec3| {
            let d = (to - t).abs();
            let (lo, hi) = (d.x.min(d.y), d.x.max(d.y));
            MOVE_COST_DIAG * lo + MOVE_COST_ORTHO * (hi - lo) + CLIMB_COST * d.z
        };

        type OpenEntry = Reverse<(i32, u32, i32, i32, i32)>;
        let mut open: BinaryHeap<OpenEntry> = BinaryHeap::new();
        let mut g: HashMap<IVec3, i32> = HashMap::new();
        let mut came: HashMap<IVec3, IVec3> = HashMap::new();
        let mut tie = 0u32;

        g.insert(from, 0);
        open.push(Reverse((h(from), tie, from.x, from.y, from.z)));

        while let Some(Reverse((_, _, cx, cy, cz))) = open.pop() {
            let current = IVec3::new(cx, cy, cz);
            if current == to {
                let mut path = vec![to];
                let mut node = to;
                while let Some(&prev) = came.get(&node) {
                    if prev == from {
                        break;
                    }
                    path.push(prev);
                    node = prev;
                }
                path.reverse();
                return Some(path);
            }
            let current_g = g[&current];

            for dz in -1i32..=1 {
                for dy in -1i32..=1 {
                    for dx in -1i32..=1 {
                        if dx == 0 && dy == 0 {
                            continue; // no purely vertical hops
                        }
                        let next = current + IVec3::new(dx, dy, dz);
                        if !self.is_walkable(next) || blocked.contains(&next) {
                            continue;
                        }
                        // Level changes only happen over climbable mass.
                        if dz != 0 && !(self.is_ramp(current) || self.is_ramp(next)) {
                            continue;
                        }
                        // No cutting corners diagonally past a blocked tile.
                        if dx != 0 && dy != 0 && dz == 0 {
                            let a = current + IVec3::new(dx, 0, 0);
                            let b = current + IVec3::new(0, dy, 0);
                            if !self.is_walkable(a)
                                || !self.is_walkable(b)
                                || blocked.contains(&a)
                                || blocked.contains(&b)
                            {
                                continue;
                            }
                        }
                        let cost = step_cost(current, next);
                        let next_g = current_g + cost;
                        if g.get(&next).is_none_or(|&old| next_g < old) {
                            g.insert(next, next_g);
                            came.insert(next, current);
                            tie += 1;
                            open.push(Reverse((
                                next_g + h(next),
                                tie,
                                next.x,
                                next.y,
                                next.z,
                            )));
                        }
                    }
                }
            }
        }
        None
    }
}

/// Extra TU for hauling yourself up or down a level.
pub const CLIMB_COST: i32 = 4;

/// TU cost of stepping between two adjacent tiles.
pub fn step_cost(a: IVec3, b: IVec3) -> i32 {
    let d = (b - a).abs();
    let flat = if d.x + d.y == 2 { MOVE_COST_DIAG } else { MOVE_COST_ORTHO };
    flat + CLIMB_COST * d.z
}

fn derive_tile(world: &VoxelWorld, tile: IVec3) -> TileKind {
    let origin = crate::tile_to_voxel_min(tile);

    // Count solid voxels per vertical band: head (top half), waist (quarter
    // to half), and feet (bottom quarter).
    let band = |z0: i32, z1: i32| -> i32 {
        let mut count = 0;
        for z in z0..z1 {
            for y in 0..TILE_VOXELS {
                for x in 0..TILE_VOXELS {
                    if world.voxel(origin + IVec3::new(x, y, z)).is_solid() {
                        count += 1;
                    }
                }
            }
        }
        count
    };

    let head = band(TILE_VOXELS * 5 / 8, TILE_VOXELS);
    if head > 0 {
        return TileKind::Blocked;
    }

    // Floor support: solid voxels in this tile's foot band, or the top band
    // of the tile below.
    let mut support = band(0, HEADROOM_Z);
    let below = origin - IVec3::new(0, 0, TILE_VOXELS);
    for z in (TILE_VOXELS - HEADROOM_Z)..TILE_VOXELS {
        for y in 0..TILE_VOXELS {
            for x in 0..TILE_VOXELS {
                if world.voxel(below + IVec3::new(x, y, z)).is_solid() {
                    support += 1;
                }
            }
        }
    }
    if support < FLOOR_SUPPORT_MIN {
        return TileKind::Blocked;
    }

    // Waist band: empty = open ground; substantial mass = climbable ramp.
    let waist = band(HEADROOM_Z, TILE_VOXELS * 5 / 8);
    if waist == 0 {
        TileKind::Open
    } else if waist >= TILE_VOXELS * TILE_VOXELS {
        // At least a full layer's worth of mass to clamber on.
        TileKind::Ramp
    } else {
        TileKind::Blocked
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;
    use ods_voxel::Voxel;

    const STONE: Voxel = Voxel(1);

    /// 8x8 tile map with a 2-voxel ground slab and a wall across x=4 with a
    /// gap at y=6.
    fn walled_world() -> (VoxelWorld, TileMap) {
        let mut world = VoxelWorld::new();
        world.fill_box(
            IVec3::new(0, 0, 0),
            IVec3::new(8 * TILE_VOXELS, 8 * TILE_VOXELS, 2),
            STONE,
        );
        for ty in 0..8 {
            if ty == 6 {
                continue; // doorway
            }
            let o = crate::tile_to_voxel_min(IVec3::new(4, ty, 0));
            world.fill_box(
                o + IVec3::new(0, 0, 2),
                o + IVec3::new(TILE_VOXELS, TILE_VOXELS, 14),
                Voxel(2),
            );
        }
        let tiles = TileMap::derive(&world, IVec3::ZERO, IVec3::new(8, 8, 1));
        (world, tiles)
    }

    #[test]
    fn ground_is_walkable_walls_are_not() {
        let (_, tiles) = walled_world();
        assert!(tiles.is_walkable(IVec3::new(0, 0, 0)));
        assert!(tiles.is_walkable(IVec3::new(7, 7, 0)));
        assert!(!tiles.is_walkable(IVec3::new(4, 0, 0)), "wall blocks");
        assert!(tiles.is_walkable(IVec3::new(4, 6, 0)), "doorway is open");
        assert!(!tiles.is_walkable(IVec3::new(-1, 0, 0)), "out of bounds");
        assert!(!tiles.is_walkable(IVec3::new(0, 0, 1)), "no floor in the sky");
    }

    #[test]
    fn path_goes_through_the_doorway() {
        let (_, tiles) = walled_world();
        let path = tiles
            .path(IVec3::new(2, 1, 0), IVec3::new(6, 1, 0), &HashSet::new())
            .expect("path exists via doorway");
        assert!(path.contains(&IVec3::new(4, 6, 0)), "must use the gap: {path:?}");
        assert_eq!(*path.last().unwrap(), IVec3::new(6, 1, 0));

        // TU cost through the door is far more than the straight-line 16.
        let cost: i32 = std::iter::once(IVec3::new(2, 1, 0))
            .chain(path.iter().copied())
            .zip(path.iter().copied())
            .map(|(a, b)| step_cost(a, b))
            .sum();
        assert!(cost > 30, "detour should be expensive, got {cost}");
    }

    #[test]
    fn blocked_tiles_reroute_or_fail() {
        let (_, tiles) = walled_world();
        let mut blocked = HashSet::new();
        blocked.insert(IVec3::new(4, 6, 0)); // someone standing in the doorway
        assert_eq!(
            tiles.path(IVec3::new(2, 1, 0), IVec3::new(6, 1, 0), &blocked),
            None,
            "the only way through is occupied"
        );
    }

    #[test]
    fn straight_and_diagonal_costs() {
        let (_, tiles) = walled_world();
        let path = tiles
            .path(IVec3::new(0, 0, 0), IVec3::new(3, 0, 0), &HashSet::new())
            .unwrap();
        assert_eq!(path.len(), 3);
        let path = tiles
            .path(IVec3::new(0, 0, 0), IVec3::new(2, 2, 0), &HashSet::new())
            .unwrap();
        assert_eq!(path.len(), 2, "pure diagonal: {path:?}");
    }

    #[test]
    fn breach_makes_wall_walkable() {
        let (mut world, mut tiles) = walled_world();
        let wall_tile = IVec3::new(4, 3, 0);
        assert!(!tiles.is_walkable(wall_tile));

        // Demolish the wall segment above the floor slab, then re-derive
        // just the affected voxel region.
        let o = crate::tile_to_voxel_min(wall_tile);
        world.fill_box(
            o + IVec3::new(0, 0, 2),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, 14),
            Voxel::EMPTY,
        );
        tiles.rederive_region(&world, o, o + IVec3::splat(TILE_VOXELS));

        assert!(
            tiles.is_walkable(wall_tile),
            "cleared breach should be passable (floor slab survives below z=2)"
        );
        // A partial carve that leaves torso-height debris must NOT open the
        // neighboring wall tile.
        let neighbor = IVec3::new(4, 4, 0);
        let c = crate::tile_to_voxel_min(neighbor).as_vec3() + Vec3::new(8.0, 8.0, 6.0);
        world.carve_sphere(c, 5.0);
        let r = IVec3::splat(6);
        tiles.rederive_region(&world, c.as_ivec3() - r, c.as_ivec3() + r);
        assert!(!tiles.is_walkable(neighbor), "half-breached wall still blocks");
    }
}

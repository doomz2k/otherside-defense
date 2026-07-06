//! Headless, deterministic Battlescape rules.
//!
//! Hard rules for this crate (see `docs/design/tech-stack.md`):
//! - No rendering types, no wall-clock time, no threads.
//! - All randomness flows through [`SimRng`] with an explicit seed, so a
//!   battle is a pure function of (initial state, seed, action list). Replays,
//!   save/load, and reproducible bug reports depend on this.

use glam::IVec3;
use rand::Rng;
use rand::SeedableRng;
use rand_pcg::Pcg32;

/// Side length of one gameplay tile, in voxels. One tile ≈ 1 m³.
pub const TILE_VOXELS: i32 = 16;

/// Voxel coordinate of a tile's minimum corner.
pub fn tile_to_voxel_min(tile: IVec3) -> IVec3 {
    tile * TILE_VOXELS
}

/// The gameplay tile containing a voxel.
pub fn voxel_to_tile(voxel: IVec3) -> IVec3 {
    IVec3::new(
        voxel.x.div_euclid(TILE_VOXELS),
        voxel.y.div_euclid(TILE_VOXELS),
        voxel.z.div_euclid(TILE_VOXELS),
    )
}

/// The only random number source the simulation may use.
pub struct SimRng(Pcg32);

impl SimRng {
    pub fn from_seed(seed: u64) -> Self {
        Self(Pcg32::seed_from_u64(seed))
    }

    /// Uniform roll in `[0, sides)` — e.g. `roll(100)` for a percentile check.
    pub fn roll(&mut self, sides: u32) -> u32 {
        self.0.random_range(0..sides)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_same_rolls() {
        let mut a = SimRng::from_seed(0xD00D);
        let mut b = SimRng::from_seed(0xD00D);
        for _ in 0..1000 {
            assert_eq!(a.roll(100), b.roll(100));
        }
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = SimRng::from_seed(1);
        let mut b = SimRng::from_seed(2);
        let same = (0..1000).filter(|_| a.roll(100) == b.roll(100)).count();
        assert!(same < 1000, "streams should not be identical");
    }

    #[test]
    fn tile_mapping_handles_negatives() {
        assert_eq!(voxel_to_tile(IVec3::new(0, 0, 0)), IVec3::new(0, 0, 0));
        assert_eq!(voxel_to_tile(IVec3::new(15, 15, 15)), IVec3::new(0, 0, 0));
        assert_eq!(voxel_to_tile(IVec3::new(16, 0, 0)), IVec3::new(1, 0, 0));
        assert_eq!(voxel_to_tile(IVec3::new(-1, -16, -17)), IVec3::new(-1, -1, -2));
        assert_eq!(tile_to_voxel_min(IVec3::new(-1, 2, 0)), IVec3::new(-16, 32, 0));
    }
}

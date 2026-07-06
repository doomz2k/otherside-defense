use glam::IVec3;

/// Side length of a chunk, in voxels.
pub const CHUNK_SIZE: i32 = 32;

const CHUNK_VOLUME: usize = (CHUNK_SIZE * CHUNK_SIZE * CHUNK_SIZE) as usize;

/// A single voxel cell: a material id, where 0 is empty air.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Voxel(pub u8);

impl Voxel {
    pub const EMPTY: Self = Voxel(0);

    #[inline]
    pub fn is_solid(self) -> bool {
        self.0 != 0
    }
}

/// Dense cube of `CHUNK_SIZE`³ voxels.
pub struct Chunk {
    voxels: Box<[Voxel; CHUNK_VOLUME]>,
}

impl Chunk {
    pub fn new() -> Self {
        Self {
            voxels: Box::new([Voxel::EMPTY; CHUNK_VOLUME]),
        }
    }

    #[inline]
    fn index(local: IVec3) -> usize {
        debug_assert!(
            local.min_element() >= 0 && local.max_element() < CHUNK_SIZE,
            "voxel coord {local} out of chunk bounds"
        );
        ((local.z * CHUNK_SIZE + local.y) * CHUNK_SIZE + local.x) as usize
    }

    #[inline]
    pub fn get(&self, local: IVec3) -> Voxel {
        self.voxels[Self::index(local)]
    }

    #[inline]
    pub fn set(&mut self, local: IVec3, voxel: Voxel) {
        self.voxels[Self::index(local)] = voxel;
    }
}

impl Default for Chunk {
    fn default() -> Self {
        Self::new()
    }
}

//! Map generation and battle setup for the first playable skirmish:
//! a ruined chapel yard, four Order soldiers, four imps.

use glam::IVec3;
use ods_voxel::{Voxel, VoxelWorld};

use crate::{TILE_VOXELS, VS};
use crate::battle::Battle;
use crate::units::Unit;

pub const MAP_TILES: IVec3 = IVec3::new(24, 24, 2);

/// Ground fills voxels z 0..4 (the tile's foot band), so shallow craters
/// don't punch through to the void.
pub const GROUND_TOP: i32 = TILE_VOXELS / 4;
/// Walls rise from the ground to torso/head height.
const WALL_TOP: i32 = TILE_VOXELS * 7 / 8;

pub const MAT_GROUND: Voxel = Voxel(1);
pub const MAT_WALL: Voxel = Voxel(2);
pub const MAT_RUBBLE: Voxel = Voxel(3);
/// Door leaf: a thin blocking slab until opened (or blown apart).
pub const MAT_DOOR: Voxel = Voxel(5);
/// Fuel cask: detonates when its shell is breached.
pub const MAT_CASK: Voxel = Voxel(6);
/// Brimstone pool: ignites at a spark.
pub const MAT_POOL: Voxel = Voxel(7);
/// Nest flesh: the living walls of a demon warren.
pub const MAT_FLESH: Voxel = Voxel(8);
/// Otherside obsidian.
pub const MAT_OBSIDIAN: Voxel = Voxel(9);
/// The rift obelisk: hell's anchor into the world. Demolish it to win.
pub const MAT_OBELISK: Voxel = Voxel(4);
/// Desert sand.
pub const MAT_SAND: Voxel = Voxel(10);
/// Snow and ice.
pub const MAT_SNOW: Voxel = Voxel(11);
/// Living green: canopy, hedgerows.
pub const MAT_FOLIAGE: Voxel = Voxel(12);
/// Tree trunks and timber.
pub const MAT_TIMBER: Voxel = Voxel(13);
/// Spilled blood, dried into the ground.
pub const MAT_BLOOD: Voxel = Voxel(14);
/// Ground stipple: grass tufts, wildflowers, frost glints.
pub const MAT_TUFT: Voxel = Voxel(19);
pub const MAT_FLOWER: Voxel = Voxel(20);
pub const MAT_GLINT: Voxel = Voxel(21);
/// Bare turned earth where the grass gives out.
pub const MAT_EARTH: Voxel = Voxel(22);
/// The packed pale road that was here before the war.
pub const MAT_PATH: Voxel = Voxel(23);
/// A lighter meadow green blended through the field grass.
pub const MAT_MEADOW: Voxel = Voxel(24);
/// Field boulder grey.
pub const MAT_STONE: Voxel = Voxel(25);
/// Limewashed plaster: cottage walls under timber studs.
pub const MAT_PLASTER: Voxel = Voxel(26);
/// Slate shingle: the roofs that survived.
pub const MAT_SLATE: Voxel = Voxel(27);
/// Viscera. What overkill leaves.
pub const MAT_GORE: Voxel = Voxel(15);
/// Glowing sigil-crimson: summoning circles, the obelisk's runes. EMISSIVE.
pub const MAT_SIGIL: Voxel = Voxel(16);
/// Witchfire teal: the Order's wards. EMISSIVE.
pub const MAT_WARD: Voxel = Voxel(17);
/// The obelisk's corruption veins. EMISSIVE.
pub const MAT_VEIN: Voxel = Voxel(18);

/// The kind of country a rift opens into. Chosen by the campaign from the
/// rift's world region; drives ground material and terrain generation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Biome {
    /// Fields, chapels, hedgerows — the old default.
    Temperate,
    /// Dunes and dry-stone ruins under a hard sun.
    Desert,
    /// Trees with climbable canopies; the understory is dark work.
    Jungle,
    /// Snowdrifts and ice boulders; open ground, long sightlines.
    Tundra,
}

impl Biome {
    pub fn name(self) -> &'static str {
        match self {
            Biome::Temperate => "temperate",
            Biome::Desert => "desert",
            Biome::Jungle => "jungle",
            Biome::Tundra => "tundra",
        }
    }
}

pub fn skirmish(seed: u64) -> Battle {
    let mut world = VoxelWorld::new();

    // Ground slab across the whole map.
    world.fill_box(
        IVec3::ZERO,
        IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, GROUND_TOP),
        MAT_GROUND,
    );
    stipple_ground(&mut world, seed, Biome::Temperate);

    // The chapel: a walled rectangle with a doorway on each long side.
    // Tile ring at x 9..=14, y 8..=15.
    for tx in 9..=14 {
        for ty in 8..=15 {
            let on_ring = tx == 9 || tx == 14 || ty == 8 || ty == 15;
            let doorway = (tx == 9 && ty == 11) || (tx == 14 && ty == 12);
            if on_ring && !doorway {
                fill_tile_walls(&mut world, IVec3::new(tx, ty, 0), MAT_WALL);
            }
        }
    }

    // Freestanding ruin wall in the west approach, with a collapsed gap.
    for ty in 3..=6 {
        fill_tile_walls(&mut world, IVec3::new(6, ty, 0), MAT_WALL);
    }
    for ty in 17..=20 {
        fill_tile_walls(&mut world, IVec3::new(6, ty, 0), MAT_WALL);
    }

    // Scattered rubble heaps: low cover that blocks movement but not sight.
    for (tx, ty) in [(3, 8), (4, 15), (11, 3), (12, 20), (17, 7), (18, 16), (8, 11)] {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(3 * VS, 3 * VS, GROUND_TOP),
            o + IVec3::new(13 * VS, 13 * VS, GROUND_TOP + 4 * VS),
            MAT_RUBBLE,
        );
    }

    let units = vec![
        Unit::soldier(0, "Sgt. Vasquez", IVec3::new(2, 9, 0)),
        Unit::soldier(1, "Kowalski", IVec3::new(2, 11, 0)),
        Unit::soldier(2, "Ito", IVec3::new(2, 13, 0)),
        Unit::soldier(3, "Moreau", IVec3::new(3, 15, 0)),
        Unit::imp(4, "Imp of Wrath", IVec3::new(21, 8, 0)),
        Unit::imp(5, "Imp of Envy", IVec3::new(21, 11, 0)),
        Unit::imp(6, "Imp of Gluttony", IVec3::new(21, 14, 0)),
        Unit::imp(7, "Imp of Sloth", IVec3::new(20, 16, 0)),
    ];

    Battle::new(world, IVec3::ZERO, MAP_TILES, units, seed)
}

/// Deployment: the gondola deck first, then the mouth, then the ramp —
/// the squad walks down out of the zeppelin the way it flew in.
const ORDER_SPAWNS: [(i32, i32); 8] =
    [(1, 11), (2, 11), (1, 12), (2, 12), (3, 11), (3, 12), (4, 11), (4, 12)];
const DEMON_SPAWNS: [(i32, i32); 8] =
    [(21, 8), (21, 11), (21, 14), (20, 16), (21, 5), (20, 18), (21, 20), (20, 3)];

/// The demon pack that answers a given incursion strength (roughly the
/// campaign month). Early months are imp swarms; later the pack diversifies,
/// gains an Overseer, and eventually brings a Taker.
pub fn demon_pack(count: u32, strength: u32, first_id: u32, spawns: &[(i32, i32)]) -> Vec<Unit> {
    let names = ["Wrath", "Envy", "Gluttony", "Sloth", "Pride", "Greed", "Lust", "Despair"];
    let mut pack = Vec::new();
    for i in 0..count.min(spawns.len() as u32) as usize {
        let id = first_id + i as u32;
        let (x, y) = spawns[i];
        let tile = IVec3::new(x, y, 0);
        let name = names[i % names.len()];
        let unit = if strength >= 10 && i == 0 {
            Unit::prince(id, &format!("Prince of {name}"), tile)
        } else if strength >= 5 && i == 0 {
            Unit::overseer(id, &format!("Overseer of {name}"), tile)
        } else if strength >= 7 && i == 1 {
            Unit::taker(id, "The Taker", tile)
        } else if strength >= 10 && i == 2 {
            Unit::overseer(id, &format!("Overseer of {name}"), tile)
        } else if strength >= 8 && i == 2 {
            Unit::behemoth(id, &format!("Behemoth of {name}"), tile)
        } else if strength >= 3 {
            match i % 5 {
                2 => Unit::hellhound(id, &format!("Hound of {name}"), tile),
                3 => Unit::bile_wisp(id, &format!("Wisp of {name}"), tile),
                4 if strength >= 4 => Unit::gargoyle(id, &format!("Gargoyle of {name}"), tile),
                _ => Unit::imp(id, &format!("Imp of {name}"), tile),
            }
        } else {
            Unit::imp(id, &format!("Imp of {name}"), tile)
        };
        pack.push(unit);
    }
    pack
}

/// Build a battle on the standard map from campaign-supplied soldiers, a
/// demon head-count, and an escalation strength. Unit ids are reassigned to
/// match battle indexing; squad order is preserved so the caller can map
/// results back to its roster.
pub fn incursion(seed: u64, soldiers: Vec<Unit>, demon_count: u32, strength: u32) -> Battle {
    incursion_with_civilians(seed, soldiers, demon_count, strength, 0)
}

/// Massacre sites have townsfolk still alive in the chapel — for now.
pub fn incursion_with_civilians(
    seed: u64,
    soldiers: Vec<Unit>,
    demon_count: u32,
    strength: u32,
    civilians: u32,
) -> Battle {
    incursion_in_biome(seed, soldiers, demon_count, strength, civilians, Biome::Temperate)
}

/// What the campaign wants from this field, before the sim details it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MissionSpec {
    Standard,
    /// Walk the survivors out before the clock dies.
    Evacuate,
    /// The harvest completes on a timer unless the obelisk falls first.
    Interrupt,
    /// Take the ringleader ALIVE.
    Snatch,
}

/// The full generator: one of three structural layouts (seeded), dressed by
/// the biome — ground material and a seeded scatter of biome features, so no
/// two sites in the same country fight the same.
pub fn incursion_in_biome(
    seed: u64,
    soldiers: Vec<Unit>,
    demon_count: u32,
    strength: u32,
    civilians: u32,
    biome: Biome,
) -> Battle {
    incursion_mission(seed, soldiers, demon_count, strength, civilians, biome, MissionSpec::Standard)
}

/// The generator with a mission rule layered on.
pub fn incursion_mission(
    seed: u64,
    mut soldiers: Vec<Unit>,
    demon_count: u32,
    strength: u32,
    civilians: u32,
    biome: Biome,
    spec: MissionSpec,
) -> Battle {
    let ground = match biome {
        Biome::Temperate | Biome::Jungle => MAT_GROUND,
        Biome::Desert => MAT_SAND,
        Biome::Tundra => MAT_SNOW,
    };
    let mut world = VoxelWorld::new();
    world.fill_box(
        IVec3::ZERO,
        IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, GROUND_TOP),
        ground,
    );
    sculpt_ground(&mut world, seed, biome);
    stipple_ground(&mut world, seed, biome);
    build_gondola(&mut world);
    match seed % 3 {
        0 => {
            // The chapel yard (the original) — now with a loft: an upper
            // floor over the nave, a rubble stair inside, and shuttered
            // window gaps for whoever holds the high ground.
            for tx in 9..=14 {
                for ty in 8..=15 {
                    let on_ring = tx == 9 || tx == 14 || ty == 8 || ty == 15;
                    let doorway = (tx == 9 && ty == 11) || (tx == 14 && ty == 12);
                    if on_ring && !doorway {
                        fill_tile_walls(&mut world, IVec3::new(tx, ty, 0), MAT_WALL);
                    }
                }
            }
            for ty in [3, 4, 5, 6, 17, 18, 19, 20] {
                fill_tile_walls(&mut world, IVec3::new(6, ty, 0), MAT_WALL);
            }
            // Loft slab over the interior, except the stairwell at (10, 9).
            for tx in 10..=13 {
                for ty in 9..=14 {
                    if (tx, ty) == (10, 9) {
                        continue;
                    }
                    let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
                    world.fill_box(
                        o + IVec3::new(0, 0, TILE_VOXELS),
                        o + IVec3::new(TILE_VOXELS, TILE_VOXELS, TILE_VOXELS + GROUND_TOP),
                        MAT_TIMBER,
                    );
                }
            }
            // The stair: a climbable rubble ramp in the stairwell.
            let stair = IVec3::new(10, 9, 0) * TILE_VOXELS;
            world.fill_box(
                stair + IVec3::new(0, 0, GROUND_TOP),
                stair + IVec3::new(TILE_VOXELS, TILE_VOXELS, 10 * VS),
                MAT_RUBBLE,
            );
            // Upper walls with window gaps on alternating ring tiles.
            for tx in 9..=14 {
                for ty in 8..=15 {
                    let on_ring = tx == 9 || tx == 14 || ty == 8 || ty == 15;
                    if !on_ring {
                        continue;
                    }
                    let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
                    let top = TILE_VOXELS + GROUND_TOP;
                    world.fill_box(
                        o + IVec3::new(0, 0, top),
                        o + IVec3::new(TILE_VOXELS, TILE_VOXELS, top + 10 * VS),
                        MAT_WALL,
                    );
                    // A shutter gap: waist-to-head open on every other tile.
                    if (tx + ty) % 2 == 0 {
                        world.fill_box(
                            o + IVec3::new(2 * VS, 2 * VS, top + 5 * VS),
                            o + IVec3::new(14 * VS, 14 * VS, top + 10 * VS),
                            Voxel::EMPTY,
                        );
                    }
                }
            }
        }
        1 => {
            // Twin ruins: two gutted farmhouses on the approach —
            // limewashed plaster between the timbers.
            for (bx, by) in [(7, 4), (10, 14)] {
                for tx in bx..bx + 4 {
                    for ty in by..by + 5 {
                        let ring = tx == bx || tx == bx + 3 || ty == by || ty == by + 4;
                        let doorway = tx == bx && ty == by + 2;
                        if ring && !doorway {
                            fill_tile_walls(&mut world, IVec3::new(tx, ty, 0), MAT_PLASTER);
                        }
                    }
                }
            }
            // The first farmhouse keeps its loft: an open fighting deck
            // over the interior, reached by a rubble stair inside.
            for tx in 8..=9 {
                for ty in 5..=7 {
                    if (tx, ty) == (8, 5) {
                        continue; // the stairwell
                    }
                    let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
                    world.fill_box(
                        o + IVec3::new(0, 0, TILE_VOXELS),
                        o + IVec3::new(TILE_VOXELS, TILE_VOXELS, TILE_VOXELS + GROUND_TOP),
                        MAT_TIMBER,
                    );
                }
            }
            let stair = IVec3::new(8, 5, 0) * TILE_VOXELS;
            world.fill_box(
                stair + IVec3::new(0, 0, GROUND_TOP),
                stair + IVec3::new(TILE_VOXELS, TILE_VOXELS, 10 * VS),
                MAT_RUBBLE,
            );
            for ty in [8, 9, 10, 11] {
                fill_tile_walls(&mut world, IVec3::new(15, ty, 0), MAT_WALL);
            }
        }
        _ => {
            // The shattered street: rubble rows and abandoned fuel casks.
            for ty in [5, 11, 17] {
                for tx in [7, 8, 10, 11, 13, 14] {
                    let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
                    world.fill_box(
                        o + IVec3::new(2 * VS, 2 * VS, GROUND_TOP),
                        o + IVec3::new(14 * VS, 14 * VS, 10 * VS),
                        MAT_RUBBLE,
                    );
                }
            }
            for tx in 9..=12 {
                fill_tile_walls(&mut world, IVec3::new(tx, 8, 0), MAT_STONE);
                fill_tile_walls(&mut world, IVec3::new(tx, 14, 0), MAT_STONE);
            }
        }
    }
    // Fuel casks wait wherever men once worked.
    let mut hazard_casks = Vec::new();
    for (tx, ty) in [(8, 7), (13, 16)] {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(5 * VS, 5 * VS, GROUND_TOP),
            o + IVec3::new(11 * VS, 11 * VS, GROUND_TOP + 7 * VS),
            MAT_CASK,
        );
        hazard_casks.push(IVec3::new(tx, ty, 0));
    }

    // A gutted watchtower: raised floor reached over a rubble mound. High
    // ground overwatches the yard.
    world.fill_box(
        IVec3::new(16 * TILE_VOXELS, 3 * TILE_VOXELS, TILE_VOXELS),
        IVec3::new(20 * TILE_VOXELS, 6 * TILE_VOXELS, TILE_VOXELS + GROUND_TOP),
        MAT_WALL,
    );
    for (px, py) in [(16, 3), (19, 3), (16, 5), (19, 5)] {
        world.fill_box(
            IVec3::new(px * TILE_VOXELS + 4 * VS, py * TILE_VOXELS + 4 * VS, GROUND_TOP),
            IVec3::new(px * TILE_VOXELS + 12 * VS, py * TILE_VOXELS + 12 * VS, TILE_VOXELS),
            MAT_WALL,
        );
    }
    // The climbable mound at the tower's west face.
    let mound = IVec3::new(15, 4, 0) * TILE_VOXELS;
    world.fill_box(
        mound + IVec3::new(0, 0, GROUND_TOP),
        mound + IVec3::new(TILE_VOXELS, TILE_VOXELS, 10 * VS),
        MAT_RUBBLE,
    );

    // The masonry gets dressed (plinth, studs, lintels, window reliefs),
    // roofless buildings that still have families get roofs, and the
    // rooms keep the furniture of interrupted lives.
    dress_walls(&mut world, seed);
    match seed % 3 {
        0 => furnish_rooms(&mut world, seed, IVec3::new(10, 9, 0), IVec3::new(13, 14, 0)),
        1 => {
            furnish_rooms(&mut world, seed, IVec3::new(8, 5, 0), IVec3::new(9, 7, 0));
            furnish_rooms(&mut world, seed, IVec3::new(11, 15, 0), IVec3::new(12, 17, 0));
            roof_over(&mut world, IVec3::new(10, 14, 0), IVec3::new(13, 18, 0));
        }
        _ => {}
    }

    // The rift obelisk, deep in demon territory (clear of spawn tiles).
    let obelisk_min = IVec3::new(22 * TILE_VOXELS, 11 * TILE_VOXELS, GROUND_TOP);
    let obelisk_max = IVec3::new(23 * TILE_VOXELS, 13 * TILE_VOXELS, 24 * VS);
    world.fill_box(obelisk_min, obelisk_max, MAT_OBELISK);
    carve_runes(&mut world, obelisk_min, obelisk_max);

    // ------------------------------------------------------------------
    // Biome dressing: seeded scatter over whatever ground the fixed
    // structures left open. Both deployment strips, the watchtower, the
    // casks, and the shelter yard stay clear so every map stays winnable.
    let mut rng = crate::SimRng::from_seed(seed ^ 0x00B1_05E5);
    let is_open = |world: &VoxelWorld, tile: IVec3| -> bool {
        if !(5..=19).contains(&tile.x) || !(1..=22).contains(&tile.y) {
            return false;
        }
        if (14..=20).contains(&tile.x) && (2..=6).contains(&tile.y) {
            return false; // watchtower and its mound
        }
        if [(8, 7), (13, 16)].contains(&(tile.x, tile.y)) {
            return false; // fuel casks
        }
        if (9..=14).contains(&tile.x) && (8..=15).contains(&tile.y) {
            return false; // the shelter yard where civilians hide
        }
        let probe = tile * TILE_VOXELS + IVec3::new(8, 8, GROUND_TOP + 1);
        // Open ground AND open sky: nothing sprouts inside a building or
        // under a roof, a loft, or a canopy.
        let sky = tile * TILE_VOXELS + IVec3::new(8, 8, TILE_VOXELS + 4);
        world.voxel(probe) == Voxel::EMPTY && world.voxel(sky) == Voxel::EMPTY
    };
    let hash = |a: i32, b: i32, k: u32| -> u32 {
        let mut h = (seed as u32)
            .wrapping_mul(747796405)
            .wrapping_add(a as u32)
            .wrapping_mul(2654435761)
            .wrapping_add(b as u32)
            .wrapping_mul(1274126177)
            .wrapping_add(k);
        h ^= h >> 15;
        h.wrapping_mul(2246822519) >> 8
    };
    // A climbable mound of loose material: walkable high ground (a ramp),
    // with its rim shaved down so it reads as a heap, not a die.
    let mound_at = move |world: &mut VoxelWorld, tile: IVec3, mat: Voxel| {
        let o = tile * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(0, 0, GROUND_TOP),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, 10 * VS),
            mat,
        );
        // Shave the top rim on a dome profile; the climbable core stays.
        let c = TILE_VOXELS as f32 / 2.0;
        for y in 0..TILE_VOXELS {
            for x in 0..TILE_VOXELS {
                let d = (((x as f32 - c).powi(2) + (y as f32 - c).powi(2)).sqrt() / c).min(1.0);
                let bumpy = (hash(tile.x * 64 + x, tile.y * 64 + y, 5) % 3) as i32;
                let keep = 10 * VS - (d * 7.0) as i32 - bumpy;
                for z in keep.max(GROUND_TOP + 3)..(10 * VS) {
                    world.set_voxel(o + IVec3::new(x, y, z), Voxel::EMPTY);
                }
            }
        }
    };
    // A great fieldstone: an irregular half-buried dome, not a die.
    let boulder_at = move |world: &mut VoxelWorld, tile: IVec3, mat: Voxel| {
        let o = tile * TILE_VOXELS;
        let c = TILE_VOXELS as f32 / 2.0;
        let r = 11.0 + (hash(tile.x, tile.y, 7) % 4) as f32;
        for z in GROUND_TOP..(GROUND_TOP + r as i32) {
            for y in 0..TILE_VOXELS {
                for x in 0..TILE_VOXELS {
                    let dz = (z - GROUND_TOP) as f32 * 1.35;
                    let wob = (hash(x * 3 + z, y * 3 - z, 9) % 5) as f32 * 0.5;
                    let d = ((x as f32 - c).powi(2) + (y as f32 - c).powi(2) + dz * dz).sqrt();
                    if d + wob < r {
                        world.set_voxel(o + IVec3::new(x, y, z), mat);
                    }
                }
            }
        }
    };
    // A hedge, wall stub, or drift: a shaped bar with gnawed edges.
    let bar_at = move |world: &mut VoxelWorld, tile: IVec3, mat: Voxel, top: i32, along_x: bool| {
        let o = tile * TILE_VOXELS;
        let (w0, w1) = (TILE_VOXELS / 2 - 6, TILE_VOXELS / 2 + 6);
        let (min, max) = if along_x {
            (IVec3::new(0, w0, GROUND_TOP), IVec3::new(TILE_VOXELS, w1, top * VS))
        } else {
            (IVec3::new(w0, 0, GROUND_TOP), IVec3::new(w1, TILE_VOXELS, top * VS))
        };
        world.fill_box(o + min, o + max, mat);
        // Gnaw the crown and flanks so the bar reads grown, not extruded.
        for i in 0..TILE_VOXELS {
            let drop = (hash(tile.x * 64 + i, tile.y, 13) % 4) as i32;
            let pinch = (hash(tile.y * 64 + i, tile.x, 17) % 3) as i32;
            for d in 0..drop {
                let z = top * VS - 1 - d;
                for w in (w0 - 1)..=(w1 + 1) {
                    let p = if along_x { IVec3::new(i, w, z) } else { IVec3::new(w, i, z) };
                    world.set_voxel(o + p, Voxel::EMPTY);
                }
            }
            for d in 0..pinch {
                let (wa, wb) = (w0 + d, w1 - 1 - d);
                for z in (GROUND_TOP + 6)..(top * VS) {
                    let (pa, pb) = if along_x {
                        (IVec3::new(i, wa, z), IVec3::new(i, wb, z))
                    } else {
                        (IVec3::new(wa, i, z), IVec3::new(wb, i, z))
                    };
                    if d == pinch - 1 && z % 3 != 0 {
                        continue;
                    }
                    world.set_voxel(o + pa, Voxel::EMPTY);
                    world.set_voxel(o + pb, Voxel::EMPTY);
                }
            }
        }
    };
    // An oak: a real trunk under a lobed canopy. The trunk's tile blocks;
    // the canopy rides above head height (neighbors keep their ground).
    let tree_at = move |world: &mut VoxelWorld, tile: IVec3| {
        let o = tile * TILE_VOXELS + IVec3::new(TILE_VOXELS / 2, TILE_VOXELS / 2, 0);
        world.fill_box(
            o + IVec3::new(-2, -2, GROUND_TOP),
            o + IVec3::new(2, 2, TILE_VOXELS + 4),
            MAT_TIMBER,
        );
        // Root flare.
        world.fill_box(
            o + IVec3::new(-4, -4, GROUND_TOP),
            o + IVec3::new(4, 4, GROUND_TOP + 3),
            MAT_TIMBER,
        );
        // Canopy: overlapping leaf-blobs, clamped above the head band so
        // the tiles around the trunk stay walkable underneath.
        for (bx, by, bz, r) in [
            (0i32, 0i32, TILE_VOXELS + 8, 9i32),
            (-6, 3, TILE_VOXELS + 6, 7),
            (5, -4, TILE_VOXELS + 7, 7),
            (2, 6, TILE_VOXELS + 10, 6),
        ] {
            let center = o + IVec3::new(bx, by, bz);
            for z in -r..=r {
                for y in -r..=r {
                    for x in -r..=r {
                        let wob = (hash(x * 5 + z, y * 5 - z, 19) % 4) as f32 * 0.6;
                        let d = ((x * x + y * y) as f32 + (z as f32 * 1.3).powi(2)).sqrt();
                        let p = center + IVec3::new(x, y, z);
                        if d + wob < r as f32 && p.z >= TILE_VOXELS {
                            world.set_voxel(p, MAT_FOLIAGE);
                        }
                    }
                }
            }
        }
    };
    let roll_open = |world: &VoxelWorld, rng: &mut crate::SimRng| -> Option<IVec3> {
        for _ in 0..12 {
            let tile = IVec3::new(5 + rng.roll(15) as i32, 1 + rng.roll(22) as i32, 0);
            if is_open(world, tile) {
                return Some(tile);
            }
        }
        None
    };
    match biome {
        Biome::Temperate => {
            // Hedgerows, old rubble, fieldstones, and oaks.
            for _ in 0..4 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    let along_x = rng.roll(2) == 0;
                    let step =
                        if along_x { IVec3::new(1, 0, 0) } else { IVec3::new(0, 1, 0) };
                    for i in 0..2 + rng.roll(2) as i32 {
                        let seg = t + step * i;
                        if is_open(&world, seg) {
                            bar_at(&mut world, seg, MAT_FOLIAGE, 11, along_x);
                        }
                    }
                }
            }
            for _ in 0..2 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    mound_at(&mut world, t, MAT_RUBBLE);
                }
            }
            for _ in 0..2 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    boulder_at(&mut world, t, MAT_STONE);
                }
            }
            for _ in 0..3 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    tree_at(&mut world, t);
                }
            }
        }
        Biome::Desert => {
            // Dunes to climb, dry-stone stubs, and sun-split boulders.
            for _ in 0..7 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    mound_at(&mut world, t, MAT_SAND);
                }
            }
            for _ in 0..3 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    let along_x = rng.roll(2) == 0;
                    let step =
                        if along_x { IVec3::new(1, 0, 0) } else { IVec3::new(0, 1, 0) };
                    for i in 0..2 {
                        let seg = t + step * i;
                        if is_open(&world, seg) {
                            bar_at(&mut world, seg, MAT_STONE, 12, along_x);
                        }
                    }
                }
            }
            for _ in 0..2 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    boulder_at(&mut world, t, MAT_STONE);
                }
            }
        }
        Biome::Jungle => {
            // Trees: a blocking trunk with a walkable canopy roof above head
            // height — gargoyles perch on treetops, soldiers slip beneath.
            for _ in 0..8 {
                let Some(t) = roll_open(&world, &mut rng) else { continue };
                let o = t * TILE_VOXELS;
                world.fill_box(
                    o + IVec3::new(6 * VS, 6 * VS, GROUND_TOP),
                    o + IVec3::new(10 * VS, 10 * VS, TILE_VOXELS),
                    MAT_TIMBER,
                );
                for cy in -1..=1 {
                    for cx in -1..=1 {
                        let c = t + IVec3::new(cx, cy, 0);
                        if (0..MAP_TILES.x).contains(&c.x) && (0..MAP_TILES.y).contains(&c.y) {
                            let co = c * TILE_VOXELS;
                            world.fill_box(
                                co + IVec3::new(2 * VS, 2 * VS, TILE_VOXELS),
                                co + IVec3::new(14 * VS, 14 * VS, TILE_VOXELS + GROUND_TOP),
                                MAT_FOLIAGE,
                            );
                        }
                    }
                }
            }
        }
        Biome::Tundra => {
            // Snowdrifts and ice boulders on hard white ground.
            for _ in 0..5 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    mound_at(&mut world, t, MAT_SNOW);
                }
            }
            for _ in 0..4 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    boulder_at(&mut world, t, MAT_GLINT);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // Map furniture: the small litter of interrupted lives. All of it
    // destructible, most of it cover.
    let cart_at = |world: &mut VoxelWorld, t: IVec3| {
        let o = t * TILE_VOXELS;
        // Bed and sideboards, up at axle height.
        world.fill_box(
            o + IVec3::new(3 * VS, 5 * VS, GROUND_TOP + 2 * VS),
            o + IVec3::new(13 * VS, 11 * VS, GROUND_TOP + 5 * VS),
            MAT_TIMBER,
        );
        // Wheels.
        for (wx, wy) in [(4, 4), (4, 11), (11, 4), (11, 11)] {
            world.fill_box(
                o + IVec3::new(wx * VS, wy * VS, GROUND_TOP),
                o + IVec3::new((wx + 2) * VS, (wy + 1) * VS, GROUND_TOP + 3 * VS),
                MAT_TIMBER,
            );
        }
    };
    let grave_at = |world: &mut VoxelWorld, t: IVec3| {
        let o = t * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(6 * VS, 7 * VS, GROUND_TOP),
            o + IVec3::new(10 * VS, 9 * VS, GROUND_TOP + 6 * VS),
            MAT_WALL,
        );
    };
    let scarecrow_at = |world: &mut VoxelWorld, t: IVec3| {
        let o = t * TILE_VOXELS;
        // The post and crossbar...
        world.fill_box(
            o + IVec3::new(7 * VS, 7 * VS, GROUND_TOP),
            o + IVec3::new(9 * VS, 9 * VS, GROUND_TOP + 11 * VS),
            MAT_TIMBER,
        );
        world.fill_box(
            o + IVec3::new(3 * VS, 7 * VS, GROUND_TOP + 8 * VS),
            o + IVec3::new(13 * VS, 9 * VS, GROUND_TOP + 9 * VS),
            MAT_TIMBER,
        );
        // ...and a head nobody made from straw.
        world.fill_box(
            o + IVec3::new(7 * VS, 7 * VS, GROUND_TOP + 11 * VS),
            o + IVec3::new(9 * VS, 9 * VS, GROUND_TOP + 13 * VS),
            MAT_GORE,
        );
    };
    let stall_at = |world: &mut VoxelWorld, t: IVec3| {
        let o = t * TILE_VOXELS;
        for (px, py) in [(2, 2), (13, 2), (2, 13), (13, 13)] {
            world.fill_box(
                o + IVec3::new(px * VS, py * VS, GROUND_TOP),
                o + IVec3::new((px + 1) * VS, (py + 1) * VS, GROUND_TOP + 12 * VS),
                MAT_TIMBER,
            );
        }
        // The awning: head-height cover to duck beneath.
        world.fill_box(
            o + IVec3::new(VS, VS, GROUND_TOP + 12 * VS),
            o + IVec3::new(15 * VS, 15 * VS, GROUND_TOP + 13 * VS),
            MAT_TIMBER,
        );
    };
    // A fence run with a stile gap in the middle: cover with a way through.
    let fence_from = |world: &mut VoxelWorld, t: IVec3, step: IVec3, len: i32| {
        for i in 0..len {
            if i == len / 2 {
                continue; // the stile
            }
            let seg = t + step * i;
            if !is_open(world, seg) {
                continue;
            }
            let o = seg * TILE_VOXELS;
            world.fill_box(
                o + IVec3::new(7 * VS, 7 * VS, GROUND_TOP),
                o + IVec3::new(9 * VS, 9 * VS, GROUND_TOP + 7 * VS),
                MAT_TIMBER,
            );
            let (rmin, rmax) = if step.x != 0 {
                (IVec3::new(0, 7 * VS, 0), IVec3::new(TILE_VOXELS, 9 * VS, 0))
            } else {
                (IVec3::new(7 * VS, 0, 0), IVec3::new(9 * VS, TILE_VOXELS, 0))
            };
            world.fill_box(
                o + rmin + IVec3::new(0, 0, GROUND_TOP + 4 * VS),
                o + rmax + IVec3::new(0, 0, GROUND_TOP + 5 * VS),
                MAT_TIMBER,
            );
        }
    };
    let roll_dir = |rng: &mut crate::SimRng| {
        if rng.roll(2) == 0 { IVec3::new(1, 0, 0) } else { IVec3::new(0, 1, 0) }
    };
    match biome {
        Biome::Temperate => {
            for _ in 0..2 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    let step = roll_dir(&mut rng);
                    fence_from(&mut world, t, step, 3 + rng.roll(2) as i32);
                }
            }
            if let Some(t) = roll_open(&world, &mut rng) {
                cart_at(&mut world, t);
            }
            if let Some(t) = roll_open(&world, &mut rng) {
                scarecrow_at(&mut world, t);
            }
            for _ in 0..2 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    grave_at(&mut world, t);
                }
            }
        }
        Biome::Desert => {
            if let Some(t) = roll_open(&world, &mut rng) {
                stall_at(&mut world, t);
            }
            if let Some(t) = roll_open(&world, &mut rng) {
                cart_at(&mut world, t);
            }
            if let Some(t) = roll_open(&world, &mut rng) {
                grave_at(&mut world, t);
            }
        }
        Biome::Jungle => {
            // An old shrine the canopy swallowed, and a rotting cart.
            if let Some(t) = roll_open(&world, &mut rng) {
                grave_at(&mut world, t);
            }
            if let Some(t) = roll_open(&world, &mut rng) {
                cart_at(&mut world, t);
            }
        }
        Biome::Tundra => {
            if let Some(t) = roll_open(&world, &mut rng) {
                let step = roll_dir(&mut rng);
                fence_from(&mut world, t, step, 3);
            }
            for _ in 0..2 {
                if let Some(t) = roll_open(&world, &mut rng) {
                    grave_at(&mut world, t);
                }
            }
        }
    }

    soldiers.truncate(ORDER_SPAWNS.len());
    let mut units = Vec::new();
    for (i, mut s) in soldiers.into_iter().enumerate() {
        s.id = crate::units::UnitId(units.len() as u32);
        let (x, y) = ORDER_SPAWNS[i];
        s.tile = IVec3::new(x, y, 0);
        units.push(s);
    }
    units.extend(demon_pack(demon_count, strength, units.len() as u32, &DEMON_SPAWNS));

    // Gargoyles greet the squad the way stone does: from above. They start
    // perched on the watchtower roof.
    let mut perches = [IVec3::new(17, 4, 1), IVec3::new(18, 4, 1)].into_iter();
    for u in units
        .iter_mut()
        .filter(|u| u.species == crate::units::Species::Gargoyle)
    {
        if let Some(p) = perches.next() {
            u.tile = p;
        }
    }

    // Townsfolk sheltering inside the chapel walls.
    const CIV_SPAWNS: [(i32, i32); 4] = [(11, 10), (12, 13), (10, 12), (13, 10)];
    let civ_names = ["Aldwin", "Berta", "Cosmin", "Delia"];
    for i in 0..civilians.min(4) as usize {
        let (x, y) = CIV_SPAWNS[i];
        units.push(Unit::civilian(
            units.len() as u32,
            civ_names[i],
            IVec3::new(x, y, 0),
        ));
    }

    // Hang door leaves in the chapel doorways: thin slabs across the passage.
    let mut door_tiles = Vec::new();
    for (tx, ty) in [(9, 11), (14, 12)] {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(6 * VS, 0, GROUND_TOP),
            o + IVec3::new(10 * VS, TILE_VOXELS, 14 * VS),
            MAT_DOOR,
        );
        door_tiles.push(IVec3::new(tx, ty, 0));
    }

    // Terror sites are dressed with what the demons did before you came —
    // three set-piece vignettes, each a different composed horror. Finding
    // one hits the mind; cleansing the site is why you're here.
    let mut atrocity_tiles = Vec::new();
    if civilians > 0 {
        let mut arng = crate::SimRng::from_seed(seed ^ 0x0A7770C1);
        for vignette in 0..3 {
            for _try in 0..10 {
                let t = IVec3::new(6 + arng.roll(12) as i32, 2 + arng.roll(20) as i32, 0);
                // Never inside the shelter yard where the living still hide.
                if (9..=14).contains(&t.x) && (8..=15).contains(&t.y) {
                    continue;
                }
                let probe = t * TILE_VOXELS + IVec3::new(8, 8, GROUND_TOP + 1);
                if world.voxel(probe) != Voxel::EMPTY {
                    continue;
                }
                let o = t * TILE_VOXELS;
                match vignette {
                    // The gibbet: a post, and what hangs from it.
                    0 => {
                        world.fill_box(
                            o + IVec3::new(7 * VS, 7 * VS, GROUND_TOP),
                            o + IVec3::new(9 * VS, 9 * VS, GROUND_TOP + 12 * VS),
                            MAT_TIMBER,
                        );
                        world.fill_box(
                            o + IVec3::new(6 * VS, 6 * VS, GROUND_TOP + 6 * VS),
                            o + IVec3::new(10 * VS, 10 * VS, GROUND_TOP + 9 * VS),
                            MAT_GORE,
                        );
                        for (dx, dy) in [(3, 4), (11, 5), (5, 11), (12, 12), (8, 3), (4, 8)] {
                            world.set_voxel(
                                o + IVec3::new(dx * VS, dy * VS, GROUND_TOP - 1),
                                MAT_BLOOD,
                            );
                        }
                        for (dx, dy) in [(2, 2), (13, 2), (2, 13), (13, 13)] {
                            world.set_voxel(
                                o + IVec3::new(dx * VS, dy * VS, GROUND_TOP - 1),
                                MAT_SIGIL,
                            );
                        }
                    }
                    // The ravaged wagon: tipped, torn open, and a blood
                    // trail leading away to somewhere worse.
                    1 => {
                        world.fill_box(
                            o + IVec3::new(3 * VS, 5 * VS, GROUND_TOP),
                            o + IVec3::new(12 * VS, 11 * VS, GROUND_TOP + 4 * VS),
                            MAT_TIMBER,
                        );
                        world.fill_box(
                            o + IVec3::new(5 * VS, 6 * VS, GROUND_TOP + 4 * VS),
                            o + IVec3::new(10 * VS, 10 * VS, GROUND_TOP + 6 * VS),
                            MAT_GORE,
                        );
                        // The trail: surface blood walking east, two tiles.
                        for step in 0..(2 * TILE_VOXELS) / (3 * VS) {
                            let p = o
                                + IVec3::new(
                                    12 * VS + step * 3 * VS,
                                    8 * VS + ((step * 5) % 7 - 3) * VS / 2,
                                    GROUND_TOP - 1,
                                );
                            if world.voxel(p).is_solid() {
                                world.set_voxel(p, MAT_BLOOD);
                            }
                        }
                    }
                    // The kneeling ring: what's left of the congregation,
                    // arranged around a sigil that finished with them.
                    _ => {
                        for (dx, dy) in [(3, 8), (13, 8), (8, 3), (8, 13), (4, 4), (12, 12)] {
                            world.fill_box(
                                o + IVec3::new(dx * VS - VS, dy * VS - VS, GROUND_TOP),
                                o + IVec3::new(dx * VS + VS, dy * VS + VS, GROUND_TOP + 3 * VS),
                                MAT_GORE,
                            );
                        }
                        for (dx, dy) in [(7, 7), (8, 7), (7, 8), (8, 8), (6, 8), (9, 7)] {
                            world.set_voxel(
                                o + IVec3::new(dx * VS, dy * VS, GROUND_TOP - 1),
                                MAT_SIGIL,
                            );
                        }
                    }
                }
                atrocity_tiles.push(t);
                break;
            }
        }
    }

    let mut battle = Battle::new(world, IVec3::ZERO, MAP_TILES, units, seed);
    for tile in atrocity_tiles {
        battle.register_atrocity(tile);
    }
    for tile in door_tiles {
        battle.doors.push((tile, false));
    }
    for tile in hazard_casks {
        battle.register_cask(tile);
    }
    battle.set_objective(obelisk_min, obelisk_max);
    // The rift keeps giving: a second wave scribes in at the spawn ground
    // unless the obelisk falls first. Stronger months send more, sooner.
    if strength >= 2 {
        battle.schedule_summon(IVec3::new(DEMON_SPAWNS[4].0, DEMON_SPAWNS[4].1, 0), 4, strength);
    }
    if strength >= 5 {
        battle.schedule_summon(IVec3::new(DEMON_SPAWNS[5].0, DEMON_SPAWNS[5].1, 0), 4, strength);
    }
    if strength >= 7 {
        battle.schedule_summon(IVec3::new(DEMON_SPAWNS[6].0, DEMON_SPAWNS[6].1, 0), 7, strength);
    }
    // The mission rule, made concrete.
    match spec {
        MissionSpec::Standard => {}
        MissionSpec::Evacuate => {
            let pool = battle.units.iter().filter(|u| u.civilian).count() as u32;
            battle.rule = crate::battle::MissionRule::Evacuate {
                needed: (pool / 2).max(1),
                turns: 14,
            };
        }
        MissionSpec::Interrupt => {
            battle.rule = crate::battle::MissionRule::Interrupt { turns: 14 };
        }
        MissionSpec::Snatch => {
            // The mark: the pack's Overseer, or its first grunt promoted.
            let target = battle
                .units
                .iter()
                .find(|u| u.species == crate::units::Species::Overseer)
                .map(|u| u.id)
                .unwrap_or_else(|| {
                    let mark = battle
                        .units
                        .iter()
                        .find(|u| u.side == crate::units::Side::Demons && !u.civilian)
                        .map(|u| u.id)
                        .expect("an incursion has demons");
                    let tile = battle.units[mark.0 as usize].tile;
                    battle.units[mark.0 as usize] =
                        Unit::overseer(mark.0, "the Infiltrator", tile);
                    mark
                });
            battle.rule = crate::battle::MissionRule::Snatch { target };
        }
    }
    // Strong incursions keep the way open behind them: summoning circles
    // scribe themselves in the yard, burning where everyone can see them.
    if strength >= 4 {
        for (anchor, delay) in [(IVec3::new(17, 8, 0), 3), (IVec3::new(18, 15, 0), 5)] {
            if delay == 5 && strength < 7 {
                continue; // the second circle takes a stronger rift
            }
            if let Some(open) = nearest_walkable(&battle, anchor) {
                battle.schedule_summon(open, delay, strength);
            }
        }
    }
    battle
}

/// The Order's zeppelin, grounded at the west edge: a planked gondola with
/// gunwale-height timber sides (boots stop, bullets fly over), a mouth
/// facing the field flanked by witchfire lanterns, and a boarding ramp.
/// The squad deploys from its deck; evacuations end here — the west-edge
/// evacuation line IS the gondola.
fn build_gondola(world: &mut VoxelWorld) {
    let gunwale = TILE_VOXELS * 5 / 8;
    // Deck and ramp planking, set into the ground surface.
    for (tx, ty) in (0..=3)
        .flat_map(|tx| (10..=13).map(move |ty| (tx, ty)))
        .chain([(4, 11), (4, 12), (5, 11), (5, 12)])
    {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(0, 0, GROUND_TOP - 1),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, GROUND_TOP),
            MAT_TIMBER,
        );
    }
    // Gunwales around the hull; the mouth (east face, two tiles) stays open.
    for ty in 10..=13 {
        for tx in 0..=3 {
            let ring = tx == 0 || tx == 3 || ty == 10 || ty == 13;
            let mouth = tx == 3 && (ty == 11 || ty == 12);
            if ring && !mouth {
                let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
                world.fill_box(
                    o + IVec3::new(0, 0, GROUND_TOP),
                    o + IVec3::new(TILE_VOXELS, TILE_VOXELS, gunwale),
                    MAT_TIMBER,
                );
            }
        }
    }
    // Witchfire lanterns on the gunwale corners flanking the mouth.
    for ty in [10, 13] {
        let o = IVec3::new(3, ty, 0) * TILE_VOXELS;
        let ly = if ty == 10 { TILE_VOXELS - 3 * VS } else { VS };
        world.fill_box(
            o + IVec3::new(TILE_VOXELS - 3 * VS, ly, gunwale),
            o + IVec3::new(TILE_VOXELS - VS, ly + 2 * VS, gunwale + 3 * VS),
            MAT_WARD,
        );
    }
}

/// Sculpt the raw ground plane into a place: broad noise-blended patches
/// of second and third surface materials, shallow dips where the topsoil
/// gives way to turned earth, and one worn road wandering the long axis.
/// Everything happens AT or BELOW the ground top, so walkability, sight,
/// and pathfinding never move — only the eye does.
fn sculpt_ground(world: &mut VoxelWorld, seed: u64, biome: Biome) {
    let span = IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, 0);
    let hash01 = |x: i32, y: i32, k: u32| -> f32 {
        let mut h = (seed as u32)
            .wrapping_mul(747796405)
            .wrapping_add(x as u32)
            .wrapping_mul(2654435761)
            .wrapping_add(y as u32)
            .wrapping_mul(1274126177)
            .wrapping_add(k);
        h ^= h >> 15;
        ((h.wrapping_mul(2246822519) >> 8) & 0xFFFF) as f32 / 65535.0
    };
    // Smooth value noise on an 8-voxel lattice: patches, not confetti.
    let noise = |x: i32, y: i32, k: u32| -> f32 {
        let (gx, gy) = (x.div_euclid(8), y.div_euclid(8));
        let (fx, fy) = (x.rem_euclid(8) as f32 / 8.0, y.rem_euclid(8) as f32 / 8.0);
        let (a, b) = (hash01(gx, gy, k), hash01(gx + 1, gy, k));
        let (c, d) = (hash01(gx, gy + 1, k), hash01(gx + 1, gy + 1, k));
        // Smoothstep the lerp so lattice lines don't show.
        let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
        a + (b - a) * sx + (c - a) * sy + (a - b - c + d) * sx * sy
    };
    let (patch_a, patch_b) = match biome {
        Biome::Temperate => (MAT_EARTH, MAT_MEADOW),
        Biome::Jungle => (MAT_EARTH, MAT_FOLIAGE),
        Biome::Desert => (MAT_STONE, MAT_PATH),
        Biome::Tundra => (MAT_GLINT, MAT_EARTH),
    };
    let top = GROUND_TOP - 1;
    for y in 0..span.y {
        for x in 0..span.x {
            let p = IVec3::new(x, y, top);
            let n = noise(x, y, 11);
            if n > 0.64 {
                world.set_voxel(p, patch_a);
            } else if n < 0.24 {
                world.set_voxel(p, patch_b);
            }
            // Shallow dips: the topsoil gives way to darker turned earth,
            // one or two voxels down. Support stays; only the eye drops.
            let m = noise(x, y, 23);
            if m > 0.82 {
                world.set_voxel(p, Voxel::EMPTY);
                world.set_voxel(p - IVec3::Z, MAT_EARTH);
            }
        }
    }
    // The road: it was here before the war and will outlast it. It wanders
    // the long axis, and it refills any dip it crosses — roads are packed.
    let amp = span.y as f32 * 0.16;
    let base_y = span.y as f32 * (0.42 + 0.2 * hash01(7, 7, 31));
    for x in 0..span.x {
        let center = base_y
            + (x as f32 * 0.035 + hash01(3, 3, 41) * 6.0).sin() * amp
            + noise(x, 0, 53) * 6.0
            - 3.0;
        let half = 3 + (noise(x, 9, 61) * 3.0) as i32;
        for dy in -half..=half {
            let y = center as i32 + dy;
            if y < 0 || y >= span.y {
                continue;
            }
            let p = IVec3::new(x, y, top);
            world.set_voxel(p, MAT_PATH);
            // Level the roadbed through any dip.
            world.set_voxel(p - IVec3::Z, MAT_PATH);
        }
    }
}

/// The 1994 ground texture, in voxels: every tile gets a seeded scatter of
/// surface specks — tufts and wildflowers on grass, scree on sand, frost
/// glints on snow — set INTO the ground surface so walkability never moves.
fn stipple_ground(world: &mut VoxelWorld, seed: u64, biome: Biome) {
    let hash = |x: i32, y: i32, k: u32| -> u32 {
        let mut h = (seed as u32)
            .wrapping_mul(747796405)
            .wrapping_add(x as u32)
            .wrapping_mul(2654435761)
            .wrapping_add(y as u32)
            .wrapping_mul(1274126177)
            .wrapping_add(k);
        h ^= h >> 15;
        h.wrapping_mul(2246822519) >> 8
    };
    // (per-tile speck count, primary, accent, accent frequency)
    let (count, primary, accent, accent_every) = match biome {
        Biome::Temperate => (10, MAT_TUFT, MAT_FLOWER, 6),
        Biome::Jungle => (14, MAT_TUFT, MAT_FLOWER, 9),
        Biome::Desert => (6, MAT_RUBBLE, MAT_TIMBER, 8),
        Biome::Tundra => (6, MAT_GLINT, MAT_RUBBLE, 7),
    };
    for ty in 0..MAP_TILES.y {
        for tx in 0..MAP_TILES.x {
            let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
            for k in 0..count {
                let hx = (hash(tx, ty, k * 3) % TILE_VOXELS as u32) as i32;
                let hy = (hash(tx, ty, k * 3 + 1) % TILE_VOXELS as u32) as i32;
                let p = o + IVec3::new(hx, hy, GROUND_TOP - 1);
                if world.voxel(p).is_solid() {
                    let mat = if hash(tx, ty, k * 3 + 2) % accent_every == 0 {
                        accent
                    } else {
                        primary
                    };
                    world.set_voxel(p, mat);
                }
            }
        }
    }
}

/// The nearest walkable tile to an anchor (spiral out to radius 2).
fn nearest_walkable(battle: &Battle, anchor: IVec3) -> Option<IVec3> {
    for r in 0..=2 {
        for dy in -r..=r {
            for dx in -r..=r {
                let t = anchor + IVec3::new(dx, dy, 0);
                if battle.tiles.is_walkable(t) {
                    return Some(t);
                }
            }
        }
    }
    None
}

/// A demon warren: tunnels gnawed through living flesh, with the nest-heart
/// pulsing at the deep end. Demolish the heart or kill everything.
pub fn nest_map(seed: u64, mut soldiers: Vec<Unit>, demon_count: u32, strength: u32) -> Battle {
    let mut world = VoxelWorld::new();
    let span = IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, GROUND_TOP);
    world.fill_box(IVec3::ZERO, span, MAT_GROUND);
    // Solid flesh, then gnaw the warren out of it.
    world.fill_box(
        IVec3::new(0, 0, GROUND_TOP),
        IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, 14 * VS),
        MAT_FLESH,
    );
    let carve_tile = |world: &mut VoxelWorld, tx: i32, ty: i32| {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(0, 0, GROUND_TOP),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, 14 * VS),
            Voxel::EMPTY,
        );
    };
    // Main gullet, winding east, with side chambers.
    let mut rng = crate::SimRng::from_seed(seed);
    let mut y = 11i32;
    for x in 1..22 {
        carve_tile(&mut world, x, y);
        carve_tile(&mut world, x, y + 1);
        if x % 3 == 0 {
            y = (y + rng.roll(3) as i32 - 1).clamp(3, 19);
        }
        if x % 6 == 2 {
            for cy in (y - 2)..=(y + 3) {
                carve_tile(&mut world, x, cy.clamp(1, 22));
            }
        }
    }
    for (cx, cy) in [(5, 5), (12, 18), (19, 6)] {
        for tx in cx - 1..=cx + 1 {
            for ty in cy - 1..=cy + 1 {
                carve_tile(&mut world, tx, ty);
            }
        }
        // A tunnel back to the gullet.
        for ty in cy.min(y)..=cy.max(y) {
            carve_tile(&mut world, cx, ty);
        }
    }
    // The nest-heart, deep east.
    let heart_min = IVec3::new(21 * TILE_VOXELS, 11 * TILE_VOXELS, GROUND_TOP);
    let heart_max = IVec3::new(22 * TILE_VOXELS, 13 * TILE_VOXELS, 20 * VS);
    for tx in 20..=22 {
        for ty in 10..=13 {
            carve_tile(&mut world, tx, ty);
        }
    }
    world.fill_box(heart_min, heart_max, MAT_FLESH);

    soldiers.truncate(4);
    let mut units = Vec::new();
    for (i, mut s) in soldiers.into_iter().enumerate() {
        s.id = crate::units::UnitId(units.len() as u32);
        s.tile = IVec3::new(1, 11 + (i as i32 % 2), 0);
        units.push(s);
    }
    let spawns: [(i32, i32); 8] =
        [(5, 5), (12, 18), (19, 6), (20, 10), (20, 13), (12, 19), (5, 4), (19, 7)];
    units.extend(demon_pack(demon_count, strength, units.len() as u32, &spawns));

    let mut battle = Battle::new(world, IVec3::ZERO, MAP_TILES, units, seed);
    battle.set_objective(heart_min, heart_max);
    // The warren is theirs: its garrison holds the dark and waits.
    battle.demons_hold = true;
    battle
}

/// The Otherside: obsidian, ash, and burning ground under no sun.
pub fn otherside(seed: u64, mut soldiers: Vec<Unit>, demon_count: u32, strength: u32) -> Battle {
    let mut world = VoxelWorld::new();
    world.fill_box(
        IVec3::ZERO,
        IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, GROUND_TOP),
        MAT_OBSIDIAN,
    );
    // Obsidian spires and ash drifts.
    for (tx, ty) in [(6, 5), (9, 12), (14, 7), (16, 16), (11, 19), (18, 3), (7, 18)] {
        fill_tile_walls(&mut world, IVec3::new(tx, ty, 0), MAT_OBSIDIAN);
    }
    for (tx, ty) in [(5, 10), (12, 4), (15, 12), (10, 16), (18, 9)] {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(2 * VS, 2 * VS, GROUND_TOP),
            o + IVec3::new(14 * VS, 14 * VS, 10 * VS),
            MAT_RUBBLE,
        );
    }
    // Brimstone seeps everywhere here.
    let pool_tiles = [(8, 8), (13, 14), (17, 6), (6, 14), (15, 18), (19, 12)];
    for &(tx, ty) in &pool_tiles {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(VS, VS, GROUND_TOP - 1),
            o + IVec3::new(15 * VS, 15 * VS, GROUND_TOP),
            MAT_POOL,
        );
    }
    // The throne of the Name.
    let throne_min = IVec3::new(22 * TILE_VOXELS, 11 * TILE_VOXELS, GROUND_TOP);
    let throne_max = IVec3::new(23 * TILE_VOXELS, 13 * TILE_VOXELS, 26 * VS);
    world.fill_box(throne_min, throne_max, MAT_FLESH);
    carve_runes(&mut world, throne_min, throne_max);

    soldiers.truncate(ORDER_SPAWNS.len());
    let mut units = Vec::new();
    for (i, mut s) in soldiers.into_iter().enumerate() {
        s.id = crate::units::UnitId(units.len() as u32);
        let (x, y) = ORDER_SPAWNS[i];
        s.tile = IVec3::new(x, y, 0);
        units.push(s);
    }
    units.extend(demon_pack(demon_count, strength, units.len() as u32, &DEMON_SPAWNS));

    let mut battle = Battle::new(world, IVec3::ZERO, MAP_TILES, units, seed);
    for &(tx, ty) in &pool_tiles {
        battle.register_pool(IVec3::new(tx, ty, 0));
    }
    battle.set_objective(throne_min, throne_max);
    // The throne is scripture; the court keeps a circle burning before it.
    if let Some(open) = nearest_walkable(&battle, IVec3::new(19, 12, 0)) {
        battle.schedule_summon(open, 4, strength);
    }
    battle
}

/// A corrupted patron's manor: room-to-room work under chandeliers. Built
/// on the base-defense generator's bones; half the garrison are CULTISTS —
/// human traitors in human shapes, fighting for the other side.
pub fn manor_purge(seed: u64, soldiers: Vec<Unit>, demon_count: u32) -> Battle {
    const MANOR: [(usize, usize); 9] =
        [(1, 1), (2, 1), (1, 2), (2, 2), (3, 2), (2, 3), (3, 3), (4, 2), (4, 3)];
    let mut battle = base_defense(seed, soldiers, demon_count, &MANOR, (4, 3));
    // The house is theirs: cultists hold their rooms and bank the shot.
    battle.demons_hold = true;
    // Every second demon is a turned servant of the house.
    let mut cultist = 0;
    for u in &mut battle.units {
        if u.side == crate::units::Side::Demons && !u.civilian {
            cultist += 1;
            if cultist % 2 == 0 {
                let (id, tile) = (u.id.0, u.tile);
                let mut c = Unit::soldier(id, &format!("Cultist {}", cultist / 2), tile);
                c.side = crate::units::Side::Demons;
                c.weapon = crate::units::hellspit();
                c.bravery = 80; // faith of a kind
                c.armor_front = 0;
                c.armor_side = 0;
                c.armor_rear = 0;
                *u = c;
            }
        }
    }
    battle
}

/// Tiles per chapterhouse grid cell in a base-defense map.
const CELL_TILES: i32 = 4;

/// Build a base-defense battle from the chapterhouse layout: each occupied
/// facility cell becomes a 2x2-tile room carved out of solid rock, connected
/// by doorways to adjacent occupied cells. Demons breach through the
/// gatehouse; defenders muster in the deepest rooms. Fighting happens in
/// *your* floor plan — base architecture is fortress design.
pub fn base_defense(
    seed: u64,
    soldiers: Vec<Unit>,
    demon_count: u32,
    cells: &[(usize, usize)],
    gate: (usize, usize),
) -> Battle {
    // Untagged floor plans (the manor, old callers) dress their rooms as
    // a lived-in household: hall, library, chapel, cellar, and so on.
    const HOUSEHOLD: [RoomKind; 5] = [
        RoomKind::Quarters,
        RoomKind::Library,
        RoomKind::Chapel,
        RoomKind::Vault,
        RoomKind::Infirmary,
    ];
    let rooms: Vec<(i32, i32, RoomKind)> = cells
        .iter()
        .enumerate()
        .map(|(i, &(x, y))| {
            let kind = if (x, y) == gate {
                RoomKind::Gatehouse
            } else {
                HOUSEHOLD[i % HOUSEHOLD.len()]
            };
            (x as i32, y as i32, kind)
        })
        .collect();
    let spec = DefenseSpec {
        rooms: &rooms,
        gate: (gate.0 as i32, gate.1 as i32),
        wards: 2,
        hounds: 0,
        breach: None,
        behemoth: false,
    };
    base_defense_fortified(seed, soldiers, demon_count, &spec)
}

/// What a chapterhouse cell was in peacetime — the fight dresses each
/// room to match.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    Gatehouse,
    Quarters,
    Augury,
    Library,
    Infirmary,
    Workshop,
    Chapel,
    Sanctum,
    DrillYard,
    WardTower,
    Kennel,
    Vault,
}

/// Everything the campaign knows about the house under attack.
pub struct DefenseSpec<'a> {
    /// Cells reachable from the gate, each tagged with what stands there.
    pub rooms: &'a [(i32, i32, RoomKind)],
    pub gate: (i32, i32),
    /// Baseline standing wards chalked along the entry approach.
    pub wards: u32,
    pub hounds: u32,
    /// A cell whose outer wall has been smashed open: the assault pours
    /// in here instead of the gate.
    pub breach: Option<(i32, i32)>,
    /// The wall-breaker itself leads the assault.
    pub behemoth: bool,
}

/// Dress a room as the facility it was the day before the alarm. Props
/// stay inside the room's far corner tile so the junction tile and both
/// doorway lanes remain clear.
fn furnish_room(world: &mut VoxelWorld, cx: i32, cy: i32, kind: RoomKind) {
    let o = IVec3::new(cx * CELL_TILES + 2, cy * CELL_TILES + 2, 0) * TILE_VOXELS;
    let g = GROUND_TOP;
    let t = TILE_VOXELS;
    let mut prop = |min: IVec3, max: IVec3, mat: Voxel| {
        world.fill_box(
            o + IVec3::new(min.x, min.y, g + min.z),
            o + IVec3::new(max.x, max.y, g + max.z),
            mat,
        );
    };
    match kind {
        RoomKind::Gatehouse => {
            // The way in stays clear; one salt-scoured brazier watches it.
            prop(IVec3::new(4, 4, 0), IVec3::new(8, 8, 5 * VS), MAT_WALL);
            prop(IVec3::new(4, 4, 5 * VS), IVec3::new(8, 8, 6 * VS), MAT_SIGIL);
        }
        RoomKind::Quarters => {
            // Two bunks, blankets still thrown back.
            prop(IVec3::new(4, t - 12, 0), IVec3::new(18, t - 4, 3 * VS), MAT_TIMBER);
            prop(IVec3::new(t - 14, 4, 0), IVec3::new(t - 4, 12, 3 * VS), MAT_TIMBER);
            prop(IVec3::new(5, t - 11, 3 * VS), IVec3::new(9, t - 5, 4 * VS), MAT_SNOW);
        }
        RoomKind::Augury => {
            // The listening stone, banded with script.
            prop(IVec3::new(12, 12, 0), IVec3::new(20, 20, 10 * VS), MAT_OBELISK);
            prop(IVec3::new(12, 12, 10 * VS), IVec3::new(20, 20, 11 * VS), MAT_SIGIL);
        }
        RoomKind::Library => {
            // Shelf walls of dry old timber. It burns like confession.
            prop(IVec3::new(4, t - 8, 0), IVec3::new(t - 4, t - 3, 8 * VS), MAT_TIMBER);
            prop(IVec3::new(t - 8, 4, 0), IVec3::new(t - 3, t - 10, 8 * VS), MAT_TIMBER);
        }
        RoomKind::Infirmary => {
            // Two cots in clean linen, one not so clean.
            prop(IVec3::new(4, 6, 0), IVec3::new(16, 14, 2 * VS), MAT_SNOW);
            prop(IVec3::new(4, t - 14, 0), IVec3::new(16, t - 6, 2 * VS), MAT_SNOW);
            prop(IVec3::new(8, t - 12, 2 * VS), IVec3::new(13, t - 8, 2 * VS + 1), MAT_BLOOD);
        }
        RoomKind::Workshop => {
            // A bench, and a powder cask nobody moved to safety in time.
            prop(IVec3::new(4, t - 12, 0), IVec3::new(t - 8, t - 5, 3 * VS), MAT_TIMBER);
            prop(IVec3::new(t - 12, 4, 0), IVec3::new(t - 5, 11, 6 * VS), MAT_CASK);
        }
        RoomKind::Chapel => {
            // The altar, and candles that never go out.
            prop(IVec3::new(8, t - 12, 0), IVec3::new(24, t - 5, 4 * VS), MAT_OBELISK);
            prop(IVec3::new(10, t - 10, 4 * VS), IVec3::new(12, t - 8, 6 * VS), MAT_WARD);
            prop(IVec3::new(20, t - 10, 4 * VS), IVec3::new(22, t - 8, 6 * VS), MAT_WARD);
        }
        RoomKind::Sanctum => {
            // The corners of a chalked meditation ring, knee-high.
            for (dx, dy) in [(8, 8), (t - 12, 8), (8, t - 12), (t - 12, t - 12)] {
                prop(IVec3::new(dx, dy, 0), IVec3::new(dx + 4, dy + 4, VS), MAT_SIGIL);
            }
        }
        RoomKind::DrillYard => {
            // Sparring posts, notched by years of blades.
            prop(IVec3::new(8, 8, 0), IVec3::new(11, 11, 6 * VS), MAT_TIMBER);
            prop(IVec3::new(t - 11, t - 11, 0), IVec3::new(t - 8, t - 8, 6 * VS), MAT_TIMBER);
        }
        RoomKind::WardTower => {
            // The tower's warded heart.
            prop(IVec3::new(13, 13, 0), IVec3::new(19, 19, 9 * VS), MAT_WALL);
            prop(IVec3::new(13, 13, 9 * VS), IVec3::new(19, 19, 10 * VS), MAT_WARD);
        }
        RoomKind::Kennel => {
            // Straw, a low fence, and the smell of blessed dog.
            prop(IVec3::new(4, 4, 0), IVec3::new(t - 4, t - 4, 1), MAT_TUFT);
            prop(IVec3::new(4, t - 6, 0), IVec3::new(t - 4, t - 4, 3 * VS), MAT_TIMBER);
        }
        RoomKind::Vault => {
            // Casks and strongboxes; something in them still glints.
            prop(IVec3::new(4, t - 12, 0), IVec3::new(12, t - 4, 6 * VS), MAT_CASK);
            prop(IVec3::new(14, t - 10, 0), IVec3::new(20, t - 4, 5 * VS), MAT_CASK);
            prop(IVec3::new(t - 10, 4, 0), IVec3::new(t - 6, 8, 3 * VS), MAT_GLINT);
        }
    }
}

pub fn base_defense_fortified(
    seed: u64,
    mut soldiers: Vec<Unit>,
    demon_count: u32,
    spec: &DefenseSpec,
) -> Battle {
    let grid = 6i32;
    let map_tiles = IVec3::new(grid * CELL_TILES, grid * CELL_TILES, 1);
    let mut world = VoxelWorld::new();

    // Ground slab, then solid rock at torso height everywhere.
    world.fill_box(
        IVec3::ZERO,
        IVec3::new(map_tiles.x * TILE_VOXELS, map_tiles.y * TILE_VOXELS, GROUND_TOP),
        MAT_GROUND,
    );
    world.fill_box(
        IVec3::new(0, 0, GROUND_TOP),
        IVec3::new(map_tiles.x * TILE_VOXELS, map_tiles.y * TILE_VOXELS, 14 * VS),
        MAT_WALL,
    );

    let clear_tile = |world: &mut VoxelWorld, tx: i32, ty: i32| {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(0, 0, GROUND_TOP),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, 14 * VS),
            Voxel::EMPTY,
        );
    };

    let occupied: std::collections::HashSet<(i32, i32)> =
        spec.rooms.iter().map(|&(x, y, _)| (x, y)).collect();

    // Carve 2x2 room interiors.
    for &(cx, cy, _) in spec.rooms {
        for dy in 1..=2 {
            for dx in 1..=2 {
                clear_tile(&mut world, cx * CELL_TILES + dx, cy * CELL_TILES + dy);
            }
        }
    }
    // Carve doorways between adjacent occupied cells.
    for &(cx, cy, _) in spec.rooms {
        if occupied.contains(&(cx + 1, cy)) {
            let row = cy * CELL_TILES + 1;
            clear_tile(&mut world, cx * CELL_TILES + 3, row);
            clear_tile(&mut world, (cx + 1) * CELL_TILES, row);
        }
        if occupied.contains(&(cx, cy + 1)) {
            let col = cx * CELL_TILES + 1;
            clear_tile(&mut world, col, cy * CELL_TILES + 3);
            clear_tile(&mut world, col, (cy + 1) * CELL_TILES);
        }
    }

    // Dress every room as the facility it was yesterday.
    for &(cx, cy, kind) in spec.rooms {
        furnish_room(&mut world, cx, cy, kind);
    }

    // The breach: where the wall-breaker came through, masonry is a
    // memory. Two wall tiles come down to rubble shoulders.
    if let Some((bx, by)) = spec.breach {
        for dy in 1..=2 {
            let (tx, ty) = (bx * CELL_TILES + 3, by * CELL_TILES + dy);
            clear_tile(&mut world, tx, ty);
            let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
            world.fill_box(
                o + IVec3::new(2, 3, GROUND_TOP),
                o + IVec3::new(12, 12, GROUND_TOP + 2 * VS),
                MAT_RUBBLE,
            );
            world.fill_box(
                o + IVec3::new(TILE_VOXELS - 12, TILE_VOXELS - 10, GROUND_TOP),
                o + IVec3::new(TILE_VOXELS - 3, TILE_VOXELS - 2, GROUND_TOP + VS),
                MAT_RUBBLE,
            );
        }
    }

    // Deployment: breadth-first flood from where the enemy enters — the
    // gatehouse, or the smashed wall. Demons pour in nearest the entry;
    // defenders hold the deepest rooms.
    let tiles = crate::tiles::TileMap::derive(&world, IVec3::ZERO, map_tiles);
    let entry = spec.breach.unwrap_or(spec.gate);
    let start = IVec3::new(entry.0 * CELL_TILES + 1, entry.1 * CELL_TILES + 1, 0);
    let mut order = vec![start];
    let mut seen = std::collections::HashSet::from([start]);
    let mut head = 0;
    while head < order.len() {
        let cur = order[head];
        head += 1;
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = cur + IVec3::new(dx, dy, 0);
            if tiles.is_walkable(next) && seen.insert(next) {
                order.push(next);
            }
        }
    }

    let mut ward_tiles = Vec::new();
    for k in 0..spec.wards as usize {
        let idx = 5 + k * 3;
        if idx < order.len().saturating_sub(8) {
            ward_tiles.push(order[idx]);
        }
    }
    // Every Ward Tower's floor was chalked years ago, for exactly today.
    for &(cx, cy, kind) in spec.rooms {
        if kind == RoomKind::WardTower {
            for (dx, dy) in [(1, 1), (2, 1), (1, 2)] {
                let tile = IVec3::new(cx * CELL_TILES + dx, cy * CELL_TILES + dy, 0);
                if tiles.is_walkable(tile) {
                    ward_tiles.push(tile);
                }
            }
        }
    }

    let demon_names = ["Wrath", "Envy", "Gluttony", "Sloth", "Pride", "Greed", "Lust", "Despair"];
    let mut units = Vec::new();
    soldiers.truncate(8);
    let defenders = soldiers.len();
    for (i, mut s) in soldiers.into_iter().enumerate() {
        s.id = crate::units::UnitId(i as u32);
        s.tile = order[order.len() - 1 - i];
        units.push(s);
    }
    let demon_count =
        (demon_count as usize).min(8).min(order.len().saturating_sub(defenders));
    let lead = if spec.behemoth && demon_count > 0 {
        units.push(Unit::behemoth(defenders as u32, "Doorbreaker", order[0]));
        1
    } else {
        0
    };
    for i in lead..demon_count {
        units.push(Unit::imp(
            (defenders + i) as u32,
            &format!("Imp of {}", demon_names[i]),
            order[i],
        ));
    }
    // The kennels open: blessed hounds hold their own runs if they can,
    // and fall back beside the garrison if the runs are contested.
    let kennel_tiles: Vec<IVec3> = spec
        .rooms
        .iter()
        .filter(|r| r.2 == RoomKind::Kennel)
        .flat_map(|&(cx, cy, _)| {
            [(2, 1), (1, 2)]
                .map(|(dx, dy)| IVec3::new(cx * CELL_TILES + dx, cy * CELL_TILES + dy, 0))
        })
        .filter(|tile| {
            tiles.is_walkable(*tile)
                && !order[..demon_count].contains(tile)
                && !order[order.len() - defenders..].contains(tile)
        })
        .collect();
    for h in 0..spec.hounds as usize {
        let tile = kennel_tiles.get(h).copied().or_else(|| {
            let idx = order.len().saturating_sub(defenders + h + 1);
            (idx >= demon_count).then(|| order[idx])
        });
        let Some(tile) = tile else { break };
        let id = units.len() as u32;
        let mut hound = Unit::hellhound(id, &format!("Blessed Hound {}", h + 1), tile);
        hound.side = crate::units::Side::Order;
        units.push(hound);
    }

    let mut battle = Battle::new(world, IVec3::ZERO, map_tiles, units, seed);
    // The entry corridor is chalked and salted in advance: standing ward
    // lines the breach must cross before it reaches the halls.
    for tile in ward_tiles {
        battle.place_ward(tile);
    }
    battle
}

/// Band an objective column with glowing rune rings: every few voxels of
/// height, the column's outer shell burns sigil-crimson.
fn carve_runes(world: &mut VoxelWorld, min: IVec3, max: IVec3) {
    let mut z = min.z + 4 * VS;
    while z < max.z - 1 {
        for y in min.y..max.y {
            for x in min.x..max.x {
                let on_shell = x == min.x || x == max.x - 1 || y == min.y || y == max.y - 1;
                // A broken stitch pattern reads as script better than a band.
                if on_shell && (x + y + z) % 3 != 0 {
                    world.set_voxel(IVec3::new(x, y, z), MAT_SIGIL);
                }
            }
        }
        z += 5 * VS;
    }
}

/// Dress raw masonry so buildings read BUILT, not extruded: a stone
/// plinth at the boot, timber studs up the face, a lintel band under the
/// eave, and dark window reliefs recessed into the long runs. Purely a
/// recolor of solid voxels — walkability, sight, and cover never move.
fn dress_walls(world: &mut VoxelWorld, seed: u64) {
    let span = IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, 0);
    let hash = |x: i32, y: i32, k: u32| -> u32 {
        let mut h = (seed as u32)
            .wrapping_mul(747796405)
            .wrapping_add(x as u32)
            .wrapping_mul(2654435761)
            .wrapping_add(y as u32)
            .wrapping_mul(1274126177)
            .wrapping_add(k);
        h ^= h >> 15;
        h.wrapping_mul(2246822519) >> 8
    };
    let mid = GROUND_TOP + 6;
    for y in 0..span.y {
        for x in 0..span.x {
            let mat = world.voxel(IVec3::new(x, y, mid));
            if mat != MAT_WALL && mat != MAT_PLASTER && mat != MAT_STONE {
                continue;
            }
            let out_dir = [(1, 0), (-1, 0), (0, 1), (0, -1)].iter().copied().find(|&(dx, dy)| {
                world.voxel(IVec3::new(x + dx, y + dy, mid)) == Voxel::EMPTY
            });
            let Some((dx, dy)) = out_dir else {
                continue;
            };
            // The boot: three courses of field stone.
            for z in GROUND_TOP..(GROUND_TOP + 3) {
                world.set_voxel(IVec3::new(x, y, z), MAT_STONE);
            }
            // Studs: a timber upright every eighth voxel ALONG the run —
            // measured on the axis the wall actually travels, so a wall
            // sitting on a tile boundary doesn't become all timber.
            let run = if dx != 0 { y } else { x };
            if run.rem_euclid(8) == 0 {
                for z in (GROUND_TOP + 3)..WALL_TOP {
                    world.set_voxel(IVec3::new(x, y, z), MAT_TIMBER);
                }
                continue;
            }
            // The lintel band riding under the eave.
            for z in (WALL_TOP - 3)..(WALL_TOP - 1) {
                world.set_voxel(IVec3::new(x, y, z), MAT_TIMBER);
            }
            // Windows: some eight-voxel bays get a dark shuttered relief
            // with a timber sill. Solid voxels still — the eye falls in,
            // the quarrel doesn't.
            // Weathering: the odd tooth gone from the exposed wall top.
            if hash(x, y, 91) % 5 == 0 {
                world.set_voxel(IVec3::new(x, y, WALL_TOP - 1), Voxel::EMPTY);
            }
            let (bx, by) = (x.div_euclid(8), y.div_euclid(8));
            if hash(bx, by, 71) % 3 == 0 {
                let along = x.rem_euclid(8) >= 2 && x.rem_euclid(8) <= 5;
                let across = y.rem_euclid(8) >= 2 && y.rem_euclid(8) <= 5;
                if along || across {
                    // A real reveal: the shell voxel is carved away and
                    // the pane sits one voxel deep, in shadow. Sight and
                    // shot still stop on the wall behind it.
                    world.set_voxel(IVec3::new(x, y, GROUND_TOP + 5), MAT_TIMBER);
                    for z in (GROUND_TOP + 6)..(GROUND_TOP + 13) {
                        world.set_voxel(IVec3::new(x, y, z), Voxel::EMPTY);
                        world.set_voxel(IVec3::new(x - dx, y - dy, z), MAT_OBSIDIAN);
                    }
                }
            }
        }
    }
}

/// A low stepped roof over a rectangle of tiles: rises from the eaves to
/// a ridge, shallow enough that the deck above stays walkable ground for
/// whoever climbs there.
fn roof_over(world: &mut VoxelWorld, t0: IVec3, t1: IVec3) {
    let (v0, v1) = (t0 * TILE_VOXELS, (t1 + IVec3::new(1, 1, 0)) * TILE_VOXELS);
    for y in v0.y..v1.y {
        for x in v0.x..v1.x {
            let edge = (x - v0.x).min(v1.x - 1 - x).min(y - v0.y).min(v1.y - 1 - y);
            let h = TILE_VOXELS + 1 + (edge / 2).min(7);
            // Over the walls the roof meets the eave; over the rooms it
            // starts above head height, so the interior stays open under
            // its own ceiling.
            let base = if world.voxel(IVec3::new(x, y, WALL_TOP - 2)).is_solid() {
                WALL_TOP
            } else {
                TILE_VOXELS
            };
            for z in base..h {
                world.set_voxel(IVec3::new(x, y, z), MAT_SLATE);
            }
        }
    }
}

/// Scatter interrupted lives through a building's rooms: beds, boards,
/// and a cold hearth. Everything stands below head height, so no tile
/// blocks — soldiers step past the furniture the family left.
fn furnish_rooms(world: &mut VoxelWorld, seed: u64, t0: IVec3, t1: IVec3) {
    let mut k = seed as u32 | 1;
    let mut roll = |n: u32| -> u32 {
        k = k.wrapping_mul(747796405).wrapping_add(2891336453);
        (k >> 16) % n.max(1)
    };
    for ty in t0.y..=t1.y {
        for tx in t0.x..=t1.x {
            let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
            // Only furnish clear interior floor.
            if world.voxel(o + IVec3::new(TILE_VOXELS / 2, TILE_VOXELS / 2, mid_z())) != Voxel::EMPTY
            {
                continue;
            }
            match roll(4) {
                0 => {
                    // A bedframe with a pale blanket, shoved to the wall.
                    world.fill_box(
                        o + IVec3::new(3, 4, GROUND_TOP),
                        o + IVec3::new(17, 12, GROUND_TOP + 3),
                        MAT_TIMBER,
                    );
                    world.fill_box(
                        o + IVec3::new(4, 5, GROUND_TOP + 3),
                        o + IVec3::new(16, 11, GROUND_TOP + 5),
                        MAT_SNOW,
                    );
                }
                1 => {
                    // The table, still set: a board on four legs.
                    for (lx, ly) in [(10, 10), (10, 20), (20, 10), (20, 20)] {
                        world.fill_box(
                            o + IVec3::new(lx, ly, GROUND_TOP),
                            o + IVec3::new(lx + 2, ly + 2, GROUND_TOP + 5),
                            MAT_TIMBER,
                        );
                    }
                    world.fill_box(
                        o + IVec3::new(8, 8, GROUND_TOP + 5),
                        o + IVec3::new(24, 24, GROUND_TOP + 7),
                        MAT_TIMBER,
                    );
                }
                2 => {
                    // A cold hearth against the wall, embers not quite out.
                    world.fill_box(
                        o + IVec3::new(2, 12, GROUND_TOP),
                        o + IVec3::new(8, 20, GROUND_TOP + 7),
                        MAT_STONE,
                    );
                    world.fill_box(
                        o + IVec3::new(4, 14, GROUND_TOP),
                        o + IVec3::new(6, 18, GROUND_TOP + 2),
                        MAT_POOL,
                    );
                }
                _ => {} // bare boards
            }
        }
    }
}

/// Eye height for the interior-clearance probe.
fn mid_z() -> i32 {
    GROUND_TOP + 6
}

/// Fill a tile's footprint with wall from the ground to `WALL_TOP`.
fn fill_tile_walls(world: &mut VoxelWorld, tile: IVec3, material: Voxel) {
    let o = tile * TILE_VOXELS;
    world.fill_box(
        o + IVec3::new(0, 0, GROUND_TOP),
        o + IVec3::new(TILE_VOXELS, TILE_VOXELS, WALL_TOP),
        material,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::Side;

    #[test]
    fn skirmish_is_sane() {
        let b = skirmish(1);
        assert_eq!(b.living(Side::Order).count(), 4);
        assert_eq!(b.living(Side::Demons).count(), 4);
        for u in &b.units {
            assert!(
                b.tiles.is_walkable(u.tile),
                "{} spawns on unwalkable tile {}",
                u.name,
                u.tile
            );
        }
        // Chapel walls block, doorways don't.
        assert!(!b.tiles.is_walkable(IVec3::new(9, 9, 0)));
        assert!(b.tiles.is_walkable(IVec3::new(9, 11, 0)), "west doorway");
        assert!(b.tiles.is_walkable(IVec3::new(14, 12, 0)), "east doorway");
        // Interior is open.
        assert!(b.tiles.is_walkable(IVec3::new(11, 11, 0)));
    }

    #[test]
    fn the_gondola_deploys_walkable_and_its_gunwale_holds() {
        let squad: Vec<Unit> =
            (0..4).map(|i| Unit::soldier(i, &format!("S{i}"), IVec3::ZERO)).collect();
        let b = incursion_mission(
            7,
            squad,
            2,
            1,
            0,
            Biome::Temperate,
            MissionSpec::Standard,
        );
        // Every deployment tile — deck, mouth, ramp — must be standable.
        for (x, y) in ORDER_SPAWNS {
            assert!(
                b.tiles.is_walkable(IVec3::new(x, y, 0)),
                "spawn ({x},{y}) is blocked"
            );
        }
        // The gunwale stands as real waist-height cover on its tiles (it
        // can be clambered over, like any mound — but not walked through).
        let probe = IVec3::new(2, 10, 0) * TILE_VOXELS
            + IVec3::new(TILE_VOXELS / 2, TILE_VOXELS / 2, GROUND_TOP + 2);
        assert!(b.world.voxel(probe).is_solid(), "the gunwale is timber, not air");
        // The mouth is open: the squad walks out onto the ramp and the field.
        assert!(b.tiles.is_walkable(IVec3::new(3, 11, 0)));
        assert!(b.tiles.is_walkable(IVec3::new(4, 11, 0)));
    }

    #[test]
    fn gargoyles_start_perched_on_the_watchtower_roof() {
        let squad: Vec<Unit> =
            (0..4).map(|i| Unit::soldier(i, &format!("S{i}"), IVec3::ZERO)).collect();
        // Strength 4 with six demons puts a gargoyle at pack index 4.
        let b = incursion_mission(
            11,
            squad,
            6,
            4,
            0,
            Biome::Temperate,
            MissionSpec::Standard,
        );
        let gargoyle = b
            .units
            .iter()
            .find(|u| u.species == crate::units::Species::Gargoyle)
            .expect("the pack brings a gargoyle at this strength");
        assert_eq!(gargoyle.tile.z, 1, "it starts on the roof");
        assert!(
            b.tiles.is_walkable(gargoyle.tile),
            "the perch {} is real footing",
            gargoyle.tile
        );
    }

    #[test]
    fn opposing_lines_start_hidden_by_distance_or_ruins() {
        let b = skirmish(1);
        assert!(
            b.visible_enemies(Side::Order).is_empty(),
            "the yard should start quiet — imps out of sight"
        );
    }

    #[test]
    fn base_defense_map_deploys_and_resolves() {
        use crate::ai;

        let cells = [(2usize, 2usize), (2, 3), (3, 2), (3, 3), (4, 3)];
        let soldiers: Vec<Unit> = (0..4)
            .map(|i| Unit::soldier(i, &format!("Guard {i}"), glam::IVec3::ZERO))
            .collect();
        let mut b = super::base_defense(5, soldiers, 5, &cells, (2, 2));

        assert_eq!(b.living(Side::Order).count(), 4);
        assert_eq!(b.living(Side::Demons).count(), 5);
        for u in &b.units {
            assert!(b.tiles.is_walkable(u.tile), "{} spawns in rock at {}", u.name, u.tile);
        }
        let tiles: std::collections::HashSet<_> = b.units.iter().map(|u| u.tile).collect();
        assert_eq!(tiles.len(), b.units.len(), "no two units share a tile");

        // The corridor fight must resolve, not stalemate.
        let mut turns = 0;
        while b.winner.is_none() && turns < 60 {
            ai::run_order_turn(&mut b);
            if b.winner.is_none() {
                ai::run_demon_turn(&mut b);
            }
            turns += 1;
        }
        assert!(b.winner.is_some(), "base defense must resolve within 60 turns");
    }

    #[test]
    fn a_breached_defense_is_led_by_the_doorbreaker() {
        use crate::units::Species;

        let rooms = [
            (2, 2, RoomKind::Gatehouse),
            (2, 3, RoomKind::Quarters),
            (3, 2, RoomKind::Library),
            (3, 3, RoomKind::Workshop),
            (4, 3, RoomKind::Kennel),
            (4, 2, RoomKind::WardTower),
        ];
        let soldiers: Vec<Unit> = (0..4)
            .map(|i| Unit::soldier(i, &format!("Guard {i}"), glam::IVec3::ZERO))
            .collect();
        let spec = DefenseSpec {
            rooms: &rooms,
            gate: (2, 2),
            wards: 2,
            hounds: 2,
            breach: Some((4, 3)),
            behemoth: true,
        };
        let b = super::base_defense_fortified(11, soldiers, 5, &spec);

        for u in &b.units {
            assert!(b.tiles.is_walkable(u.tile), "{} spawns in rock at {}", u.name, u.tile);
        }
        let tiles: std::collections::HashSet<_> = b.units.iter().map(|u| u.tile).collect();
        assert_eq!(tiles.len(), b.units.len(), "no two units share a tile");
        assert!(
            b.units.iter().any(|u| u.species == Species::Behemoth && u.side == Side::Demons),
            "the wall-breaker leads the assault"
        );
        let hounds = b
            .units
            .iter()
            .filter(|u| u.species == Species::Hellhound && u.side == Side::Order)
            .count();
        assert!(hounds > 0, "the kennels open");
        // The demons enter at the breached kennel, not the gatehouse: the
        // behemoth stands in the kennel cell's rooms, far from the gate.
        let lead = b
            .units
            .iter()
            .find(|u| u.species == Species::Behemoth)
            .expect("just asserted");
        assert!(
            lead.tile.x / CELL_TILES >= 4,
            "the doorbreaker enters through the smashed wall, at {}",
            lead.tile
        );
    }

    #[test]
    fn battles_award_experience() {
        use crate::battle::Action;
        use crate::units::{FireMode, UnitId};

        let mut units = vec![
            Unit::soldier(0, "Vet", glam::IVec3::new(1, 5, 0)),
            Unit::imp(1, "Imp", glam::IVec3::new(8, 5, 0)),
        ];
        units[0].accuracy = 90; // make hits likely so the test converges fast
        let mut b = super::incursion(3, units, 0, 1);
        // incursion() repositions; overwrite for a clean point-blank duel.
        b.units[0].tile = glam::IVec3::new(4, 11, 0);
        b.units[1].tile = glam::IVec3::new(6, 11, 0);

        for _ in 0..30 {
            if b.winner.is_some() {
                break;
            }
            while b.winner.is_none()
                && b.unit(UnitId(0)).fire_cost(FireMode::Snap).is_some_and(|c| b.unit(UnitId(0)).tu >= c)
            {
                b.perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap })
                    .unwrap();
            }
            if b.winner.is_none() {
                b.perform(Action::EndTurn).unwrap();
                b.perform(Action::EndTurn).unwrap();
            }
        }
        let xp = b.experience(UnitId(0));
        assert!(xp.shots_hit > 0, "hits must be recorded: {xp:?}");
        assert_eq!(xp.kills, 1, "the kill goes on the record: {xp:?}");
    }

    #[test]
    fn escalation_changes_the_pack() {
        use crate::units::Species;
        let spawns: Vec<(i32, i32)> = (0..8).map(|i| (21, 2 + i * 2)).collect();

        let early = super::demon_pack(6, 1, 0, &spawns);
        assert!(early.iter().all(|u| u.species == Species::Imp));

        let mid = super::demon_pack(6, 4, 0, &spawns);
        assert!(mid.iter().any(|u| u.species == Species::Hellhound));
        assert!(mid.iter().any(|u| u.species == Species::BileWisp));

        let late = super::demon_pack(6, 7, 0, &spawns);
        assert!(late.iter().any(|u| u.species == Species::Overseer));
        assert!(late.iter().any(|u| u.species == Species::Taker));
    }

    #[test]
    fn tower_is_reachable_over_the_mound() {
        use std::collections::HashSet;
        let b = super::incursion(9, vec![Unit::soldier(0, "S", glam::IVec3::ZERO)], 0, 1);
        let mound = glam::IVec3::new(15, 4, 0);
        let tower = glam::IVec3::new(17, 4, 1);
        assert!(b.tiles.is_ramp(mound), "the rubble mound is climbable");
        assert!(b.tiles.is_walkable(tower), "the tower floor holds");
        let path = b
            .tiles
            .path(glam::IVec3::new(12, 4, 0), tower, &HashSet::new())
            .expect("a route up the mound exists");
        assert!(path.contains(&mound), "the climb goes over the rubble: {path:?}");
    }

    #[test]
    fn demolishing_the_obelisk_wins_the_battle() {
        use glam::Vec3;
        let mut b = super::incursion(21, vec![Unit::soldier(0, "S", glam::IVec3::ZERO)], 2, 1);
        assert!(b.objective.is_some());
        // Cheat demolition charges straight onto the obelisk (the sim only
        // cares that the voxels die, not who is holding the detonator).
        let mut all_events = Vec::new();
        for dy in 0..3 {
            let at = glam::IVec3::new(22, 11 + dy, 0);
            let center =
                (at * TILE_VOXELS).as_vec3() + Vec3::new(8.0, 8.0, 12.0) * VS as f32;
            b.world.carve_sphere(center, 14.0 * VS as f32);
            let mut events = Vec::new();
            b.check_objective_for_test(&mut events);
            all_events.extend(events);
            if b.winner.is_some() {
                break;
            }
        }
        assert!(
            all_events.iter().any(|e| matches!(e, crate::battle::Event::ObjectiveDestroyed)),
            "{all_events:?}"
        );
        assert_eq!(b.winner, Some(crate::units::Side::Order));
    }

    #[test]
    fn chapel_doors_block_until_opened() {
        use crate::battle::{Action, Event};
        use crate::units::UnitId;

        let mut soldiers = vec![Unit::soldier(0, "S", glam::IVec3::ZERO)];
        soldiers[0].tu_max = 99;
        let mut b = super::incursion(3, soldiers, 0, 1);
        let door = glam::IVec3::new(9, 11, 0);
        assert!(!b.tiles.is_walkable(door), "a closed door bars the way");

        b.units[0].tile = glam::IVec3::new(8, 11, 0); // on the stoop
        b.units[0].tu = 99;
        let events = b.perform(Action::OpenDoor { unit: UnitId(0), at: door }).unwrap();
        assert!(matches!(events[0], Event::DoorOpened { .. }));
        assert!(b.tiles.is_walkable(door), "an open door is a doorway again");
        assert_eq!(
            b.perform(Action::OpenDoor { unit: UnitId(0), at: door }),
            Err(crate::battle::ActionError::NoDoor)
        );
    }

    #[test]
    fn massacre_sites_shelter_civilians() {
        use crate::ai;
        use crate::units::Side;

        let soldiers = vec![Unit::soldier(0, "S", glam::IVec3::ZERO)];
        let mut b = super::incursion_with_civilians(5, soldiers, 3, 1, 4);
        let civs = b.units.iter().filter(|u| u.civilian).count();
        assert_eq!(civs, 4);
        for u in b.units.iter().filter(|u| u.civilian) {
            assert!(b.tiles.is_walkable(u.tile), "{} spawns clear", u.name);
        }
        // They flee on their own during the Order turn.
        let events = ai::run_civilian_moves(&mut b);
        let _ = events; // may be empty if no demon is near enough — fine
        assert_eq!(b.side_to_move, Side::Order, "flight is not a turn");
    }

    #[test]
    fn gargoyles_fly_and_behemoths_smash() {
        use crate::battle::{Action, Event};
        use crate::units::UnitId;

        // Gargoyle before the freestanding ruin wall flies straight over
        // it. (The chapel is roofed now — wings don't help indoors.)
        let mut units = vec![Unit::soldier(0, "S", glam::IVec3::ZERO)];
        units[0].tile = glam::IVec3::new(2, 2, 0);
        let mut b = super::incursion(3, units, 0, 1); // chapel variant (3 % 3 == 0)
        let g = b.units.len() as u32;
        b.units.push(Unit::gargoyle(g, "Gargoyle", glam::IVec3::new(5, 4, 0)));
        b.xp_push_for_test();
        b.perform(Action::EndTurn).unwrap();
        // The wall at x=6 bars walkers; wings cross it in a straight line.
        let beyond = glam::IVec3::new(7, 4, 0);
        assert!(!b.tiles.is_walkable(glam::IVec3::new(6, 4, 0)), "the wall stands");
        let ev = b.perform(Action::Move { unit: UnitId(g), to: beyond });
        assert!(ev.is_ok(), "wings ignore walls: {ev:?}");
        assert_eq!(b.units[g as usize].tile, beyond);

        // Behemoth walks THROUGH the chapel wall, leaving a hole.
        let mut units = vec![Unit::soldier(0, "S", glam::IVec3::ZERO)];
        units[0].tile = glam::IVec3::new(2, 2, 0);
        let mut b = super::incursion(3, units, 0, 1);
        let m = b.units.len() as u32;
        b.units.push(Unit::behemoth(m, "Behemoth", glam::IVec3::new(8, 9, 0)));
        b.xp_push_for_test();
        b.perform(Action::EndTurn).unwrap();
        let wall = glam::IVec3::new(9, 9, 0);
        assert!(!b.tiles.is_walkable(wall));
        let ev = b
            .perform(Action::Move { unit: UnitId(m), to: wall })
            .unwrap();
        assert!(
            ev.iter().any(|e| matches!(e, Event::WallSmashed { .. })),
            "{ev:?}"
        );
        assert!(b.tiles.is_walkable(wall), "the wall is a doorway now");
    }

    #[test]
    fn casks_detonate_when_breached() {
        use crate::battle::{Action, Event};
        use crate::units::UnitId;

        let mut soldiers = vec![Unit::soldier(0, "S", glam::IVec3::ZERO)];
        soldiers[0].grenades = 2;
        let mut b = super::incursion(3, soldiers, 0, 1);
        let cask = glam::IVec3::new(8, 7, 0); // placed by the generator
        b.units[0].tile = glam::IVec3::new(5, 7, 0);
        let events = b
            .perform(Action::Throw { unit: UnitId(0), at: cask })
            .unwrap();
        let blasts = events
            .iter()
            .filter(|e| matches!(e, Event::Exploded { .. }))
            .count();
        assert!(blasts >= 2, "the grenade and the cask both go up: {events:?}");
    }

    #[test]
    fn nest_and_otherside_maps_deploy_sanely() {
        let squad = |n: u32| -> Vec<Unit> {
            (0..n).map(|i| Unit::soldier(i, &format!("S{i}"), glam::IVec3::ZERO)).collect()
        };
        let b = super::nest_map(11, squad(4), 5, 6);
        assert!(b.objective.is_some(), "the heart is the objective");
        for u in &b.units {
            let ok = if u.flies { b.tiles.is_open_air(u.tile) } else { b.tiles.is_walkable(u.tile) };
            assert!(ok, "{} stuck in flesh at {}", u.name, u.tile);
        }
        let b = super::otherside(13, squad(6), 7, 10);
        assert!(b.units.iter().any(|u| u.species == crate::units::Species::Prince));
        assert!(!b.pools.is_empty(), "brimstone seeps everywhere there");
        for u in &b.units {
            assert!(b.tiles.is_walkable(u.tile), "{} in obsidian", u.name);
        }
    }

    #[test]
    fn every_biome_deploys_sane_and_the_obelisk_stays_reachable() {
        use std::collections::HashSet;
        for biome in [Biome::Temperate, Biome::Desert, Biome::Jungle, Biome::Tundra] {
            for seed in [3, 7, 20] {
                // Three seeds x three structural variants x each biome.
                let squad: Vec<Unit> = (0..4)
                    .map(|i| Unit::soldier(i, &format!("S{i}"), glam::IVec3::ZERO))
                    .collect();
                let b = super::incursion_in_biome(seed, squad, 4, 3, 0, biome);
                assert!(b.objective.is_some());
                for u in &b.units {
                    let ok = if u.flies {
                        b.tiles.is_open_air(u.tile) || b.tiles.is_walkable(u.tile)
                    } else {
                        b.tiles.is_walkable(u.tile)
                    };
                    assert!(ok, "{biome:?} seed {seed}: {} spawns badly at {}", u.name, u.tile);
                }
                // The scatter must never wall off the advance: a soldier can
                // still reach the obelisk's doorstep.
                let path = b.tiles.path(
                    glam::IVec3::new(2, 11, 0),
                    glam::IVec3::new(21, 12, 0),
                    &HashSet::new(),
                );
                assert!(path.is_some(), "{biome:?} seed {seed}: the way east is sealed");
            }
        }
    }

    #[test]
    fn jungle_canopies_are_perches_not_ceilings() {
        // Find a tree in a jungle map and check its shape: the trunk tile is
        // blocked at ground level, its neighbor stays walkable underneath,
        // and the canopy above that neighbor is walkable roof.
        for seed in 0..6u64 {
            let b = super::incursion_in_biome(
                seed,
                vec![Unit::soldier(0, "S", glam::IVec3::ZERO)],
                0,
                1,
                0,
                Biome::Jungle,
            );
            for y in 1..23 {
                for x in 5..20 {
                    let trunk = glam::IVec3::new(x, y, 0);
                    let probe = trunk * crate::TILE_VOXELS
                        + glam::IVec3::new(crate::TILE_VOXELS / 2, crate::TILE_VOXELS / 2, GROUND_TOP + 1);
                    if b.world.voxel(probe) == MAT_TIMBER {
                        assert!(!b.tiles.is_walkable(trunk), "trunks block");
                        let above = glam::IVec3::new(x, y, 1);
                        // Somewhere in the 3x3 canopy there is a walkable top.
                        let mut perch = b.tiles.is_walkable(above);
                        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                            perch |= b
                                .tiles
                                .is_walkable(glam::IVec3::new(x + dx, y + dy, 1));
                        }
                        assert!(perch, "seed {seed}: canopy at {trunk} has no perch");
                        return;
                    }
                }
            }
        }
        panic!("no tree found across six jungle seeds");
    }

    #[test]
    fn full_ai_battle_runs_to_completion_or_stalemate_guard() {
        use crate::ai::run_demon_turn;
        use crate::battle::Action;
        use crate::units::{FireMode, UnitId};

        // Order AI stand-in: every soldier shoots the nearest visible imp or
        // advances east; then the demon AI plays. The battle must resolve
        // (someone wins) well within 40 turns.
        let mut b = skirmish(2025);
        for _turn in 0..40 {
            if b.winner.is_some() {
                break;
            }
            for id in b.living(Side::Order).map(|u| u.id).collect::<Vec<_>>() {
                loop {
                    if b.winner.is_some() || !b.unit(id).alive {
                        break;
                    }
                    let me = b.unit(id);
                    let targets: Vec<UnitId> = b
                        .visible_enemies(Side::Order)
                        .into_iter()
                        .filter(|&t| b.can_see(id, t))
                        .collect();
                    if let Some(&t) = targets.first() {
                        if me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c) {
                            let _ = b.perform(Action::Fire {
                                unit: id,
                                target: t,
                                mode: FireMode::Snap,
                            });
                            continue;
                        }
                        break;
                    }
                    let goal = me.tile + IVec3::new(3, 0, 0);
                    match b.perform(Action::Move { unit: id, to: goal }) {
                        Ok(ev) if !ev.is_empty() => continue,
                        _ => break,
                    }
                }
            }
            if b.winner.is_none() {
                b.perform(Action::EndTurn).unwrap();
                run_demon_turn(&mut b);
            }
        }
        assert!(
            b.winner.is_some(),
            "a 4v4 in a courtyard must not last 40 turns"
        );
    }
}

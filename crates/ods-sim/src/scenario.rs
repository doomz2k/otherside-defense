//! Map generation and battle setup for the first playable skirmish:
//! a ruined chapel yard, four Order soldiers, four imps.

use glam::IVec3;
use ods_voxel::{Voxel, VoxelWorld};

use crate::TILE_VOXELS;
use crate::battle::Battle;
use crate::units::Unit;

pub const MAP_TILES: IVec3 = IVec3::new(24, 24, 2);

/// Ground fills voxels z 0..4 (the tile's foot band), so shallow craters
/// don't punch through to the void.
pub const GROUND_TOP: i32 = 4;
/// Walls rise from the ground to torso/head height.
const WALL_TOP: i32 = 14;

pub const MAT_GROUND: Voxel = Voxel(1);
pub const MAT_WALL: Voxel = Voxel(2);
pub const MAT_RUBBLE: Voxel = Voxel(3);
/// The rift obelisk: hell's anchor into the world. Demolish it to win.
pub const MAT_OBELISK: Voxel = Voxel(4);

pub fn skirmish(seed: u64) -> Battle {
    let mut world = VoxelWorld::new();

    // Ground slab across the whole map.
    world.fill_box(
        IVec3::ZERO,
        IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, GROUND_TOP),
        MAT_GROUND,
    );

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
            o + IVec3::new(3, 3, GROUND_TOP),
            o + IVec3::new(13, 13, GROUND_TOP + 4),
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

/// West-side deployment tiles and east-side incursion tiles, in fill order.
const ORDER_SPAWNS: [(i32, i32); 8] =
    [(2, 9), (2, 11), (2, 13), (3, 15), (2, 7), (3, 17), (2, 5), (3, 19)];
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
        let unit = if strength >= 5 && i == 0 {
            Unit::overseer(id, &format!("Overseer of {name}"), tile)
        } else if strength >= 7 && i == 1 {
            Unit::taker(id, "The Taker", tile)
        } else if strength >= 3 {
            match i % 4 {
                2 => Unit::hellhound(id, &format!("Hound of {name}"), tile),
                3 => Unit::bile_wisp(id, &format!("Wisp of {name}"), tile),
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
pub fn incursion(seed: u64, mut soldiers: Vec<Unit>, demon_count: u32, strength: u32) -> Battle {
    let mut world = VoxelWorld::new();
    world.fill_box(
        IVec3::ZERO,
        IVec3::new(MAP_TILES.x * TILE_VOXELS, MAP_TILES.y * TILE_VOXELS, GROUND_TOP),
        MAT_GROUND,
    );
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

    // A gutted watchtower: raised floor reached over a rubble mound. High
    // ground overwatches the yard.
    world.fill_box(
        IVec3::new(16 * TILE_VOXELS, 3 * TILE_VOXELS, TILE_VOXELS),
        IVec3::new(20 * TILE_VOXELS, 6 * TILE_VOXELS, TILE_VOXELS + GROUND_TOP),
        MAT_WALL,
    );
    for (px, py) in [(16, 3), (19, 3), (16, 5), (19, 5)] {
        world.fill_box(
            IVec3::new(px * TILE_VOXELS + 4, py * TILE_VOXELS + 4, GROUND_TOP),
            IVec3::new(px * TILE_VOXELS + 12, py * TILE_VOXELS + 12, TILE_VOXELS),
            MAT_WALL,
        );
    }
    // The climbable mound at the tower's west face.
    let mound = IVec3::new(15, 4, 0) * TILE_VOXELS;
    world.fill_box(
        mound + IVec3::new(0, 0, GROUND_TOP),
        mound + IVec3::new(TILE_VOXELS, TILE_VOXELS, 10),
        MAT_RUBBLE,
    );

    // The rift obelisk, deep in demon territory (clear of spawn tiles).
    let obelisk_min = IVec3::new(22 * TILE_VOXELS, 11 * TILE_VOXELS, GROUND_TOP);
    let obelisk_max = IVec3::new(23 * TILE_VOXELS, 13 * TILE_VOXELS, 24);
    world.fill_box(obelisk_min, obelisk_max, MAT_OBELISK);

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
    battle.set_objective(obelisk_min, obelisk_max);
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
    mut soldiers: Vec<Unit>,
    demon_count: u32,
    cells: &[(usize, usize)],
    gate: (usize, usize),
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
        IVec3::new(map_tiles.x * TILE_VOXELS, map_tiles.y * TILE_VOXELS, 14),
        MAT_WALL,
    );

    let clear_tile = |world: &mut VoxelWorld, tx: i32, ty: i32| {
        let o = IVec3::new(tx, ty, 0) * TILE_VOXELS;
        world.fill_box(
            o + IVec3::new(0, 0, GROUND_TOP),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, 14),
            Voxel::EMPTY,
        );
    };

    // Carve 2x2 room interiors.
    for &(cx, cy) in cells {
        for dy in 1..=2 {
            for dx in 1..=2 {
                clear_tile(&mut world, cx as i32 * CELL_TILES + dx, cy as i32 * CELL_TILES + dy);
            }
        }
    }
    // Carve doorways between adjacent occupied cells.
    for &(cx, cy) in cells {
        let (cx, cy) = (cx as i32, cy as i32);
        if cells.contains(&((cx + 1) as usize, cy as usize)) {
            let row = cy * CELL_TILES + 1;
            clear_tile(&mut world, cx * CELL_TILES + 3, row);
            clear_tile(&mut world, (cx + 1) * CELL_TILES, row);
        }
        if cells.contains(&(cx as usize, (cy + 1) as usize)) {
            let col = cx * CELL_TILES + 1;
            clear_tile(&mut world, col, cy * CELL_TILES + 3);
            clear_tile(&mut world, col, (cy + 1) * CELL_TILES);
        }
    }

    // Deployment: breadth-first flood from the gatehouse over walkable tiles.
    // Demons pour in nearest the gate; defenders hold the deepest rooms.
    let tiles = crate::tiles::TileMap::derive(&world, IVec3::ZERO, map_tiles);
    let start = IVec3::new(
        gate.0 as i32 * CELL_TILES + 1,
        gate.1 as i32 * CELL_TILES + 1,
        0,
    );
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

    let demon_names = ["Wrath", "Envy", "Gluttony", "Sloth", "Pride", "Greed", "Lust", "Despair"];
    let mut units = Vec::new();
    soldiers.truncate(8);
    let defenders = soldiers.len();
    for (i, mut s) in soldiers.into_iter().enumerate() {
        s.id = crate::units::UnitId(i as u32);
        s.tile = order[order.len() - 1 - i];
        units.push(s);
    }
    let demon_count = (demon_count as usize).min(8).min(order.len() - defenders);
    for i in 0..demon_count {
        units.push(Unit::imp(
            (defenders + i) as u32,
            &format!("Imp of {}", demon_names[i]),
            order[i],
        ));
    }

    Battle::new(world, IVec3::ZERO, map_tiles, units, seed)
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
            let center = (at * TILE_VOXELS).as_vec3() + Vec3::new(8.0, 8.0, 12.0);
            b.world.carve_sphere(center, 14.0);
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
    fn full_ai_battle_runs_to_completion_or_stalemate_guard() {
        use crate::ai::run_demon_turn;
        use crate::battle::Action;
        use crate::units::{FireMode, UnitId};

        // Order AI stand-in: every soldier shoots the nearest visible imp or
        // advances east; then the demon AI plays. The battle must resolve
        // (someone wins) well within 40 turns.
        let mut b = skirmish(2024);
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

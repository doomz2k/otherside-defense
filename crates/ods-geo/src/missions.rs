//! Auto-resolution of ground missions: a real `ods-sim` battle, both sides
//! played by AI, deterministic given the seed. No abstract dice — if a wall
//! gets blown open in the tactical sim, that's how the strategic result came
//! to be.

use ods_sim::battle::{Battle, Experience};
use ods_sim::units::{Side, Unit, UnitId};
use ods_sim::{ai, scenario};

use crate::campaign::Soldier;
use crate::research::{Project, ResearchState};

#[derive(Clone, Debug, PartialEq)]
pub struct BattleReport {
    pub victory: bool,
    pub turns: u32,
    /// Indexes into the squad passed in, for the fallen.
    pub dead: Vec<usize>,
    /// (squad index, health remaining, experience) for survivors.
    pub survivors: Vec<(usize, i32, Experience)>,
    pub demons_slain: u32,
}

const MAX_AUTO_TURNS: u32 = 40;

/// Resolve a rift-site assault on the standard field map.
pub fn auto_resolve(
    seed: u64,
    squad: &[&Soldier],
    demon_count: u32,
    research: &ResearchState,
) -> BattleReport {
    let units = make_units(squad, research);
    let squad_len = units.len();
    resolve(scenario::incursion(seed, units, demon_count), squad_len)
}

/// Resolve a Reckoning: demons breaching the chapterhouse itself. The map is
/// generated from the actual facility layout.
pub fn auto_resolve_defense(
    seed: u64,
    squad: &[&Soldier],
    demon_count: u32,
    research: &ResearchState,
    cells: &[(usize, usize)],
    gate: (usize, usize),
) -> BattleReport {
    let units = make_units(squad, research);
    let squad_len = units.len();
    resolve(
        scenario::base_defense(seed, units, demon_count, cells, gate),
        squad_len,
    )
}

fn resolve(mut battle: Battle, squad_len: usize) -> BattleReport {
    let demons_start = battle.living(Side::Demons).count() as u32;

    let mut turns = 0;
    while battle.winner.is_none() && turns < MAX_AUTO_TURNS {
        ai::run_order_turn(&mut battle);
        if battle.winner.is_none() {
            ai::run_demon_turn(&mut battle);
        }
        turns += 1;
    }

    let mut dead = Vec::new();
    let mut survivors = Vec::new();
    for i in 0..squad_len {
        let u = &battle.units[i];
        if u.alive {
            survivors.push((i, u.health, battle.experience(UnitId(i as u32))));
        } else {
            dead.push(i);
        }
    }

    BattleReport {
        // A timeout is a withdrawal: the incursion holds the field.
        victory: battle.winner == Some(Side::Order),
        turns,
        dead,
        survivors,
        demons_slain: demons_start - battle.living(Side::Demons).count() as u32,
    }
}

fn make_units(squad: &[&Soldier], research: &ResearchState) -> Vec<Unit> {
    squad
        .iter()
        .enumerate()
        .map(|(i, s)| make_unit(i as u32, s, research))
        .collect()
}

fn make_unit(id: u32, s: &Soldier, research: &ResearchState) -> Unit {
    // Placeholder tile; `scenario::incursion` assigns the real deployment.
    let mut u = Unit::soldier(id, &s.name, glam::IVec3::ZERO);
    u.tu_max = s.stats.tu;
    u.tu = s.stats.tu;
    u.reactions = s.stats.reactions;
    u.accuracy = s.stats.accuracy;
    u.bravery = s.stats.bravery;
    u.health_max = s.stats.health;
    if research.is_complete(Project::HellsteelPlate) {
        u.health_max += 8;
    }
    u.health = u.health_max;
    if research.is_complete(Project::HellfireLance) {
        u.weapon.power += 16;
    } else if research.is_complete(Project::BlessedArms) {
        u.weapon.power += 8;
    }
    u
}

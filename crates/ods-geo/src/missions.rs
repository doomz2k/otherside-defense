//! Ground-mission plumbing between the campaign and the tactical sim.
//!
//! The campaign builds a real `ods-sim` battle (assault or base defense),
//! and folds the finished battle back into a report. Between those two
//! moments the battle can be driven by AI (auto-resolve) or by the player
//! (the interactive Battlescape) — the campaign doesn't care which.

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

/// Build a rift-site assault on the standard field map.
pub(crate) fn build_assault(
    seed: u64,
    squad: &[&Soldier],
    demon_count: u32,
    research: &ResearchState,
) -> Battle {
    scenario::incursion(seed, make_units(squad, research), demon_count)
}

/// Build a Reckoning: demons breaching the chapterhouse itself, on a map
/// generated from the actual facility layout.
pub(crate) fn build_defense(
    seed: u64,
    squad: &[&Soldier],
    demon_count: u32,
    research: &ResearchState,
    cells: &[(usize, usize)],
    gate: (usize, usize),
) -> Battle {
    scenario::base_defense(seed, make_units(squad, research), demon_count, cells, gate)
}

/// Drive a battle to its end with AI on both sides.
pub(crate) fn run_auto(battle: &mut Battle) -> u32 {
    let mut turns = 0;
    while battle.winner.is_none() && turns < MAX_AUTO_TURNS {
        ai::run_order_turn(battle);
        if battle.winner.is_none() {
            ai::run_demon_turn(battle);
        }
        turns += 1;
    }
    turns
}

/// Read a finished (or abandoned) battle back into a campaign-level report.
/// A battle with no winner counts as a withdrawal: the demons hold the field.
pub(crate) fn report_from(battle: &Battle, squad_len: usize) -> BattleReport {
    let demons_total = (battle.units.len() - squad_len) as u32;

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
        victory: battle.winner == Some(Side::Order),
        turns: battle.turn,
        dead,
        survivors,
        demons_slain: demons_total - battle.living(Side::Demons).count() as u32,
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
    // Placeholder tile; the scenario builders assign the real deployment.
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

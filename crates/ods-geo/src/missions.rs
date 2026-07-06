//! Auto-resolution of ground missions: a real `ods-sim` battle, both sides
//! played by AI, deterministic given the seed. No abstract dice — if a wall
//! gets blown open in the tactical sim, that's how the strategic result came
//! to be.

use ods_sim::units::{Side, Unit};
use ods_sim::{ai, scenario};

use crate::campaign::Soldier;
use crate::research::{Project, ResearchState};

#[derive(Clone, Debug, PartialEq)]
pub struct BattleReport {
    pub victory: bool,
    pub turns: u32,
    /// Indexes into the squad passed in, for the fallen.
    pub dead: Vec<usize>,
    /// (squad index, health remaining) for survivors.
    pub survivors: Vec<(usize, i32)>,
    pub demons_slain: u32,
}

const MAX_AUTO_TURNS: u32 = 40;

pub fn auto_resolve(
    seed: u64,
    squad: &[&Soldier],
    demon_count: u32,
    research: &ResearchState,
) -> BattleReport {
    let units: Vec<Unit> = squad
        .iter()
        .enumerate()
        .map(|(i, s)| make_unit(i as u32, s, research))
        .collect();
    let squad_len = units.len();
    let mut battle = scenario::incursion(seed, units, demon_count);
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
            survivors.push((i, u.health));
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
    if research.is_complete(Project::BlessedArms) {
        u.weapon.power += 8;
    }
    u
}

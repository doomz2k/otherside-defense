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
    /// Indexes into the squad passed in, for the fallen (including the
    /// Taken — a soldier walking around as a Husk is not coming home).
    pub dead: Vec<usize>,
    /// (squad index, health remaining, experience) for survivors.
    pub survivors: Vec<(usize, i32, Experience)>,
    pub demons_slain: u32,
    /// Unconscious demons on a held field: bound and dragged home.
    pub captured_grunts: u32,
    pub captured_overseers: u32,
    /// Townsfolk alive / lost on massacre sites.
    pub civilians_saved: u32,
    pub civilians_dead: u32,
}

const MAX_AUTO_TURNS: u32 = 40;

/// Build a rift-site assault on the standard field map.
pub(crate) fn build_assault(
    seed: u64,
    squad: &[&Soldier],
    kits: &[(u32, u32)],
    demon_count: u32,
    strength: u32,
    civilians: u32,
    research: &ResearchState,
) -> Battle {
    scenario::incursion_with_civilians(
        seed,
        make_units(squad, kits, research),
        demon_count,
        strength,
        civilians,
    )
}

/// Build a Reckoning: demons breaching the chapterhouse itself, on a map
/// generated from the actual facility layout.
pub(crate) fn build_defense(
    seed: u64,
    squad: &[&Soldier],
    kits: &[(u32, u32)],
    demon_count: u32,
    research: &ResearchState,
    cells: &[(usize, usize)],
    gate: (usize, usize),
) -> Battle {
    scenario::base_defense(seed, make_units(squad, kits, research), demon_count, cells, gate)
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
    use ods_sim::units::Species;

    let demons_total = battle
        .units
        .iter()
        .skip(squad_len)
        .filter(|u| !u.civilian)
        .count() as u32;
    let victory = battle.winner == Some(Side::Order);

    let mut dead = Vec::new();
    let mut survivors = Vec::new();
    for i in 0..squad_len {
        let u = &battle.units[i];
        // A Taken soldier is alive, walking, and lost forever.
        if u.alive && u.side == Side::Order {
            survivors.push((i, u.health, battle.experience(UnitId(i as u32))));
        } else {
            dead.push(i);
        }
    }

    let (mut captured_grunts, mut captured_overseers) = (0, 0);
    if victory {
        for u in battle.units.iter().skip(squad_len) {
            if u.alive && !u.conscious && u.side == Side::Demons {
                match u.species {
                    Species::Overseer | Species::Prince => captured_overseers += 1,
                    _ => captured_grunts += 1,
                }
            }
        }
    }

    let (mut civilians_saved, mut civilians_dead) = (0, 0);
    for u in battle.units.iter().skip(squad_len) {
        if u.civilian {
            if u.alive && u.side == Side::Order {
                civilians_saved += 1;
            } else {
                civilians_dead += 1;
            }
        }
    }

    BattleReport {
        victory,
        turns: battle.turn,
        dead,
        survivors,
        demons_slain: demons_total.saturating_sub(battle.living(Side::Demons).count() as u32),
        captured_grunts,
        captured_overseers,
        civilians_saved,
        civilians_dead,
    }
}

fn make_units(squad: &[&Soldier], kits: &[(u32, u32)], research: &ResearchState) -> Vec<Unit> {
    squad
        .iter()
        .zip(kits)
        .enumerate()
        .map(|(i, (s, &kit))| make_unit(i as u32, s, kit, research))
        .collect()
}

fn make_unit(id: u32, s: &Soldier, kit: (u32, u32), research: &ResearchState) -> Unit {
    // Placeholder tile; the scenario builders assign the real deployment.
    let mut u = Unit::soldier(id, &s.name, glam::IVec3::ZERO);
    u.tu_max = s.stats.tu;
    u.reactions = s.stats.reactions;
    u.accuracy = s.stats.accuracy;
    u.bravery = (s.stats.bravery + s.rank_bravery()).min(95);
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
    let (grenades, dressings) = kit;
    u.grenades = grenades;
    u.heal_charges = dressings;
    // An overloaded pack slows the hand.
    if grenades + dressings > 5 {
        u.tu_max -= 4;
    }
    u.tu = u.tu_max;
    u
}

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
    /// Crippled parts per surviving squad member (squad index, parts).
    pub injuries: Vec<(usize, Vec<ods_sim::body::BodyPart>)>,
    /// Parts severed outright per surviving squad member — gone for good.
    pub severed: Vec<(usize, Vec<ods_sim::body::BodyPart>)>,
    /// Unconscious demons on a held field: bound and dragged home.
    pub captured_grunts: u32,
    pub captured_overseers: u32,
    /// Townsfolk alive / lost on massacre sites.
    pub civilians_saved: u32,
    pub civilians_dead: u32,
    /// Breeds encountered / dragged home bound — feeds the codex.
    pub species_seen: Vec<ods_sim::units::Species>,
    pub species_captured: Vec<ods_sim::units::Species>,
    /// Horrors witnessed per survivor (squad index, count) — sanity damage.
    pub horrors: Vec<(usize, u32)>,
    /// Breeds the squad put down this battle (necropsy-tier codex).
    pub species_slain: Vec<ods_sim::units::Species>,
    /// Atrocity sites the squad discovered (cleansed if the field was held).
    pub atrocities_found: u32,
}

const MAX_AUTO_TURNS: u32 = 40;

/// Build a rift-site assault on the standard field map.
pub(crate) fn build_nest(
    seed: u64,
    squad: &[&Soldier],
    kits: &[(u32, u32)],
    demon_count: u32,
    strength: u32,
    research: &ResearchState,
) -> Battle {
    scenario::nest_map(seed, make_units(squad, kits, research), demon_count, strength)
}

pub(crate) fn build_otherside(
    seed: u64,
    squad: &[&Soldier],
    kits: &[(u32, u32)],
    demon_count: u32,
    strength: u32,
    research: &ResearchState,
) -> Battle {
    scenario::otherside(seed, make_units(squad, kits, research), demon_count, strength)
}

#[allow(clippy::too_many_arguments)] // a mission brief simply has this many parts
pub(crate) fn build_assault(
    seed: u64,
    squad: &[&Soldier],
    kits: &[(u32, u32)],
    demon_count: u32,
    strength: u32,
    civilians: u32,
    biome: scenario::Biome,
    spec: scenario::MissionSpec,
    research: &ResearchState,
) -> Battle {
    scenario::incursion_mission(
        seed,
        make_units(squad, kits, research),
        demon_count,
        strength,
        civilians,
        biome,
        spec,
    )
}

/// Build a Reckoning: demons breaching the chapterhouse itself, on a map
/// generated from the actual facility layout — plus whatever fortifications
/// the house actually built.
#[allow(clippy::too_many_arguments)] // a garrison brief has this many parts
pub(crate) fn build_defense(
    seed: u64,
    squad: &[&Soldier],
    kits: &[(u32, u32)],
    demon_count: u32,
    research: &ResearchState,
    cells: &[(usize, usize)],
    gate: (usize, usize),
    wards: u32,
    hounds: u32,
) -> Battle {
    scenario::base_defense_fortified(
        seed,
        make_units(squad, kits, research),
        demon_count,
        cells,
        gate,
        wards,
        hounds,
    )
}

/// Build a purge: storming a corrupted patron's manor.
pub(crate) fn build_purge(
    seed: u64,
    squad: &[&Soldier],
    kits: &[(u32, u32)],
    demon_count: u32,
    research: &ResearchState,
) -> Battle {
    scenario::manor_purge(seed, make_units(squad, kits, research), demon_count)
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
    let mut injuries = Vec::new();
    let mut severed = Vec::new();
    let mut horrors = Vec::new();
    for i in 0..squad_len {
        let u = &battle.units[i];
        // A Taken soldier is alive, walking, and lost forever.
        if u.alive && u.side == Side::Order {
            survivors.push((i, u.health, battle.experience(UnitId(i as u32))));
            // Crippled-but-attached parts convalesce; severed ones don't.
            let crippled: Vec<_> = u
                .injuries
                .iter()
                .copied()
                .filter(|p| !u.severed.contains(p))
                .collect();
            if !crippled.is_empty() {
                injuries.push((i, crippled));
            }
            if !u.severed.is_empty() {
                severed.push((i, u.severed.clone()));
            }
            if u.horror > 0 {
                horrors.push((i, u.horror));
            }
        } else {
            dead.push(i);
        }
    }

    let (mut captured_grunts, mut captured_overseers) = (0, 0);
    let mut species_captured = Vec::new();
    if victory {
        for u in battle.units.iter().skip(squad_len) {
            if u.alive && !u.conscious && u.side == Side::Demons {
                match u.species {
                    Species::Overseer | Species::Prince => captured_overseers += 1,
                    _ => captured_grunts += 1,
                }
                if !species_captured.contains(&u.species) {
                    species_captured.push(u.species);
                }
            }
        }
    }

    // Everything that walked the field goes in the field reports.
    let mut species_seen = Vec::new();
    let mut species_slain = Vec::new();
    for u in battle.units.iter().skip(squad_len) {
        if !u.civilian && !species_seen.contains(&u.species) {
            species_seen.push(u.species);
        }
        if !u.alive && !u.civilian && !species_slain.contains(&u.species) {
            species_slain.push(u.species);
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
        injuries,
        severed,
        demons_slain: demons_total.saturating_sub(battle.living(Side::Demons).count() as u32),
        captured_grunts,
        captured_overseers,
        civilians_saved,
        civilians_dead,
        species_seen,
        species_captured,
        horrors,
        species_slain,
        atrocities_found: battle.atrocities.iter().filter(|(_, found)| *found).count() as u32,
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
    if s.has_lance && research.is_complete(Project::HellfireLance) {
        // A forged lance replaces everything else outright.
        u.weapon = ods_sim::units::hellfire_lance();
    } else {
        // The issued weapon, from the armoury tables.
        let display = match s.weapon_key.as_str() {
            "arbalest" => "consecrated arbalest",
            "censer" => "censer",
            "ram_hammer" => "ram hammer",
            "salt_mortar" => "salt-shot mortar",
            _ => "consecrated rifle",
        };
        let key = if ods_sim::data::weapons().contains_key(&s.weapon_key) {
            s.weapon_key.as_str()
        } else {
            "rifle"
        };
        u.weapon = ods_sim::units::Weapon::from_data(display, key);
        if research.is_complete(Project::BlessedArms) {
            u.weapon.power += 8;
        }
    }
    u.blade = s.has_blade;
    u.circlet = s.has_circlet;
    match s.armor {
        crate::campaign::ArmorTier::Vestments => {}
        crate::campaign::ArmorTier::Plate => {
            u.armor_front += 3;
            u.armor_side += 2;
            u.armor_rear += 1;
            u.health_max += 8;
            u.tu_max -= 2;
        }
        crate::campaign::ArmorTier::Aegis => {
            u.armor_front += 6;
            u.armor_side += 5;
            u.armor_rear += 3;
            u.health_max += 12;
            u.tu_max -= 6;
        }
    }
    u.health = u.health_max;
    if let Some(relic) = &s.relic {
        match relic.affix {
            crate::campaign::Affix::Vigil => u.reactions += 10,
            crate::campaign::Affix::SteadyHand => u.accuracy += 8,
            crate::campaign::Affix::Vigor => u.tu_max += 5,
            crate::campaign::Affix::Bulwark => {
                u.armor_front += 2;
                u.armor_side += 2;
                u.armor_rear += 2;
            }
            crate::campaign::Affix::Grisly => u.bravery = (u.bravery + 8).min(95),
        }
    }
    // Officers rally (wave T's teeth, wired where the rank already lives).
    u.can_rally = s.missions + s.kills * 2 >= 13;
    for &part in &s.lost_parts {
        if !u.injuries.contains(&part) {
            u.injuries.push(part);
        }
        u.severed.push(part);
    }
    match s.quirk {
        Some(crate::campaign::Quirk::Marksman) => u.accuracy += 8,
        Some(crate::campaign::Quirk::Squeamish) => u.bravery = (u.bravery - 8).max(5),
        Some(crate::campaign::Quirk::Jumpy) => {
            u.bravery = (u.bravery - 10).max(5);
            u.reactions += 8;
        }
        Some(crate::campaign::Quirk::IronNerves) => u.bravery = (u.bravery + 15).min(95),
        Some(crate::campaign::Quirk::Swift) => u.tu_max += 5,
        _ => {}
    }
    let (grenades, dressings) = kit;
    u.grenades = grenades;
    u.heal_charges = dressings;
    // An overloaded pack slows the hand (unless born to haul).
    if grenades + dressings > 5 && s.quirk != Some(crate::campaign::Quirk::PackMule) {
        u.tu_max -= 4;
    }
    u.tu = u.tu_max;
    u
}

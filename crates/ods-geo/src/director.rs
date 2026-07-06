//! The invisible director: hell's monthly plan. Like the original's alien
//! mission generator, it creates a readable, escalating rhythm of incursions
//! the player learns to interpret and disrupt.

use crate::geography::Region;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RiftKind {
    /// Probing the veil. Weak garrison, small stakes.
    Scouting,
    /// Reaping souls from the countryside. Lootable in later versions.
    Harvest,
    /// A massacre in a population center. Ignoring it is very costly.
    Terror,
    /// Cultists suborn a regional government: permanent funding damage.
    Infiltration,
    /// A stabilizing rift that births a permanent nest if left alone.
    NestBuilding,
}

impl RiftKind {
    /// Days the rift stays open before its mission completes.
    pub fn lifetime(self) -> u32 {
        match self {
            RiftKind::Scouting => 3,
            RiftKind::Harvest => 5,
            RiftKind::Terror => 4,
            RiftKind::Infiltration => 6,
            RiftKind::NestBuilding => 7,
        }
    }

    /// Demons defending the rift site.
    pub fn garrison(self) -> u32 {
        match self {
            RiftKind::Scouting => 3,
            RiftKind::Harvest => 4,
            RiftKind::Terror => 6,
            RiftKind::Infiltration => 5,
            RiftKind::NestBuilding => 5,
        }
    }

    /// Score for banishing the incursion.
    pub fn banish_score(self) -> i64 {
        match self {
            RiftKind::Scouting => 15,
            RiftKind::Harvest => 25,
            RiftKind::Terror => 40,
            RiftKind::Infiltration => 30,
            RiftKind::NestBuilding => 20,
        }
    }

    /// Score penalty when the rift completes its mission unopposed.
    pub fn expire_penalty(self) -> i64 {
        match self {
            RiftKind::Scouting => 10,
            RiftKind::Harvest => 20,
            RiftKind::Terror => 40,
            RiftKind::Infiltration => 30,
            RiftKind::NestBuilding => 15,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            RiftKind::Scouting => "scouting incursion",
            RiftKind::Harvest => "soul harvest",
            RiftKind::Terror => "massacre",
            RiftKind::Infiltration => "cult infiltration",
            RiftKind::NestBuilding => "nest founding",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Rift {
    pub id: u32,
    pub kind: RiftKind,
    pub region: Region,
    pub days_left: u32,
    pub detected: bool,
}

/// A completed NestBuilding mission: bleeds score daily until razed.
#[derive(Clone, Debug)]
pub struct Nest {
    pub id: u32,
    pub region: Region,
}

/// Demons defending an established nest (tougher than any rift).
pub const NEST_GARRISON: u32 = 7;
pub const NEST_RAZE_SCORE: i64 = 50;
pub const NEST_DAILY_PENALTY: i64 = 5;

/// One rift the director has scheduled for the coming month.
#[derive(Clone, Copy, Debug)]
pub struct PlannedRift {
    pub day: u32,
    pub kind: RiftKind,
    pub region: Region,
}

/// Draw up hell's plan for a month. Escalates with the campaign month.
pub fn plan_month(rng: &mut ods_sim::SimRng, month: u32) -> Vec<PlannedRift> {
    let count = (3 + month).min(12);
    let mut plan: Vec<PlannedRift> = (0..count)
        .map(|_| {
            let kind = match rng.roll(100) {
                0..30 => RiftKind::Scouting,
                30..55 => RiftKind::Harvest,
                55..75 => RiftKind::NestBuilding,
                75..90 => RiftKind::Infiltration,
                _ => RiftKind::Terror,
            };
            PlannedRift {
                day: 1 + rng.roll(28),
                kind,
                region: Region::ALL[rng.roll(Region::ALL.len() as u32) as usize],
            }
        })
        .collect();
    plan.sort_by_key(|p| p.day);
    plan
}

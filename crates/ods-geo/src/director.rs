//! The invisible director: hell's monthly plan. Like the original's alien
//! mission generator, it creates a readable, escalating rhythm of incursions
//! the player learns to interpret and disrupt.

use crate::geography::Region;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
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

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Rift {
    pub id: u32,
    pub kind: RiftKind,
    pub region: Region,
    /// Where on the globe reality tore (degrees).
    pub lat: f32,
    pub lon: f32,
    pub days_left: u32,
    /// Days since the rift opened. It stabilizes after two days.
    pub days_open: u32,
    pub detected: bool,
}

impl Rift {
    /// A fresh rift is chaotic and lightly held; a stabilized one has dug in.
    pub fn is_stabilized(&self) -> bool {
        self.days_open >= 2
    }

    /// Garrison actually defending the site right now — the reason to strike
    /// fast instead of waiting for a convenient day.
    pub fn effective_garrison(&self) -> u32 {
        if self.is_stabilized() {
            self.kind.garrison() + 2
        } else {
            self.kind.garrison().saturating_sub(1).max(2)
        }
    }
}

/// A completed NestBuilding mission: bleeds score daily until razed.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Nest {
    pub id: u32,
    pub region: Region,
    pub lat: f32,
    pub lon: f32,
}

/// Demons defending an established nest (tougher than any rift).
pub const NEST_GARRISON: u32 = 7;
pub const NEST_RAZE_SCORE: i64 = 50;
pub const NEST_DAILY_PENALTY: i64 = 5;

/// One rift the director has scheduled for the coming month.
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
pub struct PlannedRift {
    pub day: u32,
    pub kind: RiftKind,
    pub region: Region,
}

/// Draw up hell's plan for a month. Escalates with the campaign month;
/// `extra` is the difficulty's cruelty bonus.
pub fn plan_month(rng: &mut ods_sim::SimRng, month: u32, extra: u32) -> Vec<PlannedRift> {
    let count = (3 + month + extra).min(14);
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

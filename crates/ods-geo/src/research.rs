//! The Forbidden Codex. Occultists grind through projects at a rate limited
//! by headcount and library space. The deeper entries demand more than time:
//! materials torn from banished rifts, and living demons to question.

use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum Project {
    /// Consecrated ammunition: +8 weapon power in battle.
    BlessedArms,
    /// Hellsteel-laced vestments: +8 soldier health in battle.
    HellsteelPlate,
    /// Reading the veil's tremors: +15% rift detection everywhere.
    RiftAugury,
    /// Hell's own fire, bound in a lance: +16 weapon power (supersedes
    /// Blessed Arms). Requires salvaged materials to even begin.
    HellfireLance,
    /// Question a bound demon: +10% detection (they talk, eventually).
    Interrogation,
    /// Break an Overseer's silence: the way to the Name is open.
    HeraldsConfession,
    /// The arch-demon's true name. Unlocks the final assault.
    NameOfTheEnemy,
}

impl Project {
    pub const ALL: [Project; 7] = [
        Project::BlessedArms,
        Project::HellsteelPlate,
        Project::RiftAugury,
        Project::HellfireLance,
        Project::Interrogation,
        Project::HeraldsConfession,
        Project::NameOfTheEnemy,
    ];

    /// Total occultist-days required.
    pub fn cost(self) -> u32 {
        match self {
            Project::BlessedArms => 120,
            Project::HellsteelPlate => 160,
            Project::RiftAugury => 100,
            Project::HellfireLance => 200,
            Project::Interrogation => 80,
            Project::HeraldsConfession => 150,
            Project::NameOfTheEnemy => 250,
        }
    }

    /// Salvage consumed when the project begins: (brimstone, hellsteel).
    pub fn materials(self) -> (u32, u32) {
        match self {
            Project::HellfireLance => (10, 15),
            _ => (0, 0),
        }
    }

    /// Bound demons consumed when the project begins: (grunts, overseers).
    /// Interrogation is not gentle.
    pub fn prisoners(self) -> (u32, u32) {
        match self {
            Project::Interrogation => (1, 0),
            Project::HeraldsConfession => (0, 1),
            Project::NameOfTheEnemy => (0, 2),
        _ => (0, 0),
        }
    }

    /// Must be complete before this project can start.
    pub fn prerequisite(self) -> Option<Project> {
        match self {
            Project::HellfireLance => Some(Project::BlessedArms),
            Project::HeraldsConfession => Some(Project::Interrogation),
            Project::NameOfTheEnemy => Some(Project::HeraldsConfession),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Project::BlessedArms => "Blessed Arms",
            Project::HellsteelPlate => "Hellsteel Plate",
            Project::RiftAugury => "Rift Augury",
            Project::HellfireLance => "Hellfire Lance",
            Project::Interrogation => "Interrogation",
            Project::HeraldsConfession => "The Herald's Confession",
            Project::NameOfTheEnemy => "The Name of the Enemy",
        }
    }
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct ResearchState {
    pub completed: HashSet<Project>,
    pub active: Option<(Project, u32)>,
}

impl ResearchState {
    /// Advance one day at the given effective occultist count. Returns the
    /// project if it completed today.
    pub fn advance_day(&mut self, effective_occultists: u32) -> Option<Project> {
        let (project, left) = self.active.as_mut()?;
        *left = left.saturating_sub(effective_occultists);
        if *left == 0 {
            let done = *project;
            self.active = None;
            self.completed.insert(done);
            Some(done)
        } else {
            None
        }
    }

    pub fn is_complete(&self, project: Project) -> bool {
        self.completed.contains(&project)
    }
}

/// What the workshop can produce.
#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum ManufactureItem {
    /// +4 hellfire charges to the armoury (costs 2 brimstone).
    HellfireCharges,
    /// +4 field dressings to the infirmary stores.
    FieldDressings,
    /// Blessed arms for national reliquaries: sold on completion for 45k
    /// (costs 5 hellsteel). The tamed factory economy.
    TradeArms,
    /// Forge one Hellfire Lance for the armoury (needs the research).
    ForgeLance,
}

impl ManufactureItem {
    pub const ALL: [ManufactureItem; 4] = [
        ManufactureItem::HellfireCharges,
        ManufactureItem::FieldDressings,
        ManufactureItem::TradeArms,
        ManufactureItem::ForgeLance,
    ];

    /// Artificer-days of work.
    pub fn cost(self) -> u32 {
        match self {
            ManufactureItem::HellfireCharges => 40,
            ManufactureItem::FieldDressings => 30,
            ManufactureItem::TradeArms => 60,
            ManufactureItem::ForgeLance => 50,
        }
    }

    /// (brimstone, hellsteel) consumed at start.
    pub fn materials(self) -> (u32, u32) {
        match self {
            ManufactureItem::HellfireCharges => (2, 0),
            ManufactureItem::FieldDressings => (0, 0),
            ManufactureItem::TradeArms => (0, 5),
            ManufactureItem::ForgeLance => (2, 4),
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ManufactureItem::HellfireCharges => "Hellfire charges",
            ManufactureItem::FieldDressings => "Field dressings",
            ManufactureItem::TradeArms => "Trade arms",
            ManufactureItem::ForgeLance => "Forge a hellfire lance",
        }
    }
}

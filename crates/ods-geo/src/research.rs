//! The Forbidden Codex, volume one. Occultists grind through projects at a
//! rate limited by both headcount and library space.

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
}

impl Project {
    pub const ALL: [Project; 4] = [
        Project::BlessedArms,
        Project::HellsteelPlate,
        Project::RiftAugury,
        Project::HellfireLance,
    ];

    /// Total occultist-days required.
    pub fn cost(self) -> u32 {
        match self {
            Project::BlessedArms => 120,
            Project::HellsteelPlate => 160,
            Project::RiftAugury => 100,
            Project::HellfireLance => 200,
        }
    }

    /// Salvage consumed when the project begins: (brimstone, hellsteel).
    pub fn materials(self) -> (u32, u32) {
        match self {
            Project::HellfireLance => (10, 15),
            _ => (0, 0),
        }
    }

    /// Must be complete before this project can start.
    pub fn prerequisite(self) -> Option<Project> {
        match self {
            Project::HellfireLance => Some(Project::BlessedArms),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Project::BlessedArms => "Blessed Arms",
            Project::HellsteelPlate => "Hellsteel Plate",
            Project::RiftAugury => "Rift Augury",
            Project::HellfireLance => "Hellfire Lance",
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

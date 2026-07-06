//! The Geoscape: the strategic campaign layer.
//!
//! Same rules as the tactical crate: headless, deterministic, no wall-clock.
//! A campaign is a pure function of (seed, decision list). Battles are not
//! abstracted dice rolls — an assault runs a real `ods-sim` battle with AI on
//! both sides and feeds the outcome (deaths, wounds, survivors) back into the
//! roster.

mod base;
mod campaign;
mod director;
mod geography;
mod missions;
mod research;

pub use base::{Chapterhouse, Facility, GRID};
pub use campaign::{
    ARTIFICER_HIRE_COST, Campaign, CampaignOutcome, CampaignStats, CHAPTERHOUSE_COST, Difficulty,
    Fallen, FINAL_ASSAULT_BRIMSTONE, GeoError, GeoEvent, MissionKind, MissionToken,
    OCCULTIST_HIRE_COST, PANIC_BREAKPOINT, Prisoners, Quirk, SOLDIER_HIRE_COST, Soldier,
    SoldierStats, Sortie,
};
pub use director::{Nest, Rift, RiftKind};
pub use geography::Region;
pub use missions::BattleReport;
pub use research::{ManufactureItem, Project, ResearchState};

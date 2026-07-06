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
    Campaign, CampaignOutcome, GeoError, GeoEvent, Soldier, SoldierStats,
};
pub use director::{Nest, Rift, RiftKind};
pub use missions::BattleReport;
pub use research::Project;

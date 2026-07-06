//! The campaign state machine: days in, events out, deterministic given the
//! seed and the player's decisions.

use std::collections::HashMap;

use ods_sim::SimRng;

use crate::base::{Chapterhouse, Facility};
use crate::director::{
    self, NEST_DAILY_PENALTY, NEST_GARRISON, NEST_RAZE_SCORE, Nest, PlannedRift, Rift, RiftKind,
};
use crate::geography::Region;
use crate::missions::{self, BattleReport};
use crate::research::{Project, ResearchState};

pub const DAYS_PER_MONTH: u32 = 30;
pub const STARTING_FUNDS: i64 = 2000;
pub const SOLDIER_HIRE_COST: i64 = 40;
pub const SOLDIER_SALARY: i64 = 20;
pub const OCCULTIST_HIRE_COST: i64 = 60;
pub const OCCULTIST_SALARY: i64 = 30;
pub const SQUAD_SIZE: usize = 6;
/// A month at or below this score is a "losing badly" month.
pub const BAD_MONTH_SCORE: i64 = -100;
pub const DEBT_LIMIT: i64 = -500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SoldierStats {
    pub tu: i32,
    pub health: i32,
    pub reactions: i32,
    pub accuracy: i32,
    pub bravery: i32,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Soldier {
    pub name: String,
    pub stats: SoldierStats,
    /// Days until fit for duty. 0 = ready.
    pub recovery_days: u32,
    pub missions: u32,
    pub kills: u32,
}

impl Soldier {
    pub fn is_fit(&self) -> bool {
        self.recovery_days == 0
    }
}

/// Learn-by-doing, deterministic: stats grow from what a soldier actually
/// did, with hard caps. There are no classes — biography is the build.
fn apply_growth(stats: &mut SoldierStats, xp: ods_sim::battle::Experience) {
    stats.accuracy = (stats.accuracy + (xp.shots_hit as i32 / 2).min(3)).min(95);
    stats.reactions = (stats.reactions + (xp.reaction_shots as i32 / 2).min(2)).min(90);
    stats.bravery = (stats.bravery + 4 * xp.dread_survived as i32).min(90);
    if xp.shots_hit > 0 {
        stats.tu = (stats.tu + 1).min(65);
    }
    if xp.kills >= 2 {
        stats.health = (stats.health + 1).min(40);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum CampaignOutcome {
    /// Two consecutive badly-losing months: the council pulls the plug.
    FundingWithdrawn,
    /// Debt beyond the limit.
    Bankrupt,
    /// A Reckoning overran the chapterhouse.
    ChapterhouseFallen,
}

#[derive(Clone, Debug, PartialEq)]
pub enum GeoEvent {
    FacilityComplete { facility: Facility },
    ResearchComplete { project: Project },
    SoldierRecovered { name: String },
    RiftDetected { id: u32, kind: RiftKind, region: Region, days_left: u32 },
    RiftExpired { id: u32, kind: RiftKind, region: Region, penalty: i64 },
    NestFounded { id: u32, region: Region },
    RegionInfiltrated { region: Region },
    MonthlyReport { month: u32, score: i64, income: i64, expenses: i64, funds: i64 },
    /// Demons assaulted the chapterhouse and were driven out.
    ReckoningRepelled { demons_slain: u32, dead: usize },
    CampaignOver { outcome: CampaignOutcome },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GeoError {
    CampaignOver,
    UnknownRift,
    UnknownNest,
    NotDetected,
    NoSquadFit,
    NoFunds,
    Occupied,
    QuartersFull,
    ResearchBusy,
    AlreadyResearched,
    /// Not enough salvaged brimstone/hellsteel.
    NoMaterials,
    /// A required project is not yet complete.
    PrerequisiteMissing,
}

/// A ground mission the campaign can stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionKind {
    Rift(u32),
    Nest(u32),
    Reckoning,
}

/// Receipt from [`Campaign::begin_mission`]; hand it back to
/// [`Campaign::conclude_mission`] with the finished battle. Not saveable —
/// finish the fight before the world moves on.
pub struct MissionToken {
    kind: MissionKind,
    squad_idx: Vec<usize>,
}

const RECRUIT_NAMES: [&str; 16] = [
    "Adeyemi", "Brandt", "Castillo", "Dubois", "Eriksen", "Farah", "Grigorescu", "Hale",
    "Iwata", "Jansen", "Karimi", "Lindqvist", "Mbeki", "Novak", "Oyelaran", "Petrov",
];

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Campaign {
    pub funds: i64,
    pub month: u32,
    /// Day of month, 1..=DAYS_PER_MONTH.
    pub day: u32,
    pub base: Chapterhouse,
    pub soldiers: Vec<Soldier>,
    pub occultists: u32,
    pub region_funding: HashMap<Region, i64>,
    pub rifts: Vec<Rift>,
    pub nests: Vec<Nest>,
    pub research: ResearchState,
    /// Salvage stockpiles from banished incursions.
    pub brimstone: u32,
    pub hellsteel: u32,
    pub month_score: i64,
    pub bad_months: u32,
    pub over: Option<CampaignOutcome>,
    /// Rises with every banishment; at 5+, hell schedules a Reckoning.
    reckoning_heat: u32,
    reckoning_day: Option<u32>,
    month_plan: Vec<PlannedRift>,
    region_score: HashMap<Region, i64>,
    rng: SimRng,
    next_id: u32,
    recruits_hired: usize,
}

impl Campaign {
    pub fn new(seed: u64) -> Self {
        let mut rng = SimRng::from_seed(seed);
        let mut c = Self {
            funds: STARTING_FUNDS,
            month: 1,
            day: 1,
            base: Chapterhouse::founding(Region::Europe),
            soldiers: Vec::new(),
            occultists: 4,
            region_funding: Region::ALL.iter().map(|&r| (r, 150)).collect(),
            rifts: Vec::new(),
            nests: Vec::new(),
            research: ResearchState::default(),
            brimstone: 0,
            hellsteel: 0,
            month_score: 0,
            bad_months: 0,
            over: None,
            reckoning_heat: 0,
            reckoning_day: None,
            month_plan: director::plan_month(&mut rng, 1),
            region_score: HashMap::new(),
            rng,
            next_id: 0,
            recruits_hired: 0,
        };
        for _ in 0..6 {
            let s = c.roll_recruit();
            c.soldiers.push(s);
        }
        c
    }

    fn roll_recruit(&mut self) -> Soldier {
        let name = format!(
            "{} {}",
            RECRUIT_NAMES[self.recruits_hired % RECRUIT_NAMES.len()],
            // Roman-numeral-ish suffix once the name list wraps.
            if self.recruits_hired >= RECRUIT_NAMES.len() { "II" } else { "" }
        )
        .trim()
        .to_string();
        self.recruits_hired += 1;
        Soldier {
            name,
            stats: SoldierStats {
                tu: 50 + self.rng.roll(11) as i32,
                health: 28 + self.rng.roll(9) as i32,
                reactions: 40 + self.rng.roll(21) as i32,
                accuracy: 50 + self.rng.roll(21) as i32,
                bravery: 20 + self.rng.roll(41) as i32,
            },
            recovery_days: 0,
            missions: 0,
            kills: 0,
        }
    }

    fn guard_over(&self) -> Result<(), GeoError> {
        if self.over.is_some() {
            Err(GeoError::CampaignOver)
        } else {
            Ok(())
        }
    }

    // ------------------------------------------------------------------
    // Save / load

    /// Serialize the entire campaign — including the RNG state, so a loaded
    /// game continues with an identical stream of fate.
    pub fn save_to_string(&self) -> String {
        serde_json::to_string(self).expect("campaign state is always serializable")
    }

    pub fn load_from_str(save: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(save)
    }

    // ------------------------------------------------------------------
    // Player decisions

    pub fn hire_soldier(&mut self) -> Result<&Soldier, GeoError> {
        self.guard_over()?;
        if self.funds < SOLDIER_HIRE_COST {
            return Err(GeoError::NoFunds);
        }
        if self.soldiers.len() + self.occultists as usize >= self.base.quarters_capacity() {
            return Err(GeoError::QuartersFull);
        }
        self.funds -= SOLDIER_HIRE_COST;
        let s = self.roll_recruit();
        self.soldiers.push(s);
        Ok(self.soldiers.last().expect("just pushed"))
    }

    pub fn hire_occultist(&mut self) -> Result<(), GeoError> {
        self.guard_over()?;
        if self.funds < OCCULTIST_HIRE_COST {
            return Err(GeoError::NoFunds);
        }
        if self.soldiers.len() + self.occultists as usize >= self.base.quarters_capacity() {
            return Err(GeoError::QuartersFull);
        }
        self.funds -= OCCULTIST_HIRE_COST;
        self.occultists += 1;
        Ok(())
    }

    pub fn start_build(&mut self, facility: Facility, x: usize, y: usize) -> Result<(), GeoError> {
        self.guard_over()?;
        if self.funds < facility.cost() {
            return Err(GeoError::NoFunds);
        }
        if !self.base.start_build(facility, x, y) {
            return Err(GeoError::Occupied);
        }
        self.funds -= facility.cost();
        Ok(())
    }

    pub fn start_research(&mut self, project: Project) -> Result<(), GeoError> {
        self.guard_over()?;
        if self.research.is_complete(project) {
            return Err(GeoError::AlreadyResearched);
        }
        if self.research.active.is_some() {
            return Err(GeoError::ResearchBusy);
        }
        if let Some(prereq) = project.prerequisite()
            && !self.research.is_complete(prereq)
        {
            return Err(GeoError::PrerequisiteMissing);
        }
        let (brim, steel) = project.materials();
        if self.brimstone < brim || self.hellsteel < steel {
            return Err(GeoError::NoMaterials);
        }
        self.brimstone -= brim;
        self.hellsteel -= steel;
        self.research.active = Some((project, project.cost()));
        Ok(())
    }

    /// Sell salvage to national reliquaries (brimstone 15k, hellsteel 5k each).
    pub fn sell_brimstone(&mut self, amount: u32) -> Result<i64, GeoError> {
        self.guard_over()?;
        if amount > self.brimstone {
            return Err(GeoError::NoMaterials);
        }
        self.brimstone -= amount;
        let gain = amount as i64 * 15;
        self.funds += gain;
        Ok(gain)
    }

    pub fn sell_hellsteel(&mut self, amount: u32) -> Result<i64, GeoError> {
        self.guard_over()?;
        if amount > self.hellsteel {
            return Err(GeoError::NoMaterials);
        }
        self.hellsteel -= amount;
        let gain = amount as i64 * 5;
        self.funds += gain;
        Ok(gain)
    }

    /// Send the squad through a detected rift, AI-resolved. The battle
    /// really happens; `begin_mission`/`conclude_mission` is the same path
    /// the interactive Battlescape uses.
    pub fn assault_rift(&mut self, rift_id: u32) -> Result<BattleReport, GeoError> {
        self.fight(MissionKind::Rift(rift_id))
    }

    /// What the field teams drag back from a banished incursion.
    fn collect_salvage(&mut self, kind: RiftKind, demons_slain: u32) {
        self.hellsteel += demons_slain;
        self.brimstone += match kind {
            RiftKind::Scouting => 1,
            RiftKind::Harvest => 4,
            RiftKind::Terror => 2,
            RiftKind::Infiltration => 2,
            RiftKind::NestBuilding => 3,
        };
    }

    /// Storm an established nest, AI-resolved.
    pub fn raze_nest(&mut self, nest_id: u32) -> Result<BattleReport, GeoError> {
        self.fight(MissionKind::Nest(nest_id))
    }

    /// Set up a mission as a real Battle for the caller to drive — either
    /// interactively (the Battlescape screen) or via AI. Squad selection,
    /// research bonuses, and the battle seed all come from campaign state.
    /// Feed the finished battle back through [`Campaign::conclude_mission`].
    pub fn begin_mission(
        &mut self,
        kind: MissionKind,
    ) -> Result<(ods_sim::battle::Battle, MissionToken), GeoError> {
        self.guard_over()?;
        let (garrison, defense) = match kind {
            MissionKind::Rift(id) => {
                let rift = self
                    .rifts
                    .iter()
                    .find(|r| r.id == id)
                    .ok_or(GeoError::UnknownRift)?;
                if !rift.detected {
                    return Err(GeoError::NotDetected);
                }
                (rift.effective_garrison(), false)
            }
            MissionKind::Nest(id) => {
                self.nests
                    .iter()
                    .find(|n| n.id == id)
                    .ok_or(GeoError::UnknownNest)?;
                (NEST_GARRISON, false)
            }
            MissionKind::Reckoning => ((5 + self.month / 2).min(8), true),
        };

        let squad_idx: Vec<usize> = self
            .soldiers
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_fit())
            .map(|(i, _)| i)
            .take(SQUAD_SIZE)
            .collect();
        if squad_idx.is_empty() {
            return Err(GeoError::NoSquadFit);
        }
        let squad: Vec<&Soldier> = squad_idx.iter().map(|&i| &self.soldiers[i]).collect();
        let seed = (self.rng.roll(1 << 30) as u64) << 30 | self.rng.roll(1 << 30) as u64;
        let battle = if defense {
            missions::build_defense(
                seed,
                &squad,
                garrison,
                &self.research,
                &self.base.occupied_cells(),
                self.base.gate(),
            )
        } else {
            missions::build_assault(seed, &squad, garrison, &self.research)
        };
        Ok((battle, MissionToken { kind, squad_idx }))
    }

    /// Fold a finished battle back into the campaign: casualties, wounds,
    /// growth, and the strategic outcome of the mission.
    pub fn conclude_mission(
        &mut self,
        token: MissionToken,
        battle: &ods_sim::battle::Battle,
    ) -> BattleReport {
        let report = missions::report_from(battle, token.squad_idx.len());
        self.apply_to_roster(&token.squad_idx, &report);

        match token.kind {
            MissionKind::Rift(id) => {
                if let Some(rift) = self.rifts.iter().find(|r| r.id == id) {
                    let (kind, region) = (rift.kind, rift.region);
                    if report.victory {
                        self.rifts.retain(|r| r.id != id);
                        self.score(region, kind.banish_score());
                        self.collect_salvage(kind, report.demons_slain);
                        self.reckoning_heat += 1;
                    } else {
                        // The squad withdraws; the incursion continues.
                        self.score(region, -5);
                    }
                }
            }
            MissionKind::Nest(id) => {
                if let Some(nest) = self.nests.iter().find(|n| n.id == id) {
                    let region = nest.region;
                    if report.victory {
                        self.nests.retain(|n| n.id != id);
                        self.score(region, NEST_RAZE_SCORE);
                        self.brimstone += 6;
                        self.hellsteel += report.demons_slain;
                        self.reckoning_heat += 1;
                    } else {
                        self.score(region, -5);
                    }
                }
            }
            MissionKind::Reckoning => {
                if report.victory {
                    self.score(self.base.region, 30);
                } else {
                    self.over = Some(CampaignOutcome::ChapterhouseFallen);
                }
            }
        }
        report
    }

    fn fight(&mut self, kind: MissionKind) -> Result<BattleReport, GeoError> {
        let (mut battle, token) = self.begin_mission(kind)?;
        missions::run_auto(&mut battle);
        Ok(self.conclude_mission(token, &battle))
    }

    /// Casualties are removed, the wounded convalesce roughly a day per
    /// missing hit point, and survivors grow by what they did out there.
    fn apply_to_roster(&mut self, squad_idx: &[usize], report: &BattleReport) {
        let infirmary = self.base.count_active(Facility::Infirmary) > 0;
        for &(squad_pos, health, xp) in &report.survivors {
            let s = &mut self.soldiers[squad_idx[squad_pos]];
            let missing = (s.stats.health - health).max(0) as u32;
            s.recovery_days += if infirmary { missing / 2 } else { missing };
            s.missions += 1;
            s.kills += xp.kills;
            apply_growth(&mut s.stats, xp);
        }
        let mut dead_roster: Vec<usize> = report.dead.iter().map(|&p| squad_idx[p]).collect();
        dead_roster.sort_unstable_by(|a, b| b.cmp(a));
        for idx in dead_roster {
            self.soldiers.remove(idx);
        }
    }

    fn score(&mut self, region: Region, delta: i64) {
        self.month_score += delta;
        *self.region_score.entry(region).or_insert(0) += delta;
    }

    /// Hell answers success. Fought in the chapterhouse's own floor plan;
    /// with no fit defenders, the chapterhouse simply falls.
    fn resolve_reckoning(&mut self, events: &mut Vec<GeoEvent>) {
        match self.fight(MissionKind::Reckoning) {
            Ok(report) if report.victory => {
                events.push(GeoEvent::ReckoningRepelled {
                    demons_slain: report.demons_slain,
                    dead: report.dead.len(),
                });
            }
            Ok(_) => {
                // `conclude_mission` already marked the campaign over.
                events.push(GeoEvent::CampaignOver {
                    outcome: CampaignOutcome::ChapterhouseFallen,
                });
            }
            Err(GeoError::NoSquadFit) => {
                self.over = Some(CampaignOutcome::ChapterhouseFallen);
                events.push(GeoEvent::CampaignOver {
                    outcome: CampaignOutcome::ChapterhouseFallen,
                });
            }
            Err(_) => {}
        }
    }

    // ------------------------------------------------------------------
    // The clock

    pub fn advance_day(&mut self) -> Vec<GeoEvent> {
        let mut events = Vec::new();
        if self.over.is_some() {
            return events;
        }

        // Construction.
        for facility in self.base.advance_day() {
            events.push(GeoEvent::FacilityComplete { facility });
        }

        // Research, throttled by library space.
        let effective = self.occultists.min(self.base.library_capacity() as u32);
        if let Some(project) = self.research.advance_day(effective) {
            events.push(GeoEvent::ResearchComplete { project });
        }

        // Convalescence.
        for s in &mut self.soldiers {
            if s.recovery_days > 0 {
                s.recovery_days -= 1;
                if s.recovery_days == 0 {
                    events.push(GeoEvent::SoldierRecovered { name: s.name.clone() });
                }
            }
        }

        // New rifts scheduled for today.
        let due: Vec<PlannedRift> = self
            .month_plan
            .iter()
            .filter(|p| p.day == self.day)
            .copied()
            .collect();
        for plan in due {
            self.rifts.push(Rift {
                id: self.next_id,
                kind: plan.kind,
                region: plan.region,
                days_left: plan.kind.lifetime(),
                days_open: 0,
                detected: false,
            });
            self.next_id += 1;
        }

        // Detection sweeps.
        let augury_bonus = if self.research.is_complete(Project::RiftAugury) { 15 } else { 0 };
        let home_chance =
            (25 + 25 * self.base.count_active(Facility::AugurArray) as u32 + augury_bonus).min(90);
        let away_chance = (10 + augury_bonus).min(90);
        for i in 0..self.rifts.len() {
            if self.rifts[i].detected {
                continue;
            }
            let chance = if self.rifts[i].region == self.base.region {
                home_chance
            } else {
                away_chance
            };
            if self.rng.roll(100) < chance {
                let r = &mut self.rifts[i];
                r.detected = true;
                events.push(GeoEvent::RiftDetected {
                    id: r.id,
                    kind: r.kind,
                    region: r.region,
                    days_left: r.days_left,
                });
            }
        }

        // A scheduled Reckoning arrives.
        if self.reckoning_day == Some(self.day) {
            self.reckoning_day = None;
            self.resolve_reckoning(&mut events);
            if self.over.is_some() {
                return events;
            }
        }

        // Rift missions run their course (and dig in as they age).
        let mut expired = Vec::new();
        for r in &mut self.rifts {
            r.days_open += 1;
            r.days_left -= 1;
            if r.days_left == 0 {
                expired.push((r.id, r.kind, r.region));
            }
        }
        self.rifts.retain(|r| r.days_left > 0);
        for (id, kind, region) in expired {
            let penalty = kind.expire_penalty();
            self.score(region, -penalty);
            events.push(GeoEvent::RiftExpired { id, kind, region, penalty });
            match kind {
                RiftKind::NestBuilding => {
                    self.nests.push(Nest { id: self.next_id, region });
                    events.push(GeoEvent::NestFounded { id: self.next_id, region });
                    self.next_id += 1;
                }
                RiftKind::Infiltration => {
                    let f = self.region_funding.get_mut(&region).expect("region exists");
                    *f /= 2;
                    events.push(GeoEvent::RegionInfiltrated { region });
                }
                _ => {}
            }
        }

        // Standing nests poison their regions.
        let nest_regions: Vec<Region> = self.nests.iter().map(|n| n.region).collect();
        for region in nest_regions {
            self.score(region, -NEST_DAILY_PENALTY);
        }

        // Month end.
        if self.day == DAYS_PER_MONTH {
            events.extend(self.monthly_report());
        } else {
            self.day += 1;
        }
        events
    }

    fn monthly_report(&mut self) -> Vec<GeoEvent> {
        let mut events = Vec::new();

        // The council reads the month's regional scores.
        for region in Region::ALL {
            let score = self.region_score.get(&region).copied().unwrap_or(0);
            let funding = self.region_funding.get_mut(&region).expect("region exists");
            if score >= 20 {
                *funding += (*funding / 10).max(5);
            } else if score <= -20 {
                *funding -= *funding / 10;
            }
        }
        let income: i64 = Region::ALL
            .iter()
            .map(|r| self.region_funding[r])
            .sum();
        let expenses = self.soldiers.len() as i64 * SOLDIER_SALARY
            + self.occultists as i64 * OCCULTIST_SALARY
            + self.base.maintenance();
        self.funds += income - expenses;

        events.push(GeoEvent::MonthlyReport {
            month: self.month,
            score: self.month_score,
            income,
            expenses,
            funds: self.funds,
        });

        if self.month_score <= BAD_MONTH_SCORE {
            self.bad_months += 1;
        } else {
            self.bad_months = 0;
        }
        if self.bad_months >= 2 {
            self.over = Some(CampaignOutcome::FundingWithdrawn);
            events.push(GeoEvent::CampaignOver { outcome: CampaignOutcome::FundingWithdrawn });
            return events;
        }
        if self.funds <= DEBT_LIMIT {
            self.over = Some(CampaignOutcome::Bankrupt);
            events.push(GeoEvent::CampaignOver { outcome: CampaignOutcome::Bankrupt });
            return events;
        }

        self.month += 1;
        self.day = 1;
        self.month_score = 0;
        self.region_score.clear();
        self.month_plan = director::plan_month(&mut self.rng, self.month);
        // Enough banishments and hell comes looking for the source.
        if self.reckoning_heat >= 5 {
            self.reckoning_heat = 0;
            self.reckoning_day = Some(1 + self.rng.roll(28));
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected_rift(c: &mut Campaign, kind: RiftKind, region: Region) -> u32 {
        let id = c.next_id;
        c.next_id += 1;
        c.rifts.push(Rift {
            id,
            kind,
            region,
            days_left: kind.lifetime(),
            days_open: 0,
            detected: true,
        });
        id
    }

    #[test]
    fn founding_state_is_sane() {
        let c = Campaign::new(1);
        assert_eq!(c.funds, STARTING_FUNDS);
        assert_eq!(c.soldiers.len(), 6);
        assert!(c.soldiers.iter().all(|s| s.is_fit()));
        assert_eq!(c.base.count_active(Facility::AugurArray), 1);
        assert!(c.base.quarters_capacity() >= 10);
        // Different recruits get different stats from the seeded roll.
        assert_ne!(c.soldiers[0].stats, c.soldiers[1].stats);
    }

    #[test]
    fn construction_takes_days_and_money() {
        let mut c = Campaign::new(2);
        let before = c.funds;
        c.start_build(Facility::Quarters, 0, 0).unwrap();
        assert_eq!(c.funds, before - Facility::Quarters.cost());
        assert_eq!(c.start_build(Facility::Quarters, 0, 0), Err(GeoError::Occupied));

        let mut completed = false;
        for _ in 0..Facility::Quarters.build_days() {
            let events = c.advance_day();
            completed |= events
                .iter()
                .any(|e| matches!(e, GeoEvent::FacilityComplete { facility: Facility::Quarters }));
        }
        assert!(completed, "quarters must finish in {} days", Facility::Quarters.build_days());
        assert_eq!(c.base.count_active(Facility::Quarters), 2);
    }

    #[test]
    fn hiring_respects_funds_and_beds() {
        let mut c = Campaign::new(3);
        let capacity = c.base.quarters_capacity();
        while c.soldiers.len() + (c.occultists as usize) < capacity {
            c.hire_soldier().unwrap();
        }
        assert_eq!(c.hire_soldier().err(), Some(GeoError::QuartersFull));
        c.funds = 10;
        assert_eq!(c.hire_occultist().err(), Some(GeoError::NoFunds));
    }

    #[test]
    fn research_completes_and_applies() {
        let mut c = Campaign::new(4);
        c.start_research(Project::RiftAugury).unwrap();
        assert_eq!(c.start_research(Project::BlessedArms), Err(GeoError::ResearchBusy));

        // 4 occultists, library capacity 5 -> 4 points/day, cost 100 -> 25 days.
        let mut days = 0;
        while !c.research.is_complete(Project::RiftAugury) {
            let events = c.advance_day();
            days += 1;
            if events
                .iter()
                .any(|e| matches!(e, GeoEvent::ResearchComplete { project: Project::RiftAugury }))
            {
                break;
            }
            assert!(days < 200, "research must terminate");
        }
        assert_eq!(days, 25);
        assert_eq!(
            c.start_research(Project::RiftAugury),
            Err(GeoError::AlreadyResearched)
        );
    }

    #[test]
    fn home_region_rifts_get_detected() {
        let mut c = Campaign::new(5);
        c.month_plan.clear(); // quiet month; we inject our own rift
        let id = c.next_id;
        c.next_id += 1;
        c.rifts.push(Rift {
            id,
            kind: RiftKind::Harvest,
            region: Region::Europe,
            days_left: 30,
            days_open: 0,
            detected: false,
        });
        let mut detected = false;
        for _ in 0..10 {
            let events = c.advance_day();
            if events
                .iter()
                .any(|e| matches!(e, GeoEvent::RiftDetected { id: rid, .. } if *rid == id))
            {
                detected = true;
                break;
            }
        }
        assert!(detected, "50%/day at home should detect within 10 days");
    }

    #[test]
    fn assault_resolves_a_real_battle() {
        let mut c = Campaign::new(6);
        let id = detected_rift(&mut c, RiftKind::Scouting, Region::Europe);
        let roster_before = c.soldiers.len();
        let score_before = c.month_score;

        let report = c.assault_rift(id).unwrap();
        assert!(report.turns > 0);
        assert_eq!(report.dead.len() + report.survivors.len(), 6.min(roster_before));

        if report.victory {
            assert!(c.rifts.iter().all(|r| r.id != id), "banished rifts close");
            assert_eq!(c.month_score, score_before + RiftKind::Scouting.banish_score());
        } else {
            assert!(c.rifts.iter().any(|r| r.id == id), "failed assaults leave it open");
        }
        assert_eq!(c.soldiers.len(), roster_before - report.dead.len());
        // Survivors flew the mission; their record shows it.
        assert!(c.soldiers.iter().any(|s| s.missions == 1) || report.dead.len() == roster_before);

        // Unknown and undetected rifts are not valid targets.
        assert_eq!(c.assault_rift(9999), Err(GeoError::UnknownRift));
    }

    #[test]
    fn wounded_soldiers_convalesce() {
        let mut c = Campaign::new(7);
        c.soldiers[0].recovery_days = 2;
        c.month_plan.clear();
        let events = c.advance_day();
        assert!(!events.iter().any(|e| matches!(e, GeoEvent::SoldierRecovered { .. })));
        let events = c.advance_day();
        assert!(
            events.iter().any(|e| matches!(e, GeoEvent::SoldierRecovered { name } if *name == c.soldiers[0].name)),
            "{events:?}"
        );
        assert!(c.soldiers[0].is_fit());
    }

    #[test]
    fn expiries_punish_and_infiltration_halves_funding() {
        let mut c = Campaign::new(8);
        c.month_plan.clear();
        let before = c.region_funding[&Region::Asia];
        c.rifts.push(Rift {
            id: 900,
            kind: RiftKind::Infiltration,
            region: Region::Asia,
            days_left: 1,
            days_open: 0,
            detected: false,
        });
        let events = c.advance_day();
        assert!(events.iter().any(|e| matches!(e, GeoEvent::RiftExpired { id: 900, .. })));
        assert!(events.iter().any(|e| matches!(e, GeoEvent::RegionInfiltrated { region: Region::Asia })));
        assert_eq!(c.region_funding[&Region::Asia], before / 2);
        assert_eq!(c.month_score, -(RiftKind::Infiltration.expire_penalty()));
    }

    #[test]
    fn nests_bleed_score_until_razed() {
        let mut c = Campaign::new(9);
        c.month_plan.clear();
        c.nests.push(Nest { id: 1, region: Region::Africa });
        let before = c.month_score;
        c.advance_day();
        assert_eq!(c.month_score, before - NEST_DAILY_PENALTY);

        let report = c.raze_nest(1).unwrap();
        if report.victory {
            assert!(c.nests.is_empty());
        } else {
            assert_eq!(c.nests.len(), 1);
        }
        assert_eq!(c.raze_nest(999), Err(GeoError::UnknownNest));
    }

    #[test]
    fn monthly_report_moves_money() {
        let mut c = Campaign::new(10);
        c.month_plan.clear(); // no surprises this month
        c.rifts.clear();
        let mut report = None;
        for _ in 0..DAYS_PER_MONTH {
            for e in c.advance_day() {
                if let GeoEvent::MonthlyReport { income, expenses, funds, .. } = e {
                    report = Some((income, expenses, funds));
                }
            }
        }
        let (income, expenses, funds) = report.expect("a month has passed");
        assert_eq!(income, 8 * 150);
        assert!(expenses > 0);
        assert_eq!(funds, STARTING_FUNDS + income - expenses);
        assert_eq!(c.month, 2);
        assert_eq!(c.day, 1);
    }

    #[test]
    fn two_terrible_months_end_the_campaign() {
        let mut c = Campaign::new(11);
        for _ in 0..2 {
            c.month_plan.clear();
            c.rifts.clear();
            c.month_score = BAD_MONTH_SCORE - 50;
            // Jump to month end.
            c.day = DAYS_PER_MONTH;
            let _ = c.advance_day();
        }
        assert_eq!(c.over, Some(CampaignOutcome::FundingWithdrawn));
        assert_eq!(c.hire_soldier().err(), Some(GeoError::CampaignOver));
        assert!(c.advance_day().is_empty());
    }

    #[test]
    fn fresh_rifts_are_soft_and_stabilized_ones_dig_in() {
        let r = Rift {
            id: 0,
            kind: RiftKind::Terror,
            region: Region::Asia,
            days_left: 4,
            days_open: 0,
            detected: true,
        };
        assert!(!r.is_stabilized());
        assert_eq!(r.effective_garrison(), RiftKind::Terror.garrison() - 1);
        let dug_in = Rift { days_open: 2, ..r };
        assert!(dug_in.is_stabilized());
        assert_eq!(dug_in.effective_garrison(), RiftKind::Terror.garrison() + 2);
    }

    #[test]
    fn victories_bring_salvage_growth_and_heat() {
        let mut c = Campaign::new(20);
        let id = detected_rift(&mut c, RiftKind::Harvest, Region::Europe);
        let stats_before: Vec<SoldierStats> = c.soldiers.iter().map(|s| s.stats).collect();

        let report = c.assault_rift(id).unwrap();
        if report.victory {
            assert!(c.hellsteel >= report.demons_slain, "corpses become hellsteel");
            assert!(c.brimstone >= 4, "harvest rifts carry brimstone");
            assert_eq!(c.reckoning_heat, 1);
            // Someone should have learned something (hits happened: demons died).
            if report.demons_slain > 0 {
                let grown = c
                    .soldiers
                    .iter()
                    .any(|s| stats_before.iter().all(|b| *b != s.stats));
                assert!(grown, "victorious squads grow");
            }
        }
        // Caps hold regardless of outcome.
        for s in &c.soldiers {
            assert!(s.stats.accuracy <= 95 && s.stats.reactions <= 90);
        }
    }

    #[test]
    fn selling_salvage_pays() {
        let mut c = Campaign::new(21);
        c.brimstone = 4;
        c.hellsteel = 10;
        let before = c.funds;
        assert_eq!(c.sell_brimstone(2).unwrap(), 30);
        assert_eq!(c.sell_hellsteel(10).unwrap(), 50);
        assert_eq!(c.funds, before + 80);
        assert_eq!(c.sell_brimstone(99), Err(GeoError::NoMaterials));
    }

    #[test]
    fn hellfire_lance_demands_prereq_and_materials() {
        let mut c = Campaign::new(22);
        assert_eq!(
            c.start_research(Project::HellfireLance),
            Err(GeoError::PrerequisiteMissing)
        );
        c.research.completed.insert(Project::BlessedArms);
        assert_eq!(
            c.start_research(Project::HellfireLance),
            Err(GeoError::NoMaterials)
        );
        c.brimstone = 10;
        c.hellsteel = 15;
        c.start_research(Project::HellfireLance).unwrap();
        assert_eq!((c.brimstone, c.hellsteel), (0, 0), "materials are consumed");
    }

    #[test]
    fn reckonings_hit_home_and_can_end_everything() {
        // With a full squad the Reckoning resolves one way or the other.
        let mut c = Campaign::new(23);
        c.month_plan.clear();
        c.reckoning_day = Some(c.day);
        let events = c.advance_day();
        let fought = events.iter().any(|e| {
            matches!(
                e,
                GeoEvent::ReckoningRepelled { .. }
                    | GeoEvent::CampaignOver { outcome: CampaignOutcome::ChapterhouseFallen }
            )
        });
        assert!(fought, "the scheduled Reckoning must resolve: {events:?}");

        // With no fit defenders, the chapterhouse falls, full stop.
        let mut c = Campaign::new(24);
        c.month_plan.clear();
        c.soldiers.clear();
        c.reckoning_day = Some(c.day);
        let events = c.advance_day();
        assert_eq!(c.over, Some(CampaignOutcome::ChapterhouseFallen), "{events:?}");
    }

    #[test]
    fn heat_schedules_a_reckoning_at_month_end() {
        let mut c = Campaign::new(25);
        c.month_plan.clear();
        c.rifts.clear();
        c.reckoning_heat = 5;
        c.day = DAYS_PER_MONTH;
        c.advance_day();
        assert!(c.reckoning_day.is_some(), "5 heat means hell answers next month");
        assert_eq!(c.reckoning_heat, 0);
    }

    #[test]
    fn save_load_roundtrip_preserves_fate() {
        let mut original = Campaign::new(404);
        // Muddy the state first: build, research, take a fight.
        original.start_build(Facility::Quarters, 0, 0).unwrap();
        original.start_research(Project::RiftAugury).unwrap();
        let id = detected_rift(&mut original, RiftKind::Scouting, Region::Africa);
        original.assault_rift(id).unwrap();
        for _ in 0..10 {
            original.advance_day();
        }

        let save = original.save_to_string();
        let mut loaded = Campaign::load_from_str(&save).unwrap();

        // The load must not merely match — it must CONTINUE identically,
        // which requires the RNG stream to have survived the round trip.
        let mut log_a = Vec::new();
        let mut log_b = Vec::new();
        for _ in 0..60 {
            log_a.extend(original.advance_day().into_iter().map(|e| format!("{e:?}")));
            log_b.extend(loaded.advance_day().into_iter().map(|e| format!("{e:?}")));
        }
        assert_eq!(log_a, log_b, "a loaded game continues the same timeline");
        assert_eq!(original.funds, loaded.funds);
        assert_eq!(original.soldiers.len(), loaded.soldiers.len());
    }

    #[test]
    fn interactive_missions_flow_through_begin_and_conclude() {
        use ods_sim::ai;

        let mut c = Campaign::new(31);
        let id = detected_rift(&mut c, RiftKind::Scouting, Region::Europe);
        let roster_before = c.soldiers.len();

        let (mut battle, token) = c.begin_mission(MissionKind::Rift(id)).unwrap();
        // "Player" plays exactly like the AI would, for the test's purposes.
        let mut turns = 0;
        while battle.winner.is_none() && turns < 40 {
            ai::run_order_turn(&mut battle);
            if battle.winner.is_none() {
                ai::run_demon_turn(&mut battle);
            }
            turns += 1;
        }
        let report = c.conclude_mission(token, &battle);
        assert_eq!(c.soldiers.len(), roster_before - report.dead.len());
        if report.victory {
            assert!(c.rifts.iter().all(|r| r.id != id));
            assert_eq!(c.reckoning_heat, 1);
        }

        // Undetected or missing rifts refuse to stage.
        assert_eq!(
            c.begin_mission(MissionKind::Rift(4242)).err(),
            Some(GeoError::UnknownRift)
        );
    }

    #[test]
    fn campaigns_are_deterministic() {
        let run = |seed: u64| -> (i64, usize, String) {
            let mut c = Campaign::new(seed);
            let mut log = String::new();
            for _ in 0..90 {
                for e in c.advance_day() {
                    log.push_str(&format!("{e:?}\n"));
                    if let GeoEvent::RiftDetected { id, .. } = e
                        && let Ok(r) = c.assault_rift(id)
                    {
                        log.push_str(&format!("assault: {r:?}\n"));
                    }
                }
                if c.over.is_some() {
                    break;
                }
            }
            (c.funds, c.soldiers.len(), log)
        };
        assert_eq!(run(77), run(77), "same seed, same three months");
        assert_ne!(run(77).2, run(78).2, "different seeds diverge");
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoldierStats {
    pub tu: i32,
    pub health: i32,
    pub reactions: i32,
    pub accuracy: i32,
    pub bravery: i32,
}

#[derive(Clone, Debug)]
pub struct Soldier {
    pub name: String,
    pub stats: SoldierStats,
    /// Days until fit for duty. 0 = ready.
    pub recovery_days: u32,
    pub missions: u32,
}

impl Soldier {
    pub fn is_fit(&self) -> bool {
        self.recovery_days == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CampaignOutcome {
    /// Two consecutive badly-losing months: the council pulls the plug.
    FundingWithdrawn,
    /// Debt beyond the limit.
    Bankrupt,
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
}

const RECRUIT_NAMES: [&str; 16] = [
    "Adeyemi", "Brandt", "Castillo", "Dubois", "Eriksen", "Farah", "Grigorescu", "Hale",
    "Iwata", "Jansen", "Karimi", "Lindqvist", "Mbeki", "Novak", "Oyelaran", "Petrov",
];

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
    pub month_score: i64,
    pub bad_months: u32,
    pub over: Option<CampaignOutcome>,
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
            month_score: 0,
            bad_months: 0,
            over: None,
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
        self.research.active = Some((project, project.cost()));
        Ok(())
    }

    /// Send the squad through a detected rift. The battle really happens.
    pub fn assault_rift(&mut self, rift_id: u32) -> Result<BattleReport, GeoError> {
        self.guard_over()?;
        let rift = self
            .rifts
            .iter()
            .find(|r| r.id == rift_id)
            .ok_or(GeoError::UnknownRift)?;
        if !rift.detected {
            return Err(GeoError::NotDetected);
        }
        let (kind, region) = (rift.kind, rift.region);
        let report = self.fight(kind.garrison())?;
        if report.victory {
            self.rifts.retain(|r| r.id != rift_id);
            self.score(region, kind.banish_score());
        } else {
            // The squad withdraws; the incursion continues, emboldened.
            self.score(region, -5);
        }
        Ok(report)
    }

    /// Storm an established nest.
    pub fn raze_nest(&mut self, nest_id: u32) -> Result<BattleReport, GeoError> {
        self.guard_over()?;
        let nest = self
            .nests
            .iter()
            .find(|n| n.id == nest_id)
            .ok_or(GeoError::UnknownNest)?;
        let region = nest.region;
        let report = self.fight(NEST_GARRISON)?;
        if report.victory {
            self.nests.retain(|n| n.id != nest_id);
            self.score(region, NEST_RAZE_SCORE);
        } else {
            self.score(region, -5);
        }
        Ok(report)
    }

    fn fight(&mut self, garrison: u32) -> Result<BattleReport, GeoError> {
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
        let battle_seed = (self.rng.roll(1 << 30) as u64) << 30 | self.rng.roll(1 << 30) as u64;
        let report = missions::auto_resolve(battle_seed, &squad, garrison, &self.research);

        // Fold the outcome back into the roster. Dead soldiers are gone;
        // the wounded convalesce roughly a day per missing hit point.
        let infirmary = self.base.count_active(Facility::Infirmary) > 0;
        for &(squad_pos, health) in &report.survivors {
            let s = &mut self.soldiers[squad_idx[squad_pos]];
            let missing = (s.stats.health - health).max(0) as u32;
            s.recovery_days += if infirmary { missing / 2 } else { missing };
            s.missions += 1;
        }
        let mut dead_roster: Vec<usize> = report.dead.iter().map(|&p| squad_idx[p]).collect();
        dead_roster.sort_unstable_by(|a, b| b.cmp(a));
        for idx in dead_roster {
            self.soldiers.remove(idx);
        }
        Ok(report)
    }

    fn score(&mut self, region: Region, delta: i64) {
        self.month_score += delta;
        *self.region_score.entry(region).or_insert(0) += delta;
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

        // Rift missions run their course.
        let mut expired = Vec::new();
        for r in &mut self.rifts {
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
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected_rift(c: &mut Campaign, kind: RiftKind, region: Region) -> u32 {
        let id = c.next_id;
        c.next_id += 1;
        c.rifts.push(Rift { id, kind, region, days_left: kind.lifetime(), detected: true });
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

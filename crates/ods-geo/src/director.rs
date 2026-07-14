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
            RiftKind::Terror => 35,
            RiftKind::Infiltration => 28,
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

/// What hell has learned about the Order, read fresh each month. The
/// director is invisible, but it is not blind.
#[derive(Clone, Debug, Default)]
pub struct Mood {
    /// Regions with no chapterhouse and no augur coverage: soft ground,
    /// probed three times as often.
    pub neglected: Vec<Region>,
    /// Bound demons in the cells. Enough of them, and something comes
    /// looking — a raid pointed at the founding house's own region.
    pub captures_held: u32,
    /// Rifts banished last month. A winning tempo is answered with tempo.
    pub banished_last_month: u32,
    /// The council's heresy ledger: cultists find willing ears where the
    /// faith thins, so infiltrations multiply.
    pub heresy: u32,
    /// The founding house's region, for raids that come looking.
    pub home: Option<Region>,
}

/// Draw up hell's plan for a month. Escalates with the campaign month;
/// `extra` is the difficulty's cruelty bonus — and the `mood` bends the
/// whole plan toward what hell learned watching last month.
pub fn plan_month(
    rng: &mut ods_sim::SimRng,
    month: u32,
    extra: u32,
    mood: &Mood,
) -> Vec<PlannedRift> {
    let mut count = (3 + month + extra).min(14);
    // A winning tempo is answered with tempo.
    if mood.banished_last_month >= 4 {
        count = (count + 2).min(16);
    }
    // Soft ground draws the knife: neglected regions hold three tickets
    // in the lottery, watched ones hold one.
    let mut tickets: Vec<Region> = Vec::new();
    for r in Region::ALL {
        tickets.push(r);
        if mood.neglected.contains(&r) {
            tickets.push(r);
            tickets.push(r);
        }
    }
    let terror_boost = if mood.banished_last_month >= 4 { 10u32 } else { 0 };
    let inf_boost = if mood.heresy >= 10 { 15u32 } else { 0 };
    let scout_top = 30u32.saturating_sub(terror_boost + inf_boost).max(5);
    let mut plan: Vec<PlannedRift> = (0..count)
        .map(|_| {
            let roll = rng.roll(100);
            let kind = if roll < scout_top {
                RiftKind::Scouting
            } else if roll < scout_top + 25 {
                RiftKind::Harvest
            } else if roll < scout_top + 45 {
                RiftKind::NestBuilding
            } else if roll < scout_top + 60 + inf_boost {
                RiftKind::Infiltration
            } else {
                RiftKind::Terror
            };
            PlannedRift {
                day: 1 + rng.roll(28),
                kind,
                region: tickets[rng.roll(tickets.len() as u32) as usize],
            }
        })
        .collect();
    // From the third month, hell learns to strike chords instead of notes:
    // sometimes two rifts land on the SAME day in different regions, and
    // one squad cannot be in both places.
    if month >= 3 && plan.len() >= 2 && rng.roll(100) < 45 {
        let day = plan[rng.roll(plan.len() as u32) as usize].day;
        let i = rng.roll(plan.len() as u32) as usize;
        plan[i].day = day;
        if plan.iter().filter(|p| p.day == day).count() < 2 {
            // The reroll landed on itself; force a partner.
            let region = tickets[rng.roll(tickets.len() as u32) as usize];
            plan.push(PlannedRift { day, kind: RiftKind::Harvest, region });
        }
    }

    // The bound are come for: a full enough cell block draws a raid at
    // the founding house's own region.
    if mood.captures_held >= 3
        && let Some(home) = mood.home
    {
        plan.push(PlannedRift {
            day: 1 + rng.roll(28),
            kind: if mood.captures_held >= 6 { RiftKind::Terror } else { RiftKind::Harvest },
            region: home,
        });
    }
    plan.sort_by_key(|p| p.day);
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(mood: &Mood, seed: u64) -> (Vec<PlannedRift>, usize, usize) {
        let mut rng = ods_sim::SimRng::from_seed(seed);
        let mut all = Vec::new();
        for month in 1..=12 {
            all.extend(plan_month(&mut rng, month, 0, mood));
        }
        let infiltrations =
            all.iter().filter(|p| p.kind == RiftKind::Infiltration).count();
        let len = all.len();
        (all, infiltrations, len)
    }

    #[test]
    fn soft_ground_draws_the_knife() {
        let mood = Mood { neglected: vec![Region::Asia], ..Default::default() };
        let (plan, _, _) = tally(&mood, 7);
        let asia = plan.iter().filter(|p| p.region == Region::Asia).count();
        let per_other = (plan.len() - asia) / (Region::ALL.len() - 1);
        assert!(
            asia > per_other * 2,
            "the unwatched region draws far more than its share: {asia} vs ~{per_other}"
        );
    }

    #[test]
    fn full_cells_draw_a_raid_home() {
        let mood = Mood {
            captures_held: 4,
            home: Some(Region::Europe),
            ..Default::default()
        };
        let mut rng = ods_sim::SimRng::from_seed(9);
        let plan = plan_month(&mut rng, 2, 0, &mood);
        assert!(
            plan.iter().any(|p| p.region == Region::Europe),
            "something comes looking for the bound: {plan:?}"
        );
    }

    #[test]
    fn heresy_feeds_the_cults_and_tempo_answers_tempo() {
        let quiet = tally(&Mood::default(), 11);
        let heretic = tally(&Mood { heresy: 20, ..Default::default() }, 11);
        assert!(
            heretic.1 > quiet.1,
            "cultists find willing ears: {} vs {}",
            heretic.1,
            quiet.1
        );
        let winning =
            tally(&Mood { banished_last_month: 5, ..Default::default() }, 11);
        assert!(
            winning.2 > quiet.2,
            "a winning tempo is answered with tempo: {} vs {}",
            winning.2,
            quiet.2
        );
    }
}

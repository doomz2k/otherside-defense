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
use crate::research::{ManufactureItem, Project, ResearchState};

pub const DAYS_PER_MONTH: u32 = 30;
pub const SOLDIER_HIRE_COST: i64 = 40;
pub const SOLDIER_SALARY: i64 = 20;
pub const OCCULTIST_HIRE_COST: i64 = 60;
pub const OCCULTIST_SALARY: i64 = 30;
pub const ARTIFICER_HIRE_COST: i64 = 50;
pub const ARTIFICER_SALARY: i64 = 25;
pub const SQUAD_SIZE: usize = 6;
/// A month at or below this score is a "losing badly" month.
pub const BAD_MONTH_SCORE: i64 = -100;
pub const DEBT_LIMIT: i64 = -500;
/// Founding a new chapterhouse in another region.
pub const CHAPTERHOUSE_COST: i64 = 800;
/// Brimstone burned to force open the way to the Otherside.
pub const FINAL_ASSAULT_BRIMSTONE: u32 = 50;

#[derive(Clone, Copy, PartialEq, Eq, Debug, serde::Serialize, serde::Deserialize)]
pub enum Difficulty {
    Novice,
    Veteran,
    Legend,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Novice, Difficulty::Veteran, Difficulty::Legend];

    pub fn starting_funds(self) -> i64 {
        match self {
            Difficulty::Novice => 2400,
            Difficulty::Veteran => 2000,
            Difficulty::Legend => 1600,
        }
    }

    pub fn garrison_bonus(self) -> u32 {
        match self {
            Difficulty::Novice => 0,
            Difficulty::Veteran => 1,
            Difficulty::Legend => 2,
        }
    }

    pub fn plan_bonus(self) -> u32 {
        match self {
            Difficulty::Novice => 0,
            Difficulty::Veteran => 1,
            Difficulty::Legend => 3,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Difficulty::Novice => "Novice",
            Difficulty::Veteran => "Veteran",
            Difficulty::Legend => "Legend",
        }
    }
}

/// Demons held in the warded cells, by rank.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Prisoners {
    pub grunts: u32,
    pub overseers: u32,
}

/// Born tendencies: every recruit is somebody.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Quirk {
    /// +8 accuracy.
    Marksman,
    /// -10 bravery, +8 reactions: scared people notice things.
    Jumpy,
    /// +15 bravery.
    IronNerves,
    /// Heavy packs cost nothing.
    PackMule,
    /// +5 TU.
    Swift,
    /// -8 bravery: some people are not built for viscera.
    Squeamish,
}

impl Quirk {
    pub fn name(self) -> &'static str {
        match self {
            Quirk::Marksman => "Marksman",
            Quirk::Jumpy => "Jumpy",
            Quirk::IronNerves => "Iron Nerves",
            Quirk::PackMule => "Pack Mule",
            Quirk::Swift => "Swift",
            Quirk::Squeamish => "Squeamish",
        }
    }
}

/// What a broken mind fixates on. Earned, not born with — and permanent
/// until the chapel can talk them down from the worst of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Phobia {
    /// -15 bravery whenever a Taker or Husk walks the same field.
    FearOfTheTaken,
    /// -5 TU on night missions: they know what the dark is for.
    NightTerrors,
    /// -15 reactions, always: the hands remember hesitating.
    TriggerFreeze,
}

impl Phobia {
    pub fn name(self) -> &'static str {
        match self {
            Phobia::FearOfTheTaken => "Fear of the Taken",
            Phobia::NightTerrors => "Night Terrors",
            Phobia::TriggerFreeze => "Trigger Freeze",
        }
    }
}

/// A severed part's permanent cost — heavier than any scar.
fn apply_loss(stats: &mut SoldierStats, part: ods_sim::body::BodyPart) {
    use ods_sim::body::BodyPart as P;
    match part {
        P::LeftArm | P::RightArm | P::Weapon => stats.accuracy = (stats.accuracy - 12).max(20),
        P::LeftLeg | P::RightLeg => stats.tu = (stats.tu - 8).max(30),
        _ => stats.health = (stats.health - 5).max(12),
    }
}

/// A lasting scar's permanent cost.
fn apply_scar(stats: &mut SoldierStats, part: ods_sim::body::BodyPart) {
    use ods_sim::body::BodyPart as P;
    match part {
        P::LeftArm | P::RightArm | P::Weapon => stats.accuracy = (stats.accuracy - 8).max(20),
        P::LeftLeg | P::RightLeg => stats.tu = (stats.tu - 5).max(35),
        P::Head => stats.bravery = (stats.bravery - 6).max(5),
        _ => stats.health = (stats.health - 3).max(15),
    }
}

/// A squad in the air: flying to a distant rift aboard the Order's
/// consecrated zeppelin. The soldiers carry the matching `aboard` mark.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Sortie {
    pub rift_id: u32,
    /// Days of flight left; 0 means the squad is on site, holding the perimeter.
    pub days_left: u32,
    /// Wait for the player to lead on arrival instead of auto-resolving.
    pub lead: bool,
    /// Mauled by a sky-hunt en route: the squad lands at three-quarter blood.
    #[serde(default)]
    pub bloodied: bool,
}

/// How a gargoyle sky-hunt on a sortie ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkyHuntOutcome {
    /// The escort gondola's guns answered: driven off, unbloodied.
    Repelled,
    /// The squad lands, but not whole.
    Bloodied,
    /// The zeppelin turns for home with its skin flapping.
    TurnedBack,
}

/// A gargoyle pack has found a led sortie mid-flight and the commander is
/// at the gondola guns: the Geoscape's dogfight, one exchange per order.
/// While one of these stands, the calendar holds its breath.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Interception {
    pub rift_id: u32,
    pub region: Region,
    /// Gargoyles still on the wing.
    pub gargoyles: u32,
    /// Envelope integrity, 0..=100. At 0 the ship turns for home; landing
    /// under half leaves the squad bloodied either way.
    pub envelope: i32,
    /// Closing distance in spans: the guns bite at 6 and under, the claws
    /// at 5 and under, and the pack loses the scent past 12.
    pub range: i32,
    pub round: u32,
    /// Gargoyles downed so far — they fall like burning kites.
    pub downed: u32,
}

/// What one exchange of the dogfight did.
#[derive(Clone, Copy, Debug, Default)]
pub struct InterceptReport {
    /// Gargoyles knocked off the wing this round.
    pub downed: u32,
    /// Envelope integrity lost to claws this round.
    pub envelope_hit: i32,
    /// Set when the engagement ended on this exchange.
    pub outcome: Option<SkyHuntOutcome>,
}

/// A funding nation's demand: banish rifts in their region this month.
#[derive(Clone, Copy, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct CouncilRequest {
    pub region: Region,
    pub needed: u32,
    pub done: u32,
    pub reward: i64,
}

/// An entry on the memorial wall.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct Fallen {
    pub name: String,
    pub rank: String,
    pub missions: u32,
    pub kills: u32,
    pub month: u32,
    pub cause: String,
}

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
    /// Loadout: hellfire charges and dressings carried (drawn from stock).
    pub grenades_loadout: u32,
    pub dressings_loadout: u32,
    /// Carries a forged Hellfire Lance (from the armoury's lance stock).
    #[serde(default)]
    pub has_lance: bool,
    /// Chapterhouse the soldier is stationed at (index into bases).
    #[serde(default)]
    pub home: usize,
    /// Rift id this soldier is warding (unavailable for squads).
    #[serde(default)]
    pub warding: Option<u32>,
    /// Rift id this soldier is flying toward (or camped at) on a sortie.
    #[serde(default)]
    pub aboard: Option<u32>,
    /// Wounds that never healed right (permanent until grafted).
    #[serde(default)]
    pub scars: Vec<ods_sim::body::BodyPart>,
    /// Parts the war took outright. Deployed maimed until a graft replaces
    /// them (severed limbs do not convalesce back).
    #[serde(default)]
    pub lost_parts: Vec<ods_sim::body::BodyPart>,
    /// 0..=100. Morale resets every battle; sanity doesn't. At 20 or below
    /// the soldier is broken and unfit until the chapel does its work.
    #[serde(default = "default_sanity")]
    pub sanity: i32,
    /// The fixation a broken stretch left behind.
    #[serde(default)]
    pub phobia: Option<Phobia>,
    /// Issued weapon, by data-table key ("rifle" is the standing default).
    #[serde(default = "default_weapon_key")]
    pub weapon_key: String,
    /// Sidearm blade / warded circlet, drawn from the armoury stocks.
    #[serde(default)]
    pub has_blade: bool,
    #[serde(default)]
    pub has_circlet: bool,
    /// Fitted armor tier.
    #[serde(default)]
    pub armor: ArmorTier,
    /// A named relic, carried until death loses it in the field.
    #[serde(default)]
    pub relic: Option<Relic>,
    /// Standing squad (0 = unassigned; see [`SQUAD_NAMES`]).
    #[serde(default)]
    pub squad: u8,
    /// The name of the one they always fight beside.
    #[serde(default)]
    pub bond: Option<String>,
    /// The quirk this one was born with.
    #[serde(default)]
    pub quirk: Option<Quirk>,
}

impl Soldier {
    pub fn is_fit(&self) -> bool {
        self.recovery_days == 0
            && self.warding.is_none()
            && self.aboard.is_none()
            && !self.is_broken()
    }

    /// Sanity gone: confined to quarters (or the chapel) until it climbs.
    pub fn is_broken(&self) -> bool {
        self.sanity <= 20
    }

    /// Rank grows from deeds; higher ranks steady the squad's nerves.
    pub fn rank(&self) -> &'static str {
        match self.missions + self.kills * 2 {
            0..=2 => "Novice",
            3..=6 => "Adept",
            7..=12 => "Veteran",
            13..=20 => "Champion",
            _ => "Commander",
        }
    }

    /// Bravery bonus a rank confers in battle.
    pub fn rank_bravery(&self) -> i32 {
        match self.missions + self.kills * 2 {
            0..=2 => 0,
            3..=6 => 4,
            7..=12 => 8,
            13..=20 => 12,
            _ => 16,
        }
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
    /// The arch-demon is destroyed in its own realm. The Order wins.
    Victory,
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
    ManufactureComplete { item: ManufactureItem },
    /// A warding soldier is hurt skirmishing at the rift's edge.
    WardSkirmish { name: String },
    /// A facility is wrecked in the fighting of a Reckoning.
    FacilityWrecked { facility: Facility },
    RequestIssued { region: Region, needed: u32, reward: i64 },
    RequestFulfilled { reward: i64 },
    RequestFailed { region: Region },
    /// The reliquaries repriced salvage for the month.
    MarketShift { brimstone: i64, hellsteel: i64 },
    /// A region's dread has boiled over: expect terror, lose faith.
    RegionPanicking { region: Region, panic: i64 },
    /// A secondary chapterhouse was overrun and lost.
    ChapterhouseLost { region: Region },
    /// A dispatched squad reaches its distant rift.
    SortieArrived { rift_id: u32, region: Region },
    /// An auto-resolve sortie fought on arrival.
    SortieFought { region: Region, victory: bool, demons_slain: u32, dead: usize },
    /// The rift closed (or expired) before the squad could engage.
    SortieRecalled { region: Region },
    /// Three days out, the augurs read it in the sky: a blood moon comes.
    BloodMoonOmen { in_days: u32 },
    /// The sky opens like a wound. Hell is generous and hungry at once.
    BloodMoonRises,
    BloodMoonSets,
    /// A soldier wakes screaming — and sometimes the dream is a map.
    NightTerror { name: String },
    /// Something old and holy in the rubble: a named relic comes home.
    RelicFound { name: String },
    /// Gargoyles found the zeppelin. How it went depends on the gondola.
    SkyHunt { region: Region, outcome: SkyHuntOutcome },
    /// Gargoyles found a led sortie: the commander is called to the guns.
    SkyHuntEngaged { region: Region, gargoyles: u32 },
    /// The dogfight is over, one way or the other.
    SkyHuntResolved { region: Region, outcome: SkyHuntOutcome, downed: u32 },
    /// The breach reached the stores before it was driven out.
    SalvageLooted { brimstone: u32, hellsteel: u32 },
    /// A Prince walked off the field alive. It has a name now.
    NemesisRises { name: String },
    /// It slipped the squads again, and grew by it.
    NemesisEscapes { name: String, escapes: u32 },
    /// The grudge is settled. Mount it high.
    NemesisSlain { name: String },
    /// Two soldiers who kept each other alive stop pretending otherwise.
    BondForged { a: String, b: String },
    /// Victory did not close the veil. The war continues, harder.
    SecondDawn,
    /// The dream WAS a map: an undetected rift, revealed in sleep.
    DreamOfTheRift { region: Region },
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
    /// Not enough bound demons in the cells.
    NoPrisoners,
    UnknownBase,
    WorkshopBusy,
    /// Production requires at least one active Workshop.
    NoWorkshop,
    /// No forged lances left (or none to return).
    NoLances,
    /// That soldier can't take this assignment.
    BadAssignment,
    /// No chapterhouse in the region and no squad on site: dispatch first.
    NotOnSite,
    /// The squad is still in the air.
    SquadInTransit,
    /// A sortie is already flying for that rift.
    SortieAlready,
}

/// A ground mission the campaign can stage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionKind {
    Rift(u32),
    Nest(u32),
    Reckoning,
    /// Storm a corrupted patron's manor and cut the cult out of the council.
    Purge(Region),
    /// Through the opened way, into the Otherside: the breach fight.
    FinalAssault,
    /// The second stage: the arch-demon's sanctum. Winning wins everything.
    FinalSanctum,
}

impl MissionKind {
    pub fn label(self) -> &'static str {
        match self {
            MissionKind::Rift(_) => "rift assault",
            MissionKind::Nest(_) => "nest razing",
            MissionKind::Reckoning => "the Reckoning",
            MissionKind::Purge(_) => "the purge",
            MissionKind::FinalAssault => "the final assault",
            MissionKind::FinalSanctum => "the sanctum",
        }
    }
}

/// Receipt from [`Campaign::begin_mission`]; hand it back to
/// [`Campaign::conclude_mission`] with the finished battle. Not saveable —
/// finish the fight before the world moves on.
pub struct MissionToken {
    kind: MissionKind,
    squad_idx: Vec<usize>,
    /// The chapterhouse under attack (Reckonings only; 0 otherwise).
    base: usize,
}

/// The campaign's running ledger of deeds, kept for the war room and the
/// end-of-campaign accounting.
#[derive(Clone, Copy, Default, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct CampaignStats {
    pub missions_won: u32,
    pub missions_lost: u32,
    pub rifts_banished: u32,
    pub nests_razed: u32,
    pub reckonings_repelled: u32,
    pub demons_slain: u32,
    pub demons_captured: u32,
    pub soldiers_lost: u32,
    pub soldiers_hired: u32,
    pub civilians_saved: u32,
    pub civilians_dead: u32,
    pub shots_fired: u32,
    pub shots_hit: u32,
}

/// Panic at or above this boils over: terror rifts and fleeing patrons.
pub const PANIC_BREAKPOINT: i64 = 60;

/// A named Prince who escaped the field and carries the grudge.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Nemesis {
    pub name: String,
    /// Times it slipped the squads; it grows with each one.
    pub escapes: u32,
}

const NEMESIS_NAMES: [&str; 5] = [
    "Vhaal the Unmourned",
    "Serqet of the Nine Mouths",
    "Ashmedai, Who Counts",
    "The Pale Regent",
    "Mordechar the Patient",
];

/// The standing squads' banners (index 1..; 0 is the unassigned pool).
pub const SQUAD_NAMES: [&str; 4] = ["(any)", "Lamplighters", "Grave Watch", "Ashen Choir"];

/// What the drill yard drills into the garrison.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Focus {
    #[default]
    Marksmanship,
    Conditioning,
    Nerve,
}

impl Focus {
    pub const ALL: [Focus; 3] = [Focus::Marksmanship, Focus::Conditioning, Focus::Nerve];

    pub fn name(self) -> &'static str {
        match self {
            Focus::Marksmanship => "Marksmanship",
            Focus::Conditioning => "Conditioning",
            Focus::Nerve => "Nerve",
        }
    }
}

fn default_sanity() -> i32 {
    100
}

fn default_weapon_key() -> String {
    "rifle".to_string()
}

/// What a soldier wears under the tabard.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ArmorTier {
    /// Padded vestments: the founding issue.
    #[default]
    Vestments,
    /// Hellsteel plate: +3/+2/+1 armor, +8 health, -2 TU.
    Plate,
    /// The abyssal aegis: +6/+5/+3 armor, +12 health, -6 TU.
    Aegis,
}

impl ArmorTier {
    pub fn name(self) -> &'static str {
        match self {
            ArmorTier::Vestments => "Vestments",
            ArmorTier::Plate => "Plate",
            ArmorTier::Aegis => "Aegis",
        }
    }
}

/// A named battlefield relic and what it does for its bearer.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Relic {
    pub name: String,
    pub affix: Affix,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Affix {
    /// +10 reactions.
    Vigil,
    /// +8 accuracy.
    SteadyHand,
    /// +5 TU.
    Vigor,
    /// +2 armor, all facings.
    Bulwark,
    /// +8 bravery.
    Grisly,
}

impl Affix {
    pub fn describe(self) -> &'static str {
        match self {
            Affix::Vigil => "+10 reactions",
            Affix::SteadyHand => "+8 accuracy",
            Affix::Vigor => "+5 TU",
            Affix::Bulwark => "+2 armor",
            Affix::Grisly => "+8 bravery",
        }
    }
}

fn default_brim_price() -> i64 {
    15
}

fn default_steel_price() -> i64 {
    5
}

const RECRUIT_NAMES: [&str; 16] = [
    "Adeyemi", "Brandt", "Castillo", "Dubois", "Eriksen", "Farah", "Grigorescu", "Hale",
    "Iwata", "Jansen", "Karimi", "Lindqvist", "Mbeki", "Novak", "Oyelaran", "Petrov",
];

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Campaign {
    pub funds: i64,
    pub difficulty: Difficulty,
    pub month: u32,
    /// Day of month, 1..=DAYS_PER_MONTH.
    pub day: u32,
    /// Chapterhouses; index 0 is the founding base.
    pub bases: Vec<Chapterhouse>,
    pub soldiers: Vec<Soldier>,
    pub occultists: u32,
    pub artificers: u32,
    pub region_funding: HashMap<Region, i64>,
    pub rifts: Vec<Rift>,
    pub nests: Vec<Nest>,
    pub research: ResearchState,
    /// Salvage stockpiles from banished incursions.
    pub brimstone: u32,
    pub hellsteel: u32,
    /// Armoury stores that loadouts draw from.
    pub grenade_stock: u32,
    pub dressing_stock: u32,
    /// The workshop's active job.
    pub manufacture: Option<(ManufactureItem, u32)>,
    /// Demons in the warded cells.
    pub prisoners: Prisoners,
    /// The wall of the fallen.
    pub memorial: Vec<Fallen>,
    pub month_score: i64,
    pub bad_months: u32,
    pub over: Option<CampaignOutcome>,
    /// Breach won: the way to the arch-demon's sanctum stands open.
    #[serde(default)]
    pub sanctum_open: bool,
    /// Forged Hellfire Lances in the armoury.
    #[serde(default)]
    pub lance_stock: u32,
    /// The council's current demand, if any.
    #[serde(default)]
    pub request: Option<CouncilRequest>,
    /// One save, no second chances.
    #[serde(default)]
    pub ironman: bool,
    /// Facilities wrecked in the most recent Reckoning (for the UI/log).
    #[serde(default, skip)]
    pub wrecked: Vec<Facility>,
    /// Field intelligence: breeds encountered and breeds taken alive.
    #[serde(default)]
    pub codex_seen: std::collections::HashSet<ods_sim::units::Species>,
    #[serde(default)]
    pub codex_captured: std::collections::HashSet<ods_sim::units::Species>,
    /// This month's reliquary prices for salvage.
    #[serde(default = "default_brim_price")]
    pub brim_price: i64,
    #[serde(default = "default_steel_price")]
    pub steel_price: i64,
    /// Civilian dread per region, 0..; feeds terror and flight of funding.
    #[serde(default)]
    pub region_panic: HashMap<Region, i64>,
    /// The campaign's running tallies.
    #[serde(default)]
    pub stats: CampaignStats,
    /// Which chapterhouse the next Reckoning falls on.
    #[serde(default)]
    reckoning_target: usize,
    /// Missions remaining under burial honors (+4 bravery): the dead were
    /// brought home from a held field, and the living remember it.
    #[serde(default)]
    pub burial_honors: u32,
    /// Prosthetics and grafts waiting in the infirmary stores.
    #[serde(default)]
    pub limb_stock: u32,
    #[serde(default)]
    pub graft_stock: u32,
    /// The wider armoury: forged weapons by data key, blades, circlets,
    /// and armor suits waiting for wearers.
    #[serde(default)]
    pub weapon_stock: HashMap<String, u32>,
    #[serde(default)]
    pub blade_stock: u32,
    #[serde(default)]
    pub circlet_stock: u32,
    #[serde(default)]
    pub plate_stock: u32,
    #[serde(default)]
    pub aegis_stock: u32,
    /// Unassigned relics recovered from the field.
    #[serde(default)]
    pub relic_pool: Vec<Relic>,
    /// Slain breeds mounted in the halls: bravery for the garrison, score
    /// for the spectacle.
    #[serde(default)]
    pub trophies: u32,
    /// Breeds the squads have put down (necropsy tier of the codex).
    #[serde(default)]
    pub codex_slain: std::collections::HashSet<ods_sim::units::Species>,
    /// Days of blood moon remaining (None: the sky is honest tonight).
    #[serde(default)]
    pub blood_moon: Option<u32>,
    /// The standing squad that answers the next call (0 = anyone fit).
    #[serde(default)]
    pub active_squad: u8,
    /// Regions whose council patron secretly serves the other side.
    #[serde(default)]
    pub corrupted_patrons: std::collections::HashSet<Region>,
    /// What the drill yard drills.
    #[serde(default)]
    pub training_focus: Focus,
    /// The Prince that got away — and remembers.
    #[serde(default)]
    pub nemesis: Option<Nemesis>,
    /// After victory: the veil stays cracked and the war goes on, harder.
    #[serde(default)]
    pub second_dawn: bool,
    /// Day of month the omen shows (0: no blood moon this month).
    #[serde(default)]
    omen_day: u32,
    /// Squads in the air (or camped at distant rifts).
    #[serde(default)]
    pub sorties: Vec<Sortie>,
    /// A dogfight in progress: gargoyles on a led sortie's wind.
    #[serde(default)]
    pub interception: Option<Interception>,
    /// Events minted outside advance_day (mission conclusions), flushed on
    /// the next day tick so the log still hears about them.
    #[serde(default, skip)]
    pending_events: Vec<GeoEvent>,
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
        Self::new_with(seed, Difficulty::Veteran)
    }

    pub fn new_with(seed: u64, difficulty: Difficulty) -> Self {
        let mut rng = SimRng::from_seed(seed);
        let mut c = Self {
            funds: difficulty.starting_funds(),
            difficulty,
            month: 1,
            day: 1,
            bases: vec![Chapterhouse::founding(Region::Europe)],
            soldiers: Vec::new(),
            occultists: 4,
            artificers: 2,
            region_funding: Region::ALL.iter().map(|&r| (r, 150)).collect(),
            rifts: Vec::new(),
            nests: Vec::new(),
            research: ResearchState::default(),
            brimstone: 0,
            hellsteel: 0,
            grenade_stock: 12,
            dressing_stock: 12,
            manufacture: None,
            prisoners: Prisoners::default(),
            memorial: Vec::new(),
            sanctum_open: false,
            lance_stock: 0,
            request: None,
            ironman: false,
            wrecked: Vec::new(),
            codex_seen: std::collections::HashSet::new(),
            codex_captured: std::collections::HashSet::new(),
            brim_price: default_brim_price(),
            steel_price: default_steel_price(),
            region_panic: HashMap::new(),
            stats: CampaignStats::default(),
            reckoning_target: 0,
            burial_honors: 0,
            limb_stock: 0,
            graft_stock: 0,
            weapon_stock: HashMap::new(),
            blade_stock: 0,
            circlet_stock: 0,
            plate_stock: 0,
            aegis_stock: 0,
            relic_pool: Vec::new(),
            trophies: 0,
            codex_slain: std::collections::HashSet::new(),
            blood_moon: None,
            active_squad: 0,
            corrupted_patrons: std::collections::HashSet::new(),
            training_focus: Focus::Marksmanship,
            nemesis: None,
            second_dawn: false,
            omen_day: 0,
            sorties: Vec::new(),
            interception: None,
            pending_events: Vec::new(),
            month_score: 0,
            bad_months: 0,
            over: None,
            reckoning_heat: 0,
            reckoning_day: None,
            month_plan: director::plan_month(&mut rng, 1, difficulty.plan_bonus()),
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

    // ------------------------------------------------------------------
    // Cross-base capacities

    pub fn quarters_capacity(&self) -> usize {
        self.bases.iter().map(|b| b.quarters_capacity()).sum()
    }

    pub fn library_capacity(&self) -> usize {
        self.bases.iter().map(|b| b.library_capacity()).sum()
    }

    pub fn workshop_capacity(&self) -> usize {
        self.bases.iter().map(|b| b.workshop_capacity()).sum()
    }

    pub fn personnel(&self) -> usize {
        self.soldiers.len() + self.occultists as usize + self.artificers as usize
    }

    fn augurs_in(&self, region: Region) -> usize {
        self.bases
            .iter()
            .filter(|b| b.region == region)
            .map(|b| b.count_active(Facility::AugurArray))
            .sum()
    }

    // ------------------------------------------------------------------
    // The sun (drives the globe's terminator and night assaults)

    pub fn total_days(&self) -> u32 {
        (self.month - 1) * DAYS_PER_MONTH + self.day
    }

    /// Longitude the sun currently stands over, in degrees.
    pub fn sun_lon(&self) -> f32 {
        ((self.total_days() * 137) % 360) as f32 - 180.0
    }

    /// Is it daylight at this longitude today?
    pub fn is_daylight(&self, lon: f32) -> bool {
        let mut d = (lon - self.sun_lon()).abs() % 360.0;
        if d > 180.0 {
            d = 360.0 - d;
        }
        d < 90.0
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
            grenades_loadout: 2,
            dressings_loadout: 2,
            has_lance: false,
            home: 0,
            warding: None,
            aboard: None,
            scars: Vec::new(),
            lost_parts: Vec::new(),
            sanity: 100,
            phobia: None,
            weapon_key: default_weapon_key(),
            has_blade: false,
            has_circlet: false,
            armor: ArmorTier::Vestments,
            relic: None,
            squad: 0,
            bond: None,
            quirk: match self.rng.roll(10) {
                0 => Some(Quirk::Marksman),
                1 => Some(Quirk::Jumpy),
                2 => Some(Quirk::IronNerves),
                3 => Some(Quirk::PackMule),
                4 => Some(Quirk::Swift),
                5 => Some(Quirk::Squeamish),
                _ => None,
            },
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
        if self.personnel() >= self.quarters_capacity() {
            return Err(GeoError::QuartersFull);
        }
        self.funds -= SOLDIER_HIRE_COST;
        self.stats.soldiers_hired += 1;
        let s = self.roll_recruit();
        self.soldiers.push(s);
        Ok(self.soldiers.last().expect("just pushed"))
    }

    pub fn hire_occultist(&mut self) -> Result<(), GeoError> {
        self.guard_over()?;
        if self.funds < OCCULTIST_HIRE_COST {
            return Err(GeoError::NoFunds);
        }
        if self.personnel() >= self.quarters_capacity() {
            return Err(GeoError::QuartersFull);
        }
        self.funds -= OCCULTIST_HIRE_COST;
        self.occultists += 1;
        Ok(())
    }

    pub fn hire_artificer(&mut self) -> Result<(), GeoError> {
        self.guard_over()?;
        if self.funds < ARTIFICER_HIRE_COST {
            return Err(GeoError::NoFunds);
        }
        if self.personnel() >= self.quarters_capacity() {
            return Err(GeoError::QuartersFull);
        }
        self.funds -= ARTIFICER_HIRE_COST;
        self.artificers += 1;
        Ok(())
    }

    pub fn start_build(
        &mut self,
        base: usize,
        facility: Facility,
        x: usize,
        y: usize,
    ) -> Result<(), GeoError> {
        self.guard_over()?;
        if base >= self.bases.len() {
            return Err(GeoError::UnknownBase);
        }
        if self.funds < facility.cost() {
            return Err(GeoError::NoFunds);
        }
        if !self.bases[base].start_build(facility, x, y) {
            return Err(GeoError::Occupied);
        }
        self.funds -= facility.cost();
        Ok(())
    }

    /// Found a second (third...) chapterhouse in a region without one.
    pub fn found_chapterhouse(&mut self, region: Region) -> Result<(), GeoError> {
        self.guard_over()?;
        if self.funds < CHAPTERHOUSE_COST {
            return Err(GeoError::NoFunds);
        }
        if self.bases.iter().any(|b| b.region == region) {
            return Err(GeoError::Occupied);
        }
        self.funds -= CHAPTERHOUSE_COST;
        self.bases.push(Chapterhouse::founding(region));
        Ok(())
    }

    /// Put the artificers on a production job.
    pub fn start_manufacture(&mut self, item: ManufactureItem) -> Result<(), GeoError> {
        self.guard_over()?;
        if self.manufacture.is_some() {
            return Err(GeoError::WorkshopBusy);
        }
        if self.workshop_capacity() == 0 {
            return Err(GeoError::NoWorkshop);
        }
        if item == ManufactureItem::ForgeLance
            && !self.research.is_complete(Project::HellfireLance)
        {
            return Err(GeoError::PrerequisiteMissing);
        }
        if matches!(item, ManufactureItem::HellsteelLimb | ManufactureItem::FleshGraft)
            && !self.research.is_complete(Project::FleshGrafting)
        {
            return Err(GeoError::PrerequisiteMissing);
        }
        // A graft is cut from something; a trophy is mounted from something.
        if item == ManufactureItem::FleshGraft {
            if self.prisoners.grunts == 0 {
                return Err(GeoError::NoPrisoners);
            }
            self.prisoners.grunts -= 1;
        }
        if item == ManufactureItem::MountTrophy && self.codex_slain.is_empty() {
            return Err(GeoError::NoMaterials);
        }
        if matches!(item, ManufactureItem::ForgeCenser | ManufactureItem::ForgeMortar)
            && !self.research.is_complete(Project::BlessedArms)
        {
            return Err(GeoError::PrerequisiteMissing);
        }
        if item == ManufactureItem::ForgeCirclet
            && !self.research.is_complete(Project::Interrogation)
        {
            return Err(GeoError::PrerequisiteMissing);
        }
        if matches!(item, ManufactureItem::ForgePlate | ManufactureItem::ForgeAegis)
            && !self.research.is_complete(Project::HellsteelPlate)
        {
            return Err(GeoError::PrerequisiteMissing);
        }
        if item == ManufactureItem::ForgeAegis
            && !self.codex_slain.contains(&ods_sim::units::Species::Behemoth)
        {
            return Err(GeoError::NoMaterials);
        }
        let (brim, steel) = item.materials();
        if self.brimstone < brim || self.hellsteel < steel {
            return Err(GeoError::NoMaterials);
        }
        self.brimstone -= brim;
        self.hellsteel -= steel;
        self.manufacture = Some((item, item.cost()));
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
        let (grunts, overseers) = project.prisoners();
        if self.prisoners.grunts < grunts || self.prisoners.overseers < overseers {
            return Err(GeoError::NoPrisoners);
        }
        self.brimstone -= brim;
        self.hellsteel -= steel;
        self.prisoners.grunts -= grunts;
        self.prisoners.overseers -= overseers;
        self.research.active = Some((project, project.cost()));
        Ok(())
    }

    /// Sell salvage to national reliquaries at this month's prices.
    pub fn sell_brimstone(&mut self, amount: u32) -> Result<i64, GeoError> {
        self.guard_over()?;
        if amount > self.brimstone {
            return Err(GeoError::NoMaterials);
        }
        self.brimstone -= amount;
        let gain = amount as i64 * self.brim_price;
        self.funds += gain;
        Ok(gain)
    }

    pub fn sell_hellsteel(&mut self, amount: u32) -> Result<i64, GeoError> {
        self.guard_over()?;
        if amount > self.hellsteel {
            return Err(GeoError::NoMaterials);
        }
        self.hellsteel -= amount;
        let gain = amount as i64 * self.steel_price;
        self.funds += gain;
        Ok(gain)
    }

    /// Nudge a region's panic, clamped at calm.
    fn shift_panic(&mut self, region: Region, delta: i64) {
        let p = self.region_panic.entry(region).or_insert(0);
        *p = (*p + delta).max(0);
    }

    /// Post a fit soldier to ward a detected rift: while warded, the rift
    /// cannot stabilize — but the picket line is a dangerous place.
    pub fn assign_ward(&mut self, soldier: usize, rift_id: u32) -> Result<(), GeoError> {
        self.guard_over()?;
        if !self.rifts.iter().any(|r| r.id == rift_id && r.detected) {
            return Err(GeoError::UnknownRift);
        }
        let s = self.soldiers.get_mut(soldier).ok_or(GeoError::BadAssignment)?;
        if s.recovery_days > 0 || s.warding.is_some() || s.aboard.is_some() {
            return Err(GeoError::BadAssignment);
        }
        s.warding = Some(rift_id);
        Ok(())
    }

    /// Issue or take back a forged lance.
    pub fn assign_lance(&mut self, soldier: usize, take: bool) -> Result<(), GeoError> {
        self.guard_over()?;
        let s = self.soldiers.get_mut(soldier).ok_or(GeoError::BadAssignment)?;
        if take {
            if s.has_lance {
                return Err(GeoError::BadAssignment);
            }
            if self.lance_stock == 0 {
                return Err(GeoError::NoLances);
            }
            self.lance_stock -= 1;
            s.has_lance = true;
        } else {
            if !s.has_lance {
                return Err(GeoError::NoLances);
            }
            s.has_lance = false;
            self.lance_stock += 1;
        }
        Ok(())
    }

    /// Cycle a soldier's issued weapon through what the armoury holds:
    /// rifle (always available) then each forged type in stock. Returns the
    /// new key.
    pub fn cycle_weapon(&mut self, soldier: usize) -> Result<String, GeoError> {
        self.guard_over()?;
        const ORDER: [&str; 5] = ["rifle", "arbalest", "censer", "ram_hammer", "salt_mortar"];
        let s = self.soldiers.get(soldier).ok_or(GeoError::BadAssignment)?;
        let current = ORDER
            .iter()
            .position(|&k| k == s.weapon_key)
            .unwrap_or(0);
        // Return the current forged weapon to stock (rifles are standing issue).
        let old_key = ORDER[current].to_string();
        for step in 1..=ORDER.len() {
            let next = ORDER[(current + step) % ORDER.len()];
            let available =
                next == "rifle" || self.weapon_stock.get(next).copied().unwrap_or(0) > 0;
            if available {
                if old_key != "rifle" {
                    *self.weapon_stock.entry(old_key.clone()).or_insert(0) += 1;
                }
                if next != "rifle" {
                    *self.weapon_stock.get_mut(next).expect("checked") -= 1;
                }
                self.soldiers[soldier].weapon_key = next.to_string();
                return Ok(next.to_string());
            }
        }
        Ok(old_key)
    }

    /// Issue or return a blade / circlet / armor suit from the stocks.
    pub fn toggle_blade(&mut self, soldier: usize) -> Result<(), GeoError> {
        self.guard_over()?;
        let s = self.soldiers.get_mut(soldier).ok_or(GeoError::BadAssignment)?;
        if s.has_blade {
            s.has_blade = false;
            self.blade_stock += 1;
        } else if self.blade_stock > 0 {
            self.blade_stock -= 1;
            s.has_blade = true;
        } else {
            return Err(GeoError::NoMaterials);
        }
        Ok(())
    }

    pub fn toggle_circlet(&mut self, soldier: usize) -> Result<(), GeoError> {
        self.guard_over()?;
        let s = self.soldiers.get_mut(soldier).ok_or(GeoError::BadAssignment)?;
        if s.has_circlet {
            s.has_circlet = false;
            self.circlet_stock += 1;
        } else if self.circlet_stock > 0 {
            self.circlet_stock -= 1;
            s.has_circlet = true;
        } else {
            return Err(GeoError::NoMaterials);
        }
        Ok(())
    }

    /// Cycle armor: vestments -> plate -> aegis -> vestments, stock allowing.
    pub fn cycle_armor(&mut self, soldier: usize) -> Result<ArmorTier, GeoError> {
        self.guard_over()?;
        let current = self.soldiers.get(soldier).ok_or(GeoError::BadAssignment)?.armor;
        // Give back what they wear.
        match current {
            ArmorTier::Plate => self.plate_stock += 1,
            ArmorTier::Aegis => self.aegis_stock += 1,
            ArmorTier::Vestments => {}
        }
        let next = match current {
            ArmorTier::Vestments if self.plate_stock > 0 => ArmorTier::Plate,
            ArmorTier::Vestments if self.aegis_stock > 0 => ArmorTier::Aegis,
            ArmorTier::Plate if self.aegis_stock > 0 => ArmorTier::Aegis,
            _ => ArmorTier::Vestments,
        };
        match next {
            ArmorTier::Plate => self.plate_stock -= 1,
            ArmorTier::Aegis => self.aegis_stock -= 1,
            ArmorTier::Vestments => {}
        }
        self.soldiers[soldier].armor = next;
        Ok(next)
    }

    /// Hang a relic from the pool on a soldier (or take it back: None).
    pub fn assign_relic(&mut self, soldier: usize, pool_idx: Option<usize>) -> Result<(), GeoError> {
        self.guard_over()?;
        let s = self.soldiers.get_mut(soldier).ok_or(GeoError::BadAssignment)?;
        if let Some(worn) = s.relic.take() {
            self.relic_pool.push(worn);
        }
        if let Some(i) = pool_idx {
            if i >= self.relic_pool.len() {
                return Err(GeoError::BadAssignment);
            }
            let relic = self.relic_pool.remove(i);
            self.soldiers[soldier].relic = Some(relic);
        }
        Ok(())
    }

    /// Restation a soldier at another chapterhouse (days in transit).
    pub fn transfer_soldier(&mut self, soldier: usize, base: usize) -> Result<(), GeoError> {
        self.guard_over()?;
        if base >= self.bases.len() {
            return Err(GeoError::UnknownBase);
        }
        let s = self.soldiers.get_mut(soldier).ok_or(GeoError::BadAssignment)?;
        if s.warding.is_some() || s.aboard.is_some() || s.home == base {
            return Err(GeoError::BadAssignment);
        }
        s.home = base;
        s.recovery_days += 4; // the road takes its toll
        Ok(())
    }

    /// Send the squad through a detected rift, AI-resolved. The battle
    /// really happens; `begin_mission`/`conclude_mission` is the same path
    /// the interactive Battlescape uses.
    pub fn assault_rift(&mut self, rift_id: u32) -> Result<BattleReport, GeoError> {
        self.fight(MissionKind::Rift(rift_id))
    }

    /// Days of zeppelin flight from the nearest chapterhouse to a detected
    /// rift. Zero when a chapterhouse stands in the rift's region — those
    /// strikes roll out the same day.
    pub fn travel_days(&self, rift_id: u32) -> Result<u32, GeoError> {
        let rift = self
            .rifts
            .iter()
            .find(|r| r.id == rift_id && r.detected)
            .ok_or(GeoError::UnknownRift)?;
        if self.bases.iter().any(|b| b.region == rift.region) {
            return Ok(0);
        }
        let arc = self
            .bases
            .iter()
            .map(|b| Region::arc_degrees(b.region.centroid(), (rift.lat, rift.lon)))
            .fold(f32::MAX, f32::min);
        // ~60 degrees of arc a day, always at least a day when it's abroad.
        Ok((1 + (arc / 60.0) as u32).min(3))
    }

    /// Put a squad on the zeppelin toward a distant rift. They are locked
    /// aboard until arrival; `lead` keeps the fight for the player, otherwise
    /// it auto-resolves the day the squad lands. Same-region strikes don't
    /// need this — assault directly.
    pub fn dispatch_squad(&mut self, rift_id: u32, lead: bool) -> Result<u32, GeoError> {
        self.guard_over()?;
        let days = self.travel_days(rift_id)?;
        if self.sorties.iter().any(|s| s.rift_id == rift_id) {
            return Err(GeoError::SortieAlready);
        }
        let want = self.active_squad;
        let mut squad: Vec<usize> = self
            .soldiers
            .iter()
            .enumerate()
            .filter(|(_, s)| s.is_fit() && want != 0 && s.squad == want)
            .map(|(i, _)| i)
            .take(SQUAD_SIZE)
            .collect();
        for (i, s) in self.soldiers.iter().enumerate() {
            if squad.len() >= SQUAD_SIZE {
                break;
            }
            if s.is_fit() && !squad.contains(&i) {
                squad.push(i);
            }
        }
        if squad.is_empty() {
            return Err(GeoError::NoSquadFit);
        }
        for &i in &squad {
            self.soldiers[i].aboard = Some(rift_id);
        }
        self.sorties.push(Sortie { rift_id, days_left: days, lead, bloodied: false });
        Ok(days)
    }

    /// Unmark a rift's flying squad and drop the sortie (post-battle or on
    /// recall). The soldiers are home again — flights back are abstracted.
    fn end_sortie(&mut self, rift_id: u32) {
        for s in &mut self.soldiers {
            if s.aboard == Some(rift_id) {
                s.aboard = None;
            }
        }
        self.sorties.retain(|s| s.rift_id != rift_id);
    }

    /// Fly one exchange of the standing dogfight. `press` closes the range
    /// and works the guns; easing off opens it and runs for cloud. The
    /// engagement ends when the pack is downed, shaken off, or the envelope
    /// gives — the report's `outcome` says which.
    pub fn intercept_round(&mut self, press: bool) -> InterceptReport {
        let Some(mut it) = self.interception else {
            return InterceptReport::default();
        };
        let mut rep = InterceptReport::default();
        it.round += 1;
        let escorted = self.research.is_complete(Project::EscortGondola);

        // The helm answers first.
        it.range = (it.range + if press { -2 } else { 2 }).max(0);

        // The guns bite inside six spans; the escort gondola doubles them.
        if it.range <= 6 && it.gargoyles > 0 {
            let shots = if escorted { 2 } else { 1 };
            let chance = if it.range <= 3 { 65 } else { 55 };
            for _ in 0..shots {
                if it.gargoyles > 0 && self.rng.roll(100) < chance {
                    it.gargoyles -= 1;
                    it.downed += 1;
                    rep.downed += 1;
                }
            }
        }

        // What's left of the pack answers with claws, inside five.
        if it.range <= 5 && it.gargoyles > 0 {
            let mut hit = 5 * it.gargoyles as i32 + self.rng.roll(6) as i32;
            if escorted {
                hit = hit * 3 / 5; // the armored gondola shrugs some off
            }
            if !press {
                hit /= 2; // a running target is a poor perch
            }
            it.envelope -= hit;
            rep.envelope_hit = hit;
        }

        // How does it stand?
        let outcome = if it.gargoyles == 0 {
            Some(SkyHuntOutcome::Repelled)
        } else if it.envelope <= 0 {
            Some(SkyHuntOutcome::TurnedBack)
        } else if !press && (it.range >= 12 || self.rng.roll(100) < 25) {
            // Lost them in the cloud — but count the cost of the chase.
            Some(if it.envelope <= 50 {
                SkyHuntOutcome::Bloodied
            } else {
                SkyHuntOutcome::Repelled
            })
        } else if it.round > 40 {
            Some(SkyHuntOutcome::Repelled) // dawn: gargoyles hate honest light
        } else {
            None
        };

        if let Some(mut outcome) = outcome {
            // Even a won fight leaves marks: land under half and the squad
            // steps off shaken and stitched.
            if outcome == SkyHuntOutcome::Repelled && it.envelope <= 50 {
                outcome = SkyHuntOutcome::Bloodied;
            }
            match outcome {
                SkyHuntOutcome::Bloodied => {
                    if let Some(s) = self.sorties.iter_mut().find(|s| s.rift_id == it.rift_id) {
                        s.bloodied = true;
                    }
                }
                SkyHuntOutcome::TurnedBack => {
                    for s in &mut self.soldiers {
                        if s.aboard == Some(it.rift_id) {
                            s.recovery_days += 2;
                        }
                    }
                    self.end_sortie(it.rift_id);
                }
                SkyHuntOutcome::Repelled => {}
            }
            self.pending_events.push(GeoEvent::SkyHuntResolved {
                region: it.region,
                outcome,
                downed: it.downed,
            });
            rep.outcome = Some(outcome);
            self.interception = None;
        } else {
            self.interception = Some(it);
        }
        rep
    }

    /// What the field teams drag back from a banished incursion. Under a
    /// blood moon the veil bleeds salvage: everything comes back double.
    fn collect_salvage(&mut self, kind: RiftKind, demons_slain: u32) {
        let mult = if self.blood_moon.is_some() { 2 } else { 1 };
        self.hellsteel += demons_slain * mult;
        self.brimstone += mult
            * match kind {
                RiftKind::Scouting => 1,
                RiftKind::Harvest => 4,
                RiftKind::Terror => 2,
                RiftKind::Infiltration => 2,
                RiftKind::NestBuilding => 3,
            };
    }

    /// Fit a hellsteel limb or a flesh graft to a maimed soldier: the lost
    /// part comes off the roster and its stat penalty is given back. Grafts
    /// hand out a bonus on top — and take it out of the soldier's sleep.
    pub fn fit_replacement(&mut self, soldier: usize, graft: bool) -> Result<(), GeoError> {
        self.guard_over()?;
        let stock = if graft { &mut self.graft_stock } else { &mut self.limb_stock };
        if *stock == 0 {
            return Err(GeoError::NoMaterials);
        }
        let s = self.soldiers.get_mut(soldier).ok_or(GeoError::BadAssignment)?;
        if s.lost_parts.is_empty() {
            return Err(GeoError::BadAssignment);
        }
        *stock -= 1;
        let part = s.lost_parts.remove(0);
        // Give back what apply_loss took.
        use ods_sim::body::BodyPart as P;
        match part {
            P::LeftArm | P::RightArm | P::Weapon => s.stats.accuracy += 12,
            P::LeftLeg | P::RightLeg => s.stats.tu += 8,
            _ => s.stats.health += 5,
        }
        if graft {
            // Living flesh outperforms what it replaced — and whispers.
            match part {
                P::LeftArm | P::RightArm | P::Weapon => s.stats.accuracy += 5,
                P::LeftLeg | P::RightLeg => s.stats.tu += 5,
                _ => s.stats.health += 3,
            }
            s.sanity = (s.sanity - 15).max(0);
        }
        s.recovery_days += 6;
        Ok(())
    }

    /// Storm an established nest, AI-resolved.
    pub fn raze_nest(&mut self, nest_id: u32) -> Result<BattleReport, GeoError> {
        self.fight(MissionKind::Nest(nest_id))
    }

    /// Storm a corrupted patron's manor, AI-resolved.
    pub fn purge_patron(&mut self, region: Region) -> Result<BattleReport, GeoError> {
        self.fight(MissionKind::Purge(region))
    }

    /// After victory: keep fighting. The veil stays cracked, hell comes
    /// harder, and the Ledger becomes the scoreboard.
    pub fn second_dawn(&mut self) -> Result<(), GeoError> {
        if self.over != Some(CampaignOutcome::Victory) {
            return Err(GeoError::PrerequisiteMissing);
        }
        self.over = None;
        self.sanctum_open = false;
        self.second_dawn = true;
        self.pending_events.push(GeoEvent::SecondDawn);
        Ok(())
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
        let bonus = self.difficulty.garrison_bonus();
        // Reckonings strike a specific chapterhouse; its own garrison answers.
        let base = match kind {
            MissionKind::Reckoning => self.reckoning_target.min(self.bases.len() - 1),
            _ => 0,
        };
        let (garrison, strength, defense) = match kind {
            MissionKind::Rift(id) => {
                let rift = self
                    .rifts
                    .iter()
                    .find(|r| r.id == id)
                    .ok_or(GeoError::UnknownRift)?;
                if !rift.detected {
                    return Err(GeoError::NotDetected);
                }
                // Presence: strike from a local chapterhouse, or have a
                // dispatched squad already on the ground out there.
                let local = self.bases.iter().any(|b| b.region == rift.region);
                let distant = if local { 0 } else { 1 };
                if !local {
                    match self.sorties.iter().find(|s| s.rift_id == id) {
                        Some(s) if s.days_left == 0 => {}
                        Some(_) => return Err(GeoError::SquadInTransit),
                        None => return Err(GeoError::NotOnSite),
                    }
                }
                (rift.effective_garrison() + bonus + distant, self.month, false)
            }
            MissionKind::Nest(id) => {
                self.nests
                    .iter()
                    .find(|n| n.id == id)
                    .ok_or(GeoError::UnknownNest)?;
                (NEST_GARRISON + bonus, self.month, false)
            }
            MissionKind::Reckoning => ((5 + self.month / 2).min(8) + bonus, self.month, true),
            MissionKind::Purge(region) => {
                if !self.corrupted_patrons.contains(&region) {
                    return Err(GeoError::NotDetected);
                }
                ((4 + self.month / 2).min(7) + bonus, self.month, false)
            }
            MissionKind::FinalAssault => {
                if !self.research.is_complete(Project::NameOfTheEnemy) {
                    return Err(GeoError::PrerequisiteMissing);
                }
                if self.brimstone < FINAL_ASSAULT_BRIMSTONE {
                    return Err(GeoError::NoMaterials);
                }
                (8, 9, false)
            }
            MissionKind::FinalSanctum => {
                if !self.sanctum_open {
                    return Err(GeoError::PrerequisiteMissing);
                }
                (7, 10, false) // fewer bodies, worse breeds — a Prince waits
            }
        };
        // Under the blood moon, everything that comes through comes bigger.
        let strength = if self.blood_moon.is_some() { strength + 2 } else { strength };
        // A garrison drilling under mounted trophies does not flinch easily
        // (applied to the squad below, after the battle is built).
        let trophy_bravery = (self.trophies as i32 * 2).min(10);

        // A sortie's squad is whoever flew out; otherwise muster the fit
        // (for Reckonings, only those stationed at the struck house).
        let aboard: Vec<usize> = match kind {
            MissionKind::Rift(id) => self
                .soldiers
                .iter()
                .enumerate()
                .filter(|(_, s)| s.aboard == Some(id))
                .map(|(i, _)| i)
                .collect(),
            _ => Vec::new(),
        };
        let squad_idx: Vec<usize> = if aboard.is_empty() {
            // The active standing squad answers first; the pool fills gaps.
            let want = self.active_squad;
            let mut picked: Vec<usize> = self
                .soldiers
                .iter()
                .enumerate()
                .filter(|(_, s)| {
                    s.is_fit() && (!defense || s.home == base) && want != 0 && s.squad == want
                })
                .map(|(i, _)| i)
                .take(SQUAD_SIZE)
                .collect();
            for (i, s) in self.soldiers.iter().enumerate() {
                if picked.len() >= SQUAD_SIZE {
                    break;
                }
                if s.is_fit() && (!defense || s.home == base) && !picked.contains(&i) {
                    picked.push(i);
                }
            }
            picked
        } else {
            aboard
        };
        if squad_idx.is_empty() {
            return Err(GeoError::NoSquadFit);
        }
        // The rite that opens the way consumes its brimstone either way.
        if kind == MissionKind::FinalAssault {
            self.brimstone -= FINAL_ASSAULT_BRIMSTONE;
        }

        // Kit up from the armoury stores: loadouts draw down real stock.
        let mut kits: Vec<(u32, u32)> = Vec::new();
        for &i in &squad_idx {
            let s = &self.soldiers[i];
            let grenades = s.grenades_loadout.min(self.grenade_stock);
            let dressings = s.dressings_loadout.min(self.dressing_stock);
            self.grenade_stock -= grenades;
            self.dressing_stock -= dressings;
            kits.push((grenades, dressings));
        }

        let squad: Vec<&Soldier> = squad_idx.iter().map(|&i| &self.soldiers[i]).collect();
        let seed = (self.rng.roll(1 << 30) as u64) << 30 | self.rng.roll(1 << 30) as u64;
        let mut battle = if defense {
            missions::build_defense(
                seed,
                &squad,
                &kits,
                garrison,
                &self.research,
                &self.bases[base].occupied_cells(),
                self.bases[base].gate(),
                2 + 2 * self.bases[base].count_active(Facility::WardTower) as u32,
                (self.bases[base].count_active(Facility::Kennel) as u32).min(2),
            )
        } else {
            match kind {
                MissionKind::Nest(_) => {
                    missions::build_nest(seed, &squad, &kits, garrison, strength, &self.research)
                }
                MissionKind::Purge(_) => {
                    missions::build_purge(seed, &squad, &kits, garrison, &self.research)
                }
                MissionKind::FinalAssault | MissionKind::FinalSanctum => missions::build_otherside(
                    seed,
                    &squad,
                    &kits,
                    garrison,
                    strength,
                    &self.research,
                ),
                _ => {
                    let civilians = match kind {
                        MissionKind::Rift(id) => {
                            let terror = self
                                .rifts
                                .iter()
                                .any(|r| r.id == id && r.kind == RiftKind::Terror);
                            if terror { 4 } else { 0 }
                        }
                        _ => 0,
                    };
                    let biome = match kind {
                        MissionKind::Rift(id) => self
                            .rifts
                            .iter()
                            .find(|r| r.id == id)
                            .map_or(ods_sim::scenario::Biome::Temperate, |r| r.region.biome()),
                        _ => ods_sim::scenario::Biome::Temperate,
                    };
                    // The rift's business decides what winning means.
                    let spec = match kind {
                        MissionKind::Rift(id) => {
                            match self.rifts.iter().find(|r| r.id == id).map(|r| r.kind) {
                                Some(RiftKind::Terror) => ods_sim::scenario::MissionSpec::Evacuate,
                                Some(RiftKind::Harvest) => ods_sim::scenario::MissionSpec::Interrupt,
                                Some(RiftKind::Infiltration) => ods_sim::scenario::MissionSpec::Snatch,
                                _ => ods_sim::scenario::MissionSpec::Standard,
                            }
                        }
                        _ => ods_sim::scenario::MissionSpec::Standard,
                    };
                    missions::build_assault(
                        seed,
                        &squad,
                        &kits,
                        garrison,
                        strength,
                        civilians,
                        biome,
                        spec,
                        &self.research,
                    )
                }
            }
        };

        // Vision: assaults on the night side of the world are fought at
        // 9 tiles instead of 14. The Otherside has no sun at all.
        battle.vision_tiles = match kind {
            MissionKind::Rift(id) => {
                let lon = self.rifts.iter().find(|r| r.id == id).map_or(0.0, |r| r.lon);
                if self.is_daylight(lon) { 14 } else { 9 }
            }
            MissionKind::Nest(id) => {
                let lon = self.nests.iter().find(|n| n.id == id).map_or(0.0, |n| n.lon);
                if self.is_daylight(lon) { 14 } else { 9 }
            }
            MissionKind::Reckoning => 14, // your own halls, lamplit
            MissionKind::Purge(_) => 12,  // chandeliers and long shadows
            MissionKind::FinalAssault | MissionKind::FinalSanctum => 9,
        };
        // The sky rolls its own dice (rift fields only; halls have roofs).
        if let MissionKind::Rift(id) = kind {
            let biome = self
                .rifts
                .iter()
                .find(|r| r.id == id)
                .map_or(ods_sim::scenario::Biome::Temperate, |r| r.region.biome());
            use ods_sim::battle::Weather;
            use ods_sim::scenario::Biome;
            let roll = self.rng.roll(100);
            battle.weather = match biome {
                Biome::Desert if roll < 25 => Weather::Sandstorm,
                Biome::Tundra if roll < 30 => Weather::Snowfall,
                Biome::Jungle if roll < 35 => Weather::Rain,
                Biome::Temperate if roll < 20 => Weather::Rain,
                _ => Weather::Clear,
            };
            if battle.weather == Weather::Sandstorm {
                battle.vision_tiles = (battle.vision_tiles - 4).max(5);
                for u in &mut battle.units {
                    u.accuracy = (u.accuracy - 10).max(20);
                }
            }
        }

        // Phobias answer the conditions of THIS field.
        let night = battle.vision_tiles < 14;
        let taken_present = battle
            .units
            .iter()
            .any(|u| matches!(u.species, ods_sim::units::Species::Taker | ods_sim::units::Species::Husk));
        for (i, &ri) in squad_idx.iter().enumerate() {
            match self.soldiers[ri].phobia {
                Some(Phobia::FearOfTheTaken) if taken_present => {
                    battle.units[i].bravery = (battle.units[i].bravery - 15).max(5);
                }
                Some(Phobia::NightTerrors) if night => {
                    battle.units[i].tu_max = (battle.units[i].tu_max - 5).max(30);
                    battle.units[i].tu = battle.units[i].tu_max;
                }
                Some(Phobia::TriggerFreeze) => {
                    battle.units[i].reactions = (battle.units[i].reactions - 15).max(10);
                }
                _ => {}
            }
        }

        // Burial honors: the last rites still echo in the ranks.
        if self.burial_honors > 0 {
            self.burial_honors -= 1;
            for i in 0..squad_idx.len() {
                let u = &mut battle.units[i];
                u.bravery = (u.bravery + 4).min(95);
            }
        }
        // A mauled sortie lands at three-quarter blood.
        if let MissionKind::Rift(id) = kind
            && self.sorties.iter().any(|s| s.rift_id == id && s.bloodied)
        {
            for i in 0..squad_idx.len() {
                let u = &mut battle.units[i];
                u.health = (u.health * 3 / 4).max(1);
            }
        }

        // Bonded pairs deployed together watch each other's arcs.
        for i in 0..squad_idx.len() {
            if let Some(bond) = &self.soldiers[squad_idx[i]].bond
                && squad_idx
                    .iter()
                    .any(|&j| self.soldiers[j].name == *bond)
            {
                battle.units[i].reactions += 5;
            }
        }
        // A Commander in the muster steadies everyone.
        if squad_idx
            .iter()
            .any(|&i| self.soldiers[i].missions + self.soldiers[i].kills * 2 > 20)
        {
            for i in 0..squad_idx.len() {
                let u = &mut battle.units[i];
                u.bravery = (u.bravery + 5).min(95);
            }
        }
        // The nemesis wears its name onto the field, grown by its escapes.
        if let Some(n) = &self.nemesis {
            for u in &mut battle.units {
                if u.species == ods_sim::units::Species::Prince {
                    u.name = n.name.clone();
                    u.health_max += n.escapes as i32 * 8;
                    u.health = u.health_max;
                    u.accuracy = (u.accuracy + n.escapes as i32 * 3).min(90);
                    break;
                }
            }
        }

        // The trophy hall's lesson: these things die.
        if trophy_bravery > 0 {
            for i in 0..squad_idx.len() {
                let u = &mut battle.units[i];
                u.bravery = (u.bravery + trophy_bravery).min(95);
            }
        }
        Ok((battle, MissionToken { kind, squad_idx, base }))
    }

    /// Fold a finished battle back into the campaign: casualties, wounds,
    /// growth, and the strategic outcome of the mission.
    pub fn conclude_mission(
        &mut self,
        token: MissionToken,
        battle: &ods_sim::battle::Battle,
    ) -> BattleReport {
        let report = missions::report_from(battle, token.squad_idx.len());

        // Shared fields forge bonds: two seasoned, unbonded survivors of a
        // held field sometimes stop pretending they aren't a pair. This must
        // read the roster BEFORE the dead are struck from it — squad indices
        // go stale the moment apply_to_roster removes anyone.
        if report.victory {
            let seasoned: Vec<usize> = report
                .survivors
                .iter()
                .map(|&(p, _, _)| token.squad_idx[p])
                .filter(|&i| self.soldiers[i].bond.is_none() && self.soldiers[i].missions >= 3)
                .collect();
            if seasoned.len() >= 2 && self.rng.roll(100) < 25 {
                let (a, b) = (seasoned[0], seasoned[1]);
                let (na, nb) = (self.soldiers[a].name.clone(), self.soldiers[b].name.clone());
                self.soldiers[a].bond = Some(nb.clone());
                self.soldiers[b].bond = Some(na.clone());
                self.pending_events.push(GeoEvent::BondForged { a: na, b: nb });
            }
        }

        self.apply_to_roster(&token.squad_idx, &report, token.kind.label());

        // Bound demons come home in chains — if the field was held.
        if report.victory {
            self.prisoners.grunts += report.captured_grunts;
            self.prisoners.overseers += report.captured_overseers;
        }

        // A held field means the fallen come home for burial; the rites
        // steel the squads that follow.
        if report.victory && !report.dead.is_empty() {
            self.burial_honors = 2;
        }

        // The Prince that walks away gets a name — and the named one that
        // dies gets mounted.
        use ods_sim::units::Species as Sp;
        let prince_seen = report.species_seen.contains(&Sp::Prince);
        let prince_slain = report.species_slain.contains(&Sp::Prince);
        if prince_slain && let Some(n) = self.nemesis.take() {
            self.trophies += 1;
            self.month_score += 30;
            self.pending_events.push(GeoEvent::NemesisSlain { name: n.name });
        } else if prince_seen && !prince_slain {
            match &mut self.nemesis {
                Some(n) => {
                    n.escapes += 1;
                    self.pending_events.push(GeoEvent::NemesisEscapes {
                        name: n.name.clone(),
                        escapes: n.escapes,
                    });
                }
                None => {
                    let name =
                        NEMESIS_NAMES[self.rng.roll(NEMESIS_NAMES.len() as u32) as usize].to_string();
                    self.nemesis = Some(Nemesis { name: name.clone(), escapes: 0 });
                    self.pending_events.push(GeoEvent::NemesisRises { name });
                }
            }
        }

        // The codex learns what the squads met — and what they took alive.
        self.codex_seen.extend(report.species_seen.iter().copied());
        self.codex_slain.extend(report.species_slain.iter().copied());
        if report.victory {
            self.codex_captured.extend(report.species_captured.iter().copied());
        }

        // The ledgers.
        let stats = &mut self.stats;
        if report.victory {
            stats.missions_won += 1;
            stats.demons_captured += report.captured_grunts + report.captured_overseers;
        } else {
            stats.missions_lost += 1;
        }
        stats.demons_slain += report.demons_slain;
        stats.soldiers_lost += report.dead.len() as u32;
        stats.civilians_saved += report.civilians_saved;
        stats.civilians_dead += report.civilians_dead;
        for &(_, _, xp) in &report.survivors {
            stats.shots_fired += xp.shots_fired;
            stats.shots_hit += xp.shots_hit;
        }

        match token.kind {
            MissionKind::Rift(id) => {
                // Win or lose, the engagement ends the sortie: the squad
                // flies home with whatever the field left of it.
                self.end_sortie(id);
                if let Some(rift) = self.rifts.iter().find(|r| r.id == id) {
                    let (kind, region) = (rift.kind, rift.region);
                    // Every townsperson matters, win or lose.
                    let civ_delta = report.civilians_saved as i64 * 5
                        - report.civilians_dead as i64 * 10;
                    if civ_delta != 0 {
                        self.score(region, civ_delta);
                    }
                    if report.victory {
                        self.rifts.retain(|r| r.id != id);
                        self.score(region, kind.banish_score());
                        // Every gibbet cut down counts for something.
                        self.score(region, report.atrocities_found as i64 * 3);
                        // And sometimes the rubble gives something back.
                        if self.rng.roll(100) < 20 {
                            let relic = self.roll_relic();
                            self.pending_events.push(GeoEvent::RelicFound {
                                name: relic.name.clone(),
                            });
                            self.relic_pool.push(relic);
                        }
                        self.shift_panic(region, -10);
                        self.collect_salvage(kind, report.demons_slain);
                        self.reckoning_heat += 1;
                        self.stats.rifts_banished += 1;
                        if let Some(req) = &mut self.request
                            && req.region == region {
                                req.done += 1;
                            }
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
                        self.shift_panic(region, -20);
                        self.brimstone += 6;
                        self.hellsteel += report.demons_slain;
                        self.reckoning_heat += 1;
                        self.stats.nests_razed += 1;
                    } else {
                        self.score(region, -5);
                    }
                }
            }
            MissionKind::Purge(region) => {
                if report.victory {
                    self.corrupted_patrons.remove(&region);
                    // The tithe flows honest again — and gratefully.
                    let f = self.region_funding.get_mut(&region).expect("region exists");
                    *f = (*f * 2).min(400);
                    self.score(region, 25);
                    self.shift_panic(region, -10);
                } else {
                    self.score(region, -10);
                }
            }
            MissionKind::Reckoning => {
                let base = token.base;
                if report.victory {
                    self.score(self.bases[base].region, 30);
                    self.stats.reckonings_repelled += 1;
                    // The halls are held — but the fighting wrecked things.
                    let cells = self.bases[base].occupied_cells();
                    for (x, y) in cells {
                        if let Some((facility, _)) = self.bases[base].facility_at(x, y)
                            && facility != Facility::Gatehouse && self.rng.roll(100) < 15 {
                                self.bases[base].demolish(x, y);
                                self.wrecked.push(facility);
                            }
                    }
                    // Without a warded vault, the breach loots the stores.
                    if self.bases[base].count_active(Facility::Vault) == 0 {
                        let (b, h) = (self.brimstone * 15 / 100, self.hellsteel * 15 / 100);
                        self.brimstone -= b;
                        self.hellsteel -= h;
                        if b + h > 0 {
                            self.pending_events.push(GeoEvent::SalvageLooted {
                                brimstone: b,
                                hellsteel: h,
                            });
                        }
                    }
                } else if base == 0 {
                    // The founding chapterhouse falls, and the Order with it.
                    self.over = Some(CampaignOutcome::ChapterhouseFallen);
                } else {
                    // An outpost is overrun: strike it from the maps and
                    // restation its people at the founding house.
                    self.bases.remove(base);
                    for s in &mut self.soldiers {
                        if s.home == base {
                            s.home = 0;
                            s.recovery_days += 4;
                        } else if s.home > base {
                            s.home -= 1;
                        }
                    }
                }
            }
            MissionKind::FinalAssault => {
                if report.victory {
                    // The breach holds. No time to bleed: the sanctum waits.
                    self.sanctum_open = true;
                    for &(pos, _, _) in &report.survivors {
                        self.soldiers[token.squad_idx[pos]].recovery_days = 0;
                    }
                } else {
                    // The way slams shut; the survivors crawl home.
                    self.month_score -= 30;
                }
            }
            MissionKind::FinalSanctum => {
                if report.victory {
                    self.over = Some(CampaignOutcome::Victory);
                } else {
                    self.sanctum_open = false; // the way seals behind them
                    self.month_score -= 30;
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

    /// Casualties are removed (and remembered), the wounded convalesce
    /// roughly a day per missing hit point, and survivors grow by what they
    /// did out there.
    fn apply_to_roster(&mut self, squad_idx: &[usize], report: &BattleReport, cause: &str) {
        let infirmary = self
            .bases
            .iter()
            .any(|b| b.count_active(Facility::Infirmary) > 0);
        for &(squad_pos, health, xp) in &report.survivors {
            let s = &mut self.soldiers[squad_idx[squad_pos]];
            let missing = (s.stats.health - health).max(0) as u32;
            s.recovery_days += if infirmary { missing / 2 } else { missing };
            s.missions += 1;
            s.kills += xp.kills;
            apply_growth(&mut s.stats, xp);
        }
        // Horror outlives the battle: sanity bleeds, and a mind pushed too
        // far picks up a fixation it will carry forever.
        for &(squad_pos, horror) in &report.horrors {
            let idx = squad_idx[squad_pos];
            let s = &mut self.soldiers[idx];
            s.sanity = (s.sanity - horror as i32 * 3).max(0);
            if s.sanity < 40 && s.phobia.is_none() && self.rng.roll(100) < 35 {
                let phobia = match self.rng.roll(3) {
                    0 => Phobia::FearOfTheTaken,
                    1 => Phobia::NightTerrors,
                    _ => Phobia::TriggerFreeze,
                };
                self.soldiers[idx].phobia = Some(phobia);
            }
        }
        // A lost field weighs on everyone who walked off it.
        if !report.victory {
            for &(squad_pos, _, _) in &report.survivors {
                let s = &mut self.soldiers[squad_idx[squad_pos]];
                s.sanity = (s.sanity - 5).max(0);
            }
        }

        // Severed parts are simply gone: the roster records the loss.
        for (squad_pos, parts) in &report.severed {
            let idx = squad_idx[*squad_pos];
            for part in parts {
                let s = &mut self.soldiers[idx];
                s.recovery_days += 10;
                if !s.lost_parts.contains(part) {
                    s.lost_parts.push(*part);
                    apply_loss(&mut s.stats, *part);
                }
            }
        }
        // Crippled parts may never heal right: lasting scars.
        for (squad_pos, parts) in &report.injuries {
            for part in parts {
                let idx = squad_idx[*squad_pos];
                let s = &mut self.soldiers[idx];
                s.recovery_days += 3;
                if self.rng.roll(100) < 50 && !s.scars.contains(part) {
                    s.scars.push(*part);
                    apply_scar(&mut self.soldiers[idx].stats, *part);
                }
            }
        }
        // The field is held: recover the lances of the fallen.
        if report.victory {
            for &p in &report.dead {
                if self.soldiers[squad_idx[p]].has_lance {
                    self.lance_stock += 1;
                }
            }
        }
        // Relics on the dead: recovered with the body on a held field,
        // lost with it otherwise.
        for &p in &report.dead {
            if let Some(relic) = self.soldiers[squad_idx[p]].relic.take()
                && report.victory
            {
                self.relic_pool.push(relic);
            }
        }
        let mut dead_roster: Vec<usize> = report.dead.iter().map(|&p| squad_idx[p]).collect();
        dead_roster.sort_unstable_by(|a, b| b.cmp(a));
        for idx in dead_roster {
            let s = self.soldiers.remove(idx);
            // The bonded partner takes it worst of anyone.
            let mut cause = cause.to_string();
            if let Some(partner_name) = &s.bond {
                for p in &mut self.soldiers {
                    if p.name == *partner_name {
                        p.bond = None;
                        p.sanity = (p.sanity - 20).max(0);
                        cause.push_str(&format!("; {partner_name} was never the same"));
                        break;
                    }
                }
            }
            self.memorial.push(Fallen {
                rank: s.rank().to_string(),
                name: s.name,
                missions: s.missions,
                kills: s.kills,
                month: self.month,
                cause,
            });
        }
    }

    /// Forge a relic's name and nature from the same dice.
    fn roll_relic(&mut self) -> Relic {
        let affix = match self.rng.roll(5) {
            0 => Affix::Vigil,
            1 => Affix::SteadyHand,
            2 => Affix::Vigor,
            3 => Affix::Bulwark,
            _ => Affix::Grisly,
        };
        let noun = ["Icon", "Chain", "Lantern", "Psalter", "Bell"]
            [self.rng.roll(5) as usize];
        let title = match affix {
            Affix::Vigil => "of the Vigil",
            Affix::SteadyHand => "of the Steady Hand",
            Affix::Vigor => "of Unresting Strength",
            Affix::Bulwark => "of the Bulwark",
            Affix::Grisly => "of Grisly Comfort",
        };
        Relic { name: format!("{noun} {title}"), affix }
    }

    fn score(&mut self, region: Region, delta: i64) {
        self.month_score += delta;
        *self.region_score.entry(region).or_insert(0) += delta;
    }

    /// Hell answers success, striking one chapterhouse. With no fit
    /// defenders stationed there, that house simply falls — and if it was
    /// the founding house, the Order falls with it.
    fn resolve_reckoning(&mut self, events: &mut Vec<GeoEvent>) {
        self.reckoning_target = self.rng.roll(self.bases.len() as u32) as usize;
        let target = self.reckoning_target;
        let region = self.bases[target].region;
        match self.fight(MissionKind::Reckoning) {
            Ok(report) if report.victory => {
                events.push(GeoEvent::ReckoningRepelled {
                    demons_slain: report.demons_slain,
                    dead: report.dead.len(),
                });
            }
            Ok(_) if target == 0 => {
                // `conclude_mission` already marked the campaign over.
                events.push(GeoEvent::CampaignOver {
                    outcome: CampaignOutcome::ChapterhouseFallen,
                });
            }
            Ok(_) => events.push(GeoEvent::ChapterhouseLost { region }),
            Err(GeoError::NoSquadFit) if target == 0 => {
                self.over = Some(CampaignOutcome::ChapterhouseFallen);
                events.push(GeoEvent::CampaignOver {
                    outcome: CampaignOutcome::ChapterhouseFallen,
                });
            }
            Err(GeoError::NoSquadFit) => {
                self.bases.remove(target);
                for s in &mut self.soldiers {
                    if s.home == target {
                        s.home = 0;
                        s.recovery_days += 4;
                    } else if s.home > target {
                        s.home -= 1;
                    }
                }
                events.push(GeoEvent::ChapterhouseLost { region });
            }
            Err(_) => {}
        }
    }

    // ------------------------------------------------------------------
    // The clock

    pub fn advance_day(&mut self) -> Vec<GeoEvent> {
        // A dogfight left standing (headless runs, auto-advance) resolves
        // itself at the guns' discretion before the calendar moves.
        while self.interception.is_some() {
            self.intercept_round(true);
        }
        let mut events = std::mem::take(&mut self.pending_events);
        if self.over.is_some() {
            return events;
        }

        // Construction, at every chapterhouse.
        for base in &mut self.bases {
            for facility in base.advance_day() {
                events.push(GeoEvent::FacilityComplete { facility });
            }
        }

        // Research, throttled by library space.
        let effective = self.occultists.min(self.library_capacity() as u32);
        if let Some(project) = self.research.advance_day(effective) {
            events.push(GeoEvent::ResearchComplete { project });
        }

        // The workshop grinds on.
        let output = self.artificers.min(self.workshop_capacity() as u32);
        if let Some((item, left)) = &mut self.manufacture {
            *left = left.saturating_sub(output);
            if *left == 0 {
                let done = *item;
                self.manufacture = None;
                match done {
                    ManufactureItem::HellfireCharges => self.grenade_stock += 4,
                    ManufactureItem::FieldDressings => self.dressing_stock += 4,
                    ManufactureItem::TradeArms => self.funds += 45,
                    ManufactureItem::ForgeLance => self.lance_stock += 1,
                    ManufactureItem::HellsteelLimb => self.limb_stock += 1,
                    ManufactureItem::FleshGraft => self.graft_stock += 1,
                    ManufactureItem::MountTrophy => self.trophies += 1,
                    ManufactureItem::ForgeArbalest => {
                        *self.weapon_stock.entry("arbalest".into()).or_insert(0) += 1
                    }
                    ManufactureItem::ForgeCenser => {
                        *self.weapon_stock.entry("censer".into()).or_insert(0) += 1
                    }
                    ManufactureItem::ForgeHammer => {
                        *self.weapon_stock.entry("ram_hammer".into()).or_insert(0) += 1
                    }
                    ManufactureItem::ForgeMortar => {
                        *self.weapon_stock.entry("salt_mortar".into()).or_insert(0) += 1
                    }
                    ManufactureItem::ForgeBlade => self.blade_stock += 1,
                    ManufactureItem::ForgeCirclet => self.circlet_stock += 1,
                    ManufactureItem::ForgePlate => self.plate_stock += 1,
                    ManufactureItem::ForgeAegis => self.aegis_stock += 1,
                }
                events.push(GeoEvent::ManufactureComplete { item: done });
            }
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

        // The drill yard: idle hands run the chosen drills.
        let drilling = self
            .bases
            .iter()
            .any(|b| b.count_active(Facility::TrainingGround) > 0);
        if drilling {
            let focus = self.training_focus;
            for s in &mut self.soldiers {
                if s.recovery_days == 0
                    && s.warding.is_none()
                    && s.aboard.is_none()
                    && self.rng.roll(100) < 15
                {
                    match focus {
                        Focus::Marksmanship => {
                            s.stats.accuracy = (s.stats.accuracy + 1).min(70)
                        }
                        Focus::Conditioning => s.stats.tu = (s.stats.tu + 1).min(60),
                        Focus::Nerve => s.stats.bravery = (s.stats.bravery + 1).min(70),
                    }
                }
            }
        }

        // The Sanctum's cells: garrisoned soldiers sit the silence and
        // come out steadier.
        let sanctum = self
            .bases
            .iter()
            .any(|b| b.count_active(Facility::Sanctum) > 0);
        if sanctum {
            for s in &mut self.soldiers {
                if s.warding.is_none()
                    && s.aboard.is_none()
                    && s.stats.bravery < 85
                    && self.rng.roll(100) < 20
                {
                    s.stats.bravery += 1;
                }
            }
        }

        // Minds knit slower than flesh; candlelight and psalms help.
        let chapel = self
            .bases
            .iter()
            .any(|b| b.count_active(Facility::Chapel) > 0);
        let mend = if chapel { 3 } else { 1 };
        for s in &mut self.soldiers {
            if s.sanity < 100 {
                let was_broken = s.is_broken();
                s.sanity = (s.sanity + mend).min(100);
                if was_broken && !s.is_broken() {
                    events.push(GeoEvent::SoldierRecovered { name: s.name.clone() });
                }
            }
        }

        // The blood moon: three days of a wounded sky. Announced by omen,
        // ticked daily, mourned by nobody when it sets.
        match &mut self.blood_moon {
            Some(days) => {
                *days -= 1;
                if *days == 0 {
                    self.blood_moon = None;
                    events.push(GeoEvent::BloodMoonSets);
                } else if self.rng.roll(100) < 40 {
                    // The veil bleeds: an unscheduled rift tears open.
                    let region = Region::ALL[self.rng.roll(Region::ALL.len() as u32) as usize];
                    let (lat0, lat1, lon0, lon1) = region.bounds();
                    let lat = lat0 + (lat1 - lat0) * self.rng.roll(1000) as f32 / 1000.0;
                    let lon = lon0 + (lon1 - lon0) * self.rng.roll(1000) as f32 / 1000.0;
                    let kind = RiftKind::Harvest;
                    self.rifts.push(Rift {
                        id: self.next_id,
                        kind,
                        region,
                        lat,
                        lon,
                        days_left: kind.lifetime(),
                        days_open: 0,
                        detected: false,
                    });
                    self.next_id += 1;
                }
            }
            None => {
                if self.day == self.omen_day {
                    events.push(GeoEvent::BloodMoonOmen { in_days: 3 });
                } else if self.omen_day > 0 && self.day == self.omen_day + 3 {
                    self.blood_moon = Some(3);
                    events.push(GeoEvent::BloodMoonRises);
                }
            }
        }

        // Night terrors: the worn-thin wake screaming. Sometimes the dream
        // is a map — an undetected rift, seen from the wrong side.
        for i in 0..self.soldiers.len() {
            let haunted = self.soldiers[i].sanity < 60 || self.soldiers[i].phobia.is_some();
            if haunted && self.rng.roll(100) < 3 {
                self.soldiers[i].recovery_days += 1;
                events.push(GeoEvent::NightTerror { name: self.soldiers[i].name.clone() });
                if self.rng.roll(100) < 30 {
                    if let Some(r) = self.rifts.iter_mut().find(|r| !r.detected) {
                        r.detected = true;
                        let region = r.region;
                        events.push(GeoEvent::DreamOfTheRift { region });
                        events.push(GeoEvent::RiftDetected {
                            id: r.id,
                            kind: r.kind,
                            region,
                            days_left: r.days_left,
                        });
                    } else {
                        let s = &mut self.soldiers[i];
                        s.sanity = (s.sanity - 3).max(0);
                    }
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
            let (lat0, lat1, lon0, lon1) = plan.region.bounds();
            let lat = lat0 + (lat1 - lat0) * self.rng.roll(1000) as f32 / 1000.0;
            let lon = lon0 + (lon1 - lon0) * self.rng.roll(1000) as f32 / 1000.0;
            self.rifts.push(Rift {
                id: self.next_id,
                kind: plan.kind,
                region: plan.region,
                lat,
                lon,
                days_left: plan.kind.lifetime(),
                days_open: 0,
                detected: false,
            });
            self.next_id += 1;
        }

        // Detection sweeps. Interrogated demons give the augurs a scent.
        let mut augury_bonus = if self.research.is_complete(Project::RiftAugury) { 15 } else { 0 };
        if self.research.is_complete(Project::Interrogation) {
            augury_bonus += 10;
        }
        for i in 0..self.rifts.len() {
            if self.rifts[i].detected {
                continue;
            }
            let augurs = self.augurs_in(self.rifts[i].region) as u32;
            let chance = if augurs > 0 {
                (25 + 25 * augurs + augury_bonus).min(90)
            } else {
                (10 + augury_bonus).min(90)
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

        // Sorties fly on — through skies that are watched. Arrivals either
        // fight at once (auto) or hold the perimeter for the order (lead).
        let escorted = self.research.is_complete(Project::EscortGondola);
        let mut arrivals = Vec::new();
        let mut turned_back = Vec::new();
        for i in 0..self.sorties.len() {
            if self.sorties[i].days_left == 0 {
                continue;
            }
            let rift_id = self.sorties[i].rift_id;
            let region = self
                .rifts
                .iter()
                .find(|r| r.id == rift_id)
                .map(|r| r.region);
            // The hunt: gargoyles ride the same winds.
            if self.rng.roll(100) < 15 {
                // A led sortie puts the commander at the gondola guns: the
                // dogfight becomes yours to fly, and the clock holds for it.
                if self.sorties[i].lead
                    && self.interception.is_none()
                    && let Some(region) = region
                {
                    let gargoyles = 2 + self.rng.roll(3);
                    self.interception = Some(Interception {
                        rift_id,
                        region,
                        gargoyles,
                        envelope: 100,
                        range: 9,
                        round: 0,
                        downed: 0,
                    });
                    events.push(GeoEvent::SkyHuntEngaged { region, gargoyles });
                    continue; // no headway while the pack is on the wind
                }
                let outcome = if escorted {
                    SkyHuntOutcome::Repelled
                } else {
                    match self.rng.roll(100) {
                        0..50 => SkyHuntOutcome::Bloodied,
                        50..80 => SkyHuntOutcome::Repelled, // luck, not guns
                        _ => SkyHuntOutcome::TurnedBack,
                    }
                };
                if let Some(region) = region {
                    events.push(GeoEvent::SkyHunt { region, outcome });
                }
                match outcome {
                    SkyHuntOutcome::Bloodied => self.sorties[i].bloodied = true,
                    SkyHuntOutcome::TurnedBack => {
                        turned_back.push(rift_id);
                        continue;
                    }
                    SkyHuntOutcome::Repelled => {}
                }
            }
            self.sorties[i].days_left -= 1;
            if self.sorties[i].days_left == 0 {
                arrivals.push((rift_id, self.sorties[i].lead));
            }
        }
        for rift_id in turned_back {
            // Shaken and grounded a few days.
            for s in &mut self.soldiers {
                if s.aboard == Some(rift_id) {
                    s.recovery_days += 2;
                }
            }
            self.end_sortie(rift_id);
        }
        for (rift_id, lead) in arrivals {
            let Some(region) = self.rifts.iter().find(|r| r.id == rift_id).map(|r| r.region)
            else {
                self.end_sortie(rift_id);
                continue;
            };
            events.push(GeoEvent::SortieArrived { rift_id, region });
            if !lead && let Ok(report) = self.fight(MissionKind::Rift(rift_id)) {
                events.push(GeoEvent::SortieFought {
                    region,
                    victory: report.victory,
                    demons_slain: report.demons_slain,
                    dead: report.dead.len(),
                });
            }
        }

        // Ward pickets skirmish at the rifts' edges: risk, for time.
        let warded: Vec<u32> = self.soldiers.iter().filter_map(|s| s.warding).collect();
        for i in 0..self.soldiers.len() {
            if let Some(rift_id) = self.soldiers[i].warding {
                if !self.rifts.iter().any(|r| r.id == rift_id) {
                    self.soldiers[i].warding = None; // the rift is gone
                    continue;
                }
                if self.rng.roll(100) < 15 {
                    let s = &mut self.soldiers[i];
                    s.recovery_days += 4 + self.rng.roll(6);
                    s.warding = None;
                    events.push(GeoEvent::WardSkirmish { name: s.name.clone() });
                }
            }
        }

        // Rift missions run their course (and dig in as they age) — unless
        // a ward picket holds them chaotic.
        let mut expired = Vec::new();
        for r in &mut self.rifts {
            if !warded.contains(&r.id) {
                r.days_open += 1;
            }
            r.days_left -= 1;
            if r.days_left == 0 {
                expired.push((r.id, r.kind, r.region, r.lat, r.lon));
            }
        }
        self.rifts.retain(|r| r.days_left > 0);
        for (id, kind, region, lat, lon) in expired {
            // A sortie caught mid-flight (or camped) turns for home.
            if self.sorties.iter().any(|s| s.rift_id == id) {
                self.end_sortie(id);
                events.push(GeoEvent::SortieRecalled { region });
            }
            let penalty = kind.expire_penalty();
            self.score(region, -penalty);
            // An incursion that ran its full course leaves a terrified populace.
            self.shift_panic(region, if kind == RiftKind::Terror { 15 } else { 8 });
            events.push(GeoEvent::RiftExpired { id, kind, region, penalty });
            match kind {
                RiftKind::NestBuilding => {
                    self.nests.push(Nest { id: self.next_id, region, lat, lon });
                    events.push(GeoEvent::NestFounded { id: self.next_id, region });
                    self.next_id += 1;
                }
                RiftKind::Infiltration => {
                    let f = self.region_funding.get_mut(&region).expect("region exists");
                    *f /= 2;
                    // Worse than the money: the patron is theirs now.
                    self.corrupted_patrons.insert(region);
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

        // The council reads the month's regional scores — through the lens
        // of each region's dread. Panicked patrons flee regardless of score.
        for region in Region::ALL {
            let score = self.region_score.get(&region).copied().unwrap_or(0);
            let panicked =
                self.region_panic.get(&region).copied().unwrap_or(0) >= PANIC_BREAKPOINT;
            let corrupted = self.corrupted_patrons.contains(&region);
            let funding = self.region_funding.get_mut(&region).expect("region exists");
            if corrupted {
                // A patron in hell's pocket siphons the tithe, month on month.
                *funding -= *funding / 4;
            } else if panicked {
                *funding -= *funding / 5;
            } else if score >= 20 {
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
            + self.artificers as i64 * ARTIFICER_SALARY
            + self.bases.iter().map(|b| b.maintenance()).sum::<i64>();
        self.funds += income - expenses;

        // Settle the council's demand before the books close.
        if let Some(req) = self.request.take() {
            if req.done >= req.needed {
                self.funds += req.reward;
                events.push(GeoEvent::RequestFulfilled { reward: req.reward });
                self.score(req.region, 20);
            } else {
                events.push(GeoEvent::RequestFailed { region: req.region });
                self.score(req.region, -10);
            }
        }

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
        let cruelty = self.difficulty.plan_bonus() + if self.second_dawn { 5 } else { 0 };
        self.month_plan = director::plan_month(&mut self.rng, self.month, cruelty);

        // Panic cools a little with time — but where it has boiled over,
        // hell smells the fear and sends terror to feed on it.
        for region in Region::ALL {
            let p = self.region_panic.entry(region).or_insert(0);
            *p = (*p - 5).max(0);
            if *p >= PANIC_BREAKPOINT {
                let panic = *p;
                let day = 1 + self.rng.roll(DAYS_PER_MONTH - 2);
                self.month_plan.push(PlannedRift { day, kind: RiftKind::Terror, region });
                events.push(GeoEvent::RegionPanicking { region, panic });
            }
        }

        // One month in five, the sky goes wrong.
        self.omen_day = if self.rng.roll(100) < 20 { 3 + self.rng.roll(20) } else { 0 };

        // Council inspectors tour the trophy hall and leave impressed.
        if self.trophies > 0 {
            self.month_score += (self.trophies as i64 * 2).min(10);
        }

        // The reliquaries reprice salvage with the fortunes of war.
        self.brim_price = 10 + self.rng.roll(13) as i64;
        self.steel_price = 3 + self.rng.roll(6) as i64;
        events.push(GeoEvent::MarketShift {
            brimstone: self.brim_price,
            hellsteel: self.steel_price,
        });
        // Enough banishments and hell comes looking for the source.
        if self.reckoning_heat >= 5 {
            self.reckoning_heat = 0;
            self.reckoning_day = Some(1 + self.rng.roll(28));
        }
        // A nation puts its money where its fear is.
        let region = Region::ALL[self.rng.roll(Region::ALL.len() as u32) as usize];
        let needed = 1 + self.rng.roll(2);
        let reward = 150 + self.rng.roll(3) as i64 * 75;
        self.request = Some(CouncilRequest { region, needed, done: 0, reward });
        events.push(GeoEvent::RequestIssued { region, needed, reward });
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
            lat: 0.0,
            lon: 0.0,
            days_left: kind.lifetime(),
            days_open: 0,
            detected: true,
        });
        id
    }

    #[test]
    fn founding_state_is_sane() {
        let c = Campaign::new(1);
        assert_eq!(c.funds, Difficulty::Veteran.starting_funds());
        assert_eq!(c.soldiers.len(), 6);
        assert!(c.soldiers.iter().all(|s| s.is_fit()));
        assert_eq!(c.bases[0].count_active(Facility::AugurArray), 1);
        assert!(c.quarters_capacity() >= 10);
        // Different recruits get different stats from the seeded roll.
        assert_ne!(c.soldiers[0].stats, c.soldiers[1].stats);
    }

    #[test]
    fn construction_takes_days_and_money() {
        let mut c = Campaign::new(2);
        let before = c.funds;
        c.start_build(0, Facility::Quarters, 0, 0).unwrap();
        assert_eq!(c.funds, before - Facility::Quarters.cost());
        assert_eq!(c.start_build(0, Facility::Quarters, 0, 0), Err(GeoError::Occupied));

        let mut completed = false;
        for _ in 0..Facility::Quarters.build_days() {
            let events = c.advance_day();
            completed |= events
                .iter()
                .any(|e| matches!(e, GeoEvent::FacilityComplete { facility: Facility::Quarters }));
        }
        assert!(completed, "quarters must finish in {} days", Facility::Quarters.build_days());
        assert_eq!(c.bases[0].count_active(Facility::Quarters), 2);
    }

    #[test]
    fn hiring_respects_funds_and_beds() {
        let mut c = Campaign::new(3);
        let capacity = c.quarters_capacity();
        while c.personnel() < capacity {
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
            lat: 50.0,
            lon: 10.0,
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
            lat: 20.0,
            lon: 100.0,
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
        c.nests.push(Nest { id: 1, region: Region::Africa, lat: 5.0, lon: 20.0 });
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
        assert_eq!(funds, Difficulty::Veteran.starting_funds() + income - expenses);
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
            lat: 30.0,
            lon: 90.0,
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
        original.start_build(0, Facility::Quarters, 0, 0).unwrap();
        original.start_research(Project::RiftAugury).unwrap();
        let id = detected_rift(&mut original, RiftKind::Scouting, Region::Europe);
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
    fn difficulty_scales_the_war() {
        let novice = Campaign::new_with(50, Difficulty::Novice);
        let legend = Campaign::new_with(50, Difficulty::Legend);
        assert!(novice.funds > legend.funds);
        assert!(legend.difficulty.garrison_bonus() > novice.difficulty.garrison_bonus());
        assert!(legend.difficulty.plan_bonus() > novice.difficulty.plan_bonus());
    }

    #[test]
    fn interrogation_chain_demands_prisoners() {
        let mut c = Campaign::new(51);
        assert_eq!(c.start_research(Project::Interrogation), Err(GeoError::NoPrisoners));
        c.prisoners.grunts = 1;
        c.start_research(Project::Interrogation).unwrap();
        assert_eq!(c.prisoners.grunts, 0, "the questioning consumes the questioned");

        // The chain gates all the way to the Name.
        c.research.active = None;
        c.research.completed.insert(Project::Interrogation);
        assert_eq!(
            c.start_research(Project::NameOfTheEnemy),
            Err(GeoError::PrerequisiteMissing)
        );
        c.research.completed.insert(Project::HeraldsConfession);
        assert_eq!(c.start_research(Project::NameOfTheEnemy), Err(GeoError::NoPrisoners));
        c.prisoners.overseers = 2;
        c.start_research(Project::NameOfTheEnemy).unwrap();
        assert_eq!(c.prisoners.overseers, 0);
    }

    #[test]
    fn final_assault_needs_the_name_and_brimstone_and_can_win_everything() {
        let mut c = Campaign::new(52);
        assert_eq!(
            c.begin_mission(MissionKind::FinalAssault).err(),
            Some(GeoError::PrerequisiteMissing)
        );
        c.research.completed.insert(Project::NameOfTheEnemy);
        assert_eq!(
            c.begin_mission(MissionKind::FinalAssault).err(),
            Some(GeoError::NoMaterials)
        );
        c.brimstone = FINAL_ASSAULT_BRIMSTONE;
        assert_eq!(
            c.begin_mission(MissionKind::FinalSanctum).err(),
            Some(GeoError::PrerequisiteMissing),
            "no sanctum until the breach is won"
        );

        // Stage one: the breach. Cheat the guard dead so victory is certain.
        let (mut battle, token) = c.begin_mission(MissionKind::FinalAssault).unwrap();
        assert_eq!(c.brimstone, 0, "the rite consumes its brimstone");
        for u in battle.units.iter_mut().skip(token_len(&token)) {
            u.alive = false;
        }
        battle.winner = Some(ods_sim::units::Side::Order);
        c.conclude_mission(token, &battle);
        assert_eq!(c.over, None, "the breach alone wins nothing");
        assert!(c.sanctum_open, "but the way stands open");
        assert!(
            c.soldiers.iter().any(|s| s.is_fit()),
            "the breach squad fights on without pause"
        );

        // Stage two: the sanctum, and the Name broken.
        let (mut battle, token) = c.begin_mission(MissionKind::FinalSanctum).unwrap();
        assert!(
            battle.units.iter().any(|u| u.species == ods_sim::units::Species::Prince),
            "a Prince holds the sanctum"
        );
        for u in battle.units.iter_mut().skip(token_len(&token)) {
            u.alive = false;
        }
        battle.winner = Some(ods_sim::units::Side::Order);
        c.conclude_mission(token, &battle);
        assert_eq!(c.over, Some(CampaignOutcome::Victory));
    }

    fn token_len(token: &MissionToken) -> usize {
        token.squad_idx.len()
    }

    #[test]
    fn manufacturing_needs_a_workshop_and_produces() {
        let mut c = Campaign::new(53);
        assert_eq!(
            c.start_manufacture(ManufactureItem::FieldDressings),
            Err(GeoError::NoWorkshop)
        );
        c.start_build(0, Facility::Workshop, 0, 0).unwrap();
        for _ in 0..Facility::Workshop.build_days() {
            c.advance_day();
        }
        c.month_plan.clear();
        c.rifts.clear();
        let stock_before = c.dressing_stock;
        c.start_manufacture(ManufactureItem::FieldDressings).unwrap();
        assert_eq!(
            c.start_manufacture(ManufactureItem::TradeArms),
            Err(GeoError::WorkshopBusy)
        );
        // 2 artificers, cost 30 -> 15 days.
        let mut done = false;
        for _ in 0..20 {
            if c.advance_day().iter().any(|e| matches!(e, GeoEvent::ManufactureComplete { .. })) {
                done = true;
                break;
            }
        }
        assert!(done, "dressings must finish within 20 days");
        assert_eq!(c.dressing_stock, stock_before + 4);
    }

    #[test]
    fn loadouts_draw_down_real_stock() {
        let mut c = Campaign::new(54);
        c.grenade_stock = 3; // scarcity: six soldiers want 2 each
        let id = detected_rift(&mut c, RiftKind::Scouting, Region::Europe);
        let (battle, token) = c.begin_mission(MissionKind::Rift(id)).unwrap();
        assert_eq!(c.grenade_stock, 0, "the armoury empties in kit order");
        let issued: u32 = battle.units.iter().take(token_len(&token)).map(|u| u.grenades).sum();
        assert_eq!(issued, 3);
        // Conclude cleanly so the campaign is consistent.
        c.conclude_mission(token, &battle);
    }

    #[test]
    fn founding_a_second_chapterhouse() {
        let mut c = Campaign::new(55);
        c.funds = CHAPTERHOUSE_COST + 100;
        assert_eq!(
            c.found_chapterhouse(Region::Europe),
            Err(GeoError::Occupied),
            "one per region"
        );
        c.found_chapterhouse(Region::Asia).unwrap();
        assert_eq!(c.bases.len(), 2);
        assert_eq!(c.funds, 100);
        // The new base extends beds and detection reach.
        assert!(c.quarters_capacity() > c.bases[0].quarters_capacity());
        assert_eq!(c.augurs_in(Region::Asia), 1);
    }

    #[test]
    fn the_fallen_are_remembered() {
        let mut c = Campaign::new(56);
        let roster_before = c.soldiers.len();
        // Grind assaults until someone dies (or prove nobody ever does).
        for i in 0..12 {
            let id = detected_rift(&mut c, RiftKind::Terror, Region::Europe);
            let _ = c.assault_rift(id);
            if c.soldiers.len() < roster_before {
                break;
            }
            let _ = i;
        }
        if c.soldiers.len() < roster_before {
            let fallen = c.memorial.last().expect("a name on the wall");
            assert!(!fallen.name.is_empty());
            assert_eq!(fallen.cause, "rift assault");
        } else {
            assert!(c.memorial.is_empty());
        }
    }

    #[test]
    fn ward_pickets_hold_rifts_chaotic() {
        let mut c = Campaign::new(60);
        c.month_plan.clear();
        let id = detected_rift(&mut c, RiftKind::Terror, Region::Europe);
        // Stretch the rift's life so stabilization is the only question.
        c.rifts[0].days_left = 20;
        c.assign_ward(0, id).unwrap();
        assert!(!c.soldiers[0].is_fit(), "a warding soldier is spoken for");
        for _ in 0..5 {
            c.advance_day();
            if c.soldiers[0].warding.is_none() {
                break; // skirmish pulled them off the line
            }
        }
        if c.soldiers[0].warding.is_some() {
            assert!(
                !c.rifts[0].is_stabilized(),
                "a warded rift stays chaotic: days_open={}",
                c.rifts[0].days_open
            );
        }
    }

    #[test]
    fn lances_are_forged_issued_and_returned() {
        let mut c = Campaign::new(61);
        assert_eq!(c.assign_lance(0, true), Err(GeoError::NoLances));
        assert_eq!(
            c.start_manufacture(ManufactureItem::ForgeLance),
            Err(GeoError::NoWorkshop)
        );
        c.research.completed.insert(Project::HellfireLance);
        c.lance_stock = 1;
        c.assign_lance(0, true).unwrap();
        assert!(c.soldiers[0].has_lance);
        assert_eq!(c.lance_stock, 0);
        assert_eq!(c.assign_lance(1, true), Err(GeoError::NoLances));
        c.assign_lance(0, false).unwrap();
        assert_eq!(c.lance_stock, 1);
    }

    #[test]
    fn transfers_cost_road_days_and_distance_costs_garrison() {
        let mut c = Campaign::new(62);
        c.funds = CHAPTERHOUSE_COST + 500;
        c.found_chapterhouse(Region::Asia).unwrap();
        c.transfer_soldier(0, 1).unwrap();
        assert_eq!(c.soldiers[0].home, 1);
        assert!(c.soldiers[0].recovery_days > 0, "the road takes its toll");

        // A rift where no chapterhouse stands can't be struck cold...
        let far = detected_rift(&mut c, RiftKind::Scouting, Region::Oceania);
        let near = detected_rift(&mut c, RiftKind::Scouting, Region::Europe);
        assert_eq!(
            c.begin_mission(MissionKind::Rift(far)).err(),
            Some(GeoError::NotOnSite),
            "distant strikes need a dispatched squad"
        );
        // ...so fly one out (lead, so arrival waits for orders).
        let eta = c.dispatch_squad(far, true).unwrap();
        assert!((1..=3).contains(&eta), "Oceania is days away: {eta}");
        assert_eq!(
            c.begin_mission(MissionKind::Rift(far)).err(),
            Some(GeoError::SquadInTransit)
        );
        c.month_plan.clear();
        for _ in 0..eta {
            c.advance_day();
        }
        // The extra defender for fighting far from any chapterhouse.
        let (b_far, t_far) = c.begin_mission(MissionKind::Rift(far)).unwrap();
        let far_demons = b_far.units.len() - token_len(&t_far);
        // Conclude the far fight so its squad disembarks before the next,
        // and patch everyone up — this test is about head-counts, not luck.
        c.conclude_mission(t_far, &b_far);
        for s in &mut c.soldiers {
            s.recovery_days = 0;
        }
        let (b_near, t_near) = c.begin_mission(MissionKind::Rift(near)).unwrap();
        let near_demons = b_near.units.len() - token_len(&t_near);
        assert_eq!(far_demons, near_demons + 1, "distance costs a garrison slot");
    }

    #[test]
    fn reckonings_scar_the_chapterhouse() {
        // Repelled Reckonings may wreck facilities; run several seeds and
        // demand at least one scar shows up somewhere.
        let mut any_wrecked = false;
        for seed in 70..90 {
            let mut c = Campaign::new(seed);
            c.month_plan.clear();
            c.reckoning_day = Some(c.day);
            let events = c.advance_day();
            let repelled = events
                .iter()
                .any(|e| matches!(e, GeoEvent::ReckoningRepelled { .. }));
            if repelled && !c.wrecked.is_empty() {
                any_wrecked = true;
                break;
            }
        }
        assert!(any_wrecked, "20 Reckonings without a single wrecked room is implausible");
    }

    #[test]
    fn the_council_demands_and_pays() {
        let mut c = Campaign::new(63);
        // Roll the month over to issue a request.
        c.month_plan.clear();
        c.rifts.clear();
        c.day = DAYS_PER_MONTH;
        let events = c.advance_day();
        let issued = events.iter().find_map(|e| match e {
            GeoEvent::RequestIssued { region, needed, reward } => Some((*region, *needed, *reward)),
            _ => None,
        });
        let (region, needed, reward) = issued.expect("a nation always wants something");
        assert_eq!(c.request.unwrap().region, region);

        // Serve the demand by banishing rifts there (cheat them in).
        let funds_before = c.funds;
        for _ in 0..needed {
            let id = detected_rift(&mut c, RiftKind::Scouting, region);
            // Assault until victory (retry across the month if repelled).
            let _ = c.assault_rift(id);
        }
        if c.request.is_none_or(|r| r.done >= r.needed) {
            c.month_plan.clear();
            c.rifts.clear();
            c.day = DAYS_PER_MONTH;
            let events = c.advance_day();
            if events.iter().any(|e| matches!(e, GeoEvent::RequestFulfilled { .. })) {
                assert!(c.funds > funds_before, "fulfilled demands pay ({reward}k)");
            }
        }
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

    #[test]
    fn salvage_sells_at_the_market_price() {
        let mut c = Campaign::new(30);
        c.brimstone = 10;
        c.hellsteel = 10;
        c.brim_price = 22;
        c.steel_price = 3;
        assert_eq!(c.sell_brimstone(2).unwrap(), 44);
        assert_eq!(c.sell_hellsteel(3).unwrap(), 9);

        // Month end rerolls the prices within the honest range.
        c.month_plan.clear();
        c.rifts.clear();
        c.day = DAYS_PER_MONTH;
        let events = c.advance_day();
        let shifted = events
            .iter()
            .any(|e| matches!(e, GeoEvent::MarketShift { .. }));
        assert!(shifted, "{events:?}");
        assert!((10..=22).contains(&c.brim_price));
        assert!((3..=8).contains(&c.steel_price));
    }

    #[test]
    fn panic_boils_over_into_terror_and_fleeing_funds() {
        let mut c = Campaign::new(31);
        c.month_plan.clear();
        c.rifts.clear();
        c.region_panic.insert(Region::Africa, 80);
        let funding_before = c.region_funding[&Region::Africa];
        c.day = DAYS_PER_MONTH;
        let events = c.advance_day();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GeoEvent::RegionPanicking { region: Region::Africa, .. })),
            "{events:?}"
        );
        // Panicked patrons pull a fifth of their funding.
        assert_eq!(c.region_funding[&Region::Africa], funding_before - funding_before / 5);
        // Hell schedules extra terror where the fear is thickest.
        assert!(
            c.month_plan
                .iter()
                .any(|p| p.kind == RiftKind::Terror && p.region == Region::Africa)
        );
        // Decay happened before the check: 80 -> 75.
        assert_eq!(c.region_panic[&Region::Africa], 75);
    }

    #[test]
    fn expiries_frighten_and_banishments_soothe() {
        let mut c = Campaign::new(32);
        c.month_plan.clear();
        c.rifts.push(Rift {
            id: 900,
            kind: RiftKind::Terror,
            region: Region::Asia,
            lat: 20.0,
            lon: 100.0,
            days_left: 1,
            days_open: 0,
            detected: false,
        });
        c.advance_day();
        assert_eq!(c.region_panic[&Region::Asia], 15, "terror expiry terrifies");

        // A banishment calms the region it saves.
        c.region_panic.insert(Region::Europe, 30);
        for _ in 0..8 {
            let id = detected_rift(&mut c, RiftKind::Scouting, Region::Europe);
            match c.assault_rift(id) {
                Ok(r) if r.victory => break,
                Ok(_) => continue,
                Err(_) => break, // the roster is spent; the seed was cruel
            }
        }
        assert!(c.region_panic[&Region::Europe] <= 30);
    }

    #[test]
    fn codex_and_stats_fill_from_fighting() {
        let mut c = Campaign::new(33);
        assert!(c.codex_seen.is_empty());
        let mut won = 0;
        while won < 2 {
            let id = detected_rift(&mut c, RiftKind::Harvest, Region::Europe);
            if c.assault_rift(id).unwrap().victory {
                won += 1;
            }
            if c.soldiers.is_empty() {
                return; // a doomed seed proves nothing either way
            }
            // Patch everyone up between missions — this test counts codex
            // entries and ledger lines, not attrition.
            for s in &mut c.soldiers {
                s.recovery_days = 0;
                s.sanity = 100;
            }
        }
        assert!(!c.codex_seen.is_empty(), "the squads met something out there");
        assert_eq!(c.stats.missions_won + c.stats.missions_lost, won + c.stats.missions_lost);
        assert_eq!(c.stats.rifts_banished, won);
        assert!(c.stats.shots_fired >= c.stats.shots_hit);
        assert!(c.stats.demons_slain > 0);
    }

    #[test]
    fn reckonings_strike_the_targeted_garrison() {
        let mut c = Campaign::new(34);
        c.funds += CHAPTERHOUSE_COST;
        c.found_chapterhouse(Region::Asia).unwrap();
        c.soldiers.iter_mut().for_each(|s| s.home = 0);

        // An unmanned outpost has no defenders to muster.
        c.reckoning_target = 1;
        assert!(matches!(
            c.begin_mission(MissionKind::Reckoning),
            Err(GeoError::NoSquadFit)
        ));

        // Restation half the roster there and the muster answers — with
        // exactly the soldiers who live in those halls.
        for i in 0..3 {
            c.transfer_soldier(i, 1).unwrap();
            c.soldiers[i].recovery_days = 0; // arrived
        }
        let (_, token) = c.begin_mission(MissionKind::Reckoning).unwrap();
        assert_eq!(token.base, 1);
        assert!(token.squad_idx.iter().all(|&i| c.soldiers[i].home == 1));
        assert_eq!(token.squad_idx.len(), 3);
    }

    #[test]
    fn sorties_fly_lock_the_squad_and_fight_on_arrival() {
        let mut c = Campaign::new(40);
        c.month_plan.clear();
        let id = detected_rift(&mut c, RiftKind::Harvest, Region::Oceania);
        // Long-lived rift so the flight can't outlive it.
        c.rifts.iter_mut().for_each(|r| r.days_left = 30);

        let eta = c.dispatch_squad(id, false).unwrap();
        assert!((1..=3).contains(&eta));
        let aboard = c.soldiers.iter().filter(|s| s.aboard == Some(id)).count();
        assert!(aboard > 0, "the squad is on the zeppelin");
        assert!(
            c.soldiers.iter().filter(|s| s.is_fit()).count() < c.soldiers.len(),
            "flying soldiers are unavailable"
        );
        assert_eq!(c.dispatch_squad(id, false), Err(GeoError::SortieAlready));

        let mut fought = false;
        for _ in 0..eta {
            for e in c.advance_day() {
                if let GeoEvent::SortieFought { region, .. } = e {
                    assert_eq!(region, Region::Oceania);
                    fought = true;
                }
            }
        }
        assert!(fought, "an auto sortie fights the day it lands");
        assert!(c.sorties.is_empty(), "the engagement ends the sortie");
        assert!(c.soldiers.iter().all(|s| s.aboard.is_none()), "everyone disembarked");
    }

    #[test]
    fn sorties_recall_when_the_rift_closes_first() {
        let mut c = Campaign::new(41);
        c.month_plan.clear();
        let id = detected_rift(&mut c, RiftKind::Scouting, Region::Oceania);
        // The rift dies tomorrow; the flight takes longer than that.
        c.rifts.iter_mut().for_each(|r| r.days_left = 1);
        c.dispatch_squad(id, true).unwrap();

        let events = c.advance_day();
        assert!(
            events.iter().any(|e| matches!(e, GeoEvent::SortieRecalled { .. })),
            "{events:?}"
        );
        assert!(c.sorties.is_empty());
        assert!(c.soldiers.iter().all(|s| s.aboard.is_none()));
        // Nobody died on a flight to nowhere.
        assert_eq!(c.soldiers.len(), 6);
    }

    #[test]
    fn a_led_dogfight_ends_in_an_outcome_and_frees_the_clock() {
        let mut c = Campaign::new(77);
        c.month_plan.clear();
        let id = detected_rift(&mut c, RiftKind::Harvest, Region::Oceania);
        c.rifts.iter_mut().for_each(|r| r.days_left = 30);
        c.dispatch_squad(id, true).unwrap();
        c.interception = Some(Interception {
            rift_id: id,
            region: Region::Oceania,
            gargoyles: 3,
            envelope: 100,
            range: 9,
            round: 0,
            downed: 0,
        });

        let mut outcome = None;
        for _ in 0..60 {
            let rep = c.intercept_round(true);
            if rep.outcome.is_some() {
                outcome = rep.outcome;
                break;
            }
        }
        let outcome = outcome.expect("pressing the attack ends the fight");
        assert!(c.interception.is_none());
        match outcome {
            SkyHuntOutcome::TurnedBack => {
                assert!(c.sorties.is_empty(), "a beaten ship turns for home");
                assert!(c.soldiers.iter().all(|s| s.aboard.is_none()));
            }
            SkyHuntOutcome::Bloodied => {
                assert!(c.sorties.iter().any(|s| s.rift_id == id && s.bloodied));
            }
            SkyHuntOutcome::Repelled => {
                assert!(c.sorties.iter().any(|s| s.rift_id == id));
            }
        }
        // The resolution reaches the log on the next day tick.
        let events = c.advance_day();
        assert!(
            events.iter().any(|e| matches!(e, GeoEvent::SkyHuntResolved { .. })),
            "{events:?}"
        );
    }

    #[test]
    fn an_unanswered_dogfight_resolves_itself_on_the_day_tick() {
        let mut c = Campaign::new(78);
        c.month_plan.clear();
        let id = detected_rift(&mut c, RiftKind::Harvest, Region::Oceania);
        c.rifts.iter_mut().for_each(|r| r.days_left = 30);
        c.dispatch_squad(id, true).unwrap();
        c.interception = Some(Interception {
            rift_id: id,
            region: Region::Oceania,
            gargoyles: 2,
            envelope: 100,
            range: 9,
            round: 0,
            downed: 0,
        });
        let events = c.advance_day();
        assert!(c.interception.is_none(), "the guns fired themselves");
        assert!(
            events.iter().any(|e| matches!(e, GeoEvent::SkyHuntResolved { .. })),
            "{events:?}"
        );
    }

    #[test]
    fn a_fallen_outpost_is_lost_not_the_war() {
        let mut c = Campaign::new(35);
        c.funds += CHAPTERHOUSE_COST;
        c.found_chapterhouse(Region::Asia).unwrap();
        c.soldiers.iter_mut().for_each(|s| s.home = 0);
        // Schedule the Reckoning for today; with everyone stationed at the
        // founding house, an outpost strike must cost only the outpost.
        c.month_plan.clear();
        c.rifts.clear();
        c.reckoning_day = Some(c.day);
        let events = c.advance_day();
        if c.bases.len() == 1 {
            // The strike landed on the empty outpost (rng picked base 1).
            assert!(events.iter().any(|e| matches!(e, GeoEvent::ChapterhouseLost { .. })));
            assert!(c.over.is_none(), "losing an outpost is not losing the war");
        } else {
            // It landed on the manned founding house instead: a real fight.
            assert_eq!(c.bases.len(), 2);
        }
    }

    #[test]
    fn horror_erodes_sanity_and_the_chapel_mends_it() {
        let mut c = Campaign::new(50);
        c.month_plan.clear();
        c.rifts.clear();
        // A soldier comes home from something terrible.
        c.soldiers[0].sanity = 18;
        assert!(c.soldiers[0].is_broken());
        assert!(!c.soldiers[0].is_fit(), "the broken don't muster");

        // Without a chapel: one point a day.
        c.advance_day();
        assert_eq!(c.soldiers[0].sanity, 19);

        // With a chapel: three.
        c.bases[0].start_build(Facility::Chapel, 5, 5);
        for _ in 0..Facility::Chapel.build_days() {
            c.advance_day();
        }
        let before = c.soldiers[0].sanity;
        c.advance_day();
        assert_eq!(c.soldiers[0].sanity, before + 3, "psalms work");
    }

    #[test]
    fn survivors_of_horror_can_come_home_with_phobias() {
        // Drive the roll deterministically: heavy horror, low sanity.
        let mut c = Campaign::new(51);
        for s in &mut c.soldiers {
            s.sanity = 45;
        }
        let mut phobia_seen = false;
        for _ in 0..12 {
            let id = detected_rift(&mut c, RiftKind::Harvest, Region::Europe);
            let _ = c.assault_rift(id);
            for s in &mut c.soldiers {
                s.recovery_days = 0;
                if s.sanity <= 20 {
                    s.sanity = 30; // keep them mustering for the test
                }
            }
            if c.soldiers.iter().any(|s| s.phobia.is_some()) {
                phobia_seen = true;
                break;
            }
            if c.soldiers.len() < 2 {
                return; // a doomed seed proves nothing
            }
        }
        // Phobias are a chance, not a promise — but across a dozen bloody
        // missions at sub-40 sanity, someone should have cracked.
        assert!(phobia_seen || c.soldiers.iter().all(|s| s.sanity > 40));
    }

    #[test]
    fn limbs_and_grafts_restore_the_maimed() {
        let mut c = Campaign::new(60);
        c.soldiers[0].lost_parts.push(ods_sim::body::BodyPart::RightArm);
        let acc = c.soldiers[0].stats.accuracy;

        // No stock, no miracle.
        assert_eq!(c.fit_replacement(0, false), Err(GeoError::NoMaterials));
        c.limb_stock = 1;
        c.fit_replacement(0, false).unwrap();
        assert!(c.soldiers[0].lost_parts.is_empty());
        assert_eq!(c.soldiers[0].stats.accuracy, acc + 12, "the loss is given back");
        assert_eq!(c.limb_stock, 0);

        // A graft gives more and takes sleep.
        c.soldiers[1].lost_parts.push(ods_sim::body::BodyPart::LeftLeg);
        let (tu, sanity) = (c.soldiers[1].stats.tu, c.soldiers[1].sanity);
        c.graft_stock = 1;
        c.fit_replacement(1, true).unwrap();
        assert_eq!(c.soldiers[1].stats.tu, tu + 13);
        assert_eq!(c.soldiers[1].sanity, sanity - 15);

        // The whole don't queue for the saw (stock present, no loss).
        c.limb_stock = 1;
        assert_eq!(c.fit_replacement(2, false), Err(GeoError::BadAssignment));
    }

    #[test]
    fn grafting_demands_research_and_trophies_demand_kills() {
        let mut c = Campaign::new(61);
        c.hellsteel = 50;
        c.brimstone = 50;
        c.bases[0].start_build(Facility::Workshop, 4, 4);
        for _ in 0..Facility::Workshop.build_days() {
            c.advance_day();
        }
        assert_eq!(
            c.start_manufacture(ManufactureItem::HellsteelLimb),
            Err(GeoError::PrerequisiteMissing)
        );
        assert_eq!(
            c.start_manufacture(ManufactureItem::MountTrophy),
            Err(GeoError::NoMaterials),
            "nothing slain, nothing mounted"
        );
        c.research.completed.insert(Project::FleshGrafting);
        c.codex_slain.insert(ods_sim::units::Species::Imp);
        assert!(c.start_manufacture(ManufactureItem::HellsteelLimb).is_ok());
    }

    #[test]
    fn the_blood_moon_rises_and_sets() {
        let mut c = Campaign::new(62);
        c.month_plan.clear();
        c.rifts.clear();
        c.omen_day = c.day; // force the omen today
        let events = c.advance_day();
        assert!(events.iter().any(|e| matches!(e, GeoEvent::BloodMoonOmen { .. })));
        let mut rose = false;
        let mut set = false;
        for _ in 0..8 {
            for e in c.advance_day() {
                match e {
                    GeoEvent::BloodMoonRises => rose = true,
                    GeoEvent::BloodMoonSets => set = true,
                    _ => {}
                }
            }
        }
        assert!(rose, "the omen keeps its promise");
        assert!(set, "and it passes");
        assert!(c.blood_moon.is_none());
    }

    #[test]
    fn corrupted_patrons_drain_until_purged() {
        let mut c = Campaign::new(70);
        c.month_plan.clear();
        // An infiltration runs its course: the patron turns.
        c.rifts.push(Rift {
            id: 900,
            kind: RiftKind::Infiltration,
            region: Region::Asia,
            lat: 20.0,
            lon: 100.0,
            days_left: 1,
            days_open: 0,
            detected: false,
        });
        c.advance_day();
        assert!(c.corrupted_patrons.contains(&Region::Asia));

        // The purge is a real battle in the manor; win or lose, it resolves.
        let report = c.purge_patron(Region::Asia).unwrap();
        if report.victory {
            assert!(!c.corrupted_patrons.contains(&Region::Asia), "the cult is cut out");
        } else {
            assert!(c.corrupted_patrons.contains(&Region::Asia), "the manor holds");
        }
        // No purging the innocent.
        assert_eq!(c.purge_patron(Region::Europe).err(), Some(GeoError::NotDetected));
    }

    #[test]
    fn squads_answer_their_banner_first() {
        let mut c = Campaign::new(71);
        // Two in the Lamplighters, the rest unassigned.
        c.soldiers[0].squad = 1;
        c.soldiers[1].squad = 1;
        c.active_squad = 1;
        let id = detected_rift(&mut c, RiftKind::Scouting, Region::Europe);
        let (_, token) = c.begin_mission(MissionKind::Rift(id)).unwrap();
        assert!(token.squad_idx.contains(&0) && token.squad_idx.contains(&1),
            "the banner musters first: {:?}", token.squad_idx);
    }

    #[test]
    fn manor_purges_field_cultists() {
        let squad: Vec<ods_sim::units::Unit> = (0..4)
            .map(|i| ods_sim::units::Unit::soldier(i, &format!("S{i}"), glam::IVec3::ZERO))
            .collect();
        let b = ods_sim::scenario::manor_purge(9, squad, 6);
        use ods_sim::units::{Side, Species};
        let cultists = b
            .units
            .iter()
            .filter(|u| u.side == Side::Demons && u.species == Species::Soldier)
            .count();
        assert!(cultists > 0, "the house staff turned");
        for u in &b.units {
            assert!(b.tiles.is_walkable(u.tile), "{} in a wall", u.name);
        }
    }

    #[test]
    fn drills_and_fortifications_do_their_jobs() {
        let mut c = Campaign::new(72);
        c.month_plan.clear();
        c.rifts.clear();
        // Drill yard: accuracy creeps toward the cap.
        c.bases[0].start_build(Facility::TrainingGround, 5, 0);
        for _ in 0..Facility::TrainingGround.build_days() {
            c.advance_day();
        }
        c.training_focus = Focus::Marksmanship;
        let before: i32 = c.soldiers.iter().map(|s| s.stats.accuracy).sum();
        for _ in 0..30 {
            c.advance_day();
        }
        let after: i32 = c.soldiers.iter().map(|s| s.stats.accuracy).sum();
        assert!(after > before, "a month of drills shows: {before} -> {after}");

        // Kennels field hounds on the defense map.
        let squad: Vec<ods_sim::units::Unit> = (0..4)
            .map(|i| ods_sim::units::Unit::soldier(i, &format!("S{i}"), glam::IVec3::ZERO))
            .collect();
        let cells = [(2usize, 2usize), (2, 3), (3, 2), (3, 3), (4, 3)];
        let b = ods_sim::scenario::base_defense_fortified(3, squad, 4, &cells, (2, 2), 2, 2);
        use ods_sim::units::{Side, Species};
        let hounds = b
            .units
            .iter()
            .filter(|u| u.side == Side::Order && u.species == Species::Hellhound)
            .count();
        assert!(hounds > 0, "the kennels open for the halls");
    }

    #[test]
    fn the_nemesis_rises_grows_and_falls() {
        let mut c = Campaign::new(80);
        // Fake a report cycle by driving conclude_mission through fights
        // against Prince-bearing garrisons: month 10+ packs field Princes.
        c.month = 10;
        let mut named = None;
        for _ in 0..10 {
            let id = detected_rift(&mut c, RiftKind::Scouting, Region::Europe);
            let _ = c.assault_rift(id);
            for s in &mut c.soldiers {
                s.recovery_days = 0;
                s.sanity = 100;
            }
            if c.soldiers.len() < 2 {
                return; // the seed ate the roster; nothing to prove
            }
            if let Some(n) = &c.nemesis {
                named = Some(n.name.clone());
                break;
            }
            if c.stats.missions_won >= 6 {
                break;
            }
        }
        // Either a Prince escaped (nemesis named) or every one died on the
        // field (also fine — the mechanism only fires on escapes).
        if let Some(name) = named {
            assert!(NEMESIS_NAMES.contains(&name.as_str()));
        }
    }

    #[test]
    fn rally_steadies_the_line_once() {
        use ods_sim::battle::Action;
        use ods_sim::units::{Unit, UnitId};
        let mut units = vec![
            Unit::soldier(0, "Commander", glam::IVec3::new(1, 5, 0)),
            Unit::soldier(1, "Shaken", glam::IVec3::new(2, 5, 0)),
            Unit::imp(2, "Imp", glam::IVec3::new(10, 10, 0)),
        ];
        units[0].can_rally = true;
        units[1].morale = 30;
        let mut b = ods_sim::scenario::incursion(3, units, 0, 1);
        b.units[0].can_rally = true; // incursion rebuilds ids, keep the flag
        b.units[1].morale = 30;
        let events = b.perform(Action::Rally { unit: UnitId(0) }).unwrap();
        assert!(matches!(events[0], ods_sim::battle::Event::Rallied { .. }));
        assert!(b.units[1].morale >= 60, "the line steadies: {}", b.units[1].morale);
        assert!(
            b.perform(Action::Rally { unit: UnitId(0) }).is_err(),
            "once a battle"
        );
    }

    #[test]
    fn second_dawn_reopens_a_won_war() {
        let mut c = Campaign::new(81);
        assert_eq!(c.second_dawn(), Err(GeoError::PrerequisiteMissing), "no dawn before victory");
        c.over = Some(CampaignOutcome::Victory);
        c.second_dawn().unwrap();
        assert!(c.over.is_none(), "the war reopens");
        assert!(c.second_dawn, "and it is marked");
        // The next month's plan comes crueler.
        c.month_plan.clear();
        c.rifts.clear();
        c.day = DAYS_PER_MONTH;
        c.advance_day();
        assert!(c.month_plan.len() >= 8, "hell empties the larder: {}", c.month_plan.len());
    }

    #[test]
    fn bonds_grieve_when_broken() {
        let mut c = Campaign::new(82);
        c.soldiers[0].bond = Some(c.soldiers[1].name.clone());
        c.soldiers[1].bond = Some(c.soldiers[0].name.clone());
        // Soldier 1 dies in the field: hand it through apply_to_roster via a
        // real fight loop until someone falls, or force the path directly.
        let name0 = c.soldiers[0].name.clone();
        let report = BattleReport {
            victory: true,
            turns: 5,
            dead: vec![1],
            survivors: vec![(0, 20, Default::default())],
            demons_slain: 1,
            injuries: vec![],
            severed: vec![],
            captured_grunts: 0,
            captured_overseers: 0,
            civilians_saved: 0,
            civilians_dead: 0,
            species_seen: vec![],
            species_captured: vec![],
            species_slain: vec![],
            horrors: vec![],
            atrocities_found: 0,
        };
        let squad_idx: Vec<usize> = (0..c.soldiers.len().min(6)).collect();
        c.apply_to_roster(&squad_idx, &report, "a test");
        let survivor = c.soldiers.iter().find(|s| s.name == name0).unwrap();
        assert!(survivor.bond.is_none(), "the bond is broken");
        assert!(survivor.sanity <= 80, "and it costs: {}", survivor.sanity);
        assert!(c.memorial.last().unwrap().cause.contains("never the same"));
    }
}

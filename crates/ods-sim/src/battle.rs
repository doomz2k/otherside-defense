//! The Battlescape state machine: actions in, events out, deterministic
//! given the seed. Nothing here renders; nothing here reads the clock.

use std::collections::{HashMap, HashSet};

use glam::{IVec3, Vec3};
use ods_voxel::VoxelWorld;

use crate::body::BodyPart;
use crate::scenario::{MAT_BLOOD, MAT_GORE};
use crate::VS;

/// Center of a tile in voxels, as a float.
const HALF_TILE: f32 = crate::TILE_VOXELS as f32 / 2.0;
use crate::tiles::{PathMode, TileMap, step_cost};
use crate::units::{FireMode, Side, Species, Unit, UnitId};
use crate::{SimRng, TILE_VOXELS};

/// Vision range in tiles (Chebyshev). The Otherside fights at night.
pub const VISION_TILES: i32 = 14;

/// Eye and chest heights in voxels above a tile's minimum corner (assumes
/// floors sit in the tile's lower quarter, which map generation guarantees).
const EYE_Z: f32 = 13.0 * VS as f32;
const CHEST_Z: f32 = 9.0 * VS as f32;

/// Hellfire charge (grenade) parameters.
pub const GRENADE_POWER: i32 = 40;
/// Damage past death at which a body simply comes apart.
const GIB_OVERKILL: i32 = 12;
pub(crate) const DEVOUR_TU: i32 = 20;
pub(crate) const DEFILE_TU: i32 = 25;
const AMPUTATE_TU: i32 = 25;
const WARD_TU: i32 = 20;
const RALLY_TU: i32 = 20;
const RALLY_RANGE_TILES: i32 = 8;
/// How far the censer throws its burning arc.
const CENSER_RANGE_TILES: i32 = 5;
/// The consecrated blade's riposte damage base.
const BLADE_POWER: i32 = 18;
/// How far the obelisk's veins can reach.
const CORRUPTION_CAP: usize = 20;
/// What a ward does to the demon that crosses it.
const WARD_BURN: i32 = 8;
/// Turns of festering before an infected soldier turns.
const INFECTION_TURNS: u32 = 4;
pub const GRENADE_RANGE_TILES: i32 = 10;
pub const GRENADE_COST_PCT: i32 = 45;
pub const GRENADE_CARVE_RADIUS: f32 = 7.0 * VS as f32;
/// Blast damages units within this many tiles (Chebyshev) of the impact.
pub const BLAST_TILES: i32 = 2;
/// Field dressing: flat TU cost, wounds staunched, health restored.
pub const HEAL_COST_TU: i32 = 12;
pub const HEAL_AMOUNT: i32 = 4;

/// TU cost to kneel or rise.
pub const KNEEL_COST: i32 = 4;
/// Binding rod: cost as % of max TUs, base stun inflicted.
pub const BIND_COST_PCT: i32 = 25;
pub const BIND_STUN: i32 = 15;
/// Terrify psi attack: cost and range.
pub const TERRIFY_COST_PCT: i32 = 25;
pub const TERRIFY_RANGE_TILES: i32 = 14;
/// Full possession (Princes only).
pub const POSSESS_COST_PCT: i32 = 40;
/// Slapping a fresh magazine into a clip-fed weapon.
pub const RELOAD_TU: i32 = 12;
/// Quick-drawing the sidearm (or holstering back).
pub const SWAP_TU: i32 = 6;
/// Taking a weapon up off the ground.
pub const SCAVENGE_TU: i32 = 8;
/// Putting down something helpless at your feet.
pub const EXECUTE_TU: i32 = 10;
/// Digging a consumable out of the pack once the belt runs empty.
pub const PACK_FETCH_TU: i32 = 6;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CloudKind {
    Smoke,
    Fire,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    TurnStarted { side: Side, turn: u32 },
    Moved { unit: UnitId, from: IVec3, to: IVec3, tu_left: i32 },
    Fired { unit: UnitId, target: UnitId, mode: FireMode, reaction: bool, hit: bool },
    Threw { unit: UnitId, at: IVec3 },
    Exploded { at: IVec3, voxels: usize },
    Damaged { unit: UnitId, amount: i32, health_left: i32 },
    /// The hit opened fatal wounds; `total` is the unit's open wound count.
    Wounded { unit: UnitId, total: i32 },
    /// Turn-start bleeding from open wounds.
    Bled { unit: UnitId, health_left: i32 },
    Healed { medic: UnitId, target: UnitId, health_left: i32 },
    Died { unit: UnitId },
    TerrainDestroyed { center: Vec3, voxels: usize },
    Panicked { unit: UnitId },
    /// Dread broke the other way: wild firing at the nearest visible enemy.
    Berserked { unit: UnitId },
    Kneeled { unit: UnitId, kneeling: bool },
    /// Non-lethal trauma from a binding rod.
    Stunned { unit: UnitId, stun: i32 },
    /// Stun overwhelmed the target: it drops, bound where it lies.
    Subdued { unit: UnitId },
    /// An unconscious unit came round.
    Awakened { unit: UnitId },
    /// A psi assault on the mind; morale_lost is 0 when resisted.
    Terrified { unit: UnitId, target: UnitId, morale_lost: i32 },
    /// A Taker's kill: the victim rises as a Husk on the demons' side.
    Taken { unit: UnitId },
    /// A destroyed Husk splits open: a fresh Taker is born.
    Hatched { unit: UnitId },
    /// A Behemoth shoulders straight through masonry.
    WallSmashed { at: IVec3, voxels: usize },
    /// The floor went out from under someone.
    Fell { unit: UnitId, to: IVec3 },
    /// Hauling a downed comrade.
    CarriedUp { unit: UnitId, carried: UnitId },
    SetDown { unit: UnitId, carried: UnitId },
    /// Recovered a weapon from the fallen.
    Scavenged { unit: UnitId },
    /// A fresh magazine goes home with a click heard too far.
    Reloaded { unit: UnitId },
    /// Weapon and sidearm trade places.
    Swapped { unit: UnitId },
    /// A weapon leaves someone's hands the hard way.
    WeaponDropped { unit: UnitId, at: IVec3 },
    /// A helpless enemy is put down where it lies.
    Executed { unit: UnitId, target: UnitId },
    /// A Husk falls, and someone the wall remembers is finally at rest.
    RestGranted { unit: UnitId },
    /// The last driver is dead: the whole pack feels the leash go slack.
    PackShaken,
    /// The pack breaks: everything that knows fear turns and runs.
    PackBroken,
    /// A routed demon reaches the way out and is gone — to tell of you.
    Escaped { unit: UnitId },
    /// A Prince's will falls across the runners: they turn back.
    Lashed { unit: UnitId },
    /// A Confessor's whisper knits a battered mind back together.
    Steadied { unit: UnitId, target: UnitId },
    /// Something moved out there, unseen. Rough bearing only.
    NoiseInDark { near: IVec3 },
    /// The rift obelisk is demolished; the incursion collapses.
    ObjectiveDestroyed,
    /// A body part is crippled by a heavy hit.
    PartCrippled { unit: UnitId, part: BodyPart },
    /// A crippled part, hit again, comes off entirely.
    PartSevered { unit: UnitId, part: BodyPart },
    /// Overkill: nothing recognizable remains.
    Gibbed { unit: UnitId },
    /// A demon feeds on the fallen to knit its own wounds.
    CorpseEaten { unit: UnitId, corpse: UnitId },
    /// A Taker raises a soldier's corpse as a Husk.
    Defiled { corpse: UnitId },
    /// Demonic rot takes hold in a crippled part.
    Infected { unit: UnitId, part: BodyPart },
    /// The saw beats the rot: the part is lost, the soldier is saved.
    Amputated { medic: UnitId, target: UnitId, part: BodyPart },
    /// The rot finished its work: the soldier rises on the other side.
    InfectionTurned { unit: UnitId },
    Turned { unit: UnitId, facing: IVec3 },
    /// A primed charge hits the ground, fuse hissing.
    ChargeDropped { at: IVec3, timer: u32 },
    SmokePopped { at: IVec3 },
    FireStarted { at: IVec3 },
    /// A witchfire flare lands and burns: light where there was none.
    FlareThrown { at: IVec3 },
    Burned { unit: UnitId, amount: i32 },
    DoorOpened { at: IVec3 },
    /// A glowing circle scribes itself onto the ground: something is coming.
    SummoningScribed { at: IVec3 },
    /// The circle delivers. A fresh demon stands where the light was.
    Summoned { unit: UnitId },
    /// The circle was fouled — a boot on the lines, or the ground destroyed.
    SummoningDisrupted { at: IVec3 },
    /// The Order chalks a burning ward onto the ground.
    WardInscribed { at: IVec3 },
    /// A demon crossed the ward, and the ward answered.
    WardBurned { unit: UnitId, at: IVec3 },
    /// The obelisk's glowing veins reach one tile further.
    CorruptionSpread { at: IVec3 },
    /// Standing on corrupted ground, a soldier hears the ground talk.
    Whispered { unit: UnitId },
    /// A soldier stumbles onto what the demons did here before you came.
    AtrocityFound { unit: UnitId, at: IVec3 },
    /// A counterstrike: the blade answers the claw.
    Riposte { unit: UnitId, target: UnitId, hit: bool },
    /// The warded circlet takes the psi blow and dies of it.
    CircletShattered { unit: UnitId },
    /// An officer steadies every heart in earshot.
    Rallied { by: UnitId },
    /// A civilian reaches the west edge and is away.
    Evacuated { unit: UnitId },
    /// The clock beat the squad: the field is lost to time.
    TimeExpired,
    /// An overloaded floor gives way.
    FloorCollapsed { at: IVec3 },
    /// A Prince seizes a mind outright.
    Possessed { unit: UnitId, by: UnitId },
    PossessionEnds { unit: UnitId },
    BattleOver { winner: Side },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Move { unit: UnitId, to: IVec3 },
    Fire { unit: UnitId, target: UnitId, mode: FireMode },
    /// Lob a hellfire charge at a tile. Arcs — no line of sight required.
    Throw { unit: UnitId, at: IVec3 },
    /// Field-dress a wounded ally (or yourself) on an adjacent tile.
    Heal { medic: UnitId, target: UnitId },
    /// Toggle kneeling (+15% accuracy; ends when the unit moves).
    Kneel { unit: UnitId },
    /// Bank TUs for a fire mode while moving; reactions answer with it.
    SetReserve { unit: UnitId, mode: Option<FireMode> },
    /// Put down an adjacent helpless enemy: certain, quick, no capture.
    Execute { unit: UnitId, target: UnitId },
    /// Strike an adjacent enemy with a binding rod: stun, not blood.
    Bind { unit: UnitId, target: UnitId },
    /// Psi assault (Overseers and worse): batters morale through walls.
    Terrify { unit: UnitId, target: UnitId },
    /// The Confessor's other hand: steady an ally's mind through walls.
    Steady { unit: UnitId, target: UnitId },
    /// Demons only: feed on an adjacent corpse to heal.
    Devour { unit: UnitId, corpse: UnitId },
    /// Takers only: raise an adjacent soldier corpse as a Husk.
    Defile { unit: UnitId, corpse: UnitId },
    /// Saw an infected part off an adjacent (or own) body before it turns.
    Amputate { medic: UnitId, target: UnitId },
    /// Chalk a burning ward on the unit's own tile (consumes a ward kit).
    InscribeWard { unit: UnitId },
    /// An officer's voice cuts the dread: morale restored in earshot.
    Rally { unit: UnitId },
    /// Face a direction (1 TU per 45°) — sets the reaction-fire arc.
    Turn { unit: UnitId, toward: IVec3 },
    /// Prime a charge and drop it at your feet; it detonates after `timer`
    /// half-turns. Then run.
    DropCharge { unit: UnitId, timer: u32 },
    /// Pop a smoke grenade at a tile: sight-blocking cover for a few turns.
    ThrowSmoke { unit: UnitId, at: IVec3 },
    /// Hurl a witchfire flare: a pool of light the dark can't argue with.
    ThrowFlare { unit: UnitId, at: IVec3 },
    /// Open an adjacent closed door (6 TU).
    OpenDoor { unit: UnitId, at: IVec3 },
    /// Seize an enemy mind outright (Princes): it acts for you next turn.
    Possess { unit: UnitId, target: UnitId },
    /// Haul an adjacent unconscious ally onto your shoulders.
    PickUp { unit: UnitId, target: UnitId },
    /// Set the carried body down on an adjacent open tile.
    PutDown { unit: UnitId, at: IVec3 },
    /// Take a weapon up off the ground, on or beside your tile (8 TU).
    Scavenge { unit: UnitId },
    /// Feed the weapon in hand a fresh magazine (12 TU).
    Reload { unit: UnitId },
    /// Trade the weapon in hand for the sidearm at the hip (6 TU).
    SwapWeapon { unit: UnitId },
    EndTurn,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionError {
    BattleOver,
    NotYourTurn,
    DeadUnit,
    NotEnoughTu,
    NoPath,
    NoLineOfSight,
    BadTarget,
    /// The weapon does not support the requested fire mode.
    UnsupportedMode,
    OutOfRange,
    /// No grenades / heal charges left.
    NoCharges,
    /// The chamber is empty: reload, swap, or run.
    NoAmmo,
    NotAdjacent,
    /// The unit has no psionic talent.
    NoPsi,
    /// No such door, or it's already open.
    NoDoor,
}

/// Is `tile` within the unit's forward 135° arc?
fn in_facing_arc(unit: &Unit, tile: IVec3) -> bool {
    let d = tile - unit.tile;
    if d.x == 0 && d.y == 0 {
        return true;
    }
    let dir = glam::Vec2::new(d.x as f32, d.y as f32).normalize_or_zero();
    let face = glam::Vec2::new(unit.facing.x as f32, unit.facing.y as f32).normalize_or_zero();
    dir.dot(face) >= 0.38
}

/// 45° steps between two facing octants (for turn costs).
fn octant_steps(from: IVec3, to: IVec3) -> i32 {
    let oct = |v: IVec3| -> i32 {
        match (v.x.signum(), v.y.signum()) {
            (1, 0) => 0,
            (1, 1) => 1,
            (0, 1) => 2,
            (-1, 1) => 3,
            (-1, 0) => 4,
            (-1, -1) => 5,
            (0, -1) => 6,
            (1, -1) => 7,
            _ => 0,
        }
    };
    let diff = (oct(from) - oct(to)).rem_euclid(8);
    diff.min(8 - diff)
}

fn cheb(a: IVec3, b: IVec3) -> i32 {
    let d = (b - a).abs();
    d.x.max(d.y).max(d.z)
}

/// What a unit did in this battle — the raw material of learn-by-doing
/// stat growth (applied by the campaign layer, not here).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Experience {
    pub shots_fired: u32,
    pub shots_hit: u32,
    pub reaction_shots: u32,
    pub kills: u32,
    /// Times the unit broke (panic or berserk) and lived through it.
    pub dread_survived: u32,
    /// Thrown charges that landed where they were aimed.
    pub throws_true: u32,
    /// Melee strikes and ripostes that connected.
    pub blade_hits: u32,
    /// Tiles walked this battle — the legs remember.
    pub tiles_moved: u32,
}

/// What this battle is FOR, beyond killing: some fields are won by the
/// clock, the saved, or the taken-alive.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissionRule {
    /// Kill or demolish: the classic field.
    Standard,
    /// Walk `needed` civilians off the west edge before `turns` runs out.
    Evacuate { needed: u32, turns: u32 },
    /// The ritual completes at `turns` if the obelisk still stands.
    Interrupt { turns: u32 },
    /// Bind this one ALIVE. Its death is the mission's death.
    Snatch { target: UnitId },
}

/// The sky the battle is fought under.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Weather {
    #[default]
    Clear,
    /// Vision and aim strangled by driven sand.
    Sandstorm,
    /// Every step costs more; every demon leaves tracks.
    Snowfall,
    /// Fire gutters; sound drowns.
    Rain,
}

/// A destructible mission objective: demolish enough of it and the
/// incursion collapses regardless of surviving defenders.
#[derive(Clone, Copy, Debug)]
pub struct Objective {
    pub min: IVec3,
    pub max: IVec3,
    initial_voxels: usize,
}

pub struct Battle {
    pub world: VoxelWorld,
    pub tiles: TileMap,
    pub units: Vec<Unit>,
    pub side_to_move: Side,
    pub turn: u32,
    pub winner: Option<Side>,
    /// Sight range in tiles; night assaults shrink it.
    pub vision_tiles: i32,
    pub objective: Option<Objective>,
    /// Primed charges on the ground: (tile, half-turns until detonation).
    pub charges: Vec<(IVec3, u32)>,
    /// Drifting smoke and burning ground: (tile, kind, half-turns left).
    pub clouds: Vec<(IVec3, CloudKind, u32)>,
    /// Door tiles: (tile, opened).
    pub doors: Vec<(IVec3, bool)>,
    /// Fuel casks: (tile, initial shell voxels, still live).
    pub casks: Vec<(IVec3, usize, bool)>,
    /// Brimstone pools: (tile, ignited).
    pub pools: Vec<(IVec3, bool)>,
    /// The loudest recent violence (demons investigate it).
    pub last_noise: Option<IVec3>,
    /// Summoning circles mid-scribe: (tile, demon-turns left, pack strength).
    pub summons: Vec<(IVec3, u32, u32)>,
    /// The Order's burning wards: demons crossing one pay for it.
    pub wards: Vec<IVec3>,
    /// Thrown witchfire flares: standing pools of light in the dark.
    pub flares: Vec<IVec3>,
    /// Ground the obelisk has veined with glowing corruption.
    pub corruption: Vec<IVec3>,
    /// Atrocity sites on terror maps: (tile, discovered).
    pub atrocities: Vec<(IVec3, bool)>,
    /// What wins this field (and what loses it).
    pub rule: MissionRule,
    /// The sky overhead.
    pub weather: Weather,
    /// Civilians walked off the west edge (Evacuate missions).
    pub evacuated: u32,
    /// Noise the squad made where no demon could see (they listen too).
    pub alarm: Vec<IVec3>,
    /// Weapons lying in the dirt: (tile, weapon, rounds still loaded).
    /// The fallen drop theirs; the living may take them up.
    pub ground: Vec<(IVec3, crate::units::Weapon, i32)>,
    /// Where the enemy came in — and where a routed pack runs back to.
    pub demon_exit: IVec3,
    /// Defenders hold: gunline demons bank overwatch instead of hunting
    /// (manors, nests — anywhere the demons own the ground).
    pub demons_hold: bool,
    /// The pack has broken once already; the shock does not repeat.
    pack_broken: bool,
    /// Helpless enemies put down where they lay (for the debrief ledger).
    pub executed: u32,
    /// The squad's memory: where each demon was LAST seen. Kept when sight
    /// is lost (the HUD draws a ghost there), cleared by the demon's death.
    pub last_known: HashMap<UnitId, IVec3>,
    /// Unseen demon movement this enemy turn (flushed as cues).
    heard: Vec<IVec3>,
    xp: Vec<Experience>,
    rng: SimRng,
}

/// What the HUD shows before the trigger is pulled: the true odds the
/// resolver will roll against, and what a hit is worth.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShotForecast {
    /// Per-round chance to hit, after the high-ground bonus.
    pub chance: i32,
    pub cost: i32,
    pub rounds: u32,
    /// Weapon power: damage rolls 0..=2x this per hit.
    pub power: i32,
    /// Salt-shot: the hit stuns instead of wounding.
    pub stun: bool,
    /// Whether the shooter can actually see the target right now.
    pub seen: bool,
}

impl Battle {
    pub fn new(
        world: VoxelWorld,
        tile_min: IVec3,
        tile_size: IVec3,
        units: Vec<Unit>,
        seed: u64,
    ) -> Self {
        let demon_exit = units
            .iter()
            .find(|u| u.side == Side::Demons)
            .map(|u| u.tile)
            .unwrap_or(IVec3::new(21, 11, 0));
        let tiles = TileMap::derive(&world, tile_min, tile_size);
        let xp = vec![Experience::default(); units.len()];
        Self {
            world,
            tiles,
            units,
            side_to_move: Side::Order,
            turn: 1,
            winner: None,
            vision_tiles: VISION_TILES,
            objective: None,
            charges: Vec::new(),
            clouds: Vec::new(),
            doors: Vec::new(),
            casks: Vec::new(),
            pools: Vec::new(),
            last_noise: None,
            ground: Vec::new(),
            demon_exit,
            demons_hold: false,
            pack_broken: false,
            executed: 0,
            summons: Vec::new(),
            wards: Vec::new(),
            flares: Vec::new(),
            corruption: Vec::new(),
            atrocities: Vec::new(),
            rule: MissionRule::Standard,
            weather: Weather::default(),
            evacuated: 0,
            alarm: Vec::new(),
            last_known: HashMap::new(),
            heard: Vec::new(),
            xp,
            rng: SimRng::from_seed(seed),
        }
    }

    /// Scribe a summoning circle: the pentagram burns into the ground now,
    /// and delivers after `delay` demon turns (unless fouled first).
    pub fn schedule_summon(&mut self, tile: IVec3, delay: u32, strength: u32) {
        self.paint_sigil(tile, crate::scenario::MAT_SIGIL);
        self.summons.push((tile, delay, strength));
    }

    /// Chalk a ward onto a tile (map setup / the InscribeWard action).
    pub fn place_ward(&mut self, tile: IVec3) {
        self.paint_sigil(tile, crate::scenario::MAT_WARD);
        self.wards.push(tile);
    }

    /// Burn an occult pattern into a tile's ground surface: a ring with a
    /// crossed center — pentagram enough at sixteen voxels a side.
    fn paint_sigil(&mut self, tile: IVec3, mat: ods_voxel::Voxel) {
        let o = tile * TILE_VOXELS;
        let z = crate::scenario::GROUND_TOP - 1;
        let c = TILE_VOXELS / 2;
        let r2 = 36 * VS * VS; // radius 6 (legacy voxels), scaled
        for y in 0..TILE_VOXELS {
            for x in 0..TILE_VOXELS {
                let (dx, dy) = (x - c, y - c);
                let d2 = dx * dx + dy * dy;
                let on_ring = (r2 - 10 * VS * VS..=r2 + 10 * VS * VS).contains(&d2);
                let on_cross = d2 < r2
                    && ((dx - dy).abs() < VS || (dx + dy).abs() < VS || dx.abs() < VS || dy.abs() < VS);
                if on_ring || on_cross {
                    let p = o + IVec3::new(x, y, z);
                    if self.world.voxel(p).is_solid() {
                        self.world.set_voxel(p, mat);
                    }
                }
            }
        }
    }

    /// Thread glowing veins across a corrupted tile's surface.
    fn paint_veins(&mut self, tile: IVec3) {
        let o = tile * TILE_VOXELS;
        let z = crate::scenario::GROUND_TOP - 1;
        for i in 0..TILE_VOXELS {
            // Two crossing wandering lines: enough to read as veins.
            for p in [
                o + IVec3::new(i, (i * 5 + 3) % TILE_VOXELS, z),
                o + IVec3::new((i * 7 + 5) % TILE_VOXELS, i, z),
            ] {
                if self.world.voxel(p).is_solid() {
                    self.world.set_voxel(p, crate::scenario::MAT_VEIN);
                }
            }
        }
    }

    /// Scorch a spent sigil to dead black.
    fn scorch_sigil(&mut self, tile: IVec3) {
        let o = tile * TILE_VOXELS;
        let z = crate::scenario::GROUND_TOP - 1;
        for y in 0..TILE_VOXELS {
            for x in 0..TILE_VOXELS {
                let p = o + IVec3::new(x, y, z);
                let v = self.world.voxel(p);
                if v == crate::scenario::MAT_SIGIL || v == crate::scenario::MAT_WARD {
                    self.world.set_voxel(p, crate::scenario::MAT_OBSIDIAN);
                }
            }
        }
    }

    pub fn experience(&self, id: UnitId) -> Experience {
        self.xp[id.0 as usize]
    }

    fn blocked_for(&self, id: UnitId) -> HashSet<IVec3> {
        self.units
            .iter()
            .filter(|u| u.is_active() && u.id != id)
            .map(|u| u.tile)
            .collect()
    }

    /// UI helper: the path a Move order would take, and its full TU cost.
    pub fn preview_path(&self, id: UnitId, to: IVec3) -> Option<(Vec<IVec3>, i32)> {
        let unit = self.unit(id);
        let path = self.tiles.path(unit.tile, to, &self.blocked_for(id))?;
        let mult = unit.move_cost_mult();
        let mut cost = 0;
        let mut here = unit.tile;
        for &next in &path {
            cost += crate::tiles::step_cost(here, next) * mult;
            here = next;
        }
        Some((path, cost))
    }

    /// UI helper: everywhere this unit could stop this turn (tile, cost).
    pub fn reachable(&self, id: UnitId) -> Vec<(IVec3, i32)> {
        let unit = self.unit(id);
        let budget = match unit.reserve {
            Some(mode) => unit.tu - unit.fire_cost(mode).unwrap_or(0),
            None => unit.tu,
        };
        if budget <= 0 {
            return Vec::new();
        }
        self.tiles
            .reachable(unit.tile, budget, unit.move_cost_mult(), &self.blocked_for(id))
    }

    /// Register the destructible objective (voxel-space AABB, `[min, max)`).
    pub fn set_objective(&mut self, min: IVec3, max: IVec3) {
        let initial_voxels = self.count_objective_voxels(min, max);
        self.objective = Some(Objective { min, max, initial_voxels });
    }

    fn count_objective_voxels(&self, min: IVec3, max: IVec3) -> usize {
        let mut count = 0;
        for z in min.z..max.z {
            for y in min.y..max.y {
                for x in min.x..max.x {
                    if self.world.voxel(IVec3::new(x, y, z)).is_solid() {
                        count += 1;
                    }
                }
            }
        }
        count
    }

    #[cfg(test)]
    pub fn settle_units_for_test(&mut self, events: &mut Vec<Event>) {
        self.settle_units(events);
    }

    #[cfg(test)]
    pub fn xp_push_for_test(&mut self) {
        while self.xp.len() < self.units.len() {
            self.xp.push(Experience::default());
        }
    }

    #[cfg(test)]
    pub fn check_objective_for_test(&mut self, events: &mut Vec<Event>) {
        self.check_objective(events);
    }

    #[cfg(test)]
    pub(crate) fn add_cloud_for_test(&mut self, at: IVec3, kind: CloudKind, ttl: u32) {
        self.add_cloud(at, kind, ttl);
    }

    /// After any destruction: has the obelisk fallen below a third?
    fn check_objective(&mut self, events: &mut Vec<Event>) {
        if self.winner.is_some() {
            return;
        }
        let Some(obj) = self.objective else { return };
        if obj.initial_voxels == 0 {
            return;
        }
        let left = self.count_objective_voxels(obj.min, obj.max);
        if left * 3 < obj.initial_voxels {
            events.push(Event::ObjectiveDestroyed);
            // The way is shut: every summoning circle still scribing dies.
            self.summons.clear();
            // The veins die with their source: scorch them to dead rock.
            let veins = std::mem::take(&mut self.corruption);
            for tile in veins {
                let o = tile * TILE_VOXELS;
                let z = crate::scenario::GROUND_TOP - 1;
                for y in 0..TILE_VOXELS {
                    for x in 0..TILE_VOXELS {
                        let p = o + IVec3::new(x, y, z);
                        if self.world.voxel(p) == crate::scenario::MAT_VEIN {
                            self.world.set_voxel(p, crate::scenario::MAT_OBSIDIAN);
                        }
                    }
                }
            }
            self.winner = Some(Side::Order);
            events.push(Event::BattleOver { winner: Side::Order });
        }
    }

    pub fn unit(&self, id: UnitId) -> &Unit {
        &self.units[id.0 as usize]
    }

    fn unit_mut(&mut self, id: UnitId) -> &mut Unit {
        &mut self.units[id.0 as usize]
    }

    /// Units still on their feet: alive AND conscious.
    pub fn living(&self, side: Side) -> impl Iterator<Item = &Unit> {
        self.units
            .iter()
            .filter(move |u| u.is_active() && u.side == side)
    }

    pub fn unit_at(&self, tile: IVec3) -> Option<UnitId> {
        self.units
            .iter()
            .find(|u| u.is_active() && u.tile == tile)
            .map(|u| u.id)
    }

    // ------------------------------------------------------------------
    // Sight

    fn eye(tile: IVec3) -> Vec3 {
        (tile * TILE_VOXELS).as_vec3() + Vec3::new(HALF_TILE, HALF_TILE, EYE_Z)
    }

    fn chest(tile: IVec3) -> Vec3 {
        (tile * TILE_VOXELS).as_vec3() + Vec3::new(HALF_TILE, HALF_TILE, CHEST_Z)
    }

    fn los_clear(&self, from: Vec3, to: Vec3) -> bool {
        let delta = to - from;
        let len = delta.length();
        if len < 1e-3 {
            return true;
        }
        // Stop the ray just short of the target point so the target's own
        // tile contents don't count as an obstruction.
        self.world.raycast(from, delta / len, len - 0.75).is_none()
    }

    /// The belt holds the first few consumables at hand; after that,
    /// everything is dug out of the pack. Returns the TU surcharge the
    /// caller must ADD to its action cost (check first, then settle).
    fn fetch_surcharge(&self, id: UnitId) -> i32 {
        if self.unit(id).belt > 0 { 0 } else { PACK_FETCH_TU }
    }

    /// Settle the belt after a paid fetch: one slot spent if any remain.
    fn spend_belt(&mut self, id: UnitId) {
        let u = self.unit_mut(id);
        u.belt = u.belt.saturating_sub(1);
    }

    /// Straight sight between two tiles (eye to chest), smoke included —
    /// the AI's "could I see, or be seen, from there" probe. Range is the
    /// caller's problem.
    pub fn sight_line(&self, from: IVec3, to: IVec3) -> bool {
        self.los_clear(Self::eye(from), Self::chest(to))
            && !self.smoke_blocks(Self::eye(from), Self::chest(to))
    }

    pub fn can_see(&self, a: UnitId, b: UnitId) -> bool {
        let (a, b) = (self.unit(a), self.unit(b));
        let d = (b.tile - a.tile).abs();
        let dist = d.x.max(d.y).max(d.z);
        // Lit ground — burning tiles, thrown flares — shows its occupants
        // far beyond the eye's dark limit.
        let in_range = dist <= self.vision_tiles || (dist <= 20 && self.lit(b.tile));
        in_range
            && self.los_clear(Self::eye(a.tile), Self::chest(b.tile))
            && !self.smoke_blocks(Self::eye(a.tile), Self::chest(b.tile))
    }

    /// Enemy units currently visible to `side` (drives fog + target picking).
    pub fn visible_enemies(&self, side: Side) -> Vec<UnitId> {
        let mut seen: Vec<UnitId> = self
            .living(side.enemy())
            .filter(|enemy| self.living(side).any(|u| self.can_see(u.id, enemy.id)))
            .map(|e| e.id)
            .collect();
        seen.sort_by_key(|id| id.0);
        seen
    }

    /// Refresh the squad's ghost intel: every demon in sight right now gets
    /// its tile stamped; the dead are forgotten. Runs after every action.
    fn note_sightings(&mut self) {
        for id in self.visible_enemies(Side::Order) {
            let tile = self.unit(id).tile;
            self.last_known.insert(id, tile);
        }
        let units = &self.units;
        self.last_known.retain(|id, _| units[id.0 as usize].is_active());
    }

    /// Tiles visible to `side`, for fog-of-war rendering.
    pub fn visible_tiles(&self, side: Side) -> HashSet<IVec3> {
        let viewers: Vec<IVec3> = self.living(side).map(|u| u.tile).collect();
        self.tiles_seen_from(&viewers)
    }

    /// Tiles any of the given watch posts can see — fog of war for a side,
    /// or the threat overlay's ground truth for known demons.
    pub fn tiles_seen_from(&self, viewers: &[IVec3]) -> HashSet<IVec3> {
        let (min, max) = self.tiles.bounds();
        let mut out = HashSet::new();
        for z in min.z..max.z {
            for y in min.y..max.y {
                for x in min.x..max.x {
                    let tile = IVec3::new(x, y, z);
                    let visible = viewers.iter().any(|&v| {
                        let d = (tile - v).abs();
                        d.x.max(d.y).max(d.z) <= self.vision_tiles
                            && self.los_clear(Self::eye(v), Self::chest(tile))
                    });
                    if visible {
                        out.insert(tile);
                    }
                }
            }
        }
        out
    }

    /// The exact odds `resolve_shot` would roll against, for the HUD's shot
    /// forecast. No dice are consumed; None means the weapon can't fire so.
    pub fn forecast_shot(
        &self,
        shooter: UnitId,
        target: UnitId,
        mode: FireMode,
    ) -> Option<ShotForecast> {
        let s = self.unit(shooter);
        let (cost, chance) = (s.fire_cost(mode)?, s.hit_chance(mode)?);
        let t = self.unit(target);
        // Mirror the resolver's high-ground bonus.
        let chance = if s.tile.z > t.tile.z { (chance + 10).min(95) } else { chance };
        Some(ShotForecast {
            chance,
            cost,
            rounds: s.rounds_per_action(mode),
            power: s.weapon.power,
            stun: s.weapon.stun_power > 0,
            seen: self.can_see(shooter, target),
        })
    }

    // ------------------------------------------------------------------
    // Actions

    pub fn perform(&mut self, action: Action) -> Result<Vec<Event>, ActionError> {
        if self.winner.is_some() {
            return Err(ActionError::BattleOver);
        }
        let result = self.dispatch(action);
        if result.is_ok() {
            self.note_sightings();
        }
        result
    }

    fn dispatch(&mut self, action: Action) -> Result<Vec<Event>, ActionError> {
        match action {
            Action::Move { unit, to } => self.do_move(unit, to),
            Action::Fire { unit, target, mode } => self.do_fire(unit, target, mode),
            Action::Throw { unit, at } => self.do_throw(unit, at),
            Action::Heal { medic, target } => self.do_heal(medic, target),
            Action::Devour { unit, corpse } => self.do_devour(unit, corpse),
            Action::Defile { unit, corpse } => self.do_defile(unit, corpse),
            Action::Amputate { medic, target } => self.do_amputate(medic, target),
            Action::InscribeWard { unit } => self.do_inscribe_ward(unit),
            Action::Rally { unit } => self.do_rally(unit),
            Action::Kneel { unit } => self.do_kneel(unit),
            Action::SetReserve { unit, mode } => self.do_set_reserve(unit, mode),
            Action::Execute { unit, target } => self.do_execute(unit, target),
            Action::Bind { unit, target } => self.do_bind(unit, target),
            Action::Terrify { unit, target } => self.do_terrify(unit, target),
            Action::Steady { unit, target } => self.do_steady(unit, target),
            Action::Turn { unit, toward } => self.do_turn(unit, toward),
            Action::DropCharge { unit, timer } => self.do_drop_charge(unit, timer),
            Action::ThrowSmoke { unit, at } => self.do_throw_smoke(unit, at),
            Action::ThrowFlare { unit, at } => self.do_throw_flare(unit, at),
            Action::OpenDoor { unit, at } => self.do_open_door(unit, at),
            Action::Possess { unit, target } => self.do_possess(unit, target),
            Action::PickUp { unit, target } => self.do_pick_up(unit, target),
            Action::PutDown { unit, at } => self.do_put_down(unit, at),
            Action::Scavenge { unit } => self.do_scavenge(unit),
            Action::Reload { unit } => self.do_reload(unit),
            Action::SwapWeapon { unit } => self.do_swap(unit),
            Action::EndTurn => Ok(self.end_turn()),
        }
    }

    fn check_actor(&self, id: UnitId) -> Result<(), ActionError> {
        let u = self.unit(id);
        if !u.is_active() {
            return Err(ActionError::DeadUnit);
        }
        // A possessed unit answers to the OTHER side; its own side has lost
        // it for the duration.
        let acting_for_enemy = u.possessed > 0 && u.side != self.side_to_move;
        let acting_normally = u.possessed == 0 && u.side == self.side_to_move;
        if !(acting_normally || acting_for_enemy) {
            return Err(ActionError::NotYourTurn);
        }
        Ok(())
    }

    fn do_possess(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if !self.unit(id).psi_master {
            return Err(ActionError::NoPsi);
        }
        let t = self.unit(target);
        if !t.is_active() || t.side == self.unit(id).side || t.possessed > 0 {
            return Err(ActionError::BadTarget);
        }
        if cheb(self.unit(id).tile, t.tile) > TERRIFY_RANGE_TILES {
            return Err(ActionError::OutOfRange);
        }
        let cost = self.unit(id).tu_max * POSSESS_COST_PCT / 100;
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= cost;

        // A warded circlet takes the seizure — once.
        if self.unit(target).circlet {
            self.unit_mut(target).circlet = false;
            return Ok(vec![Event::CircletShattered { unit: target }]);
        }

        let attack = 45 + self.rng.roll(56) as i32;
        let defense = {
            let t = self.unit(target);
            t.bravery + t.morale / 4 + self.rng.roll(31) as i32
        };
        if attack > defense {
            self.unit_mut(target).possessed = 1;
            Ok(vec![Event::Possessed { unit: target, by: id }])
        } else {
            Ok(vec![Event::Terrified { unit: id, target, morale_lost: 0 }])
        }
    }

    fn do_throw_smoke(&mut self, id: UnitId, at: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if self.unit(id).smoke_grenades == 0 {
            return Err(ActionError::NoCharges);
        }
        let cost = self.unit(id).tu_max * 20 / 100;
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        if cheb(self.unit(id).tile, at) > GRENADE_RANGE_TILES {
            return Err(ActionError::OutOfRange);
        }
        {
            let u = self.unit_mut(id);
            u.tu -= cost;
            u.smoke_grenades -= 1;
        }
        for dy in -1..=1 {
            for dx in -1..=1 {
                let tile = at + IVec3::new(dx, dy, 0);
                if self.tiles.is_walkable(tile) {
                    self.add_cloud(tile, CloudKind::Smoke, 5);
                }
            }
        }
        Ok(vec![Event::SmokePopped { at }])
    }

    /// Hurl a witchfire flare: a standing pool of light, three tiles wide,
    /// that the night cannot argue with. It burns until the field is done.
    fn do_throw_flare(&mut self, id: UnitId, at: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let u = self.unit(id);
        if u.flares == 0 {
            return Err(ActionError::NoCharges);
        }
        if cheb(u.tile, at) > 10 {
            return Err(ActionError::OutOfRange);
        }
        let cost = u.tu_max * 12 / 100 + self.fetch_surcharge(id);
        if u.tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        self.spend_belt(id);
        self.unit_mut(id).tu -= cost;
        self.unit_mut(id).flares -= 1;
        self.flares.push(at);
        Ok(vec![Event::FlareThrown { at }])
    }

    fn do_open_door(&mut self, id: UnitId, at: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if cheb(self.unit(id).tile, at) > 1 {
            return Err(ActionError::NotAdjacent);
        }
        if self.unit(id).tu < 6 {
            return Err(ActionError::NotEnoughTu);
        }
        let Some(door) = self.doors.iter_mut().find(|(tile, open)| *tile == at && !open) else {
            return Err(ActionError::NoDoor);
        };
        door.1 = true;
        self.unit_mut(id).tu -= 6;
        // Swing the leaf: clear the tile's blocking mass.
        let o = at * TILE_VOXELS;
        self.world.fill_box(
            o + IVec3::new(0, 0, crate::scenario::GROUND_TOP),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, TILE_VOXELS * 7 / 8),
            ods_voxel::Voxel::EMPTY,
        );
        self.tiles
            .rederive_region(&self.world, o, o + IVec3::splat(TILE_VOXELS));
        Ok(vec![Event::DoorOpened { at }])
    }

    fn do_pick_up(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if self.unit(id).carrying.is_some() {
            return Err(ActionError::BadTarget);
        }
        let t = self.unit(target);
        let unconscious = t.alive && !t.conscious;
        if (!unconscious && !t.is_corpse()) || t.side != self.unit(id).side {
            return Err(ActionError::BadTarget);
        }
        if cheb(self.unit(id).tile, t.tile) > 1 {
            return Err(ActionError::NotAdjacent);
        }
        if self.unit(id).tu < 8 {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= 8;
        self.unit_mut(id).carrying = Some(target);
        let tile = self.unit(id).tile;
        self.unit_mut(target).tile = tile;
        Ok(vec![Event::CarriedUp { unit: id, carried: target }])
    }

    fn do_put_down(&mut self, id: UnitId, at: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let Some(carried) = self.unit(id).carrying else {
            return Err(ActionError::BadTarget);
        };
        if cheb(self.unit(id).tile, at) > 1
            || !self.tiles.is_walkable(at)
            || self.unit_at(at).is_some()
        {
            return Err(ActionError::BadTarget);
        }
        if self.unit(id).tu < 4 {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= 4;
        self.unit_mut(id).carrying = None;
        self.unit_mut(carried).tile = at;
        Ok(vec![Event::SetDown { unit: id, carried }])
    }

    fn do_scavenge(&mut self, id: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if self.unit(id).tu < SCAVENGE_TU {
            return Err(ActionError::NotEnoughTu);
        }
        let me = self.unit(id).tile;
        // The best thing lying within reach: power first, loaded beats dry.
        let idx = self
            .ground
            .iter()
            .enumerate()
            .filter(|(_, (t, _, _))| cheb(*t, me) <= 1)
            .max_by_key(|(i, (_, w, a))| (w.power, *a, std::cmp::Reverse(*i)))
            .map(|(i, _)| i);
        let Some(idx) = idx else {
            return Err(ActionError::BadTarget);
        };
        let (_, weapon, ammo) = self.ground.remove(idx);
        let u = self.unit_mut(id);
        u.tu -= SCAVENGE_TU;
        let old = std::mem::replace(&mut u.weapon, weapon);
        let old_ammo = std::mem::replace(&mut u.ammo, ammo);
        // What was in hand goes down where we stand — nothing vanishes.
        if !old.natural {
            self.ground.push((me, old, old_ammo));
        }
        Ok(vec![Event::Scavenged { unit: id }])
    }

    /// A fresh magazine goes into whatever is in hand.
    fn do_reload(&mut self, id: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let u = self.unit(id);
        if u.weapon.clip == 0 || u.ammo >= u.weapon.clip as i32 {
            return Err(ActionError::BadTarget);
        }
        if u.mags == 0 {
            return Err(ActionError::NoCharges);
        }
        let fetch = self.fetch_surcharge(id);
        if u.tu < RELOAD_TU + fetch {
            return Err(ActionError::NotEnoughTu);
        }
        self.spend_belt(id);
        let u = self.unit_mut(id);
        u.tu -= RELOAD_TU + fetch;
        u.mags -= 1;
        u.ammo = u.weapon.clip as i32;
        // The click carries.
        self.last_noise = Some(self.unit(id).tile);
        Ok(vec![Event::Reloaded { unit: id }])
    }

    /// Weapon and sidearm trade places, each keeping its own loaded rounds.
    fn do_swap(&mut self, id: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if self.unit(id).sidearm.is_none() {
            return Err(ActionError::BadTarget);
        }
        if self.unit(id).tu < SWAP_TU {
            return Err(ActionError::NotEnoughTu);
        }
        let u = self.unit_mut(id);
        u.tu -= SWAP_TU;
        let drawn = u.sidearm.take().expect("checked above");
        u.sidearm = Some(std::mem::replace(&mut u.weapon, drawn));
        std::mem::swap(&mut u.ammo, &mut u.sidearm_ammo);
        Ok(vec![Event::Swapped { unit: id }])
    }

    /// A demon feeds on a corpse: flesh knits, the body is spent.
    fn do_devour(&mut self, id: UnitId, corpse: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if self.unit(id).side != Side::Demons || self.unit(id).civilian {
            return Err(ActionError::BadTarget);
        }
        if !self.unit(corpse).is_corpse() {
            return Err(ActionError::BadTarget);
        }
        if cheb(self.unit(id).tile, self.unit(corpse).tile) > 1 {
            return Err(ActionError::NotAdjacent);
        }
        if self.unit(id).tu < DEVOUR_TU {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= DEVOUR_TU;
        {
            let u = self.unit_mut(id);
            u.health = (u.health + 10).min(u.health_max);
        }
        self.unit_mut(corpse).consumed = true;
        let tile = self.unit(corpse).tile;
        self.spatter(tile, 4, MAT_GORE);
        // The dead being eaten is not something the living shrug off.
        let corpse_side = self.unit(corpse).side;
        for u in &mut self.units {
            if u.is_active() && u.side == corpse_side {
                u.morale = (u.morale - 8).max(0);
            }
        }
        Ok(vec![Event::CorpseEaten { unit: id, corpse }])
    }

    /// A Taker kneels over a dead soldier — and the dead soldier gets up.
    fn do_defile(&mut self, id: UnitId, corpse: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if self.unit(id).species != Species::Taker {
            return Err(ActionError::BadTarget);
        }
        let c = self.unit(corpse);
        // The claws take the dead — and the helpless. An unconscious
        // soldier can be Taken without ever waking, unless a comrade
        // stands over the body.
        let helpless = c.alive
            && !c.conscious
            && c.side == Side::Order
            && c.species == Species::Soldier
            && !c.civilian;
        if helpless {
            let body = c.tile;
            let guarded = self.units.iter().any(|g| {
                g.is_active()
                    && g.side == Side::Order
                    && !g.civilian
                    && g.tile != body
                    && cheb(g.tile, body) <= 1
            });
            if guarded {
                return Err(ActionError::BadTarget);
            }
        } else if !c.is_corpse() || c.species != Species::Soldier || c.civilian {
            return Err(ActionError::BadTarget);
        }
        // A true corpse needs its tile free for the Husk to stand on; a
        // still-breathing body already holds its own ground.
        let tile = c.tile;
        if !helpless && self.unit_at(tile).is_some() {
            return Err(ActionError::BadTarget);
        }
        if cheb(self.unit(id).tile, tile) > 1 {
            return Err(ActionError::NotAdjacent);
        }
        if self.unit(id).tu < DEFILE_TU {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= DEFILE_TU;
        self.convert_to_husk(corpse);
        let mut events = vec![Event::Defiled { corpse }];
        self.check_victory(&mut events);
        Ok(events)
    }

    /// The saw: lose the limb, keep the soldier. Painful, bloody, decisive.
    fn do_amputate(&mut self, medic: UnitId, target: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(medic)?;
        if medic != target && cheb(self.unit(medic).tile, self.unit(target).tile) > 1 {
            return Err(ActionError::NotAdjacent);
        }
        if !self.unit(target).alive {
            return Err(ActionError::BadTarget);
        }
        let Some((part, _)) = self.unit(target).infected else {
            return Err(ActionError::BadTarget);
        };
        if self.unit(medic).tu < AMPUTATE_TU {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(medic).tu -= AMPUTATE_TU;
        let mut events = vec![Event::Amputated { medic, target, part }];
        {
            let t = self.unit_mut(target);
            t.infected = None;
            t.health = (t.health - 4).max(1); // the saw is kinder than the rot
            t.wounds += 1;
            t.stun += 6;
        }
        self.sever_part(target, part, &mut events);
        // sever_part can't kill here (not the head — rot never takes heads),
        // but the shock can drop them.
        let t = self.unit_mut(target);
        if t.stun >= t.health && t.conscious {
            t.conscious = false;
            events.push(Event::Subdued { unit: target });
            self.check_victory(&mut events);
        }
        Ok(events)
    }

    /// The officer's voice: once a battle, every heart in earshot remembers
    /// what it came here to do.
    fn do_rally(&mut self, id: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let u = self.unit(id);
        if !u.can_rally || u.rally_spent || u.civilian {
            return Err(ActionError::BadTarget);
        }
        if u.tu < RALLY_TU {
            return Err(ActionError::NotEnoughTu);
        }
        let (side, from) = (u.side, u.tile);
        self.unit_mut(id).tu -= RALLY_TU;
        self.unit_mut(id).rally_spent = true;
        for u in &mut self.units {
            if u.is_active() && u.side == side && cheb(u.tile, from) <= RALLY_RANGE_TILES {
                u.morale = (u.morale + 30).min(100);
                u.suppression = 0;
            }
        }
        Ok(vec![Event::Rallied { by: id }])
    }

    /// Chalk and salt and a psalm: the ground itself takes a side.
    fn do_inscribe_ward(&mut self, id: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let u = self.unit(id);
        if u.side != Side::Order || u.civilian || u.ward_kits == 0 {
            return Err(ActionError::BadTarget);
        }
        let at = u.tile;
        if self.wards.contains(&at) {
            return Err(ActionError::BadTarget);
        }
        if u.tu < WARD_TU {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= WARD_TU;
        self.unit_mut(id).ward_kits -= 1;
        self.place_ward(at);
        Ok(vec![Event::WardInscribed { at }])
    }

    /// Register an atrocity site (terror maps): discovering it is a horror.
    pub fn register_atrocity(&mut self, tile: IVec3) {
        self.atrocities.push((tile, false));
    }

    /// Every active squadmate of `side` marks a horror on their record.
    fn witness_horror(&mut self, side: Side, except: UnitId) {
        for u in &mut self.units {
            if u.is_active() && u.side == side && u.id != except && !u.civilian {
                u.horror += 1;
            }
        }
    }

    /// Register a fuel cask hazard (counts its shell voxels).
    pub fn register_cask(&mut self, tile: IVec3) {
        let o = tile * TILE_VOXELS;
        let n = self.count_objective_voxels(o, o + IVec3::splat(TILE_VOXELS));
        self.casks.push((tile, n, true));
    }

    /// Register a brimstone pool hazard.
    pub fn register_pool(&mut self, tile: IVec3) {
        self.pools.push((tile, false));
    }

    /// Breached casks detonate; sparks near brimstone ignite it.
    fn check_hazards(&mut self, events: &mut Vec<Event>) {
        // Casks first (they may chain).
        for i in 0..self.casks.len() {
            let (tile, initial, live) = self.casks[i];
            if !live {
                continue;
            }
            let o = tile * TILE_VOXELS;
            let left = self.count_objective_voxels(o, o + IVec3::splat(TILE_VOXELS));
            if left < initial {
                self.casks[i].2 = false;
                self.explode(tile, 35, None, events);
                if self.winner.is_some() {
                    return;
                }
            }
        }
        // Pools ignite from adjacent fire.
        for i in 0..self.pools.len() {
            let (tile, lit) = self.pools[i];
            if lit {
                continue;
            }
            let sparked = self
                .clouds
                .iter()
                .any(|(t, k, _)| *k == CloudKind::Fire && cheb(*t, tile) <= 1);
            if sparked {
                self.pools[i].1 = true;
                self.add_cloud(tile, CloudKind::Fire, 6);
                events.push(Event::FireStarted { at: tile });
            }
        }
    }

    /// Gravity settles what destruction leaves unsupported.
    fn settle_units(&mut self, events: &mut Vec<Event>) {
        for i in 0..self.units.len() {
            let id = UnitId(i as u32);
            if !self.unit(id).alive || self.unit(id).flies {
                continue;
            }
            let mut fell = false;
            while self.unit(id).tile.z > 0 && !self.tiles.is_walkable(self.unit(id).tile) {
                let below = self.unit(id).tile - IVec3::new(0, 0, 1);
                if !self.tiles.in_bounds(below) {
                    break;
                }
                self.unit_mut(id).tile = below;
                fell = true;
            }
            if fell {
                let to = self.unit(id).tile;
                events.push(Event::Fell { unit: id, to });
                self.apply_damage(id, 8, None, events);
                if self.winner.is_some() {
                    return;
                }
            }
        }
    }

    fn add_cloud(&mut self, tile: IVec3, kind: CloudKind, ttl: u32) {
        if let Some(c) = self.clouds.iter_mut().find(|(t, k, _)| *t == tile && *k == kind) {
            c.2 = c.2.max(ttl);
        } else {
            self.clouds.push((tile, kind, ttl));
        }
    }

    fn smoke_blocks(&self, from: Vec3, to: Vec3) -> bool {
        // March tile centers along the sight line; any smoke between the
        // endpoints (exclusive) fogs the shot.
        let from_t = crate::voxel_to_tile(from.as_ivec3());
        let to_t = crate::voxel_to_tile(to.as_ivec3());
        let steps = cheb(from_t, to_t);
        if steps <= 1 {
            return false;
        }
        for i in 1..steps {
            let p = from + (to - from) * (i as f32 / steps as f32);
            let tile = crate::voxel_to_tile(p.as_ivec3());
            if tile != from_t
                && tile != to_t
                && self
                    .clouds
                    .iter()
                    .any(|(t, k, _)| *t == tile && *k == CloudKind::Smoke)
            {
                return true;
            }
        }
        false
    }

    fn near_fire(&self, tile: IVec3) -> bool {
        self.clouds
            .iter()
            .any(|(t, k, _)| *k == CloudKind::Fire && cheb(*t, tile) <= 1)
    }

    /// Is this ground lit — by burning tiles or a thrown flare?
    pub fn lit(&self, tile: IVec3) -> bool {
        self.near_fire(tile) || self.flares.iter().any(|f| cheb(*f, tile) <= 3)
    }

    /// Does the tile hold something that burns — timber, foliage, flesh?
    /// A sparse voxel probe, cheap enough to ask every fire, every round.
    fn tile_fuel(&self, tile: IVec3) -> bool {
        let o = tile * TILE_VOXELS;
        let flammable = [
            crate::scenario::MAT_TIMBER,
            crate::scenario::MAT_FOLIAGE,
            crate::scenario::MAT_FLESH,
        ];
        for px in [3 * VS, 8 * VS, 13 * VS] {
            for py in [3 * VS, 8 * VS, 13 * VS] {
                for pz in [
                    crate::scenario::GROUND_TOP,
                    crate::scenario::GROUND_TOP + 4 * VS,
                    crate::scenario::GROUND_TOP + 8 * VS,
                ] {
                    if flammable.contains(&self.world.voxel(o + IVec3::new(px, py, pz))) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Fire eats what feeds it: char a burning tile's flammable voxels to
    /// rubble. Cover burns away, one round at a time.
    fn char_tile(&mut self, tile: IVec3) {
        let o = tile * TILE_VOXELS;
        let flammable = [
            crate::scenario::MAT_TIMBER,
            crate::scenario::MAT_FOLIAGE,
            crate::scenario::MAT_FLESH,
        ];
        let mut burned = 0;
        'outer: for pz in [
            crate::scenario::GROUND_TOP,
            crate::scenario::GROUND_TOP + 4 * VS,
            crate::scenario::GROUND_TOP + 8 * VS,
            crate::scenario::GROUND_TOP + 12 * VS,
        ] {
            for px in 0..TILE_VOXELS / (2 * VS) {
                for py in 0..TILE_VOXELS / (2 * VS) {
                    let p = o + IVec3::new(px * 2 * VS, py * 2 * VS, pz);
                    if flammable.contains(&self.world.voxel(p)) {
                        self.world.set_voxel(p, crate::scenario::MAT_RUBBLE);
                        burned += 1;
                        if burned >= 24 {
                            break 'outer;
                        }
                    }
                }
            }
        }
        if burned > 0 {
            self.tiles
                .rederive_region(&self.world, o, o + IVec3::splat(TILE_VOXELS));
        }
    }

    fn do_turn(&mut self, id: UnitId, toward: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let toward = IVec3::new(toward.x.signum(), toward.y.signum(), 0);
        if toward.x == 0 && toward.y == 0 {
            return Err(ActionError::BadTarget);
        }
        let cost = octant_steps(self.unit(id).facing, toward);
        if cost == 0 {
            return Ok(Vec::new());
        }
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        let u = self.unit_mut(id);
        u.tu -= cost;
        u.facing = toward;
        Ok(vec![Event::Turned { unit: id, facing: toward }])
    }

    fn do_drop_charge(&mut self, id: UnitId, timer: u32) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if self.unit(id).grenades == 0 {
            return Err(ActionError::NoCharges);
        }
        let cost = self.unit(id).tu_max * 15 / 100;
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        let at = self.unit(id).tile;
        {
            let u = self.unit_mut(id);
            u.tu -= cost;
            u.grenades -= 1;
        }
        let timer = timer.clamp(1, 6);
        self.charges.push((at, timer));
        Ok(vec![Event::ChargeDropped { at, timer }])
    }

    fn do_kneel(&mut self, id: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if self.unit(id).tu < KNEEL_COST {
            return Err(ActionError::NotEnoughTu);
        }
        let u = self.unit_mut(id);
        u.tu -= KNEEL_COST;
        u.kneeling = !u.kneeling;
        let kneeling = u.kneeling;
        Ok(vec![Event::Kneeled { unit: id, kneeling }])
    }

    fn do_set_reserve(
        &mut self,
        id: UnitId,
        mode: Option<FireMode>,
    ) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if let Some(mode) = mode
            && self.unit(id).fire_cost(mode).is_none()
        {
            return Err(ActionError::UnsupportedMode);
        }
        self.unit_mut(id).reserve = mode;
        Ok(Vec::new())
    }

    /// The quick mercy of the field: a helpless enemy, put down for good.
    fn do_execute(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let t = self.unit(target);
        if !t.alive || t.conscious || t.side == self.unit(id).side {
            return Err(ActionError::BadTarget);
        }
        if cheb(self.unit(id).tile, t.tile) > 1 {
            return Err(ActionError::NotAdjacent);
        }
        if self.unit(id).tu < EXECUTE_TU {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= EXECUTE_TU;
        self.executed += 1;
        if (id.0 as usize) < self.xp.len() {
            self.xp[id.0 as usize].kills += 1;
        }
        let mut events = vec![Event::Executed { unit: id, target }];
        self.kill_unit(target, &mut events);
        Ok(events)
    }

    fn do_bind(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let t = self.unit(target);
        if !t.is_active() || t.side == self.unit(id).side {
            return Err(ActionError::BadTarget);
        }
        if cheb(self.unit(id).tile, t.tile) > 1 {
            return Err(ActionError::NotAdjacent);
        }
        let cost = self.unit(id).tu_max * BIND_COST_PCT / 100;
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= cost;

        let mut events = Vec::new();
        // A practiced strike: connects unless fate is cruel.
        if self.rng.roll(100) < 75 {
            let stun = BIND_STUN + self.rng.roll(16) as i32;
            let t = self.unit_mut(target);
            t.stun += stun;
            let total = t.stun;
            events.push(Event::Stunned { unit: target, stun: total });
            if t.stun >= t.health && t.conscious {
                t.conscious = false;
                events.push(Event::Subdued { unit: target });
                self.check_victory(&mut events);
            }
        } else {
            events.push(Event::Stunned { unit: target, stun: self.unit(target).stun });
        }
        Ok(events)
    }

    /// The Confessor's whisper runs the other way: a battered ally's mind
    /// is steadied through any wall. The channel burns the one who holds
    /// it open — a point of horror per working.
    fn do_steady(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if !self.unit(id).psi {
            return Err(ActionError::NoPsi);
        }
        let t = self.unit(target);
        if !t.is_active() || t.side != self.unit(id).side || t.id == id || t.civilian {
            return Err(ActionError::BadTarget);
        }
        if cheb(self.unit(id).tile, t.tile) > TERRIFY_RANGE_TILES {
            return Err(ActionError::OutOfRange);
        }
        let cost = self.unit(id).tu_max * TERRIFY_COST_PCT / 100;
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= cost;
        self.unit_mut(id).horror += 1;
        let t = self.unit_mut(target);
        t.morale = (t.morale + 30).min(100);
        t.suppression = 0;
        Ok(vec![Event::Steadied { unit: id, target }])
    }

    fn do_terrify(&mut self, id: UnitId, target: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        if !self.unit(id).psi {
            return Err(ActionError::NoPsi);
        }
        let t = self.unit(target);
        if !t.is_active() || t.side == self.unit(id).side {
            return Err(ActionError::BadTarget);
        }
        if cheb(self.unit(id).tile, t.tile) > TERRIFY_RANGE_TILES {
            return Err(ActionError::OutOfRange);
        }
        let cost = self.unit(id).tu_max * TERRIFY_COST_PCT / 100;
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).tu -= cost;
        // A mortal mind holding the channel open pays for it.
        if self.unit(id).side == Side::Order {
            self.unit_mut(id).horror += 1;
        }

        // A warded circlet takes the blow — once.
        if self.unit(target).circlet {
            self.unit_mut(target).circlet = false;
            return Ok(vec![Event::CircletShattered { unit: target }]);
        }

        // The whisper needs no line of sight. Will is the only wall.
        let attack = 40 + self.rng.roll(56) as i32;
        let defense = self.unit(target).bravery + self.rng.roll(31) as i32;
        let morale_lost = if attack > defense {
            let loss = 25 + self.rng.roll(21) as i32;
            let t = self.unit_mut(target);
            t.morale = (t.morale - loss).max(0);
            loss
        } else {
            0
        };
        Ok(vec![Event::Terrified { unit: id, target, morale_lost }])
    }

    fn do_move(&mut self, id: UnitId, to: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let from = self.unit(id).tile;
        let blocked = self.blocked_for(id);
        let mode = if self.unit(id).flies {
            PathMode::Fly
        } else if self.unit(id).smasher {
            PathMode::Smash
        } else {
            PathMode::Walk
        };
        let path = self
            .tiles
            .path_mode(from, to, &blocked, mode)
            .ok_or(ActionError::NoPath)?;

        // A reserving unit keeps its banked mode's worth of TUs untouched.
        let reserve = match self.unit(id).reserve {
            Some(mode) => self.unit(id).fire_cost(mode).unwrap_or(0),
            None => 0,
        };
        let budget = |u: &Unit| u.tu - reserve;

        if budget(self.unit(id)) < step_cost(from, path[0]) * self.unit(id).move_cost_mult() {
            return Err(ActionError::NotEnoughTu);
        }
        // Moving ends the kneel (you can't stay low and sprint) — but a move
        // BEGUN from a crouch is a crawl, and crawls are quiet.
        let crouched = self.unit(id).kneeling;
        self.unit_mut(id).kneeling = false;

        let mut events = Vec::new();
        let mut here = from;
        for next in path {
            let mut cost = step_cost(here, next) * self.unit(id).move_cost_mult();
            if self.unit(id).carrying.is_some() {
                cost += 2; // a body over the shoulder
            }
            if self.weather == Weather::Snowfall {
                cost += 1; // every step through the drifts
            }
            let smashing = mode == PathMode::Smash && !self.tiles.is_walkable(next);
            if smashing {
                cost += 8;
            }
            if budget(self.unit(id)) < cost {
                break;
            }
            if smashing {
                // Masonry gives way before the Behemoth.
                let o = next * TILE_VOXELS;
                let mut smashed = 0;
                for z in crate::scenario::GROUND_TOP..TILE_VOXELS {
                    for y in 0..TILE_VOXELS {
                        for x in 0..TILE_VOXELS {
                            let p = o + IVec3::new(x, y, z);
                            if self.world.voxel(p).is_solid() {
                                self.world.set_voxel(p, ods_voxel::Voxel::EMPTY);
                                smashed += 1;
                            }
                        }
                    }
                }
                self.tiles
                    .rederive_region(&self.world, o, o + IVec3::splat(TILE_VOXELS));
                events.push(Event::WallSmashed { at: next, voxels: smashed });
                self.check_collapse(&mut events);
                self.settle_units(&mut events);
                self.check_objective(&mut events);
                if self.winner.is_some() {
                    return Ok(events);
                }
            }
            {
                let climb = (next.z > here.z) as i32;
                let u = self.unit_mut(id);
                u.tu -= cost;
                // The body pays too: a point per step, two going up.
                if !u.flies {
                    u.stamina = (u.stamina - 1 - climb).max(0);
                }
                let step = next - here;
                if step.x != 0 || step.y != 0 {
                    u.facing = IVec3::new(step.x.signum(), step.y.signum(), 0);
                }
                u.tile = next;
            }
            self.xp[id.0 as usize].tiles_moved += 1;
            // The carried ride along.
            if let Some(carried) = self.unit(id).carrying {
                self.unit_mut(carried).tile = next;
            }
            // Unseen demon movement registers only as sound.
            if self.unit(id).side == Side::Demons {
                let seen = self
                    .units
                    .iter()
                    .filter(|u| u.is_active() && u.side == Side::Order && !u.civilian)
                    .any(|u| self.can_see(u.id, id));
                if !seen {
                    self.heard.push(next);
                }
            }
            events.push(Event::Moved {
                unit: id,
                from: here,
                to: next,
                tu_left: self.unit(id).tu,
            });
            here = next;

            // The Taker leaves bloody footprints between the screams.
            if self.unit(id).species == Species::Taker {
                self.spatter(next, 1, MAT_BLOOD);
            }
            // Snow holds every print: demon movement writes itself down.
            if self.weather == Weather::Snowfall && self.unit(id).side == Side::Demons {
                self.spatter(next, 1, crate::scenario::MAT_RUBBLE);
            }
            // Loud boots: an unseen, un-crouched soldier is still audible.
            if self.unit(id).side == Side::Order && !self.unit(id).civilian && !crouched {
                let seen = self
                    .units
                    .iter()
                    .filter(|u| u.is_active() && u.side == Side::Demons)
                    .any(|u| self.can_see(u.id, id));
                if !seen {
                    self.alarm.push(next);
                }
            }
            // A routed demon beside the way out slips through it mid-step.
            if self.unit(id).routed
                && self.unit(id).side == Side::Demons
                && cheb(next, self.demon_exit) <= 1
            {
                let u = self.unit_mut(id);
                u.alive = false;
                u.escaped = true;
                events.push(Event::Escaped { unit: id });
                self.check_victory(&mut events);
                return Ok(events);
            }
            // Evacuation: a civilian stepping onto the west edge is away.
            if let MissionRule::Evacuate { needed, .. } = self.rule
                && self.unit(id).civilian
                && self.unit(id).side == Side::Order
                && next.x <= 3
            {
                self.evacuated += 1;
                self.unit_mut(id).tile = IVec3::new(0, next.y, 0);
                self.unit_mut(id).tu = 0;
                events.push(Event::Evacuated { unit: id });
                if self.evacuated >= needed && self.winner.is_none() {
                    self.winner = Some(Side::Order);
                    events.push(Event::BattleOver { winner: Side::Order });
                }
                break;
            }

            // Soldiers stumbling onto atrocity sites see what was done here.
            if self.unit(id).side == Side::Order && !self.unit(id).civilian {
                for i in 0..self.atrocities.len() {
                    let (at, found) = self.atrocities[i];
                    if !found && cheb(next, at) <= 2 {
                        self.atrocities[i].1 = true;
                        events.push(Event::AtrocityFound { unit: id, at });
                        let u = self.unit_mut(id);
                        u.morale = (u.morale - 10).max(0);
                        u.horror += 1;
                    }
                }
            }

            // A demon crossing a ward line is answered by it.
            if self.unit(id).side == Side::Demons
                && let Some(pos) = self.wards.iter().position(|&w| w == next)
            {
                self.wards.remove(pos);
                self.scorch_sigil(next);
                events.push(Event::WardBurned { unit: id, at: next });
                self.unit_mut(id).morale = (self.unit(id).morale - 15).max(0);
                self.apply_damage(id, WARD_BURN, None, &mut events);
                if !self.unit(id).is_active() || self.winner.is_some() {
                    break;
                }
            }

            self.resolve_reactions(id, &mut events);
            if !self.unit(id).is_active() || self.winner.is_some() {
                break;
            }
        }
        Ok(events)
    }

    /// X-COM-style reaction fire: enemies with line of sight and banked TUs
    /// get snap shots if their initiative beats the mover's.
    fn resolve_reactions(&mut self, mover: UnitId, events: &mut Vec<Event>) {
        let m = self.unit(mover);
        let mover_initiative = m.reactions * m.tu / m.tu_max.max(1);
        let mover_side = m.side;

        let mover_tile = self.unit(mover).tile;
        let shooters: Vec<UnitId> = self
            .units
            .iter()
            .filter(|e| {
                e.is_active()
                    && e.side == mover_side.enemy()
                    && e.has_shot()
                    && e.fire_cost(FireMode::Snap).is_some_and(|c| e.tu >= c)
                    && e.reactions * e.tu / e.tu_max.max(1) > mover_initiative
                    // Melee reactions only trigger when you brush past claws.
                    && (!e.weapon.melee || cheb(e.tile, mover_tile) <= 1)
                    // And only into the watcher's forward arc.
                    && in_facing_arc(e, mover_tile)
            })
            .map(|e| e.id)
            .collect();

        for shooter in shooters {
            if !self.unit(mover).alive || self.winner.is_some() {
                return;
            }
            if self.can_see(shooter, mover) {
                // The watch answers with what it banked: a snap trip-wire,
                // one aimed killshot, or the whole storm of an auto burst.
                let s = self.unit(shooter);
                let mode = s
                    .reserve
                    .filter(|&m| s.fire_cost(m).is_some_and(|c| s.tu >= c))
                    .unwrap_or(FireMode::Snap);
                self.resolve_shot(shooter, mover, mode, true, events);
            }
        }
    }

    fn do_fire(
        &mut self,
        id: UnitId,
        target: UnitId,
        mode: FireMode,
    ) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let shooter_possessed = self.unit(id).possessed > 0;
        let t = self.unit(target);
        if !t.is_active() {
            return Err(ActionError::BadTarget);
        }
        // The possessed turn their guns on their own; the free choose enemies.
        let friendly = t.side == self.unit(id).side;
        if friendly != shooter_possessed || t.id == id {
            return Err(ActionError::BadTarget);
        }
        let cost = self
            .unit(id)
            .fire_cost(mode)
            .ok_or(ActionError::UnsupportedMode)?;
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        let shooter = self.unit(id);
        if shooter.weapon.clip > 0 && shooter.ammo <= 0 {
            return Err(ActionError::NoAmmo);
        }
        let dist = cheb(shooter.tile, self.unit(target).tile);
        if shooter.weapon.melee {
            if dist > 1 {
                return Err(ActionError::NotAdjacent);
            }
        } else if shooter.weapon.arcing {
            // Lobbed globs clear cover but only so far.
            if dist > crate::units::ARC_RANGE_TILES {
                return Err(ActionError::OutOfRange);
            }
        } else if !self.can_see(id, target) {
            return Err(ActionError::NoLineOfSight);
        }
        // The censer doesn't shoot: it hoses burning ground at the target.
        if self.unit(id).weapon.fire_cone {
            if dist > CENSER_RANGE_TILES {
                return Err(ActionError::OutOfRange);
            }
            let (from, to) = (self.unit(id).tile, self.unit(target).tile);
            {
                let u = self.unit_mut(id);
                u.tu -= cost;
                if u.weapon.clip > 0 {
                    u.ammo -= 1;
                }
                let d = to - from;
                if d.x != 0 || d.y != 0 {
                    u.facing = IVec3::new(d.x.signum(), d.y.signum(), 0);
                }
            }
            let mut events = vec![Event::Fired { unit: id, target, mode, reaction: false, hit: true }];
            self.last_noise = Some(from);
            // Fire walks the line to the target and spills one tile wide.
            let step = (to - from).signum();
            let mut at = from;
            for _ in 0..CENSER_RANGE_TILES {
                at += IVec3::new(step.x, step.y, 0);
                for spill in [at, at + IVec3::new(step.y, step.x, 0)] {
                    if self.tiles.is_walkable(spill)
                        && !self
                            .clouds
                            .iter()
                            .any(|(t, k, _)| *t == spill && *k == CloudKind::Fire)
                    {
                        self.add_cloud(spill, CloudKind::Fire, 3);
                        events.push(Event::FireStarted { at: spill });
                    }
                }
                if at == to {
                    break;
                }
            }
            self.check_hazards(&mut events);
            return Ok(events);
        }
        let mut events = Vec::new();
        self.resolve_shot(id, target, mode, false, &mut events);
        Ok(events)
    }

    /// Spend the TUs for one fire action and resolve its rounds (one for
    /// snap/aimed, a burst for auto). The mode must already be validated for
    /// player actions; internal callers only use Snap, which always exists.
    fn resolve_shot(
        &mut self,
        shooter: UnitId,
        target: UnitId,
        mode: FireMode,
        reaction: bool,
        events: &mut Vec<Event>,
    ) {
        let (cost, chance, rounds, power, breach, melee, silent, stun_power, mag) = {
            let s = self.unit(shooter);
            if !s.has_shot() {
                return; // the chamber is empty; nothing leaves the barrel
            }
            let (Some(cost), Some(chance)) = (s.fire_cost(mode), s.hit_chance(mode)) else {
                return;
            };
            let mag = if s.weapon.clip > 0 { s.mag_kind } else { crate::units::MagKind::Blessed };
            // Cold iron bites deeper; salt trades blood for trauma.
            let power = match mag {
                crate::units::MagKind::Blessed => s.weapon.power,
                crate::units::MagKind::ColdIron => s.weapon.power + 4,
                crate::units::MagKind::Salt => (s.weapon.power - 4).max(2),
            };
            (
                cost,
                chance,
                s.rounds_per_action(mode),
                power,
                s.weapon.breach_radius,
                s.weapon.melee,
                s.weapon.silent,
                s.weapon.stun_power,
                mag,
            )
        };
        {
            let target_tile = self.unit(target).tile;
            let s = self.unit_mut(shooter);
            s.tu -= cost;
            let d = target_tile - s.tile;
            if d.x != 0 || d.y != 0 {
                s.facing = IVec3::new(d.x.signum(), d.y.signum(), 0);
            }
        }

        // High ground steadies the shot.
        let chance = if self.unit(shooter).tile.z > self.unit(target).tile.z {
            (chance + 10).min(95)
        } else {
            chance
        };
        if !silent {
            self.last_noise = Some(self.unit(shooter).tile);
        }

        for _ in 0..rounds {
            if !self.unit(target).is_active() || self.winner.is_some() {
                break; // remaining rounds of the burst go wide, harmlessly
            }
            if self.unit(shooter).weapon.clip > 0 {
                if self.unit(shooter).ammo <= 0 {
                    break; // the burst clicks dry mid-squeeze
                }
                self.unit_mut(shooter).ammo -= 1;
            }
            self.xp[shooter.0 as usize].shots_fired += 1;
            let hit = (self.rng.roll(100) as i32) < chance;
            events.push(Event::Fired { unit: shooter, target, mode, reaction, hit });
            self.unit_mut(target).suppression += 1;
            if reaction {
                self.xp[shooter.0 as usize].reaction_shots += 1;
            }

            if hit {
                self.xp[shooter.0 as usize].shots_hit += 1;
                if melee {
                    self.xp[shooter.0 as usize].blade_hits += 1;
                }
                if stun_power > 0 {
                    // Salt-shot: trauma without blood, splashing the tile.
                    let center = self.unit(target).tile;
                    let hits: Vec<UnitId> = self
                        .units
                        .iter()
                        .filter(|u| u.is_active() && cheb(u.tile, center) <= 1)
                        .map(|u| u.id)
                        .collect();
                    for id in hits {
                        let full = id == target;
                        let amount =
                            stun_power * self.rng.roll(101) as i32 / (if full { 100 } else { 200 });
                        let t = self.unit_mut(id);
                        t.stun += amount;
                        let total = t.stun;
                        events.push(Event::Stunned { unit: id, stun: total });
                        if t.stun >= t.health && t.conscious {
                            t.conscious = false;
                            events.push(Event::Subdued { unit: id });
                        }
                    }
                    self.check_victory(events);
                } else {
                    // 0–200% of weapon power, the original's famous swingy roll.
                    let damage = power * self.rng.roll(201) as i32 / 100;
                    self.apply_damage(target, damage, Some(shooter), events);
                    // Salt rounds leave them ringing as well as bleeding.
                    if mag == crate::units::MagKind::Salt && self.unit(target).alive {
                        let t = self.unit_mut(target);
                        t.stun += 6;
                        let total = t.stun;
                        events.push(Event::Stunned { unit: target, stun: total });
                        if t.stun >= t.health && t.conscious {
                            t.conscious = false;
                            events.push(Event::Subdued { unit: target });
                        }
                    }
                    // The ram hammer cracks scenery through its target.
                    if melee && breach > 0.0 {
                        let c = (self.unit(target).tile * TILE_VOXELS).as_vec3()
                            + Vec3::splat(HALF_TILE);
                        let destroyed =
                            self.world.carve_sphere(c + Vec3::new(0.0, 0.0, 2.0 * VS as f32), breach * VS as f32);
                        if destroyed > 0 {
                            let ci = c.as_ivec3();
                            let r = breach.ceil() as i32 + 1;
                            self.tiles.rederive_region(
                                &self.world,
                                ci - IVec3::splat(r),
                                ci + IVec3::splat(r),
                            );
                        }
                    }
                }
            } else if !melee {
                self.stray_shot(shooter, target, breach, events);
            }
        }

        // The blade answers: a melee attacker leaves an opening, and a
        // defender with a consecrated blade takes it — once, for free.
        if melee
            && !reaction
            && self.unit(target).blade
            && self.unit(target).is_active()
            && self.unit(shooter).is_active()
            && cheb(self.unit(shooter).tile, self.unit(target).tile) <= 1
        {
            let chance = (self.unit(target).melee * 85 / 100).clamp(5, 95);
            let hit = (self.rng.roll(100) as i32) < chance;
            events.push(Event::Riposte { unit: target, target: shooter, hit });
            if hit {
                self.xp[target.0 as usize].shots_hit += 1;
                self.xp[target.0 as usize].blade_hits += 1;
                let damage = BLADE_POWER * self.rng.roll(201) as i32 / 100;
                self.apply_damage(shooter, damage, Some(target), events);
            }
        }
    }

    fn do_throw(&mut self, id: UnitId, at: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let u = self.unit(id);
        if u.grenades == 0 {
            return Err(ActionError::NoCharges);
        }
        let cost = u.tu_max * GRENADE_COST_PCT / 100 + self.fetch_surcharge(id);
        if u.tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        // The arm sets the range: strength carries the charge.
        if cheb(u.tile, at) > 6 + u.strength / 6 {
            return Err(ActionError::OutOfRange);
        }

        self.spend_belt(id);
        {
            let u = self.unit_mut(id);
            u.tu -= cost;
            u.grenades -= 1;
        }
        // A bad throw scatters: the throwing arm decides, not the eye.
        let throw_acc = (50 + self.unit(id).throwing / 2).clamp(30, 90);
        let at = if (self.rng.roll(100) as i32) < throw_acc {
            self.xp[id.0 as usize].throws_true += 1;
            at
        } else {
            const RING: [(i32, i32); 8] =
                [(1, 0), (1, 1), (0, 1), (-1, 1), (-1, 0), (-1, -1), (0, -1), (1, -1)];
            let (dx, dy) = RING[self.rng.roll(8) as usize];
            let dist = 1 + self.rng.roll(2) as i32;
            at + IVec3::new(dx * dist, dy * dist, 0)
        };
        let mut events = vec![Event::Threw { unit: id, at }];
        self.explode(at, GRENADE_POWER, Some(id), &mut events);
        Ok(events)
    }

    /// Detonate at a tile: carve the terrain, then damage every unit in the
    /// blast. Units behind cover (no line from the blast center) take half.
    fn explode(&mut self, at: IVec3, power: i32, source: Option<UnitId>, events: &mut Vec<Event>) {
        let center = (at * TILE_VOXELS).as_vec3() + Vec3::splat(HALF_TILE);
        let destroyed = self.world.carve_sphere(center, GRENADE_CARVE_RADIUS);
        let r = GRENADE_CARVE_RADIUS.ceil() as i32 + 1;
        let c = center.as_ivec3();
        self.tiles
            .rederive_region(&self.world, c - IVec3::splat(r), c + IVec3::splat(r));
        events.push(Event::Exploded { at, voxels: destroyed });
        self.last_noise = Some(at);
        self.check_collapse(events);
        self.settle_units(events);
        self.check_hazards(events);
        // Hellfire lingers: the blast site burns.
        if self.tiles.is_walkable(at) {
            self.add_cloud(at, CloudKind::Fire, 4);
            events.push(Event::FireStarted { at });
        }
        self.check_objective(events);

        let victims: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.alive && cheb(u.tile, at) <= BLAST_TILES)
            .map(|u| u.id)
            .collect();
        for victim in victims {
            let dist = cheb(self.unit(victim).tile, at);
            // Explosives are less swingy than bullets: 50–150% of power,
            // attenuated by distance.
            let rolled = power * (50 + self.rng.roll(101) as i32) / 100;
            let mut damage = rolled / (1 + dist);
            // The carve happens first, so a unit sheltered by a surviving
            // wall is genuinely in cover.
            if !self.los_clear(center, Self::chest(self.unit(victim).tile)) {
                damage /= 2;
            }
            self.apply_damage(victim, damage, source, events);
        }
    }

    fn do_heal(&mut self, medic: UnitId, target: UnitId) -> Result<Vec<Event>, ActionError> {
        self.check_actor(medic)?;
        let (m, t) = (self.unit(medic), self.unit(target));
        if !t.alive || t.side != m.side {
            return Err(ActionError::BadTarget);
        }
        if cheb(m.tile, t.tile) > 1 {
            return Err(ActionError::NotAdjacent);
        }
        if m.heal_charges == 0 {
            return Err(ActionError::NoCharges);
        }
        let fetch = self.fetch_surcharge(medic);
        if m.tu < HEAL_COST_TU + fetch {
            return Err(ActionError::NotEnoughTu);
        }

        self.spend_belt(medic);
        {
            let m = self.unit_mut(medic);
            m.tu -= HEAL_COST_TU + fetch;
            m.heal_charges -= 1;
        }
        {
            let t = self.unit_mut(target);
            t.wounds = (t.wounds - 1).max(0);
            t.health = (t.health + HEAL_AMOUNT).min(t.health_max);
        }
        Ok(vec![Event::Healed {
            medic,
            target,
            health_left: self.unit(target).health,
        }])
    }

    /// A miss travels on: deviate the aim line and chip whatever terrain it
    /// lands on. Destruction is never abstracted away.
    fn stray_shot(
        &mut self,
        shooter: UnitId,
        target: UnitId,
        breach_radius: f32,
        events: &mut Vec<Event>,
    ) {
        let from = Self::eye(self.unit(shooter).tile);
        let to = Self::chest(self.unit(target).tile);
        let dir = (to - from).normalize_or(Vec3::X);

        let mut jitter = |spread: f32| (self.rng.roll(2001) as f32 - 1000.0) / 1000.0 * spread;
        let side = dir.cross(Vec3::Z).normalize_or(Vec3::Y);
        let up = side.cross(dir);
        let deviated = (dir + side * jitter(0.12) + up * jitter(0.08)).normalize();

        if let Some(impact) = self.world.raycast(from, deviated, 640.0 * VS as f32) {
            let destroyed = self.world.carve_sphere(impact.position, breach_radius * VS as f32);
            if destroyed > 0 {
                let r = breach_radius.ceil() as i32 + 1;
                let c = impact.position.as_ivec3();
                self.tiles
                    .rederive_region(&self.world, c - IVec3::splat(r), c + IVec3::splat(r));
                events.push(Event::TerrainDestroyed {
                    center: impact.position,
                    voxels: destroyed,
                });
                self.check_objective(events);
                self.check_hazards(events);
            }
        }
    }

    fn apply_damage(
        &mut self,
        target: UnitId,
        damage: i32,
        source: Option<UnitId>,
        events: &mut Vec<Event>,
    ) {
        // Directional armor soaks its share first.
        let damage = if let Some(src) = source {
            let origin = self.unit(src).tile;
            (damage - self.unit(target).armor_against(origin)).max(0)
        } else {
            (damage - self.unit(target).armor_side).max(0)
        };
        {
            let t = self.unit_mut(target);
            t.health -= damage;
            t.morale = (t.morale - damage / 2).max(0);
        }
        events.push(Event::Damaged {
            unit: target,
            amount: damage,
            health_left: self.unit(target).health.max(0),
        });

        // Blood answers every serious wound.
        if damage >= 5 {
            let tile = self.unit(target).tile;
            let drops = 2 + self.rng.roll(3);
            self.spatter(tile, drops, MAT_BLOOD);
        }

        if self.unit(target).health <= 0 {
            if let Some(killer) = source {
                self.xp[killer.0 as usize].kills += 1;
                // A Taker's melee kill doesn't leave a corpse. It leaves a Husk.
                if self.unit(killer).species == Species::Taker
                    && self.unit(killer).weapon.melee
                    && self.unit(target).species == Species::Soldier
                {
                    self.take_unit(target, events);
                    return;
                }
            }
            // Overkill leaves nothing whole: the body comes apart.
            if -self.unit(target).health >= GIB_OVERKILL {
                self.gib_unit(target, events);
            } else {
                self.kill_unit(target, events);
            }
        } else if damage >= 5 {
            // Serious hits can open fatal wounds that bleed each turn.
            let new_wounds = self.rng.roll(3) as i32;
            if new_wounds > 0 {
                let t = self.unit_mut(target);
                t.wounds += new_wounds;
                let total = t.wounds;
                events.push(Event::Wounded { unit: target, total });
            }
            // And heavy blows land SOMEWHERE: roll the hit location.
            if damage >= 8 && self.rng.roll(100) < 35 {
                let parts = self.unit(target).species.body_parts();
                let part = parts[self.rng.roll(parts.len() as u32) as usize];
                let already_crippled = self.unit(target).injuries.contains(&part);
                let already_severed = self.unit(target).severed.contains(&part);
                if already_crippled && !already_severed {
                    // A mangled part, struck again, comes off.
                    self.sever_part(target, part, events);
                } else if !already_crippled {
                    self.unit_mut(target).injuries.push(part);
                    events.push(Event::PartCrippled { unit: target, part });
                    // Demon claws seed rot in the wounds they leave.
                    if part != BodyPart::Weapon
                        && self.unit(target).species == Species::Soldier
                        && !self.unit(target).civilian
                        && self.unit(target).infected.is_none()
                        && source.is_some_and(|src| {
                            self.unit(src).side == Side::Demons && self.unit(src).weapon.melee
                        })
                        && self.rng.roll(100) < 35
                    {
                        self.unit_mut(target).infected = Some((part, 0));
                        events.push(Event::Infected { unit: target, part });
                    }
                    if part == BodyPart::Head {
                        // Concussed: stun trauma on top of the wound.
                        let t = self.unit_mut(target);
                        t.stun += 8;
                        if t.stun >= t.health && t.conscious {
                            t.conscious = false;
                            events.push(Event::Subdued { unit: target });
                            self.check_victory(events);
                        }
                    }
                }
            }
        }
    }

    /// Paint gore onto the ground surface of a tile: the top ground voxel
    /// under scattered spots becomes blood or viscera. Stains persist for
    /// the whole battle and remesh with the terrain.
    fn spatter(&mut self, tile: IVec3, count: u32, mat: ods_voxel::Voxel) {
        let o = tile * TILE_VOXELS;
        for _ in 0..count {
            let p = o + IVec3::new(
                self.rng.roll(TILE_VOXELS as u32) as i32,
                self.rng.roll(TILE_VOXELS as u32) as i32,
                crate::scenario::GROUND_TOP - 1,
            );
            if self.world.voxel(p).is_solid() {
                self.world.set_voxel(p, mat);
            }
        }
    }

    /// Take a part clean off: permanent, bloody, and sometimes fatal.
    fn sever_part(&mut self, target: UnitId, part: BodyPart, events: &mut Vec<Event>) {
        let tile = self.unit(target).tile;
        {
            let t = self.unit_mut(target);
            if !t.injuries.contains(&part) {
                t.injuries.push(part);
            }
            t.severed.push(part);
            t.morale = (t.morale - 20).max(0);
            // Rot goes with the limb that carried it.
            if t.infected.map(|(p, _)| p) == Some(part) {
                t.infected = None;
            }
        }
        events.push(Event::PartSevered { unit: target, part });
        self.spatter(tile, 5, MAT_GORE);
        // Losing the weapon arm's grip — or the weapon part itself — means
        // fighting with what's left. The piece lands in the dirt.
        if part == BodyPart::Weapon {
            let u = self.unit_mut(target);
            let dropped = std::mem::replace(
                &mut u.weapon,
                crate::units::Weapon::from_data("bare hands", "bare_hands"),
            );
            let rounds = std::mem::replace(&mut u.ammo, 0);
            if !dropped.natural {
                self.ground.push((tile, dropped, rounds));
                events.push(Event::WeaponDropped { unit: target, at: tile });
            }
        }
        // Squadmates watch it happen.
        let side = self.unit(target).side;
        for u in &mut self.units {
            if u.is_active() && u.side == side && u.id != target {
                u.morale = (u.morale - 10).max(0);
            }
        }
        self.unit_mut(target).horror += 2;
        self.witness_horror(side, target);
        // Heads don't grow back.
        if part == BodyPart::Head {
            self.kill_unit(target, events);
        }
    }

    /// Overkill death: the unit bursts. No corpse to eat, carry, or hatch.
    fn gib_unit(&mut self, target: UnitId, events: &mut Vec<Event>) {
        let tile = self.unit(target).tile;
        self.unit_mut(target).gibbed = true;
        events.push(Event::Gibbed { unit: target });
        self.kill_unit(target, events);
        // Viscera across the tile and its neighbors.
        self.spatter(tile, 8, MAT_GORE);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let n = tile + IVec3::new(dx, dy, 0);
            self.spatter(n, 2, MAT_BLOOD);
        }
        // Watching a comrade come apart is worse than watching one fall.
        let side = self.unit(target).side;
        for u in &mut self.units {
            if u.is_active() && u.side == side && u.id != target {
                u.morale = (u.morale - (12 - u.bravery / 10).max(4)).max(0);
            }
        }
        self.witness_horror(side, target);
    }

    /// The Taking: the victim's body stands back up on the other side.
    fn take_unit(&mut self, victim: UnitId, events: &mut Vec<Event>) {
        self.convert_to_husk(victim);
        events.push(Event::Taken { unit: victim });
        self.check_victory(events);
    }

    /// The shared horror: a body (dead or living) becomes a Husk on the
    /// demons' side, and everyone who knew them watches it stand up.
    /// Someone still cracks the whip: an Overseer or Prince stands.
    fn demon_leader_alive(&self) -> bool {
        self.units.iter().any(|u| {
            u.is_active()
                && u.side == Side::Demons
                && matches!(u.species, Species::Overseer | Species::Prince)
        })
    }

    /// When most of the pack is broken, the whole pack breaks: everything
    /// that knows fear turns and runs for where it came in.
    fn check_rout(&mut self, events: &mut Vec<Event>) {
        if self.pack_broken || self.winner.is_some() {
            return;
        }
        let living: Vec<&Unit> = self
            .units
            .iter()
            .filter(|u| u.is_active() && u.side == Side::Demons && !u.civilian)
            .collect();
        if living.is_empty() {
            return;
        }
        let broken = living.iter().filter(|u| u.morale < 35 && u.bravery < 80).count();
        if broken * 2 <= living.len() {
            return;
        }
        self.pack_broken = true;
        events.push(Event::PackBroken);
        for u in &mut self.units {
            if u.alive && u.side == Side::Demons && !u.civilian && u.bravery < 80 {
                u.routed = true;
            }
        }
    }

    /// A routed demon beside the way out slips through it and is gone.
    fn check_escapes(&mut self, events: &mut Vec<Event>) {
        let exit = self.demon_exit;
        let due: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.is_active() && u.routed && u.side == Side::Demons && cheb(u.tile, exit) <= 1)
            .map(|u| u.id)
            .collect();
        for id in due {
            let u = self.unit_mut(id);
            u.alive = false;
            u.escaped = true;
            events.push(Event::Escaped { unit: id });
        }
        if !self.units.is_empty() {
            self.check_victory(events);
        }
    }

    fn convert_to_husk(&mut self, victim: UnitId) {
        let (name, tile, side) = {
            let v = self.unit(victim);
            (v.name.clone(), v.tile, v.side)
        };
        for u in &mut self.units {
            if u.alive && u.side == side && u.id != victim {
                u.morale = (u.morale - 20).max(0);
                if u.is_active() && !u.civilian {
                    u.horror += 2;
                }
            }
        }
        let husk = Unit::husk(victim.0, &format!("{name} (Taken)"), tile);
        *self.unit_mut(victim) = husk;
    }

    fn kill_unit(&mut self, target: UnitId, events: &mut Vec<Event>) {
        // A carried body tumbles free; a carrier's burden is dropped.
        if let Some(carried) = self.unit(target).carrying {
            let tile = self.unit(target).tile;
            self.unit_mut(carried).tile = tile;
        }
        for u in &mut self.units {
            if u.carrying == Some(target) {
                u.carrying = None;
            }
        }
        let (dead_side, dead_species, tile) = {
            let t = self.unit(target);
            (t.side, t.species, t.tile)
        };
        // Forged weapons tumble from dead hands; claws go wherever their
        // owners go. The corpse keeps only its bare hands.
        if !self.unit(target).weapon.natural {
            let u = self.unit_mut(target);
            let dropped = std::mem::replace(
                &mut u.weapon,
                crate::units::Weapon::from_data("bare hands", "bare_hands"),
            );
            let rounds = std::mem::replace(&mut u.ammo, 0);
            self.ground.push((tile, dropped, rounds));
            events.push(Event::WeaponDropped { unit: target, at: tile });
        }
        self.unit_mut(target).alive = false;
        events.push(Event::Died { unit: target });

        // A Snatch target dead is the mission dead.
        if let MissionRule::Snatch { target: wanted } = self.rule
            && wanted == target
            && self.winner.is_none()
        {
            self.winner = Some(Side::Demons);
            events.push(Event::BattleOver { winner: Side::Demons });
            return;
        }

        // Seeing a comrade die is the great morale killer.
        for u in &mut self.units {
            if u.alive && u.side == dead_side {
                u.morale = (u.morale - (15 - u.bravery / 10)).max(0);
            }
        }

        // A Husk destroyed is someone the wall remembers, finally at rest:
        // the squad breathes out.
        if dead_species == Species::Husk && dead_side == Side::Demons {
            events.push(Event::RestGranted { unit: target });
            for u in &mut self.units {
                if u.is_active() && u.side == Side::Order && !u.civilian {
                    u.morale = (u.morale + 6).min(100);
                    u.horror = u.horror.saturating_sub(1);
                }
            }
        }

        // Kill the driver and the rabble it drives loses its nerve: when
        // the LAST Overseer or Prince falls, the leash goes slack at once.
        let was_leader = matches!(dead_species, Species::Overseer | Species::Prince);
        if was_leader && dead_side == Side::Demons && !self.demon_leader_alive() {
            events.push(Event::PackShaken);
            for u in &mut self.units {
                if u.alive && u.side == Side::Demons && !u.civilian {
                    u.morale = (u.morale - if u.bravery >= 80 { 10 } else { 30 }).max(0);
                }
            }
        }
        self.check_rout(events);

        // A destroyed Husk splits open and something new crawls out —
        // unless overkill left nothing intact enough to hatch from.
        if dead_species == Species::Husk && !self.unit(target).gibbed && self.winner.is_none() {
            let id = self.units.len() as u32;
            self.units.push(Unit::taker(id, "Hatched Taker", tile));
            self.xp.push(Experience::default());
            events.push(Event::Hatched { unit: UnitId(id) });
        }
        self.check_victory(events);
    }

    /// Upper floors need what holds them up. A slab shot down to scraps
    /// gives way: whoever stands on it falls, whoever stands under it is
    /// buried in the coming-down.
    fn check_collapse(&mut self, events: &mut Vec<Event>) {
        let (min, max) = self.tiles.bounds();
        if max.z < 2 {
            return; // single-story map
        }
        let mut fell = Vec::new();
        for y in min.y..max.y {
            for x in min.x..max.x {
                // Count what's left of the slab (the upper tile's foot band):
                // a full slab is sound and empty air is nothing to fall —
                // only the shot-through in-between comes down.
                let o = IVec3::new(x * TILE_VOXELS, y * TILE_VOXELS, TILE_VOXELS);
                let mut left = 0;
                for dz in 0..crate::scenario::GROUND_TOP {
                    for dy in 0..TILE_VOXELS {
                        for dx in 0..TILE_VOXELS {
                            if self.world.voxel(o + IVec3::new(dx, dy, dz)).is_solid() {
                                left += 1;
                            }
                        }
                    }
                }
                let full = TILE_VOXELS * TILE_VOXELS * crate::scenario::GROUND_TOP;
                if left > 0 && left < full * 3 / 10 {
                    fell.push(IVec3::new(x, y, 0));
                    // The scraps come down.
                    self.world.fill_box(
                        o,
                        o + IVec3::new(TILE_VOXELS, TILE_VOXELS, crate::scenario::GROUND_TOP),
                        ods_voxel::Voxel::EMPTY,
                    );
                    self.tiles.rederive_region(
                        &self.world,
                        o - IVec3::new(0, 0, TILE_VOXELS),
                        o + IVec3::splat(TILE_VOXELS),
                    );
                }
            }
        }
        for at in fell {
            events.push(Event::FloorCollapsed { at: at + IVec3::new(0, 0, 1) });
            self.spatter(at, 3, MAT_GORE);
            // Buried: whoever stood underneath takes the slab.
            let below: Vec<UnitId> = self
                .units
                .iter()
                .filter(|u| u.is_active() && u.tile == at)
                .map(|u| u.id)
                .collect();
            for id in below {
                self.unit_mut(id).stun += 6;
                self.apply_damage(id, 8, None, events);
                if self.winner.is_some() {
                    return;
                }
            }
        }
    }

    fn check_victory(&mut self, events: &mut Vec<Event>) {
        // Snatch: the moment the mark is down-but-breathing, it's over.
        if let MissionRule::Snatch { target } = self.rule
            && self.winner.is_none()
        {
            let t = self.unit(target);
            if t.alive && !t.conscious {
                self.winner = Some(Side::Order);
                events.push(Event::BattleOver { winner: Side::Order });
                return;
            }
        }
        for side in [Side::Order, Side::Demons] {
            // Civilians can survive without holding the field.
            if self.living(side).filter(|u| !u.civilian).count() == 0 {
                self.winner = Some(side.enemy());
                events.push(Event::BattleOver { winner: side.enemy() });
                return;
            }
        }
    }

    fn end_turn(&mut self) -> Vec<Event> {
        self.side_to_move = self.side_to_move.enemy();
        if self.side_to_move == Side::Order {
            self.turn += 1;
        }
        let mut events = vec![Event::TurnStarted { side: self.side_to_move, turn: self.turn }];

        // The dark reports what the eyes missed — vaguely. Rain drowns it.
        if self.weather == Weather::Rain {
            self.heard.clear();
            self.alarm.clear();
        }
        if self.side_to_move == Side::Order && !self.heard.is_empty() {
            let picks: Vec<IVec3> = self.heard.iter().step_by(4).copied().collect();
            for at in picks {
                let fuzz = IVec3::new(
                    self.rng.roll(5) as i32 - 2,
                    self.rng.roll(5) as i32 - 2,
                    0,
                );
                events.push(Event::NoiseInDark { near: at + fuzz });
            }
            self.heard.clear();
        }

        // The clock is a combatant too.
        if self.side_to_move == Side::Order && self.winner.is_none() {
            let expired = match self.rule {
                MissionRule::Evacuate { turns, .. } => self.turn > turns,
                MissionRule::Interrupt { turns } => self.turn > turns && self.objective.is_some(),
                _ => false,
            };
            if expired {
                events.push(Event::TimeExpired);
                self.winner = Some(Side::Demons);
                events.push(Event::BattleOver { winner: Side::Demons });
                return events;
            }
        }

        // The routed slip out; a Prince's will drags runners back into line.
        if self.side_to_move == Side::Demons && self.winner.is_none() {
            self.check_escapes(&mut events);
            let princes: Vec<IVec3> = self
                .units
                .iter()
                .filter(|u| u.is_active() && u.side == Side::Demons && u.species == Species::Prince)
                .map(|u| u.tile)
                .collect();
            if !princes.is_empty() {
                for u in &mut self.units {
                    if u.is_active()
                        && u.routed
                        && princes.iter().any(|p| cheb(*p, u.tile) <= TERRIFY_RANGE_TILES)
                    {
                        u.routed = false;
                        u.morale = u.morale.max(50);
                        events.push(Event::Lashed { unit: u.id });
                    }
                }
            }
        }

        // Summoning circles scribe a demon-turn closer to delivering.
        if self.side_to_move == Side::Demons && self.winner.is_none() {
            let mut due = Vec::new();
            for s in &mut self.summons {
                s.1 = s.1.saturating_sub(1);
                if s.1 == 0 {
                    due.push((s.0, s.2));
                }
            }
            self.summons.retain(|(_, t, _)| *t > 0);
            for (at, strength) in due {
                // A boot on the lines fouls the working.
                let fouled = self
                    .units
                    .iter()
                    .any(|u| u.is_active() && u.tile == at)
                    || !self.tiles.is_walkable(at);
                self.scorch_sigil(at);
                if fouled {
                    events.push(Event::SummoningDisrupted { at });
                    continue;
                }
                let id = self.units.len() as u32;
                let fresh = if strength >= 5 {
                    Unit::hellhound(id, "Summoned Hound", at)
                } else {
                    Unit::imp(id, "Summoned Imp", at)
                };
                self.units.push(fresh);
                self.xp.push(Experience::default());
                events.push(Event::Summoned { unit: UnitId(id) });
            }
        }

        // The obelisk's corruption creeps — and whispers at whoever stands
        // on it. Its veins die with the obelisk.
        if self.side_to_move == Side::Order
            && self.winner.is_none()
            && let Some(obj) = self.objective
        {
            {
                if self.corruption.is_empty() {
                    // First veins break ground beside the obelisk.
                    let seed_tile = crate::voxel_to_tile(obj.min) + IVec3::new(-1, 1, 0);
                    if self.tiles.is_walkable(seed_tile) {
                        self.corruption.push(seed_tile);
                        self.paint_veins(seed_tile);
                        events.push(Event::CorruptionSpread { at: seed_tile });
                    }
                } else if self.corruption.len() < CORRUPTION_CAP {
                    // One vein per round reaches for fresh ground.
                    let from = self.corruption[self.rng.roll(self.corruption.len() as u32) as usize];
                    const RING: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                    let (dx, dy) = RING[self.rng.roll(4) as usize];
                    let next = from + IVec3::new(dx, dy, 0);
                    if self.tiles.is_walkable(next) && !self.corruption.contains(&next) {
                        self.corruption.push(next);
                        self.paint_veins(next);
                        events.push(Event::CorruptionSpread { at: next });
                    }
                }
                // The ground talks to whoever stands on it.
                let standing: Vec<UnitId> = self
                    .units
                    .iter()
                    .filter(|u| {
                        u.is_active() && u.side == Side::Order && self.corruption.contains(&u.tile)
                    })
                    .map(|u| u.id)
                    .collect();
                for id in standing {
                    let u = self.unit_mut(id);
                    u.morale = (u.morale - 8).max(0);
                    u.horror += 1;
                    events.push(Event::Whispered { unit: id });
                }
            }
        }

        // Primed charges burn down — and go off.
        let mut exploding = Vec::new();
        for (at, timer) in &mut self.charges {
            *timer -= 1;
            if *timer == 0 {
                exploding.push(*at);
            }
        }
        self.charges.retain(|(_, t)| *t > 0);
        for at in exploding {
            self.explode(at, GRENADE_POWER, None, &mut events);
            if self.winner.is_some() {
                return events;
            }
        }

        // Breath comes back to the side about to move: a third per turn.
        let side = self.side_to_move;
        for u in self.units.iter_mut().filter(|u| u.side == side && u.is_active()) {
            u.stamina = (u.stamina + u.stamina_max / 3).min(u.stamina_max);
        }

        // Smoke thins; fire gutters, burns, and spreads. Rain drowns it.
        let rain = self.weather == Weather::Rain;
        for c in &mut self.clouds {
            c.2 -= if rain && c.1 == CloudKind::Fire { 2 } else { 1 };
        }
        let expired: Vec<()> = Vec::new();
        let _ = expired;
        self.clouds.retain(|(_, _, ttl)| *ttl > 0);
        if self.side_to_move == Side::Order && !rain {
            // Once per full round: fire eats what it stands on, then
            // reaches for fresh fuel — hungrily where the fuel is real.
            let fires: Vec<IVec3> = self
                .clouds
                .iter()
                .filter(|(_, k, _)| *k == CloudKind::Fire)
                .map(|(t, _, _)| *t)
                .collect();
            for at in fires {
                self.char_tile(at);
                const RING: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                let (dx, dy) = RING[self.rng.roll(4) as usize];
                let next = at + IVec3::new(dx, dy, 0);
                let fueled = self.tile_fuel(next);
                let chance = if fueled { 70 } else { 25 };
                // Bare ground only carries fire where boots could walk;
                // fuel carries it anywhere — hedges and walls of flesh burn.
                if (self.tiles.is_walkable(next) || fueled)
                    && self.rng.roll(100) < chance
                    && !self
                        .clouds
                        .iter()
                        .any(|(t, k, _)| *t == next && *k == CloudKind::Fire)
                {
                    self.add_cloud(next, CloudKind::Fire, if fueled { 6 } else { 3 });
                    events.push(Event::FireStarted { at: next });
                }
            }
        }
        self.check_hazards(&mut events);
        if self.winner.is_some() {
            return events;
        }

        // Anyone standing in flame burns.
        let burning: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| {
                u.is_active()
                    && u.side == self.side_to_move
                    && self
                        .clouds
                        .iter()
                        .any(|(t, k, _)| *t == u.tile && *k == CloudKind::Fire)
            })
            .map(|u| u.id)
            .collect();
        for id in burning {
            let dmg = 6 + self.rng.roll(8) as i32;
            events.push(Event::Burned { unit: id, amount: dmg });
            self.unit_mut(id).morale = (self.unit(id).morale - 10).max(0);
            self.apply_damage(id, dmg, None, &mut events);
            if self.winner.is_some() {
                return events;
            }
        }

        // Seized minds come back to their owners.
        let possessed: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.alive && u.side == self.side_to_move && u.possessed > 0)
            .map(|u| u.id)
            .collect();
        for id in possessed {
            let u = self.unit_mut(id);
            u.possessed -= 1;
            if u.possessed == 0 {
                events.push(Event::PossessionEnds { unit: id });
            }
        }

        // Stun trauma fades; the subdued may come round groggy.
        let side_units: Vec<UnitId> = self
            .units
            .iter()
            .filter(|u| u.alive && u.side == self.side_to_move)
            .map(|u| u.id)
            .collect();
        for id in side_units {
            let u = self.unit_mut(id);
            u.suppression = 0; // a fresh turn steadies the nerves
            u.stun = (u.stun - 3).max(0);
            if !u.conscious && u.stun < u.health {
                u.conscious = true;
                u.tu = u.tu_max / 2;
                events.push(Event::Awakened { unit: id });
            }
        }

        // For the side coming on turn: refresh TUs, bleed open wounds, then
        // roll dread checks (panic or berserk).
        let ids: Vec<UnitId> = self.living(self.side_to_move).map(|u| u.id).collect();
        for id in ids {
            {
                let u = self.unit_mut(id);
                u.tu = u.tu_max;
            }

            let wounds = self.unit(id).wounds;
            if wounds > 0 {
                self.unit_mut(id).health -= wounds;
                let tile = self.unit(id).tile;
                self.spatter(tile, 1, MAT_BLOOD);
                events.push(Event::Bled {
                    unit: id,
                    health_left: self.unit(id).health.max(0),
                });
                if self.unit(id).health <= 0 {
                    self.kill_unit(id, &mut events);
                    if self.winner.is_some() {
                        return events;
                    }
                    continue;
                }
            }

            // Demonic rot festers a turn deeper — and finishes its work.
            if let Some((part, turns)) = self.unit(id).infected {
                if turns + 1 >= INFECTION_TURNS {
                    events.push(Event::InfectionTurned { unit: id });
                    self.convert_to_husk(id);
                    self.check_victory(&mut events);
                    if self.winner.is_some() {
                        return events;
                    }
                    continue;
                }
                self.unit_mut(id).infected = Some((part, turns + 1));
            }

            let morale = self.unit(id).morale;
            if morale < 50 {
                let chance = ((50 - morale) * 2).clamp(0, 90) as u32;
                if self.rng.roll(100) < chance {
                    self.unit_mut(id).morale = (morale + 20).min(100);
                    self.xp[id.0 as usize].dread_survived += 1;
                    if self.rng.roll(100) < 25 {
                        events.push(Event::Berserked { unit: id });
                        self.go_berserk(id, &mut events);
                    } else {
                        events.push(Event::Panicked { unit: id });
                    }
                    self.unit_mut(id).tu = 0; // the turn is lost either way
                    if self.winner.is_some() {
                        return events;
                    }
                    continue;
                }
            }
            let u = self.unit_mut(id);
            u.morale = (u.morale + 5).min(100);
        }
        events
    }

    /// Berserk: blaze away at the nearest visible enemy, then collapse.
    fn go_berserk(&mut self, id: UnitId, events: &mut Vec<Event>) {
        let me = self.unit(id).tile;
        let mut enemies: Vec<UnitId> = self
            .living(self.unit(id).side.enemy())
            .map(|u| u.id)
            .collect();
        enemies.sort_by_key(|&e| (cheb(me, self.unit(e).tile), e.0));
        let Some(&target) = enemies.iter().find(|&&e| self.can_see(id, e)) else {
            return;
        };
        for _ in 0..2 {
            if !self.unit(target).alive
                || self.winner.is_some()
                || self
                    .unit(id)
                    .fire_cost(FireMode::Snap)
                    .is_none_or(|c| self.unit(id).tu < c)
            {
                break;
            }
            self.resolve_shot(id, target, FireMode::Snap, false, events);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Unit, rifle};
    use ods_voxel::Voxel;

    const STONE: Voxel = Voxel(1);

    /// Flat 12x12 ground, floor slab z 0..2, no obstacles.
    fn open_field(units: Vec<Unit>, seed: u64) -> Battle {
        let mut world = VoxelWorld::new();
        world.fill_box(
            IVec3::new(0, 0, 0),
            IVec3::new(12 * TILE_VOXELS, 12 * TILE_VOXELS, crate::scenario::GROUND_TOP),
            STONE,
        );
        Battle::new(world, IVec3::ZERO, IVec3::new(12, 12, 1), units, seed)
    }

    /// Same field with a wall between x tiles 5|6 (no gap).
    fn walled_field(units: Vec<Unit>, seed: u64) -> Battle {
        let mut world = VoxelWorld::new();
        world.fill_box(
            IVec3::new(0, 0, 0),
            IVec3::new(12 * TILE_VOXELS, 12 * TILE_VOXELS, crate::scenario::GROUND_TOP),
            STONE,
        );
        world.fill_box(
            IVec3::new(5 * TILE_VOXELS, 0, 2),
            IVec3::new(6 * TILE_VOXELS, 12 * TILE_VOXELS, TILE_VOXELS * 7 / 8),
            Voxel(2),
        );
        Battle::new(world, IVec3::ZERO, IVec3::new(12, 12, 1), units, seed)
    }

    fn duelists() -> Vec<Unit> {
        vec![
            Unit::soldier(0, "Vasquez", IVec3::new(1, 5, 0)),
            Unit::imp(1, "Imp", IVec3::new(10, 5, 0)),
        ]
    }

    #[test]
    fn rounds_are_finite_and_the_reload_feeds_the_gun() {
        let mut b = open_field(duelists(), 41);
        b.units[0].tile = IVec3::new(4, 5, 0);
        b.units[1].tile = IVec3::new(7, 5, 0);
        let clip = b.units[0].weapon.clip as i32;
        assert!(clip > 0, "the rifle feeds from a clip");
        assert_eq!(b.units[0].ammo, clip, "it rides in loaded");

        b.perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap })
            .unwrap();
        assert_eq!(b.unit(UnitId(0)).ammo, clip - 1, "one squeeze, one round");

        // Run the chamber dry: firing refuses before spending a single TU.
        b.units[0].ammo = 0;
        let tu = b.unit(UnitId(0)).tu;
        let err = b.perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap });
        assert_eq!(err.unwrap_err(), ActionError::NoAmmo);
        assert_eq!(b.unit(UnitId(0)).tu, tu, "a dry click is free");

        // A fresh magazine fixes everything (and costs its 12 TU).
        assert!(b.units[0].mags > 0);
        let mags = b.units[0].mags;
        b.perform(Action::Reload { unit: UnitId(0) }).unwrap();
        assert_eq!(b.unit(UnitId(0)).ammo, clip);
        assert_eq!(b.unit(UnitId(0)).mags, mags - 1);
        assert_eq!(b.unit(UnitId(0)).tu, tu - RELOAD_TU);
        // Topped off, the quartermaster refuses to waste another.
        assert_eq!(
            b.perform(Action::Reload { unit: UnitId(0) }).unwrap_err(),
            ActionError::BadTarget
        );
        // An auto burst dies with the clip: one round left fires one round.
        b.units[0].ammo = 1;
        let fired = b
            .perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Auto })
            .unwrap()
            .iter()
            .filter(|e| matches!(e, Event::Fired { .. }))
            .count();
        assert_eq!(fired, 1, "the burst clicks dry mid-squeeze");
        assert_eq!(b.unit(UnitId(0)).ammo, 0);
    }

    #[test]
    fn dry_guns_do_not_react() {
        // The imp walks straight across the soldier's face; the soldier has
        // reactions to spare but an empty chamber and nothing happens.
        let mut units = duelists();
        units[0].reactions = 95;
        units[0].facing = IVec3::new(1, 0, 0);
        units[1].tile = IVec3::new(6, 5, 0);
        let mut b = open_field(units, 42);
        b.units[0].ammo = 0;
        b.units[0].mags = 0;
        b.perform(Action::EndTurn).unwrap();
        let events = b
            .perform(Action::Move { unit: UnitId(1), to: IVec3::new(6, 3, 0) })
            .unwrap();
        assert!(
            !events.iter().any(|e| matches!(e, Event::Fired { reaction: true, .. })),
            "an empty chamber watches in silence: {events:?}"
        );
    }

    #[test]
    fn the_sidearm_swap_keeps_both_loads() {
        let mut b = open_field(duelists(), 43);
        b.units[0].sidearm = Some(crate::units::Weapon::from_data("consecrated blade", "blade"));
        b.units[0].ammo = 7;
        let tu = b.units[0].tu;
        b.perform(Action::SwapWeapon { unit: UnitId(0) }).unwrap();
        let u = b.unit(UnitId(0));
        assert!(u.weapon.melee, "the blade is in hand");
        assert_eq!(u.tu, tu - SWAP_TU);
        assert_eq!(u.sidearm_ammo, 7, "the rifle holsters with its rounds");
        b.perform(Action::SwapWeapon { unit: UnitId(0) }).unwrap();
        let u = b.unit(UnitId(0));
        assert_eq!(u.weapon.key, "rifle");
        assert_eq!(u.ammo, 7, "and comes back exactly as it left");
    }

    #[test]
    fn the_fallen_drop_their_arms_and_the_living_take_them_up() {
        let mut units = duelists();
        units.push(Unit::soldier(2, "Ito", IVec3::new(2, 5, 0)));
        let mut b = open_field(units, 44);
        b.xp_push_for_test();
        // Ito dies holding a loaded rifle.
        b.units[2].ammo = 9;
        let mut events = Vec::new();
        b.kill_unit(UnitId(2), &mut events);
        assert!(
            events.iter().any(|e| matches!(e, Event::WeaponDropped { .. })),
            "{events:?}"
        );
        assert_eq!(b.ground.len(), 1);
        assert_eq!(b.ground[0].0, IVec3::new(2, 5, 0));
        assert_eq!(b.ground[0].2, 9, "the rounds go down with it");
        assert_eq!(b.unit(UnitId(2)).weapon.key, "bare_hands");

        // Vasquez steps over and takes it up, leaving her own behind.
        b.units[0].tile = IVec3::new(2, 4, 0);
        b.units[0].ammo = 3;
        b.perform(Action::Scavenge { unit: UnitId(0) }).unwrap();
        assert_eq!(b.unit(UnitId(0)).ammo, 9, "the dead man's rounds come too");
        assert_eq!(b.ground.len(), 1, "her own rifle lies where she stood");
        assert_eq!(b.ground[0].2, 3);
    }

    #[test]
    fn move_consumes_tu_and_reports() {
        let mut b = open_field(duelists(), 1);
        let events = b
            .perform(Action::Move { unit: UnitId(0), to: IVec3::new(4, 5, 0) })
            .unwrap();
        let moves = events
            .iter()
            .filter(|e| matches!(e, Event::Moved { .. }))
            .count();
        assert_eq!(moves, 3);
        assert_eq!(b.unit(UnitId(0)).tu, 55 - 12);
        assert_eq!(b.unit(UnitId(0)).tile, IVec3::new(4, 5, 0));
    }

    #[test]
    fn cannot_act_out_of_turn_or_dead() {
        let mut b = open_field(duelists(), 1);
        assert_eq!(
            b.perform(Action::Move { unit: UnitId(1), to: IVec3::new(9, 5, 0) }),
            Err(ActionError::NotYourTurn)
        );
        b.units[0].alive = false;
        assert_eq!(
            b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(2, 5, 0) }),
            Err(ActionError::DeadUnit)
        );
    }

    #[test]
    fn wall_blocks_sight_and_fire() {
        let mut b = walled_field(duelists(), 1);
        assert!(!b.can_see(UnitId(0), UnitId(1)));
        assert_eq!(
            b.perform(Action::Fire {
                unit: UnitId(0),
                target: UnitId(1),
                mode: FireMode::Snap
            }),
            Err(ActionError::NoLineOfSight)
        );
    }

    #[test]
    fn breaching_the_wall_opens_sight() {
        let mut b = walled_field(duelists(), 1);
        // Blow a man-sized hole through the wall at eye height.
        let center = Vec3::new(5.5 * TILE_VOXELS as f32, 5.5 * TILE_VOXELS as f32, 11.0);
        let destroyed = b.world.carve_sphere(center, 10.0 * VS as f32);
        assert!(destroyed > 0);
        assert!(b.can_see(UnitId(0), UnitId(1)), "sight through the breach");
    }

    #[test]
    fn firing_costs_tu_and_eventually_kills() {
        let mut b = open_field(duelists(), 42);
        let cost = b.unit(UnitId(0)).fire_cost(FireMode::Snap).unwrap();
        assert_eq!(cost, 13);

        let mut killed = false;
        'battle: for _round in 0..60 {
            while b.unit(UnitId(0)).tu >= cost {
                let events = b
                    .perform(Action::Fire {
                        unit: UnitId(0),
                        target: UnitId(1),
                        mode: FireMode::Snap,
                    })
                    .unwrap();
                if events.iter().any(|e| matches!(e, Event::Died { .. })) {
                    killed = true;
                    assert!(matches!(events.last(), Some(Event::BattleOver { winner: Side::Order })));
                    break 'battle;
                }
            }
            b.perform(Action::EndTurn).unwrap(); // demons (do nothing)
            if b.winner.is_some() {
                break;
            }
            b.perform(Action::EndTurn).unwrap(); // back to order
        }
        assert!(killed, "an imp cannot dodge rifles forever");
        assert_eq!(b.winner, Some(Side::Order));
        assert_eq!(
            b.perform(Action::EndTurn),
            Err(ActionError::BattleOver),
            "no actions after the battle ends"
        );
    }

    #[test]
    fn misses_chip_terrain() {
        // Aim at a target hiding right in front of a big wall; misses should
        // eventually carve it.
        let mut units = duelists();
        units[1].tile = IVec3::new(4, 5, 0); // in front of the x=5 wall
        let mut b = walled_field(units, 7);

        let mut destroyed = 0usize;
        for _ in 0..40 {
            if b.winner.is_some() {
                break;
            }
            while b.unit(UnitId(0)).tu >= b.unit(UnitId(0)).fire_cost(FireMode::Snap).unwrap()
                && b.winner.is_none()
            {
                let events = b
                    .perform(Action::Fire {
                        unit: UnitId(0),
                        target: UnitId(1),
                        mode: FireMode::Snap,
                    })
                    .unwrap();
                destroyed += events
                    .iter()
                    .filter_map(|e| match e {
                        Event::TerrainDestroyed { voxels, .. } => Some(voxels),
                        _ => None,
                    })
                    .sum::<usize>();
            }
            if b.winner.is_none() {
                b.perform(Action::EndTurn).unwrap();
                b.perform(Action::EndTurn).unwrap();
            }
        }
        assert!(destroyed > 0, "stray shots must scar the battlefield");
    }

    #[test]
    fn the_belt_runs_out_and_the_pack_costs_time() {
        let mut b = open_field(duelists(), 51);
        b.xp_push_for_test();
        let u = &mut b.units[0];
        u.grenades = 4;
        u.strength = 60; // range is not the question here
        u.tu = 400;
        u.tu_max = 200; // costs key off max; the pool is padded for the test
        assert_eq!(u.belt, 3);
        let base = 200 * GRENADE_COST_PCT / 100;
        // Three throws off the belt at the plain price...
        for i in 0..3 {
            let before = b.unit(UnitId(0)).tu;
            b.perform(Action::Throw { unit: UnitId(0), at: IVec3::new(4, 8, 0) }).unwrap();
            assert_eq!(before - b.unit(UnitId(0)).tu, base, "belt throw {i}");
        }
        assert_eq!(b.unit(UnitId(0)).belt, 0);
        // ...and the fourth is dug out of the pack.
        let before = b.unit(UnitId(0)).tu;
        b.perform(Action::Throw { unit: UnitId(0), at: IVec3::new(4, 8, 0) }).unwrap();
        assert_eq!(
            before - b.unit(UnitId(0)).tu,
            base + PACK_FETCH_TU,
            "the pack costs time"
        );
    }

    #[test]
    fn salt_rounds_ring_and_cold_iron_bites() {
        use crate::units::MagKind;
        // Salt: every hit adds stun on top of (reduced) blood.
        let mut units = duelists();
        units[0].accuracy = 95;
        units[1].tile = IVec3::new(3, 5, 0);
        let mut b = open_field(units, 52);
        b.xp_push_for_test();
        b.units[0].mag_kind = MagKind::Salt;
        b.units[1].armor_front = 90; // no blood, pure ring
        b.units[1].armor_side = 90;
        b.units[1].armor_rear = 90;
        let mut stunned = false;
        for _ in 0..6 {
            let ev = b
                .perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap })
                .unwrap();
            if ev.iter().any(|e| matches!(e, Event::Stunned { .. })) {
                stunned = true;
                break;
            }
            b.units[0].tu = b.units[0].tu_max;
            b.units[0].ammo = 12;
        }
        assert!(stunned, "salt leaves them ringing");
    }

    #[test]
    fn the_breaker_smashes_the_cover_you_hide_behind() {
        // A soldier hugging the chapel's west wall; the behemoth inside.
        let mut b = crate::scenario::skirmish(53);
        b.units[0].tile = IVec3::new(8, 9, 0); // west of wall tile (9,9)
        b.units[0].tu = 0;
        b.units[0].armor_front = 90;
        b.units[0].armor_side = 90;
        b.units[0].armor_rear = 90;
        for i in 1..4 {
            b.units[i].tile = IVec3::new(0, 20 + i as i32, 0);
        }
        b.units[4] = Unit::behemoth(4, "Behemoth", IVec3::new(11, 9, 0));
        for i in 5..8 {
            b.units[i].tile = IVec3::new(0, 24 + i as i32, 0);
        }
        assert!(!b.tiles.is_walkable(IVec3::new(9, 9, 0)), "the cover stands");
        b.perform(Action::EndTurn).unwrap();
        let events = crate::ai::run_demon_turn(&mut b);
        let prey = b.unit(UnitId(0)).tile;
        assert!(
            events.iter().any(|e| matches!(
                e,
                Event::WallSmashed { at, .. } if (*at - prey).abs().max_element() <= 1
            )),
            "the Doorbreaker takes the prey's own cover down: {events:?}"
        );
    }

    #[test]
    fn the_confessors_whisper_steadies_and_burns() {
        let mut units = duelists();
        units.push(Unit::soldier(2, "Shaken", IVec3::new(3, 5, 0)));
        units[0].psi = true; // anointed
        units[2].morale = 30;
        units[2].suppression = 3;
        let mut b = open_field(units, 50);
        b.xp_push_for_test();
        b.perform(Action::Steady { unit: UnitId(0), target: UnitId(2) }).unwrap();
        assert_eq!(b.unit(UnitId(2)).morale, 60, "the mind knits");
        assert_eq!(b.unit(UnitId(2)).suppression, 0, "the flinch lifts");
        assert_eq!(b.unit(UnitId(0)).horror, 1, "and the channel burns its keeper");
        // The whisper does not run toward the enemy...
        assert_eq!(
            b.perform(Action::Steady { unit: UnitId(0), target: UnitId(1) }).unwrap_err(),
            ActionError::BadTarget
        );
        // ...and the unanointed have no whisper at all.
        assert_eq!(
            b.perform(Action::Steady { unit: UnitId(2), target: UnitId(0) }).unwrap_err(),
            ActionError::NoPsi
        );
    }

    #[test]
    fn the_rift_reinforces_until_the_obelisk_falls() {
        let squad: Vec<Unit> =
            (0..4).map(|i| Unit::soldier(i, &format!("S{i}"), IVec3::ZERO)).collect();
        let mut b = crate::scenario::incursion(5, squad, 4, 3);
        assert!(!b.summons.is_empty(), "the rift keeps giving");

        // The obelisk comes down: the circles die with the way that fed them.
        let obj = b.objective.expect("rift maps carry an obelisk");
        b.world.fill_box(obj.min, obj.max, ods_voxel::Voxel::EMPTY);
        b.tiles.rederive_region(&b.world, obj.min, obj.max);
        let mut events = Vec::new();
        b.check_objective_for_test(&mut events);
        assert!(
            events.iter().any(|e| matches!(e, Event::ObjectiveDestroyed)),
            "{events:?}"
        );
        assert!(b.summons.is_empty(), "nothing more comes through a closed rift");
    }

    #[test]
    fn the_watch_answers_with_what_it_banked() {
        // A soldier on Storm watch (auto reserve) with heaps of TU; the imp
        // walks across their face and eats the whole burst.
        let mut units = duelists();
        units[0].reactions = 90;
        units[0].facing = IVec3::new(1, 0, 0);
        units[1].reactions = 20;
        units[1].tile = IVec3::new(6, 5, 0);
        let mut b = open_field(units, 45);
        b.perform(Action::SetReserve { unit: UnitId(0), mode: Some(FireMode::Auto) })
            .unwrap();
        b.perform(Action::EndTurn).unwrap();
        b.units[1].tu = 20; // low initiative: the watcher wins the draw
        let events = b
            .perform(Action::Move { unit: UnitId(1), to: IVec3::new(6, 7, 0) })
            .unwrap();
        let auto_rounds = events
            .iter()
            .filter(|e| {
                matches!(e, Event::Fired { reaction: true, mode: FireMode::Auto, .. })
            })
            .count();
        assert!(
            auto_rounds >= 2,
            "the banked storm answers as a burst, not a plink: {events:?}"
        );
    }

    #[test]
    fn killing_the_last_driver_breaks_the_pack() {
        let mut units = vec![
            Unit::soldier(0, "Vasquez", IVec3::new(1, 5, 0)),
            Unit::overseer(1, "Overseer", IVec3::new(9, 5, 0)),
            Unit::imp(2, "Imp A", IVec3::new(8, 4, 0)),
            Unit::imp(3, "Imp B", IVec3::new(8, 6, 0)),
        ];
        // The rabble is already shaky; only the driver holds them together.
        units[2].morale = 45;
        units[3].morale = 45;
        let mut b = open_field(units, 46);
        b.xp_push_for_test();
        let mut events = Vec::new();
        b.kill_unit(UnitId(1), &mut events);
        assert!(
            events.iter().any(|e| matches!(e, Event::PackShaken)),
            "the leash goes slack: {events:?}"
        );
        assert!(
            events.iter().any(|e| matches!(e, Event::PackBroken)),
            "and the pack breaks: {events:?}"
        );
        assert!(b.unit(UnitId(2)).routed && b.unit(UnitId(3)).routed);

        // Routed demons run for where they came in, and slip out.
        b.demon_exit = IVec3::new(10, 5, 0);
        b.perform(Action::EndTurn).unwrap();
        let events = crate::ai::run_demon_turn(&mut b);
        let escaped = b.units.iter().filter(|u| u.escaped).count();
        assert!(
            escaped > 0 || events.iter().any(|e| matches!(e, Event::Escaped { .. })),
            "the runners reach the way out: {events:?}"
        );
        // When the last of them is dead or gone, the field is won.
        if b.units.iter().filter(|u| u.alive && u.side == Side::Demons).count() == 0 {
            assert_eq!(b.winner, Some(Side::Order));
        }
    }

    #[test]
    fn execution_is_certain_and_priced_in_captures() {
        let mut units = duelists();
        units[1].tile = IVec3::new(2, 5, 0);
        let mut b = open_field(units, 47);
        b.xp_push_for_test();
        // A conscious enemy cannot simply be put down.
        assert_eq!(
            b.perform(Action::Execute { unit: UnitId(0), target: UnitId(1) }).unwrap_err(),
            ActionError::BadTarget
        );
        // Stunned and helpless, it can.
        b.units[1].conscious = false;
        let events = b
            .perform(Action::Execute { unit: UnitId(0), target: UnitId(1) })
            .unwrap();
        assert!(events.iter().any(|e| matches!(e, Event::Executed { .. })));
        assert!(!b.unit(UnitId(1)).alive, "certain, quick, and final");
        assert_eq!(b.executed, 1, "the ledger remembers");
        assert_eq!(b.winner, Some(Side::Order));
    }

    #[test]
    fn the_taker_takes_the_helpless_but_not_the_guarded() {
        let mut units = vec![
            Unit::soldier(0, "Downed", IVec3::new(5, 5, 0)),
            Unit::soldier(1, "Guard", IVec3::new(5, 6, 0)),
            Unit::taker(2, "The Taker", IVec3::new(6, 5, 0)),
        ];
        units[0].conscious = false;
        let mut b = open_field(units, 48);
        b.xp_push_for_test();
        b.perform(Action::EndTurn).unwrap();
        // A comrade stands over the body: the claws stay away.
        assert_eq!(
            b.perform(Action::Defile { unit: UnitId(2), corpse: UnitId(0) }).unwrap_err(),
            ActionError::BadTarget
        );
        // The guard falls back, and the helpless is Taken without waking.
        b.units[1].tile = IVec3::new(9, 9, 0);
        let events = b
            .perform(Action::Defile { unit: UnitId(2), corpse: UnitId(0) })
            .unwrap();
        assert!(events.iter().any(|e| matches!(e, Event::Defiled { .. })), "{events:?}");
        assert_eq!(b.unit(UnitId(0)).species, Species::Husk, "risen without ever waking");
        assert_eq!(b.unit(UnitId(0)).side, Side::Demons);
    }

    #[test]
    fn granting_rest_steadies_the_squad() {
        let mut units = duelists();
        units.push(Unit::husk(2, "Kowalski (Taken)", IVec3::new(8, 8, 0)));
        units[0].morale = 60;
        units[0].horror = 4;
        let mut b = open_field(units, 49);
        b.xp_push_for_test();
        let mut events = Vec::new();
        b.kill_unit(UnitId(2), &mut events);
        assert!(events.iter().any(|e| matches!(e, Event::RestGranted { .. })), "{events:?}");
        let u = b.unit(UnitId(0));
        assert!(u.morale > 60 - 15, "rest given outweighs a death seen: {}", u.morale);
        assert_eq!(u.horror, 3, "and the weight lifts a little");
    }

    #[test]
    fn reaction_fire_punishes_moving_in_the_open() {
        // Imp on overwatch with full TUs and sharp reactions; a soldier who
        // has already spent most TUs walks across its field of view.
        let mut units = duelists();
        units[0].reactions = 20;
        units[1].reactions = 90;
        let mut b = open_field(units, 3);

        // Drain the soldier's TUs to lower initiative below the imp's.
        b.units[0].tu = 20;

        let events = b
            .perform(Action::Move { unit: UnitId(0), to: IVec3::new(4, 5, 0) })
            .unwrap();
        let reactions = events
            .iter()
            .filter(|e| matches!(e, Event::Fired { reaction: true, .. }))
            .count();
        assert!(reactions > 0, "imp should take reaction shots: {events:?}");
        assert!(
            b.unit(UnitId(1)).tu < 45,
            "reaction fire spends the imp's banked TUs"
        );
    }

    #[test]
    fn deaths_drain_squad_morale_and_panic_can_follow() {
        let mut units = vec![
            Unit::soldier(0, "A", IVec3::new(1, 1, 0)),
            Unit::soldier(1, "B", IVec3::new(1, 3, 0)),
            Unit::imp(2, "Imp", IVec3::new(10, 5, 0)),
        ];
        units[1].bravery = 10; // very jumpy
        // Walled field: the imp is out of sight, so a berserk roll can't
        // shoot anyone and end the battle mid-test.
        let mut b = walled_field(units, 5);

        let before = b.unit(UnitId(1)).morale;
        // Execute soldier A via direct damage.
        let mut events = Vec::new();
        b.apply_damage(UnitId(0), 999, None, &mut events);
        assert!(events.iter().any(|e| matches!(e, Event::Died { .. })));
        assert!(
            b.unit(UnitId(1)).morale < before,
            "survivor morale must drop on a squad death"
        );

        // Grind morale to the floor and confirm panic occurs at some turn
        // start (probabilistic, so give it several attempts).
        b.units[1].morale = 5;
        let mut panicked = false;
        for _ in 0..24 {
            b.perform(Action::EndTurn).unwrap(); // demons
            let events = b.perform(Action::EndTurn).unwrap(); // order turn start
            if events.iter().any(|e| matches!(e, Event::Panicked { unit } if *unit == UnitId(1))) {
                panicked = true;
                break;
            }
            b.units[1].morale = 5; // keep them terrified for the next roll
        }
        assert!(panicked, "morale 5 should panic within a few checks");
    }

    #[test]
    fn dread_can_break_into_berserk_fire() {
        // Open field: a visible imp, an unbreakable one so the battle can't
        // end while we fish for the berserk branch.
        let mut units = duelists();
        units[0].bravery = 10;
        units[1].health_max = 5000;
        units[1].health = 5000;
        let mut b = open_field(units, 21);

        let mut berserked = false;
        for _ in 0..60 {
            b.units[0].morale = 5;
            b.perform(Action::EndTurn).unwrap(); // demons
            let events = b.perform(Action::EndTurn).unwrap(); // order turn start
            if events.iter().any(|e| matches!(e, Event::Berserked { unit } if *unit == UnitId(0))) {
                berserked = true;
                assert!(
                    events.iter().any(|e| matches!(
                        e,
                        Event::Fired { unit, mode: FireMode::Snap, .. } if *unit == UnitId(0)
                    )),
                    "berserk must actually blaze away: {events:?}"
                );
                assert_eq!(b.unit(UnitId(0)).tu, 0, "the berserk turn is lost");
                break;
            }
        }
        assert!(berserked, "60 dread checks should break someone eventually");
    }

    #[test]
    fn auto_burst_fires_multiple_rounds_for_one_cost() {
        let mut units = duelists();
        units[1].health_max = 5000; // survive the burst so all rounds fire
        units[1].health = 5000;
        let mut b = open_field(units, 9);
        let cost = b.unit(UnitId(0)).fire_cost(FireMode::Auto).unwrap();

        let events = b
            .perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Auto })
            .unwrap();
        let rounds = events
            .iter()
            .filter(|e| matches!(e, Event::Fired { mode: FireMode::Auto, .. }))
            .count();
        assert_eq!(rounds, 3, "full burst: {events:?}");
        assert_eq!(b.unit(UnitId(0)).tu, 55 - cost);

        // Imps physically can't burst.
        b.perform(Action::EndTurn).unwrap();
        assert_eq!(
            b.perform(Action::Fire { unit: UnitId(1), target: UnitId(0), mode: FireMode::Auto }),
            Err(ActionError::UnsupportedMode)
        );
    }

    #[test]
    fn grenades_arc_over_walls_and_carve() {
        // Imp hides directly behind the wall — unseeable, unshootable...
        let mut units = duelists();
        units[1].tile = IVec3::new(6, 5, 0);
        units[1].health_max = 5000; // must survive ground zero for the
        units[1].health = 5000; // follow-up range/supply assertions
        let mut b = walled_field(units, 17);
        assert!(!b.can_see(UnitId(0), UnitId(1)));

        // ...but not un-bombable. Lob a charge right onto its tile.
        let events = b
            .perform(Action::Throw { unit: UnitId(0), at: IVec3::new(6, 5, 0) })
            .unwrap();
        assert!(matches!(events[0], Event::Threw { .. }));
        let carved = events.iter().any(
            |e| matches!(e, Event::Exploded { voxels, .. } if *voxels > 0),
        );
        assert!(carved, "the blast must scar the wall/ground: {events:?}");
        assert!(
            events.iter().any(|e| matches!(e, Event::Damaged { unit, .. } if *unit == UnitId(1))),
            "imp at ground zero takes blast damage: {events:?}"
        );
        assert_eq!(b.unit(UnitId(0)).grenades, 1);

        // Range and supply limits: the arm sets the reach (6 + strength/6).
        b.units[0].tile = IVec3::new(0, 0, 0); // corner-to-corner is 11 tiles
        b.units[0].strength = 24; // reach 10: one tile short
        assert_eq!(
            b.perform(Action::Throw { unit: UnitId(0), at: IVec3::new(11, 11, 0) }),
            Err(ActionError::OutOfRange)
        );
        b.units[0].strength = 36; // reach 12: the same throw carries
        assert!(
            b.perform(Action::Throw { unit: UnitId(0), at: IVec3::new(11, 11, 0) }).is_ok(),
            "a stronger arm carries the corner-to-corner throw"
        );
        b.units[0].grenades = 0;
        assert_eq!(
            b.perform(Action::Throw { unit: UnitId(0), at: IVec3::new(3, 5, 0) }),
            Err(ActionError::NoCharges)
        );
    }

    #[test]
    fn wounds_bleed_at_turn_start_and_can_kill() {
        let mut b = open_field(duelists(), 33);
        b.units[0].wounds = 2;
        b.units[0].health = 3;

        b.perform(Action::EndTurn).unwrap(); // demons
        let events = b.perform(Action::EndTurn).unwrap(); // order start: bleed 2
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Bled { unit, health_left: 1 } if *unit == UnitId(0))),
            "{events:?}"
        );

        b.perform(Action::EndTurn).unwrap();
        let events = b.perform(Action::EndTurn).unwrap(); // bleeds to death
        assert!(events.iter().any(|e| matches!(e, Event::Died { unit } if *unit == UnitId(0))));
        assert_eq!(b.winner, Some(Side::Demons));
    }

    #[test]
    fn field_dressing_staunches_and_restores() {
        let mut units = vec![
            Unit::soldier(0, "Medic", IVec3::new(1, 5, 0)),
            Unit::soldier(1, "Patient", IVec3::new(1, 6, 0)),
            Unit::imp(2, "Imp", IVec3::new(10, 5, 0)),
        ];
        units[1].wounds = 2;
        units[1].health = 10;
        let mut b = open_field(units, 8);

        let events = b
            .perform(Action::Heal { medic: UnitId(0), target: UnitId(1) })
            .unwrap();
        assert!(matches!(events[0], Event::Healed { health_left: 14, .. }));
        assert_eq!(b.unit(UnitId(1)).wounds, 1);
        assert_eq!(b.unit(UnitId(0)).heal_charges, 2);
        assert_eq!(b.unit(UnitId(0)).tu, 55 - HEAL_COST_TU);

        // Too far away to treat.
        b.units[1].tile = IVec3::new(5, 5, 0);
        assert_eq!(
            b.perform(Action::Heal { medic: UnitId(0), target: UnitId(1) }),
            Err(ActionError::NotAdjacent)
        );
        // Demons are not patients.
        assert_eq!(
            b.perform(Action::Heal { medic: UnitId(0), target: UnitId(2) }),
            Err(ActionError::BadTarget)
        );
    }

    #[test]
    fn melee_needs_adjacency_and_never_flies_wide() {
        // An Order-side hound so it can act on the first turn of the test.
        let mut units = vec![
            Unit::hellhound(0, "Hound", IVec3::new(1, 5, 0)),
            Unit::imp(1, "Prey", IVec3::new(8, 5, 0)),
        ];
        units[0].side = Side::Order;
        let mut b = open_field(units, 3);
        assert_eq!(
            b.perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap }),
            Err(ActionError::NotAdjacent)
        );
        b.units[0].tile = IVec3::new(7, 5, 0);
        let events = b
            .perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap })
            .unwrap();
        assert!(
            !events.iter().any(|e| matches!(e, Event::TerrainDestroyed { .. })),
            "claws don't chip distant walls: {events:?}"
        );
    }

    #[test]
    fn binding_subdues_and_the_bound_can_wake() {
        let mut units = duelists();
        units[1].health_max = 12;
        units[1].health = 12;
        let mut b = open_field(units, 5);
        b.units[0].tile = IVec3::new(9, 5, 0); // adjacent to the imp

        let mut subdued = false;
        for _ in 0..12 {
            if !b.unit(UnitId(1)).conscious {
                subdued = true;
                break;
            }
            if b.unit(UnitId(0)).tu < 55 * BIND_COST_PCT / 100 {
                b.perform(Action::EndTurn).unwrap();
                b.perform(Action::EndTurn).unwrap();
            }
            b.perform(Action::Bind { unit: UnitId(0), target: UnitId(1) }).unwrap();
        }
        assert!(subdued, "repeated rod strikes must knock the imp out");
        assert_eq!(b.winner, Some(Side::Order), "an unconscious garrison holds nothing");
        assert!(b.unit(UnitId(1)).alive, "subdued is not dead — that's the point");
    }

    #[test]
    fn terrify_needs_psi_and_batters_morale() {
        let mut units = duelists();
        units[1] = Unit::overseer(1, "Overseer", IVec3::new(10, 5, 0));
        let mut b = open_field(units, 11);
        assert_eq!(
            b.perform(Action::Terrify { unit: UnitId(0), target: UnitId(1) }),
            Err(ActionError::NoPsi)
        );
        b.perform(Action::EndTurn).unwrap();

        let mut broken = false;
        for _ in 0..20 {
            if b.unit(UnitId(1)).tu < 50 * TERRIFY_COST_PCT / 100 {
                b.perform(Action::EndTurn).unwrap();
                b.perform(Action::EndTurn).unwrap();
            }
            let events = b
                .perform(Action::Terrify { unit: UnitId(1), target: UnitId(0) })
                .unwrap();
            if events
                .iter()
                .any(|e| matches!(e, Event::Terrified { morale_lost, .. } if *morale_lost > 0))
            {
                broken = true;
                break;
            }
        }
        assert!(broken, "a soldier's will (bravery 30) cannot hold forever");
        assert!(b.unit(UnitId(0)).morale < 100);
    }

    #[test]
    fn kneeling_steadies_the_hand_until_you_move() {
        let mut b = open_field(duelists(), 2);
        let standing = b.unit(UnitId(0)).hit_chance(FireMode::Snap).unwrap();
        b.perform(Action::Kneel { unit: UnitId(0) }).unwrap();
        assert!(b.unit(UnitId(0)).kneeling);
        let kneeling = b.unit(UnitId(0)).hit_chance(FireMode::Snap).unwrap();
        assert!(kneeling > standing, "{kneeling} vs {standing}");
        assert_eq!(b.unit(UnitId(0)).tu, 55 - KNEEL_COST);

        b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(2, 5, 0) }).unwrap();
        assert!(!b.unit(UnitId(0)).kneeling, "moving stands you up");
    }

    #[test]
    fn reserve_banks_a_snap_shot() {
        let mut b = open_field(duelists(), 2);
        b.units[1].tu = 0; // the imp may not interrupt this drill with reactions
        b.perform(Action::SetReserve { unit: UnitId(0), mode: Some(FireMode::Snap) }).unwrap();
        // A long march: the soldier must stop while a snap shot (13 TU) remains.
        b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(11, 11, 0) }).unwrap();
        let tu = b.unit(UnitId(0)).tu;
        assert!(tu >= 13, "reserved TU spent on walking: {tu} left");
        assert!(tu < 30, "but the unit did march: {tu} left");
    }

    #[test]
    fn bile_arcs_over_walls() {
        let mut units = duelists();
        units[1] = Unit::bile_wisp(1, "Wisp", IVec3::new(7, 5, 0));
        let mut b = walled_field(units, 13); // wall at x=5 between them
        assert!(!b.can_see(UnitId(0), UnitId(1)), "wall hides the wisp");
        b.units[0].tile = IVec3::new(3, 5, 0); // within arc range (4 tiles)
        b.perform(Action::EndTurn).unwrap();

        let events = b
            .perform(Action::Fire { unit: UnitId(1), target: UnitId(0), mode: FireMode::Snap })
            .unwrap();
        assert!(
            events.iter().any(|e| matches!(e, Event::Fired { .. })),
            "the glob is lobbed blind over the wall: {events:?}"
        );
    }

    #[test]
    fn the_taking_and_the_hatching() {
        let mut units = vec![
            Unit::soldier(0, "Victim", IVec3::new(5, 5, 0)),
            Unit::soldier(1, "Witness", IVec3::new(1, 1, 0)),
            Unit::taker(2, "The Taker", IVec3::new(6, 5, 0)),
        ];
        units[0].health = 5; // one strike will do it
        units[0].health_max = 5;
        let mut b = open_field(units, 4);
        b.perform(Action::EndTurn).unwrap();

        // Strike until the victim falls (power 90 vs 5 hp: any hit kills).
        let mut taken = false;
        for _ in 0..10 {
            if b.unit(UnitId(2)).tu < 70 / 4 {
                b.perform(Action::EndTurn).unwrap();
                b.perform(Action::EndTurn).unwrap();
            }
            let events = b
                .perform(Action::Fire { unit: UnitId(2), target: UnitId(0), mode: FireMode::Snap })
                .unwrap();
            if events.iter().any(|e| matches!(e, Event::Taken { unit } if *unit == UnitId(0))) {
                taken = true;
                break;
            }
        }
        assert!(taken, "the Taker's kill must Take");
        let husk = b.unit(UnitId(0));
        assert_eq!(husk.side, Side::Demons, "the body stands up on the other side");
        assert_eq!(husk.species, Species::Husk);
        assert!(husk.alive);

        // Now destroy the Husk — cleanly, so there is something left to
        // hatch from (overkill gibbing forecloses the hatching, by design).
        let unit_count = b.units.len();
        let mut events = Vec::new();
        let clean_kill = b.unit(UnitId(0)).health + 5;
        b.apply_damage(UnitId(0), clean_kill, None, &mut events);
        assert!(events.iter().any(|e| matches!(e, Event::Hatched { .. })), "{events:?}");
        assert_eq!(b.units.len(), unit_count + 1, "a fresh Taker joins the field");
        assert_eq!(b.units.last().unwrap().species, Species::Taker);
    }

    #[test]
    fn armor_is_directional() {
        let mut units = duelists();
        units[1].armor_front = 10;
        units[1].armor_rear = 0;
        units[1].facing = IVec3::new(-1, 0, 0); // facing the soldier
        let mut b = open_field(units, 3);
        // Shot from the front: soaked by 10.
        let front = b.unit(UnitId(1)).armor_against(IVec3::new(1, 5, 0));
        assert_eq!(front, 10);
        // Sneak around behind: nothing.
        b.units[1].facing = IVec3::new(1, 0, 0);
        let rear = b.unit(UnitId(1)).armor_against(IVec3::new(1, 5, 0));
        assert_eq!(rear, 0);
    }

    #[test]
    fn movement_sets_facing_and_reactions_need_the_arc() {
        let mut b = open_field(duelists(), 3);
        b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(3, 5, 0) }).unwrap();
        assert_eq!(b.unit(UnitId(0)).facing, IVec3::new(1, 0, 0));

        // An imp staring away cannot react, no matter its readiness.
        let mut units = duelists();
        units[0].reactions = 10;
        units[1].reactions = 90;
        units[1].facing = IVec3::new(1, 0, 0); // looking east, away from the soldier
        let mut b = open_field(units, 7);
        b.units[0].tu = 20;
        let events = b
            .perform(Action::Move { unit: UnitId(0), to: IVec3::new(4, 5, 0) })
            .unwrap();
        assert!(
            !events.iter().any(|e| matches!(e, Event::Fired { reaction: true, .. })),
            "no reaction fire outside the watch arc: {events:?}"
        );
    }

    #[test]
    fn turning_costs_tu_by_octant() {
        let mut b = open_field(duelists(), 3);
        b.units[0].facing = IVec3::new(1, 0, 0);
        b.perform(Action::Turn { unit: UnitId(0), toward: IVec3::new(-1, 0, 0) }).unwrap();
        assert_eq!(b.unit(UnitId(0)).tu, 55 - 4, "an about-face is four octants");
        assert_eq!(b.unit(UnitId(0)).facing, IVec3::new(-1, 0, 0));
    }

    #[test]
    fn crippled_legs_slow_and_crippled_arms_spoil_aim() {
        let mut b = open_field(duelists(), 3);
        let clean = b.unit(UnitId(0)).hit_chance(FireMode::Snap).unwrap();
        b.units[0].injuries.push(crate::body::BodyPart::RightArm);
        let hurt = b.unit(UnitId(0)).hit_chance(FireMode::Snap).unwrap();
        assert!(hurt < clean, "{hurt} vs {clean}");

        b.units[0].injuries.push(crate::body::BodyPart::LeftLeg);
        let tu_before = b.unit(UnitId(0)).tu;
        b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(2, 5, 0) }).unwrap();
        assert_eq!(tu_before - b.unit(UnitId(0)).tu, 8, "crippled legs double the step");
    }

    #[test]
    fn suppression_rattles_the_aim_then_fades() {
        let mut b = open_field(duelists(), 3);
        let calm = b.unit(UnitId(1)).hit_chance(FireMode::Snap).unwrap();
        b.units[1].suppression = 6;
        let rattled = b.unit(UnitId(1)).hit_chance(FireMode::Snap).unwrap();
        assert!(rattled < calm);
        // Their own turn start steadies them.
        b.perform(Action::EndTurn).unwrap();
        assert_eq!(b.unit(UnitId(1)).suppression, 0);
    }

    #[test]
    fn dropped_charges_detonate_on_schedule() {
        let mut b = open_field(duelists(), 3);
        let events = b
            .perform(Action::DropCharge { unit: UnitId(0), timer: 2 })
            .unwrap();
        assert!(matches!(events[0], Event::ChargeDropped { timer: 2, .. }));
        assert_eq!(b.unit(UnitId(0)).grenades, 1);

        let ev1 = b.perform(Action::EndTurn).unwrap(); // half-turn 1
        assert!(!ev1.iter().any(|e| matches!(e, Event::Exploded { .. })));
        let ev2 = b.perform(Action::EndTurn).unwrap(); // boom
        assert!(
            ev2.iter().any(|e| matches!(e, Event::Exploded { .. })),
            "{ev2:?}"
        );
        assert!(b.charges.is_empty());
    }

    #[test]
    fn bad_throws_scatter_but_stay_close() {
        // Low accuracy forces frequent scatter; landings stay within 2 tiles.
        let mut worst_miss = 0;
        for seed in 0..12 {
            let mut units = duelists();
            units[0].accuracy = 1;
            let mut b = open_field(units, 100 + seed);
            let target = IVec3::new(6, 6, 0);
            let events = b.perform(Action::Throw { unit: UnitId(0), at: target }).unwrap();
            if let Some(Event::Threw { at, .. }) = events.first() {
                let d = (*at - target).abs();
                worst_miss = worst_miss.max(d.x.max(d.y));
            }
        }
        assert!(worst_miss > 0, "a 1-accuracy thrower must fumble sometimes");
        assert!(worst_miss <= 2, "scatter is bounded: {worst_miss}");
    }

    #[test]
    fn smoke_blinds_and_fades() {
        let mut b = open_field(duelists(), 3);
        assert!(b.can_see(UnitId(0), UnitId(1)));
        let events = b
            .perform(Action::ThrowSmoke { unit: UnitId(0), at: IVec3::new(5, 5, 0) })
            .unwrap();
        assert!(matches!(events[0], Event::SmokePopped { .. }));
        assert!(!b.can_see(UnitId(0), UnitId(1)), "smoke between them blinds both");
        // Smoke thins over half-turns.
        for _ in 0..12 {
            b.perform(Action::EndTurn).unwrap();
        }
        assert!(b.clouds.is_empty(), "smoke cannot last forever");
        assert!(b.can_see(UnitId(0), UnitId(1)));
    }

    #[test]
    fn explosions_start_fires_that_burn() {
        let mut b = open_field(duelists(), 6);
        b.perform(Action::Throw { unit: UnitId(0), at: IVec3::new(6, 6, 0) })
            .unwrap();
        assert!(
            b.clouds.iter().any(|(_, k, _)| *k == CloudKind::Fire),
            "the blast site must burn"
        );
        // Park the imp in the flames and let its turn start.
        let fire_tile = b
            .clouds
            .iter()
            .find(|(_, k, _)| *k == CloudKind::Fire)
            .map(|(t, _, _)| *t)
            .unwrap();
        b.units[1].tile = fire_tile;
        let hp = b.unit(UnitId(1)).health;
        let events = b.perform(Action::EndTurn).unwrap();
        assert!(
            events.iter().any(|e| matches!(e, Event::Burned { unit, .. } if *unit == UnitId(1)))
                || b.unit(UnitId(1)).health < hp
                || !b.unit(UnitId(1)).alive,
            "standing in fire hurts: {events:?}"
        );
    }

    #[test]
    fn possession_turns_a_rifle_on_its_own_squad() {
        let mut units = vec![
            Unit::soldier(0, "Puppet", IVec3::new(2, 5, 0)),
            Unit::soldier(1, "Victim", IVec3::new(3, 5, 0)),
            Unit::prince(2, "Prince", IVec3::new(10, 5, 0)),
        ];
        units[0].bravery = 5;
        units[0].morale = 20; // an easy mind to take
        let mut b = open_field(units, 3);
        b.perform(Action::EndTurn).unwrap();

        let mut possessed = false;
        for _ in 0..20 {
            if b.unit(UnitId(2)).tu < 55 * POSSESS_COST_PCT / 100 {
                b.perform(Action::EndTurn).unwrap();
                if b.winner.is_some() {
                    break;
                }
                b.perform(Action::EndTurn).unwrap();
            }
            let events = b
                .perform(Action::Possess { unit: UnitId(2), target: UnitId(0) })
                .unwrap();
            if events.iter().any(|e| matches!(e, Event::Possessed { .. })) {
                possessed = true;
                break;
            }
        }
        assert!(possessed, "a broken mind cannot resist forever");
        // The puppet may now fire on its own side — during the demon turn.
        let result = b.perform(Action::Fire {
            unit: UnitId(0),
            target: UnitId(1),
            mode: FireMode::Snap,
        });
        assert!(result.is_ok(), "the possessed turn on their own: {result:?}");
        // And is not the Order's to command once play returns.
        b.perform(Action::EndTurn).unwrap();
        if b.unit(UnitId(0)).possessed > 0 {
            assert_eq!(
                b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(1, 1, 0) }),
                Err(ActionError::NotYourTurn)
            );
        }
    }

    #[test]
    fn civilians_do_not_hold_the_field() {
        let mut units = duelists();
        units.push(Unit::civilian(2, "Berta", IVec3::new(3, 3, 0)));
        let mut b = open_field(units, 3);
        let mut events = Vec::new();
        b.apply_damage(UnitId(0), 999, None, &mut events);
        assert_eq!(
            b.winner,
            Some(Side::Demons),
            "with the soldier dead, a cowering civilian holds nothing"
        );
        assert!(b.unit(UnitId(2)).alive, "but she may yet live");
    }

    #[test]
    fn the_wounded_are_carried_home() {
        let mut units = vec![
            Unit::soldier(0, "Carrier", IVec3::new(2, 5, 0)),
            Unit::soldier(1, "Down", IVec3::new(3, 5, 0)),
            Unit::imp(2, "Imp", IVec3::new(10, 5, 0)),
        ];
        units[1].conscious = false; // already out cold
        let mut b = open_field(units, 3);
        b.perform(Action::PickUp { unit: UnitId(0), target: UnitId(1) }).unwrap();
        b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(5, 7, 0) }).unwrap();
        assert_eq!(
            b.unit(UnitId(1)).tile,
            b.unit(UnitId(0)).tile,
            "the burden rides along"
        );
        b.perform(Action::PutDown { unit: UnitId(0), at: IVec3::new(5, 8, 0) }).unwrap();
        assert_eq!(b.unit(UnitId(1)).tile, IVec3::new(5, 8, 0));
        assert!(b.unit(UnitId(0)).carrying.is_none());
    }

    #[test]
    fn high_ground_and_falls() {
        // Elevation bonus is arithmetic on the chance; test the fall.
        let mut units = duelists();
        units[0].tile = IVec3::new(5, 5, 1); // somehow upstairs in a flat field
        let mut b = open_field(units, 3);
        let mut events = Vec::new();
        b.settle_units_for_test(&mut events);
        assert!(
            events.iter().any(|e| matches!(e, Event::Fell { unit, .. } if *unit == UnitId(0))),
            "no floor upstairs in an open field: {events:?}"
        );
        assert_eq!(b.unit(UnitId(0)).tile.z, 0);
    }

    #[test]
    fn identical_seeds_replay_identically() {
        let script = |b: &mut Battle| -> Vec<Event> {
            let mut log = Vec::new();
            log.extend(
                b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(5, 5, 0) })
                    .unwrap(),
            );
            for _ in 0..3 {
                if b.winner.is_none() && b.unit(UnitId(0)).tu >= 13 {
                    log.extend(
                        b.perform(Action::Fire {
                            unit: UnitId(0),
                            target: UnitId(1),
                            mode: FireMode::Snap,
                        })
                        .unwrap(),
                    );
                }
            }
            if b.winner.is_none() {
                log.extend(b.perform(Action::EndTurn).unwrap());
            }
            log
        };
        let mut a = open_field(duelists(), 99);
        let mut b = open_field(duelists(), 99);
        assert_eq!(script(&mut a), script(&mut b));
    }

    #[test]
    fn weapon_sanity() {
        let r = rifle();
        assert!(r.aimed_acc > r.snap_acc);
        assert!(r.aimed_cost_pct > r.snap_cost_pct);
        let s = Unit::soldier(0, "X", IVec3::ZERO);
        assert_eq!(s.hit_chance(FireMode::Snap), Some(36));
        assert_eq!(s.hit_chance(FireMode::Aimed), Some(66));
        assert_eq!(s.hit_chance(FireMode::Auto), Some(21));
        assert_eq!(s.fire_cost(FireMode::Auto), Some(19));
        // Imps cannot burst-fire.
        let i = Unit::imp(1, "Y", IVec3::ZERO);
        assert_eq!(i.fire_cost(FireMode::Auto), None);
    }

    #[test]
    fn crippled_parts_hit_again_come_off() {
        let mut b = open_field(duelists(), 7);
        b.units[1].injuries.push(crate::body::BodyPart::RightArm);
        let morale_before = b.unit(UnitId(1)).morale;
        let mut events = Vec::new();
        b.sever_part(UnitId(1), crate::body::BodyPart::RightArm, &mut events);
        assert!(matches!(events[0], Event::PartSevered { .. }));
        assert!(b.unit(UnitId(1)).severed.contains(&crate::body::BodyPart::RightArm));
        assert!(b.unit(UnitId(1)).morale < morale_before, "losing a limb shakes anyone");
        assert!(b.unit(UnitId(1)).alive, "an arm is not a life");

        // A severed leg reduces movement to a crawl.
        b.units[1].injuries.push(crate::body::BodyPart::LeftLeg);
        let mut events = Vec::new();
        b.sever_part(UnitId(1), crate::body::BodyPart::LeftLeg, &mut events);
        assert_eq!(b.unit(UnitId(1)).move_cost_mult(), 3);

        // Heads do not grow back.
        b.units[1].injuries.push(crate::body::BodyPart::Head);
        let mut events = Vec::new();
        b.sever_part(UnitId(1), crate::body::BodyPart::Head, &mut events);
        assert!(!b.unit(UnitId(1)).alive, "decapitation is final");
    }

    #[test]
    fn overkill_gibs_and_gibs_leave_no_corpse() {
        let mut units = duelists();
        units.push(Unit::soldier(2, "Witness", IVec3::new(1, 7, 0)));
        let mut b = open_field(units, 9);
        let mut events = Vec::new();
        let obliterate = b.unit(UnitId(1)).health + 50;
        b.apply_damage(UnitId(1), obliterate, None, &mut events);
        assert!(events.iter().any(|e| matches!(e, Event::Gibbed { unit } if *unit == UnitId(1))));
        assert!(!b.unit(UnitId(1)).is_corpse(), "nothing left to recover");
    }

    #[test]
    fn infection_festers_and_turns_the_soldier() {
        let mut units = duelists();
        units.push(Unit::soldier(2, "Anchor", IVec3::new(1, 9, 0)));
        let mut b = open_field(units, 11);
        b.units[0].infected = Some((crate::body::BodyPart::RightArm, 0));
        let mut turned = false;
        for _ in 0..12 {
            if b.winner.is_some() {
                break;
            }
            let events = b.perform(Action::EndTurn).unwrap();
            if events
                .iter()
                .any(|e| matches!(e, Event::InfectionTurned { unit } if *unit == UnitId(0)))
            {
                turned = true;
                break;
            }
        }
        assert!(turned, "untreated rot finishes its work");
        let u = b.unit(UnitId(0));
        assert_eq!(u.side, Side::Demons, "the soldier is a soldier no longer");
        assert_eq!(u.species, Species::Husk);
    }

    #[test]
    fn amputation_beats_the_rot() {
        let mut b = open_field(duelists(), 13);
        b.units[0].infected = Some((crate::body::BodyPart::LeftArm, 2));
        let events = b
            .perform(Action::Amputate { medic: UnitId(0), target: UnitId(0) })
            .unwrap();
        assert!(matches!(events[0], Event::Amputated { .. }));
        assert!(events.iter().any(|e| matches!(e, Event::PartSevered { .. })));
        let u = b.unit(UnitId(0));
        assert!(u.infected.is_none(), "the rot went with the limb");
        assert!(u.alive);
        assert!(u.severed.contains(&crate::body::BodyPart::LeftArm));
        // Nothing left to amputate.
        assert_eq!(
            b.perform(Action::Amputate { medic: UnitId(0), target: UnitId(0) }),
            Err(ActionError::BadTarget)
        );
    }

    #[test]
    fn demons_eat_the_dead() {
        let mut units = duelists();
        units[1].tile = IVec3::new(3, 5, 0); // imp beside the doomed soldier
        units.push(Unit::soldier(2, "Survivor", IVec3::new(1, 9, 0)));
        let mut b = open_field(units, 15);
        // Kill the first soldier cleanly (no gib), then wound the imp.
        let mut events = Vec::new();
        let clean = b.unit(UnitId(0)).health + 2;
        b.units[0].tile = IVec3::new(2, 5, 0);
        b.apply_damage(UnitId(0), clean, None, &mut events);
        assert!(b.unit(UnitId(0)).is_corpse());
        b.units[1].health = 5;

        b.perform(Action::EndTurn).unwrap(); // demons to move
        let events = b
            .perform(Action::Devour { unit: UnitId(1), corpse: UnitId(0) })
            .unwrap();
        assert!(matches!(events[0], Event::CorpseEaten { .. }));
        assert_eq!(b.unit(UnitId(1)).health, 15, "flesh knits");
        assert!(!b.unit(UnitId(0)).is_corpse(), "the body is spent");
        assert_eq!(
            b.perform(Action::Devour { unit: UnitId(1), corpse: UnitId(0) }),
            Err(ActionError::BadTarget)
        );
    }

    #[test]
    fn takers_raise_the_fallen() {
        let mut units = duelists();
        units.push(Unit::taker(2, "The Taker", IVec3::new(3, 5, 0)));
        units.push(Unit::soldier(3, "Survivor", IVec3::new(1, 9, 0)));
        let mut b = open_field(units, 17);
        b.units[0].tile = IVec3::new(2, 5, 0);
        let mut events = Vec::new();
        let clean = b.unit(UnitId(0)).health + 2;
        b.apply_damage(UnitId(0), clean, None, &mut events);
        assert!(b.unit(UnitId(0)).is_corpse());

        b.perform(Action::EndTurn).unwrap();
        let events = b
            .perform(Action::Defile { unit: UnitId(2), corpse: UnitId(0) })
            .unwrap();
        assert!(matches!(events[0], Event::Defiled { .. }));
        let raised = b.unit(UnitId(0));
        assert!(raised.alive);
        assert_eq!(raised.species, Species::Husk);
        assert_eq!(raised.side, Side::Demons, "the dead fight for the other side now");
    }

    #[test]
    fn the_dead_can_be_carried_home() {
        let mut units = duelists();
        units.push(Unit::soldier(2, "Bearer", IVec3::new(2, 6, 0)));
        let mut b = open_field(units, 19);
        b.units[0].tile = IVec3::new(2, 5, 0);
        let mut events = Vec::new();
        let clean = b.unit(UnitId(0)).health + 2;
        b.apply_damage(UnitId(0), clean, None, &mut events);
        assert!(b.unit(UnitId(0)).is_corpse());
        b.perform(Action::PickUp { unit: UnitId(2), target: UnitId(0) }).unwrap();
        assert_eq!(b.unit(UnitId(2)).carrying, Some(UnitId(0)));
    }

    #[test]
    fn wounds_paint_the_ground() {
        // A proper map with a full ground slab (open_field's is too thin).
        let mut b = crate::scenario::incursion(
            3,
            vec![Unit::soldier(0, "S", IVec3::ZERO)],
            2,
            1,
        );
        let tile = b.units[0].tile;
        let mut events = Vec::new();
        b.apply_damage(UnitId(0), 9, None, &mut events);
        let o = tile * TILE_VOXELS;
        let mut stains = 0;
        for y in 0..TILE_VOXELS {
            for x in 0..TILE_VOXELS {
                let p = o + IVec3::new(x, y, crate::scenario::GROUND_TOP - 1);
                if b.world.voxel(p) == crate::scenario::MAT_BLOOD {
                    stains += 1;
                }
            }
        }
        assert!(stains > 0, "serious wounds leave blood on the ground");
    }

    #[test]
    fn summoning_circles_deliver_unless_fouled() {
        // Delivery: an empty circle brings something through.
        let mut units = duelists();
        units[1].tile = IVec3::new(10, 10, 0);
        let mut b = open_field(units, 23);
        b.schedule_summon(IVec3::new(6, 6, 0), 1, 2);
        let before = b.units.len();
        b.perform(Action::EndTurn).unwrap(); // demons: circle resolves
        assert_eq!(b.units.len(), before + 1, "the circle delivers");
        assert_eq!(b.units.last().unwrap().side, Side::Demons);

        // Fouling: a boot on the lines stops the working.
        let mut units = duelists();
        units[0].tile = IVec3::new(6, 6, 0); // soldier stands on the circle
        units[1].tile = IVec3::new(10, 10, 0);
        let mut b = open_field(units, 24);
        b.schedule_summon(IVec3::new(6, 6, 0), 1, 2);
        let before = b.units.len();
        let events = b.perform(Action::EndTurn).unwrap();
        assert!(
            events.iter().any(|e| matches!(e, Event::SummoningDisrupted { .. })),
            "{events:?}"
        );
        assert_eq!(b.units.len(), before, "nothing came through");
    }

    #[test]
    fn wards_burn_the_demon_that_crosses() {
        let mut units = duelists();
        units[1] = Unit::hellhound(1, "Hound", IVec3::new(6, 5, 0));
        let mut b = open_field(units, 25);
        b.place_ward(IVec3::new(5, 5, 0));
        b.perform(Action::EndTurn).unwrap(); // demons to move
        let hp = b.unit(UnitId(1)).health;
        let events = b
            .perform(Action::Move { unit: UnitId(1), to: IVec3::new(4, 5, 0) })
            .unwrap();
        assert!(
            events.iter().any(|e| matches!(e, Event::WardBurned { .. })),
            "{events:?}"
        );
        assert!(b.unit(UnitId(1)).health < hp, "the ward answers");
        assert!(b.wards.is_empty(), "a ward spends itself");
    }

    #[test]
    fn soldiers_inscribe_wards_from_kits() {
        let mut b = open_field(duelists(), 26);
        assert_eq!(b.unit(UnitId(0)).ward_kits, 1);
        let events = b.perform(Action::InscribeWard { unit: UnitId(0) }).unwrap();
        assert!(matches!(events[0], Event::WardInscribed { .. }));
        assert!(b.wards.contains(&b.unit(UnitId(0)).tile));
        assert_eq!(b.unit(UnitId(0)).ward_kits, 0);
        assert_eq!(
            b.perform(Action::InscribeWard { unit: UnitId(0) }),
            Err(ActionError::BadTarget),
            "one kit, one ward"
        );
    }

    #[test]
    fn corruption_creeps_whispers_and_dies_with_the_obelisk() {
        let mut b = crate::scenario::incursion(
            3,
            vec![Unit::soldier(0, "S", IVec3::ZERO), Unit::soldier(1, "T", IVec3::ZERO)],
            1,
            1,
        );
        // Round-trip turns until the veins break ground and spread.
        for _ in 0..8 {
            if b.winner.is_some() {
                return; // some seeds end fast; nothing to prove here
            }
            b.perform(Action::EndTurn).unwrap();
        }
        assert!(!b.corruption.is_empty(), "the obelisk veins the ground");

        // A soldier standing on a vein hears it.
        let vein = b.corruption[0];
        b.units[0].tile = vein;
        let morale = b.unit(UnitId(0)).morale;
        // Advance to the next Order turn start.
        if b.side_to_move == Side::Order {
            b.perform(Action::EndTurn).unwrap();
        }
        let events = b.perform(Action::EndTurn).unwrap();
        assert!(
            events.iter().any(|e| matches!(e, Event::Whispered { unit } if *unit == UnitId(0))),
            "{events:?}"
        );
        assert!(b.unit(UnitId(0)).morale < morale);
    }

    #[test]
    fn the_obelisk_wears_burning_runes() {
        let b = crate::scenario::incursion(3, vec![Unit::soldier(0, "S", IVec3::ZERO)], 0, 1);
        let mut runes = 0;
        for z in 4..24 {
            for y in 11 * TILE_VOXELS..13 * TILE_VOXELS {
                for x in 22 * TILE_VOXELS..23 * TILE_VOXELS {
                    if b.world.voxel(IVec3::new(x, y, z)) == crate::scenario::MAT_SIGIL {
                        runes += 1;
                    }
                }
            }
        }
        assert!(runes > 20, "the obelisk is written on: {runes}");
    }

    #[test]
    fn atrocities_horrify_their_discoverer() {
        let mut b = open_field(duelists(), 27);
        b.register_atrocity(IVec3::new(4, 5, 0));
        let morale = b.unit(UnitId(0)).morale;
        let events = b
            .perform(Action::Move { unit: UnitId(0), to: IVec3::new(3, 5, 0) })
            .unwrap();
        assert!(
            events.iter().any(|e| matches!(e, Event::AtrocityFound { .. })),
            "{events:?}"
        );
        assert!(b.unit(UnitId(0)).morale < morale);
        assert_eq!(b.unit(UnitId(0)).horror, 1, "seeing it marks you");
        // Walking past again finds nothing new.
        let events = b
            .perform(Action::Move { unit: UnitId(0), to: IVec3::new(4, 6, 0) })
            .unwrap();
        assert!(!events.iter().any(|e| matches!(e, Event::AtrocityFound { .. })));
    }

    #[test]
    fn gibs_mark_the_witnesses() {
        let mut units = duelists();
        units.push(Unit::soldier(2, "Witness", IVec3::new(1, 7, 0)));
        let mut b = open_field(units, 29);
        let mut events = Vec::new();
        let obliterate = b.unit(UnitId(0)).health + 50;
        b.apply_damage(UnitId(0), obliterate, None, &mut events);
        assert!(b.unit(UnitId(2)).horror > 0, "the witness carries it home");
    }

    #[test]
    fn the_censer_burns_a_cone_and_the_mortar_stuns() {
        // Censer: fire clouds walk the line to the target.
        let mut units = duelists();
        units[0].weapon = crate::units::Weapon::from_data("censer", "censer");
        units[1].tile = IVec3::new(4, 5, 0);
        let mut b = open_field(units, 31);
        let events = b
            .perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap })
            .unwrap();
        assert!(
            events.iter().any(|e| matches!(e, Event::FireStarted { .. })),
            "{events:?}"
        );
        assert!(!b.clouds.is_empty(), "the ground burns");

        // Mortar: stun, not blood.
        let mut units = duelists();
        units[0].weapon = crate::units::Weapon::from_data("salt-shot mortar", "salt_mortar");
        units[0].accuracy = 95;
        units[1].tile = IVec3::new(7, 5, 0); // inside arcing range
        let mut b = open_field(units, 32);
        let hp = b.unit(UnitId(1)).health;
        for _ in 0..8 {
            if b.unit(UnitId(1)).stun > 0 || !b.unit(UnitId(1)).conscious {
                break;
            }
            if b.unit(UnitId(0)).fire_cost(FireMode::Snap).is_some_and(|c| b.unit(UnitId(0)).tu < c) {
                b.perform(Action::EndTurn).unwrap();
                b.perform(Action::EndTurn).unwrap();
            }
            b.perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap })
                .unwrap();
        }
        assert!(b.unit(UnitId(1)).stun > 0, "salt trauma lands");
        assert_eq!(b.unit(UnitId(1)).health, hp, "and draws no blood");
    }

    #[test]
    fn blades_riposte_and_circlets_shatter() {
        // A hound bites a bladed soldier and gets answered.
        let mut units = duelists();
        units[0].blade = true;
        units[0].accuracy = 95;
        units[1] = Unit::hellhound(1, "Hound", IVec3::new(2, 5, 0));
        let mut b = open_field(units, 33);
        b.perform(Action::EndTurn).unwrap(); // demons to move
        let events = b
            .perform(Action::Fire { unit: UnitId(1), target: UnitId(0), mode: FireMode::Snap })
            .unwrap();
        assert!(
            events.iter().any(|e| matches!(e, Event::Riposte { unit, .. } if *unit == UnitId(0))),
            "{events:?}"
        );

        // A circlet eats one Terrify and dies of it.
        let mut units = duelists();
        units[0].circlet = true;
        units[1] = Unit::overseer(1, "Overseer", IVec3::new(6, 5, 0));
        let mut b = open_field(units, 34);
        b.perform(Action::EndTurn).unwrap();
        let events = b
            .perform(Action::Terrify { unit: UnitId(1), target: UnitId(0) })
            .unwrap();
        assert!(matches!(events[0], Event::CircletShattered { unit: UnitId(0) }));
        assert!(!b.unit(UnitId(0)).circlet, "one blow, one circlet");
    }

    #[test]
    fn silent_weapons_leave_no_noise() {
        let mut units = duelists();
        units[0].weapon = crate::units::Weapon::from_data("consecrated arbalest", "arbalest");
        let mut b = open_field(units, 35);
        b.perform(Action::Fire { unit: UnitId(0), target: UnitId(1), mode: FireMode::Snap })
            .unwrap();
        assert!(b.last_noise.is_none(), "the arbalest tells nobody anything");
    }

    #[test]
    fn evacuation_wins_by_walking_out_and_loses_to_the_clock() {
        use crate::scenario::MissionSpec;
        let squad = vec![Unit::soldier(0, "S", IVec3::ZERO)];
        let mut b = crate::scenario::incursion_mission(
            5, squad, 1, 1, 2, crate::scenario::Biome::Temperate, MissionSpec::Evacuate,
        );
        assert!(matches!(b.rule, MissionRule::Evacuate { needed: 1, .. }));
        // Hand-walk a civilian to the west edge.
        let civ = b.units.iter().find(|u| u.civilian).map(|u| u.id).unwrap();
        b.units[civ.0 as usize].tile = IVec3::new(4, 11, 0);
        b.units[civ.0 as usize].tu = 99;
        let events = b
            .perform(Action::Move { unit: civ, to: IVec3::new(2, 11, 0) })
            .unwrap();
        assert!(events.iter().any(|e| matches!(e, Event::Evacuated { .. })), "{events:?}");
        assert_eq!(b.winner, Some(Side::Order), "the saved ARE the victory");

        // And the clock kills: a fresh evacuation left to rot times out.
        let squad = vec![Unit::soldier(0, "S", IVec3::ZERO)];
        let mut b = crate::scenario::incursion_mission(
            5, squad, 1, 1, 2, crate::scenario::Biome::Temperate, MissionSpec::Evacuate,
        );
        b.turn = 15; // past the limit
        let events = b.perform(Action::EndTurn).unwrap();
        let events2 = if b.winner.is_none() {
            b.perform(Action::EndTurn).unwrap()
        } else {
            events.clone()
        };
        assert!(
            events.iter().chain(events2.iter()).any(|e| matches!(e, Event::TimeExpired)),
            "{events:?} {events2:?}"
        );
        assert_eq!(b.winner, Some(Side::Demons));
    }

    #[test]
    fn snatch_wins_on_the_subdue_and_dies_with_the_mark() {
        use crate::scenario::MissionSpec;
        let squad = vec![Unit::soldier(0, "S", IVec3::ZERO)];
        let mut b = crate::scenario::incursion_mission(
            7, squad, 3, 2, 0, crate::scenario::Biome::Temperate, MissionSpec::Snatch,
        );
        let MissionRule::Snatch { target } = b.rule else { panic!("snatch rule") };
        assert_eq!(b.unit(target).species, Species::Overseer, "the mark leads the pack");

        // Subdue the mark: instant win.
        {
            let t = b.unit_mut(target);
            t.stun = t.health + 5;
            t.conscious = false;
        }
        let mut events = Vec::new();
        b.check_victory(&mut events);
        assert_eq!(b.winner, Some(Side::Order));

        // Fresh field: kill the mark instead — mission dead.
        let squad = vec![Unit::soldier(0, "S", IVec3::ZERO)];
        let mut b = crate::scenario::incursion_mission(
            7, squad, 3, 2, 0, crate::scenario::Biome::Temperate, MissionSpec::Snatch,
        );
        let MissionRule::Snatch { target } = b.rule else { panic!() };
        let mut events = Vec::new();
        let overkill = b.unit(target).health + 2;
        b.apply_damage(target, overkill, None, &mut events);
        assert_eq!(b.winner, Some(Side::Demons), "{events:?}");
    }

    #[test]
    fn the_chapel_loft_stands_and_collapses() {
        // The loft is reachable: stairwell ramp at (10,9), floor above.
        let b = crate::scenario::incursion(3, vec![Unit::soldier(0, "S", IVec3::ZERO)], 0, 1);
        assert!(b.tiles.is_ramp(IVec3::new(10, 9, 0)), "the stair climbs");
        assert!(b.tiles.is_walkable(IVec3::new(12, 11, 1)), "the loft floor holds");

        // Shoot the slab out from under it and it comes down.
        let mut b = b;
        let o = IVec3::new(12 * TILE_VOXELS, 11 * TILE_VOXELS, TILE_VOXELS);
        // Leave a few scraps so the collapse rule (not clean demolition) fires.
        b.world.fill_box(
            o + IVec3::new(0, 0, 0),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS - 1, crate::scenario::GROUND_TOP),
            ods_voxel::Voxel::EMPTY,
        );
        b.tiles.rederive_region(
            &b.world,
            o - IVec3::new(16, 16, 16),
            o + IVec3::splat(2 * TILE_VOXELS),
        );
        let mut events = Vec::new();
        b.check_collapse(&mut events);
        assert!(
            events.iter().any(|e| matches!(e, Event::FloorCollapsed { .. })),
            "{events:?}"
        );
        assert!(!b.tiles.is_walkable(IVec3::new(12, 11, 1)), "the loft is gone");
    }

    #[test]
    fn weather_changes_the_field() {
        // Snowfall: steps cost more, demons leave tracks.
        let mut b = open_field(duelists(), 41);
        b.weather = Weather::Snowfall;
        let tu = b.unit(UnitId(0)).tu;
        b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(2, 5, 0) }).unwrap();
        assert_eq!(tu - b.unit(UnitId(0)).tu, 5, "4 + 1 for the drifts");

        // Rain: fire dies twice as fast.
        let mut b = open_field(duelists(), 42);
        b.weather = Weather::Rain;
        b.add_cloud_for_test(IVec3::new(5, 5, 0), CloudKind::Fire, 4);
        b.perform(Action::EndTurn).unwrap();
        let ttl = b
            .clouds
            .iter()
            .find(|(_, k, _)| *k == CloudKind::Fire)
            .map(|(_, _, t)| *t);
        assert_eq!(ttl, Some(2), "rain drowns fire double-time");
    }

    #[test]
    fn quiet_boots_raise_no_alarm() {
        let b = open_field(duelists(), 43);
        // Standing tall: the step is heard (no demon can see tile 1,5 area? the
        // imp CAN see across the open field, so first check the unseen case
        // behind a wall).
        let mut units = duelists();
        units[1].tile = IVec3::new(10, 10, 0);
        let mut b2 = walled_field(units, 43);
        b2.perform(Action::Move { unit: UnitId(0), to: IVec3::new(2, 5, 0) }).unwrap();
        assert!(!b2.alarm.is_empty(), "loud boots carry through walls");

        let mut units = duelists();
        units[1].tile = IVec3::new(10, 10, 0);
        let mut b3 = walled_field(units, 44);
        b3.perform(Action::Kneel { unit: UnitId(0) }).unwrap();
        b3.perform(Action::Move { unit: UnitId(0), to: IVec3::new(2, 5, 0) }).unwrap();
        assert!(b3.alarm.is_empty(), "crouched movement is silent");
        let _ = b;
    }

    #[test]
    fn ghost_intel_remembers_lost_sight_and_forgets_the_dead() {
        let mut b = open_field(duelists(), 50);
        // Any action stamps the sighting: the imp stands in plain view.
        b.perform(Action::Kneel { unit: UnitId(0) }).unwrap();
        assert_eq!(
            b.last_known.get(&UnitId(1)),
            Some(&IVec3::new(10, 5, 0)),
            "a demon in view is on the intel map"
        );

        // The imp slips away unseen (out of sight range in the far corner);
        // the ghost stays where it was LAST seen.
        b.units[1].tile = IVec3::new(11, 11, 0);
        b.vision_tiles = 3; // and the night closes in
        b.perform(Action::Kneel { unit: UnitId(0) }).unwrap();
        assert_eq!(
            b.last_known.get(&UnitId(1)),
            Some(&IVec3::new(10, 5, 0)),
            "the ghost marks the last sighting, not the truth"
        );

        // Death clears the slate.
        b.units[1].alive = false;
        b.perform(Action::Kneel { unit: UnitId(0) }).unwrap();
        assert!(b.last_known.is_empty(), "the dead leave the intel map");
    }

    #[test]
    fn flares_light_the_dark() {
        let mut b = open_field(duelists(), 60);
        b.vision_tiles = 3; // deep night: the imp at (10,5) is invisible
        assert!(!b.can_see(UnitId(0), UnitId(1)), "the dark hides it");

        let events = b
            .perform(Action::ThrowFlare { unit: UnitId(0), at: IVec3::new(9, 5, 0) })
            .unwrap();
        assert!(events.iter().any(|e| matches!(e, Event::FlareThrown { .. })));
        assert!(b.can_see(UnitId(0), UnitId(1)), "lit ground hides nothing");
        assert_eq!(b.unit(UnitId(0)).flares, 1, "the flare is spent");

        // And the range is honest (Vasquez stands at x=1: 11 tiles east).
        assert_eq!(
            b.perform(Action::ThrowFlare { unit: UnitId(0), at: IVec3::new(12, 5, 0) }),
            Err(ActionError::OutOfRange)
        );
    }

    #[test]
    fn fire_eats_the_fuel_it_stands_on() {
        let mut b = open_field(duelists(), 61);
        // A hedge: a block of foliage on tile (5, 8).
        let o = IVec3::new(5, 8, 0) * TILE_VOXELS;
        b.world.fill_box(
            o + IVec3::new(0, 0, crate::scenario::GROUND_TOP),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, crate::scenario::GROUND_TOP + 10 * VS),
            crate::scenario::MAT_FOLIAGE,
        );
        let count_foliage = |b: &Battle| -> usize {
            let mut n = 0;
            for z in 0..TILE_VOXELS {
                for y in 0..TILE_VOXELS {
                    for x in 0..TILE_VOXELS {
                        if b.world.voxel(o + IVec3::new(x, y, z))
                            == crate::scenario::MAT_FOLIAGE
                        {
                            n += 1;
                        }
                    }
                }
            }
            n
        };
        let before = count_foliage(&b);
        assert!(before > 0);
        b.add_cloud_for_test(IVec3::new(5, 8, 0), CloudKind::Fire, 8);
        // Two full rounds of burning.
        for _ in 0..4 {
            if b.winner.is_some() {
                break;
            }
            let _ = b.perform(Action::EndTurn);
        }
        assert!(
            count_foliage(&b) < before,
            "the hedge chars: {} -> {}",
            before,
            count_foliage(&b)
        );
    }

    #[test]
    fn stamina_drains_regens_and_empty_lungs_hobble() {
        let mut b = open_field(duelists(), 70);
        let start = b.unit(UnitId(0)).stamina;
        b.perform(Action::Move { unit: UnitId(0), to: IVec3::new(6, 5, 0) }).unwrap();
        assert_eq!(
            b.unit(UnitId(0)).stamina,
            start - 5,
            "five tiles cost five breath"
        );
        assert_eq!(
            b.xp[0].tiles_moved, 5,
            "the legs remember the distance"
        );

        // Empty lungs double the step.
        assert_eq!(b.unit(UnitId(0)).move_cost_mult(), 1);
        b.units[0].stamina = 0;
        assert_eq!(b.unit(UnitId(0)).move_cost_mult(), 2, "winded walkers pay double");

        // A full round returns a third of the tank.
        b.perform(Action::EndTurn).unwrap(); // Order -> Demons (demons breathe)
        b.perform(Action::EndTurn).unwrap(); // Demons -> Order (we breathe)
        assert_eq!(
            b.unit(UnitId(0)).stamina,
            b.unit(UnitId(0)).stamina_max / 3,
            "rest refills a third"
        );
    }

    #[test]
    fn melee_skill_not_firing_skill_drives_the_blade() {
        let mut b = open_field(duelists(), 71);
        // Give the imp fangs: a melee weapon. Its firing accuracy
        // becomes irrelevant; its melee skill decides.
        b.units[1].weapon = crate::units::Weapon::from_data("fangs", "fangs");
        b.units[1].accuracy = 5;
        b.units[1].melee = 90;
        let fanged = b.unit(UnitId(1)).hit_chance(FireMode::Snap).unwrap();
        b.units[1].melee = 20;
        let clumsy = b.unit(UnitId(1)).hit_chance(FireMode::Snap).unwrap();
        assert!(
            fanged > clumsy && fanged > 50,
            "melee weapons roll on melee skill: {fanged} vs {clumsy}"
        );
        // The rifle still answers to firing accuracy.
        b.units[0].melee = 5;
        b.units[0].accuracy = 80;
        assert!(b.unit(UnitId(0)).hit_chance(FireMode::Snap).unwrap() > 20);
    }

    #[test]
    fn the_forecast_matches_what_the_resolver_rolls() {
        let mut b = open_field(duelists(), 51);
        let flat = b.forecast_shot(UnitId(0), UnitId(1), FireMode::Snap).unwrap();
        assert_eq!(flat.chance, b.unit(UnitId(0)).hit_chance(FireMode::Snap).unwrap());
        assert_eq!(flat.cost, b.unit(UnitId(0)).fire_cost(FireMode::Snap).unwrap());
        assert!(flat.seen, "an open field hides nothing");
        assert!(!flat.stun);

        // High ground steadies the forecast exactly as it steadies the shot.
        b.units[0].tile.z = 1;
        let high = b.forecast_shot(UnitId(0), UnitId(1), FireMode::Snap).unwrap();
        assert_eq!(high.chance, (flat.chance + 10).min(95));

        // A weapon without the mode gives no forecast at all.
        assert!(b.forecast_shot(UnitId(1), UnitId(0), FireMode::Auto).is_none());
    }
}

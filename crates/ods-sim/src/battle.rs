//! The Battlescape state machine: actions in, events out, deterministic
//! given the seed. Nothing here renders; nothing here reads the clock.

use std::collections::HashSet;

use glam::{IVec3, Vec3};
use ods_voxel::VoxelWorld;

use crate::body::BodyPart;
use crate::tiles::{TileMap, step_cost};
use crate::units::{FireMode, Side, Species, Unit, UnitId};
use crate::{SimRng, TILE_VOXELS};

/// Vision range in tiles (Chebyshev). The Otherside fights at night.
pub const VISION_TILES: i32 = 14;

/// Eye and chest heights in voxels above a tile's minimum corner (assumes
/// floors sit in the tile's lower quarter, which map generation guarantees).
const EYE_Z: f32 = 13.0;
const CHEST_Z: f32 = 9.0;

/// Hellfire charge (grenade) parameters.
pub const GRENADE_POWER: i32 = 40;
pub const GRENADE_RANGE_TILES: i32 = 10;
pub const GRENADE_COST_PCT: i32 = 45;
pub const GRENADE_CARVE_RADIUS: f32 = 7.0;
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
    /// The rift obelisk is demolished; the incursion collapses.
    ObjectiveDestroyed,
    /// A body part is crippled by a heavy hit.
    PartCrippled { unit: UnitId, part: BodyPart },
    Turned { unit: UnitId, facing: IVec3 },
    /// A primed charge hits the ground, fuse hissing.
    ChargeDropped { at: IVec3, timer: u32 },
    SmokePopped { at: IVec3 },
    FireStarted { at: IVec3 },
    Burned { unit: UnitId, amount: i32 },
    DoorOpened { at: IVec3 },
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
    /// Reserve enough TUs for a snap shot while moving.
    SetReserve { unit: UnitId, on: bool },
    /// Strike an adjacent enemy with a binding rod: stun, not blood.
    Bind { unit: UnitId, target: UnitId },
    /// Psi assault (Overseers and worse): batters morale through walls.
    Terrify { unit: UnitId, target: UnitId },
    /// Face a direction (1 TU per 45°) — sets the reaction-fire arc.
    Turn { unit: UnitId, toward: IVec3 },
    /// Prime a charge and drop it at your feet; it detonates after `timer`
    /// half-turns. Then run.
    DropCharge { unit: UnitId, timer: u32 },
    /// Pop a smoke grenade at a tile: sight-blocking cover for a few turns.
    ThrowSmoke { unit: UnitId, at: IVec3 },
    /// Open an adjacent closed door (6 TU).
    OpenDoor { unit: UnitId, at: IVec3 },
    /// Seize an enemy mind outright (Princes): it acts for you next turn.
    Possess { unit: UnitId, target: UnitId },
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
    pub shots_hit: u32,
    pub reaction_shots: u32,
    pub kills: u32,
    /// Times the unit broke (panic or berserk) and lived through it.
    pub dread_survived: u32,
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
    xp: Vec<Experience>,
    rng: SimRng,
}

impl Battle {
    pub fn new(
        world: VoxelWorld,
        tile_min: IVec3,
        tile_size: IVec3,
        units: Vec<Unit>,
        seed: u64,
    ) -> Self {
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
            xp,
            rng: SimRng::from_seed(seed),
        }
    }

    pub fn experience(&self, id: UnitId) -> Experience {
        self.xp[id.0 as usize]
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
    pub fn check_objective_for_test(&mut self, events: &mut Vec<Event>) {
        self.check_objective(events);
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
        (tile * TILE_VOXELS).as_vec3() + Vec3::new(8.0, 8.0, EYE_Z)
    }

    fn chest(tile: IVec3) -> Vec3 {
        (tile * TILE_VOXELS).as_vec3() + Vec3::new(8.0, 8.0, CHEST_Z)
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

    pub fn can_see(&self, a: UnitId, b: UnitId) -> bool {
        let (a, b) = (self.unit(a), self.unit(b));
        let d = (b.tile - a.tile).abs();
        let dist = d.x.max(d.y).max(d.z);
        // Burning ground lights its surroundings beyond the vision limit.
        let in_range = dist <= self.vision_tiles || (dist <= 20 && self.near_fire(b.tile));
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

    /// Tiles visible to `side`, for fog-of-war rendering.
    pub fn visible_tiles(&self, side: Side) -> HashSet<IVec3> {
        let (min, max) = self.tiles.bounds();
        let mut out = HashSet::new();
        let viewers: Vec<IVec3> = self.living(side).map(|u| u.tile).collect();
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

    // ------------------------------------------------------------------
    // Actions

    pub fn perform(&mut self, action: Action) -> Result<Vec<Event>, ActionError> {
        if self.winner.is_some() {
            return Err(ActionError::BattleOver);
        }
        match action {
            Action::Move { unit, to } => self.do_move(unit, to),
            Action::Fire { unit, target, mode } => self.do_fire(unit, target, mode),
            Action::Throw { unit, at } => self.do_throw(unit, at),
            Action::Heal { medic, target } => self.do_heal(medic, target),
            Action::Kneel { unit } => self.do_kneel(unit),
            Action::SetReserve { unit, on } => self.do_set_reserve(unit, on),
            Action::Bind { unit, target } => self.do_bind(unit, target),
            Action::Terrify { unit, target } => self.do_terrify(unit, target),
            Action::Turn { unit, toward } => self.do_turn(unit, toward),
            Action::DropCharge { unit, timer } => self.do_drop_charge(unit, timer),
            Action::ThrowSmoke { unit, at } => self.do_throw_smoke(unit, at),
            Action::OpenDoor { unit, at } => self.do_open_door(unit, at),
            Action::Possess { unit, target } => self.do_possess(unit, target),
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
            o + IVec3::new(0, 0, 4),
            o + IVec3::new(TILE_VOXELS, TILE_VOXELS, 14),
            ods_voxel::Voxel::EMPTY,
        );
        self.tiles
            .rederive_region(&self.world, o, o + IVec3::splat(TILE_VOXELS));
        Ok(vec![Event::DoorOpened { at }])
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

    fn do_set_reserve(&mut self, id: UnitId, on: bool) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        self.unit_mut(id).reserve_snap = on;
        Ok(Vec::new())
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
        let blocked: HashSet<IVec3> = self
            .units
            .iter()
            .filter(|u| u.is_active() && u.id != id)
            .map(|u| u.tile)
            .collect();
        let path = self.tiles.path(from, to, &blocked).ok_or(ActionError::NoPath)?;

        // A reserving unit keeps a snap shot's worth of TUs banked.
        let reserve = if self.unit(id).reserve_snap {
            self.unit(id).fire_cost(FireMode::Snap).unwrap_or(0)
        } else {
            0
        };
        let budget = |u: &Unit| u.tu - reserve;

        if budget(self.unit(id)) < step_cost(from, path[0]) * self.unit(id).move_cost_mult() {
            return Err(ActionError::NotEnoughTu);
        }
        self.unit_mut(id).kneeling = false; // you can't stay low and sprint

        let mut events = Vec::new();
        let mut here = from;
        for next in path {
            let cost = step_cost(here, next) * self.unit(id).move_cost_mult();
            if budget(self.unit(id)) < cost {
                break;
            }
            {
                let u = self.unit_mut(id);
                u.tu -= cost;
                let step = next - here;
                if step.x != 0 || step.y != 0 {
                    u.facing = IVec3::new(step.x.signum(), step.y.signum(), 0);
                }
                u.tile = next;
            }
            events.push(Event::Moved {
                unit: id,
                from: here,
                to: next,
                tu_left: self.unit(id).tu,
            });
            here = next;

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
                self.resolve_shot(shooter, mover, FireMode::Snap, true, events);
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
        let (cost, chance, rounds, power, breach, melee) = {
            let s = self.unit(shooter);
            let (Some(cost), Some(chance)) = (s.fire_cost(mode), s.hit_chance(mode)) else {
                return;
            };
            (
                cost,
                chance,
                s.rounds_per_action(mode),
                s.weapon.power,
                s.weapon.breach_radius,
                s.weapon.melee,
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

        for _ in 0..rounds {
            if !self.unit(target).is_active() || self.winner.is_some() {
                break; // remaining rounds of the burst go wide, harmlessly
            }
            let hit = (self.rng.roll(100) as i32) < chance;
            events.push(Event::Fired { unit: shooter, target, mode, reaction, hit });
            self.unit_mut(target).suppression += 1;
            if reaction {
                self.xp[shooter.0 as usize].reaction_shots += 1;
            }

            if hit {
                self.xp[shooter.0 as usize].shots_hit += 1;
                // 0–200% of weapon power, the original's famous swingy roll.
                let damage = power * self.rng.roll(201) as i32 / 100;
                self.apply_damage(target, damage, Some(shooter), events);
            } else if !melee {
                self.stray_shot(shooter, target, breach, events);
            }
        }
    }

    fn do_throw(&mut self, id: UnitId, at: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let u = self.unit(id);
        if u.grenades == 0 {
            return Err(ActionError::NoCharges);
        }
        let cost = u.tu_max * GRENADE_COST_PCT / 100;
        if u.tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        if cheb(u.tile, at) > GRENADE_RANGE_TILES {
            return Err(ActionError::OutOfRange);
        }

        {
            let u = self.unit_mut(id);
            u.tu -= cost;
            u.grenades -= 1;
        }
        // A bad throw scatters: the charge lands where fate says, not you.
        let throw_acc = (50 + self.unit(id).accuracy / 2).clamp(30, 90);
        let at = if (self.rng.roll(100) as i32) < throw_acc {
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
        let center = (at * TILE_VOXELS).as_vec3() + Vec3::new(8.0, 8.0, 8.0);
        let destroyed = self.world.carve_sphere(center, GRENADE_CARVE_RADIUS);
        let r = GRENADE_CARVE_RADIUS.ceil() as i32 + 1;
        let c = center.as_ivec3();
        self.tiles
            .rederive_region(&self.world, c - IVec3::splat(r), c + IVec3::splat(r));
        events.push(Event::Exploded { at, voxels: destroyed });
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
        if m.tu < HEAL_COST_TU {
            return Err(ActionError::NotEnoughTu);
        }

        {
            let m = self.unit_mut(medic);
            m.tu -= HEAL_COST_TU;
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

        if let Some(impact) = self.world.raycast(from, deviated, 640.0) {
            let destroyed = self.world.carve_sphere(impact.position, breach_radius);
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
            self.kill_unit(target, events);
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
                if !self.unit(target).injuries.contains(&part) {
                    self.unit_mut(target).injuries.push(part);
                    events.push(Event::PartCrippled { unit: target, part });
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

    /// The Taking: the victim's body stands back up on the other side.
    fn take_unit(&mut self, victim: UnitId, events: &mut Vec<Event>) {
        let (name, tile) = {
            let v = self.unit(victim);
            (v.name.clone(), v.tile)
        };
        // Squadmates witness something worse than a death.
        let side = self.unit(victim).side;
        for u in &mut self.units {
            if u.alive && u.side == side && u.id != victim {
                u.morale = (u.morale - 20).max(0);
            }
        }
        let husk = Unit::husk(victim.0, &format!("{name} (Taken)"), tile);
        *self.unit_mut(victim) = husk;
        events.push(Event::Taken { unit: victim });
        self.check_victory(events);
    }

    fn kill_unit(&mut self, target: UnitId, events: &mut Vec<Event>) {
        let (dead_side, dead_species, tile) = {
            let t = self.unit(target);
            (t.side, t.species, t.tile)
        };
        self.unit_mut(target).alive = false;
        events.push(Event::Died { unit: target });

        // Seeing a comrade die is the great morale killer.
        for u in &mut self.units {
            if u.alive && u.side == dead_side {
                u.morale = (u.morale - (15 - u.bravery / 10)).max(0);
            }
        }

        // A destroyed Husk splits open and something new crawls out.
        if dead_species == Species::Husk && self.winner.is_none() {
            let id = self.units.len() as u32;
            self.units.push(Unit::taker(id, "Hatched Taker", tile));
            self.xp.push(Experience::default());
            events.push(Event::Hatched { unit: UnitId(id) });
        }
        self.check_victory(events);
    }

    fn check_victory(&mut self, events: &mut Vec<Event>) {
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

        // Smoke thins; fire gutters, burns, and spreads.
        for c in &mut self.clouds {
            c.2 -= 1;
        }
        let expired: Vec<()> = Vec::new();
        let _ = expired;
        self.clouds.retain(|(_, _, ttl)| *ttl > 0);
        if self.side_to_move == Side::Order {
            // Once per full round: fire reaches for fresh fuel.
            let fires: Vec<IVec3> = self
                .clouds
                .iter()
                .filter(|(_, k, _)| *k == CloudKind::Fire)
                .map(|(t, _, _)| *t)
                .collect();
            for at in fires {
                if self.rng.roll(100) < 30 {
                    const RING: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
                    let (dx, dy) = RING[self.rng.roll(4) as usize];
                    let next = at + IVec3::new(dx, dy, 0);
                    if self.tiles.is_walkable(next)
                        && !self
                            .clouds
                            .iter()
                            .any(|(t, k, _)| *t == next && *k == CloudKind::Fire)
                    {
                        self.add_cloud(next, CloudKind::Fire, 3);
                        events.push(Event::FireStarted { at: next });
                    }
                }
            }
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
            IVec3::new(12 * TILE_VOXELS, 12 * TILE_VOXELS, 2),
            STONE,
        );
        Battle::new(world, IVec3::ZERO, IVec3::new(12, 12, 1), units, seed)
    }

    /// Same field with a wall between x tiles 5|6 (no gap).
    fn walled_field(units: Vec<Unit>, seed: u64) -> Battle {
        let mut world = VoxelWorld::new();
        world.fill_box(
            IVec3::new(0, 0, 0),
            IVec3::new(12 * TILE_VOXELS, 12 * TILE_VOXELS, 2),
            STONE,
        );
        world.fill_box(
            IVec3::new(5 * TILE_VOXELS, 0, 2),
            IVec3::new(6 * TILE_VOXELS, 12 * TILE_VOXELS, 14),
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
        let destroyed = b.world.carve_sphere(center, 10.0);
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

        // Range and supply limits.
        b.units[0].tile = IVec3::new(0, 0, 0); // corner-to-corner is 11 tiles
        assert_eq!(
            b.perform(Action::Throw { unit: UnitId(0), at: IVec3::new(11, 11, 0) }),
            Err(ActionError::OutOfRange)
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
        b.perform(Action::SetReserve { unit: UnitId(0), on: true }).unwrap();
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

        // Now destroy the Husk and watch what crawls out.
        let unit_count = b.units.len();
        let mut events = Vec::new();
        b.apply_damage(UnitId(0), 999, None, &mut events);
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
}

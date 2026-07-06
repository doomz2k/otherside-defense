//! The Battlescape state machine: actions in, events out, deterministic
//! given the seed. Nothing here renders; nothing here reads the clock.

use std::collections::HashSet;

use glam::{IVec3, Vec3};
use ods_voxel::VoxelWorld;

use crate::tiles::{TileMap, step_cost};
use crate::units::{FireMode, Side, Unit, UnitId};
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

pub struct Battle {
    pub world: VoxelWorld,
    pub tiles: TileMap,
    pub units: Vec<Unit>,
    pub side_to_move: Side,
    pub turn: u32,
    pub winner: Option<Side>,
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
            xp,
            rng: SimRng::from_seed(seed),
        }
    }

    pub fn experience(&self, id: UnitId) -> Experience {
        self.xp[id.0 as usize]
    }

    pub fn unit(&self, id: UnitId) -> &Unit {
        &self.units[id.0 as usize]
    }

    fn unit_mut(&mut self, id: UnitId) -> &mut Unit {
        &mut self.units[id.0 as usize]
    }

    pub fn living(&self, side: Side) -> impl Iterator<Item = &Unit> {
        self.units.iter().filter(move |u| u.alive && u.side == side)
    }

    pub fn unit_at(&self, tile: IVec3) -> Option<UnitId> {
        self.units
            .iter()
            .find(|u| u.alive && u.tile == tile)
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
        d.x.max(d.y).max(d.z) <= VISION_TILES
            && self.los_clear(Self::eye(a.tile), Self::chest(b.tile))
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
                        d.x.max(d.y).max(d.z) <= VISION_TILES
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
            Action::EndTurn => Ok(self.end_turn()),
        }
    }

    fn check_actor(&self, id: UnitId) -> Result<(), ActionError> {
        let u = self.unit(id);
        if !u.alive {
            return Err(ActionError::DeadUnit);
        }
        if u.side != self.side_to_move {
            return Err(ActionError::NotYourTurn);
        }
        Ok(())
    }

    fn do_move(&mut self, id: UnitId, to: IVec3) -> Result<Vec<Event>, ActionError> {
        self.check_actor(id)?;
        let from = self.unit(id).tile;
        let blocked: HashSet<IVec3> = self
            .units
            .iter()
            .filter(|u| u.alive && u.id != id)
            .map(|u| u.tile)
            .collect();
        let path = self.tiles.path(from, to, &blocked).ok_or(ActionError::NoPath)?;

        if self.unit(id).tu < step_cost(from, path[0]) {
            return Err(ActionError::NotEnoughTu);
        }

        let mut events = Vec::new();
        let mut here = from;
        for next in path {
            let cost = step_cost(here, next);
            if self.unit(id).tu < cost {
                break;
            }
            {
                let u = self.unit_mut(id);
                u.tu -= cost;
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
            if !self.unit(id).alive || self.winner.is_some() {
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

        let shooters: Vec<UnitId> = self
            .units
            .iter()
            .filter(|e| {
                e.alive
                    && e.side == mover_side.enemy()
                    && e.fire_cost(FireMode::Snap).is_some_and(|c| e.tu >= c)
                    && e.reactions * e.tu / e.tu_max.max(1) > mover_initiative
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
        let t = self.unit(target);
        if !t.alive || t.side == self.unit(id).side {
            return Err(ActionError::BadTarget);
        }
        let cost = self
            .unit(id)
            .fire_cost(mode)
            .ok_or(ActionError::UnsupportedMode)?;
        if self.unit(id).tu < cost {
            return Err(ActionError::NotEnoughTu);
        }
        if !self.can_see(id, target) {
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
        let (cost, chance, rounds, power, breach) = {
            let s = self.unit(shooter);
            let (Some(cost), Some(chance)) = (s.fire_cost(mode), s.hit_chance(mode)) else {
                return;
            };
            (cost, chance, s.rounds_per_action(mode), s.weapon.power, s.weapon.breach_radius)
        };
        self.unit_mut(shooter).tu -= cost;

        for _ in 0..rounds {
            if !self.unit(target).alive || self.winner.is_some() {
                break; // remaining rounds of the burst go wide, harmlessly
            }
            let hit = (self.rng.roll(100) as i32) < chance;
            events.push(Event::Fired { unit: shooter, target, mode, reaction, hit });
            if reaction {
                self.xp[shooter.0 as usize].reaction_shots += 1;
            }

            if hit {
                self.xp[shooter.0 as usize].shots_hit += 1;
                // 0–200% of weapon power, the original's famous swingy roll.
                let damage = power * self.rng.roll(201) as i32 / 100;
                self.apply_damage(target, damage, Some(shooter), events);
            } else {
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
        }
    }

    fn kill_unit(&mut self, target: UnitId, events: &mut Vec<Event>) {
        let dead_side = self.unit(target).side;
        self.unit_mut(target).alive = false;
        events.push(Event::Died { unit: target });

        // Seeing a comrade die is the great morale killer.
        for u in &mut self.units {
            if u.alive && u.side == dead_side {
                u.morale = (u.morale - (15 - u.bravery / 10)).max(0);
            }
        }
        self.check_victory(events);
    }

    fn check_victory(&mut self, events: &mut Vec<Event>) {
        for side in [Side::Order, Side::Demons] {
            if self.living(side).count() == 0 {
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

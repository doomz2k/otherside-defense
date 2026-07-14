//! Side AIs: honest — they play by the same action rules and TU budgets as
//! a human player — but no longer simple. Every unit fights by a battlefield
//! role derived from its breed, the pack converges its fire on the most
//! killable target, and ground is chosen, not stumbled onto: cover to duck
//! behind, sightlines to deny, rear arcs to open.
//!
//! Used by the demon side in interactive play and by both sides when the
//! campaign layer auto-resolves a battle (soldiers fight as skirmishers).

use glam::IVec3;

use crate::battle::{Action, ActionError, Battle, Event};
use crate::units::{FireMode, Side, Species, Unit, UnitId};

/// Demons (and desperate soldiers) sense anything this close, seen or not.
const SCENT_TILES: i32 = 10;

/// Rough TU price of one step on open ground, for "can I make it" estimates.
const STEP_TU: i32 = 4;

/// What a breed is FOR, once the killing starts.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Role {
    /// Shoot, duck, flank: imps, cultists, and auto-resolve soldiers.
    Skirmisher,
    /// Close and strike; hellhounds time the pounce, husks just keep coming.
    Charger,
    /// Hold an arcing band and drop bile over the cover.
    Lobber,
    /// Approach unseen, murder the isolated, put the dead to work.
    Stalker,
    /// Stay screened behind the pack and squeeze minds from safety.
    Warden,
    /// A Prince holds court at range: possession first, terror second.
    Lord,
    /// Perch out of sight, then dive onto a blind side.
    Flier,
    /// The shortest path is through the wall.
    Breaker,
}

fn role_of(u: &Unit) -> Role {
    match u.species {
        Species::Soldier | Species::Imp => Role::Skirmisher,
        Species::Hellhound | Species::Husk => Role::Charger,
        Species::BileWisp => Role::Lobber,
        Species::Taker => Role::Stalker,
        Species::Overseer => Role::Warden,
        Species::Prince => Role::Lord,
        Species::Gargoyle => Role::Flier,
        Species::Behemoth => Role::Breaker,
    }
}

/// Play out the demon turn and hand play back to the Order.
pub fn run_demon_turn(battle: &mut Battle) -> Vec<Event> {
    run_side_turn(battle, Side::Demons)
}

/// Play out the Order turn (campaign auto-resolve only) and hand play back.
pub fn run_order_turn(battle: &mut Battle) -> Vec<Event> {
    let mut events = run_civilian_moves(battle);
    events.extend(run_side_turn(battle, Side::Order));
    events
}

/// Civilians bolt away from the nearest demon. Call during the Order's turn
/// (the app calls it before ending the player's turn; the auto-resolver
/// folds it into `run_order_turn`).
pub fn run_civilian_moves(battle: &mut Battle) -> Vec<Event> {
    let mut events = Vec::new();
    if battle.side_to_move != Side::Order || battle.winner.is_some() {
        return events;
    }
    let civs: Vec<UnitId> = battle
        .living(Side::Order)
        .filter(|u| u.civilian && u.possessed == 0)
        .map(|u| u.id)
        .collect();
    for id in civs {
        let me = battle.unit(id).tile;
        let Some(threat) = battle
            .living(Side::Demons)
            .min_by_key(|u| (dist(me, u.tile), u.id.0))
            .map(|u| u.tile)
        else {
            break;
        };
        if dist(me, threat) > 10 {
            continue; // far enough; cower in place
        }
        let away = me + (me - threat).signum() * 4;
        if let Some(goal) = nearest_open_neighbor(battle, away, me, false)
            && goal != me
            && let Ok(ev) = battle.perform(Action::Move { unit: id, to: goal })
        {
            events.extend(ev);
        }
    }
    events
}

fn run_side_turn(battle: &mut Battle, side: Side) -> Vec<Event> {
    let mut events = Vec::new();
    if battle.side_to_move != side || battle.winner.is_some() {
        return events;
    }

    // Puppets first: enemy units under possession act for this side.
    let puppets: Vec<UnitId> = battle
        .units
        .iter()
        .filter(|u| u.is_active() && u.side != side && u.possessed > 0)
        .map(|u| u.id)
        .collect();
    for id in puppets {
        // Fire on their own nearest squadmate until dry.
        for _ in 0..4 {
            if battle.winner.is_some() || !battle.unit(id).is_active() {
                break;
            }
            let me_tile = battle.unit(id).tile;
            let victim = battle
                .units
                .iter()
                .filter(|u| u.is_active() && u.side != side && u.id != id && u.possessed == 0)
                .min_by_key(|u| (dist(me_tile, u.tile), u.id.0))
                .map(|u| u.id);
            let Some(victim) = victim else { break };
            match battle.perform(Action::Fire { unit: id, target: victim, mode: FireMode::Snap }) {
                Ok(ev) => events.extend(ev),
                Err(_) => break,
            }
        }
    }

    let troops: Vec<UnitId> = battle
        .living(side)
        .filter(|u| !u.civilian && u.possessed == 0)
        .map(|u| u.id)
        .collect();
    for id in troops {
        // Each iteration must spend TU or break, so this always terminates;
        // the cap is a backstop.
        for _ in 0..32 {
            if battle.winner.is_some() || !battle.unit(id).alive {
                break;
            }
            match pick_action(battle, id, side) {
                Some(action) => match battle.perform(action) {
                    Ok(ev) if ev.is_empty() => break,
                    Ok(ev) => events.extend(ev),
                    Err(ActionError::BattleOver) => break,
                    Err(_) => break,
                },
                None => break,
            }
        }
    }

    if battle.winner.is_none() {
        events.extend(battle.perform(Action::EndTurn).unwrap_or_default());
    }
    events
}

fn pick_action(battle: &Battle, id: UnitId, side: Side) -> Option<Action> {
    let me = battle.unit(id);
    // The routed are done fighting: run for where the pack came in.
    if me.routed {
        let exit = battle.demon_exit;
        if dist(me.tile, exit) <= 1 {
            return None; // the way out takes them at the turn's edge
        }
        let step = nearest_open_neighbor(battle, exit, me.tile, false)?;
        if step == me.tile {
            return None;
        }
        return Some(Action::Move { unit: id, to: step });
    }
    // Senses, not omniscience: hunt what the side can SEE, or what is close
    // enough to smell. The blind investigate noise, then drift toward where
    // the enemy came from. Crouched, quiet squads exploit exactly this.
    let seen = battle.visible_enemies(side);
    let known: Vec<(UnitId, IVec3)> = battle
        .living(side.enemy())
        .filter(|u| seen.contains(&u.id) || dist(me.tile, u.tile) <= SCENT_TILES)
        .map(|u| (u.id, u.tile))
        .collect();
    let prey = known
        .iter()
        .min_by_key(|(uid, t)| (dist(me.tile, *t), uid.0))
        .map(|&(uid, t)| (uid, t));
    let Some((prey_id, prey_tile)) = prey else {
        // Nothing seen, nothing smelled: follow the noise, or the sunrise.
        let goal = if side == Side::Demons {
            battle
                .alarm
                .last()
                .copied()
                .or(battle.last_noise)
                .unwrap_or(IVec3::new(3, 11, 0))
        } else {
            battle.last_noise.unwrap_or(IVec3::new(20, 11, 0))
        };
        if goal == me.tile {
            return None;
        }
        let step = nearest_open_neighbor(battle, goal, me.tile, me.weapon.arcing)?;
        if step == me.tile {
            return None;
        }
        return Some(Action::Move { unit: id, to: step });
    };
    let prey_dist = dist(me.tile, prey_tile);

    // Broken creatures run from what broke them (the fearless never do).
    if me.morale < 35 && me.bravery < 80 && prey_dist <= 6 {
        let away = me.tile + (me.tile - prey_tile).signum() * 4;
        if let Some(goal) = nearest_open_neighbor(battle, away, me.tile, false)
            && goal != me.tile
        {
            return Some(Action::Move { unit: id, to: goal });
        }
    }

    // Princes seize the deadliest mind in reach, not merely the nearest.
    if me.psi_master && me.tu >= me.tu_max * crate::battle::POSSESS_COST_PCT / 100 {
        let target = known
            .iter()
            .filter(|(uid, t)| {
                let u = battle.unit(*uid);
                dist(me.tile, *t) <= crate::battle::TERRIFY_RANGE_TILES
                    && u.possessed == 0
                    && !u.civilian
            })
            .max_by_key(|(uid, _)| (battle.unit(*uid).accuracy, std::cmp::Reverse(uid.0)));
        if let Some(&(target, _)) = target {
            return Some(Action::Possess { unit: id, target });
        }
    }

    // Psi talents whisper before they fight — at whichever mind in range is
    // already closest to the edge. A Warden standing naked in front of the
    // guns saves its breath and repositions first.
    if me.psi
        && !(role_of(me) == Role::Warden && warden_exposed(battle, me, side, prey_tile))
        && me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c + me.tu_max / 4)
    {
        let target = known
            .iter()
            .filter(|(uid, t)| {
                dist(me.tile, *t) <= crate::battle::TERRIFY_RANGE_TILES
                    && !battle.unit(*uid).civilian
            })
            .min_by_key(|(uid, _)| {
                let u = battle.unit(*uid);
                (u.morale + u.bravery / 2, uid.0)
            });
        if let Some(&(target, _)) = target {
            return Some(Action::Terrify { unit: id, target });
        }
    }

    // The dead have uses. A hurt demon feeds; a Taker recruits.
    if side == Side::Demons {
        let corpse = battle
            .units
            .iter()
            .filter(|c| c.is_corpse() && c.side != side && dist(me.tile, c.tile) <= 1)
            .min_by_key(|c| c.id.0);
        if let Some(corpse) = corpse {
            if me.species == Species::Taker
                && corpse.species == Species::Soldier
                && !corpse.civilian
                && me.tu >= crate::battle::DEFILE_TU
                && battle.unit_at(corpse.tile).is_none()
            {
                return Some(Action::Defile { unit: id, corpse: corpse.id });
            }
            if me.health < me.health_max / 2
                && me.tu >= crate::battle::DEVOUR_TU
                && prey_dist > 1
            {
                return Some(Action::Devour { unit: id, corpse: corpse.id });
            }
        }
    }

    // The dry-weapon drill: a fresh magazine if there is one, the sidearm
    // if there isn't, before any plan that involves the trigger.
    if me.weapon.clip > 0 && me.ammo <= 0 {
        if me.mags > 0 && me.tu >= crate::battle::RELOAD_TU {
            return Some(Action::Reload { unit: id });
        }
        if me.sidearm.is_some() && me.tu >= crate::battle::SWAP_TU {
            return Some(Action::SwapWeapon { unit: id });
        }
    }

    // Everything armed for the pack's fear map: civilians carry no guns.
    let guns: Vec<IVec3> = known
        .iter()
        .filter(|(uid, _)| !battle.unit(*uid).civilian)
        .map(|&(_, t)| t)
        .collect();

    match role_of(me) {
        Role::Breaker => breaker(battle, me, prey_tile),
        Role::Flier => flier(battle, me, side, &guns, prey_id, prey_tile),
        Role::Charger => charger(battle, me, &guns, prey_id, prey_tile),
        Role::Lobber => lobber(battle, me, &guns, prey_id, prey_tile),
        Role::Stalker => stalker(battle, me, side, &known, &guns, prey_id, prey_tile),
        Role::Warden => warden(battle, me, side, &guns, prey_tile),
        Role::Lord => lord(battle, me, side, &guns, prey_id, prey_tile),
        Role::Skirmisher if me.weapon.melee => charger(battle, me, &guns, prey_id, prey_tile),
        Role::Skirmisher => skirmisher(battle, me, side, &guns, prey_tile),
    }
}

// ----------------------------------------------------------------------
// Roles

/// Shoot the pack's focus, duck behind cover, work the flanks; fall back
/// firing when bleeding.
fn skirmisher(
    battle: &Battle,
    me: &Unit,
    side: Side,
    guns: &[IVec3],
    prey_tile: IVec3,
) -> Option<Action> {
    if !me.has_shot() {
        return None; // dry, no spares, nothing at the hip: hold and hope
    }
    let target = choose_target(battle, me, side);
    if let (Some(target), Some(snap)) = (target, me.fire_cost(FireMode::Snap)) {
        let hurt = me.health * 3 < me.health_max && me.bravery < 85;
        // With legs to spare, fix bad ground first: cover, kept sightlines,
        // an open rear arc — and distance, when hurt.
        if me.tu >= snap + 3 * STEP_TU {
            let wish = Wish {
                goal: battle.unit(target).tile,
                band: if hurt { (6, 12) } else { (3, 10) },
                needs_sight: true,
                fears_sight: hurt,
                cover: true,
                flank: Some(target),
            };
            if let Some(to) = better_tile(battle, me, guns, &wish) {
                return Some(Action::Move { unit: me.id, to });
            }
        }
        if me.tu >= snap {
            return Some(Action::Fire { unit: me.id, target, mode: FireMode::Snap });
        }
        return None; // in contact but dry — hold position
    }
    // A defending gunline with nothing in its sights holds its ground in
    // cover and banks the shot: the first thing through the door eats it.
    if battle.demons_hold && me.side == Side::Demons && hugs_cover(battle, me.tile) {
        if me.reserve.is_none() {
            return Some(Action::SetReserve { unit: me.id, mode: Some(FireMode::Snap) });
        }
        return None;
    }
    // Nothing in my sights: hunt a firing position on the nearest known
    // enemy, cover to cover.
    let wish = Wish {
        goal: prey_tile,
        band: (2, 9),
        needs_sight: true,
        fears_sight: false,
        cover: true,
        flank: None,
    };
    if let Some(to) = better_tile(battle, me, guns, &wish) {
        return Some(Action::Move { unit: me.id, to });
    }
    // No better perch in reach: press straight on.
    let step = nearest_open_neighbor(battle, prey_tile, me.tile, true)?;
    if step == me.tile {
        return None;
    }
    Some(Action::Move { unit: me.id, to: step })
}

/// Close and strike. A hound holds in cover until the whole leap — run AND
/// bite — fits in one turn; a husk has no such patience.
fn charger(
    battle: &Battle,
    me: &Unit,
    guns: &[IVec3],
    prey_id: UnitId,
    prey_tile: IVec3,
) -> Option<Action> {
    let strike = me.fire_cost(FireMode::Snap);
    let d = dist(me.tile, prey_tile);
    if d <= 1 {
        if strike.is_some_and(|c| me.tu >= c) {
            return Some(Action::Fire { unit: me.id, target: prey_id, mode: FireMode::Snap });
        }
        return None;
    }
    // Can the leap land this turn?
    let est = (d - 1) * STEP_TU * me.move_cost_mult().max(1) + strike.unwrap_or(8);
    if me.tu >= est {
        let to = nearest_open_neighbor(battle, prey_tile, me.tile, false)?;
        if to != me.tile {
            return Some(Action::Move { unit: me.id, to });
        }
        return None;
    }
    // Too far to land the bite this turn. Hounds stage: creep closer along
    // ground the guns can't watch.
    if me.species == Species::Hellhound && d > 6 {
        let wish = Wish {
            goal: prey_tile,
            band: (3, d - 1),
            needs_sight: false,
            fears_sight: true,
            cover: true,
            flank: None,
        };
        if let Some(to) = better_tile(battle, me, guns, &wish) {
            return Some(Action::Move { unit: me.id, to });
        }
    }
    // Husks — and hounds out of options — simply keep coming.
    let step = nearest_open_neighbor(battle, prey_tile, me.tile, true)?;
    if step == me.tile {
        return None;
    }
    Some(Action::Move { unit: me.id, to: step })
}

/// Hold the arcing band: never let anyone adjacent, never drift out of
/// lob range, and cover matters not at all — the bile goes over it.
fn lobber(
    battle: &Battle,
    me: &Unit,
    guns: &[IVec3],
    prey_id: UnitId,
    prey_tile: IVec3,
) -> Option<Action> {
    let d = dist(me.tile, prey_tile);
    let arc = crate::units::ARC_RANGE_TILES;
    // Too close: it bursts when shot, and it knows it. Open the gap first.
    if d < 3 {
        let wish = Wish {
            goal: prey_tile,
            band: (4, arc),
            needs_sight: false,
            fears_sight: false,
            cover: true,
            flank: None,
        };
        if let Some(to) = better_tile(battle, me, guns, &wish) {
            return Some(Action::Move { unit: me.id, to });
        }
    }
    if d <= arc {
        if me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c) {
            return Some(Action::Fire { unit: me.id, target: prey_id, mode: FireMode::Snap });
        }
        return None;
    }
    // Out of range: waddle into the band.
    let wish = Wish {
        goal: prey_tile,
        band: (4, arc),
        needs_sight: false,
        fears_sight: false,
        cover: false,
        flank: None,
    };
    if let Some(to) = better_tile(battle, me, guns, &wish) {
        return Some(Action::Move { unit: me.id, to });
    }
    let step = nearest_open_neighbor(battle, prey_tile, me.tile, true)?;
    if step == me.tile {
        return None;
    }
    Some(Action::Move { unit: me.id, to: step })
}

/// The horror stays out of the light: it picks the most isolated victim,
/// approaches along unseen ground, and only rushes at the end.
fn stalker(
    battle: &Battle,
    me: &Unit,
    side: Side,
    known: &[(UnitId, IVec3)],
    guns: &[IVec3],
    prey_id: UnitId,
    prey_tile: IVec3,
) -> Option<Action> {
    // The helpless are the harvest: an unconscious soldier nobody stands
    // over can be Taken without ever waking.
    let downed = battle.units.iter().find(|u| {
        u.alive
            && !u.conscious
            && u.side == side.enemy()
            && u.species == Species::Soldier
            && !u.civilian
            && dist(me.tile, u.tile) <= SCENT_TILES + 4
            && !battle.units.iter().any(|g| {
                g.is_active()
                    && g.side == u.side
                    && !g.civilian
                    && g.id != u.id
                    && dist(g.tile, u.tile) <= 1
            })
    });
    if let Some(d) = downed {
        if dist(me.tile, d.tile) <= 1 {
            if me.tu >= crate::battle::DEFILE_TU {
                return Some(Action::Defile { unit: me.id, corpse: d.id });
            }
            return None; // crouch over the body until the strength comes
        }
        let wish = Wish {
            goal: d.tile,
            band: (1, dist(me.tile, d.tile).max(2) - 1),
            needs_sight: false,
            fears_sight: true,
            cover: true,
            flank: None,
        };
        if let Some(to) = better_tile(battle, me, guns, &wish) {
            return Some(Action::Move { unit: me.id, to });
        }
        let step = nearest_open_neighbor(battle, d.tile, me.tile, true)?;
        if step != me.tile {
            return Some(Action::Move { unit: me.id, to: step });
        }
    }
    // Prefer whoever has the fewest friends in arm's reach.
    let victim = known
        .iter()
        .filter(|(uid, _)| !battle.unit(*uid).civilian)
        .min_by_key(|(uid, t)| {
            let friends = battle
                .living(side.enemy())
                .filter(|o| o.id != *uid && !o.civilian && dist(o.tile, *t) <= 3)
                .count() as i32;
            (friends * 8 + dist(me.tile, *t), uid.0)
        })
        .map(|&(uid, t)| (uid, t))
        .unwrap_or((prey_id, prey_tile));
    let d = dist(me.tile, victim.1);
    if d <= 1 {
        if me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c) {
            return Some(Action::Fire { unit: me.id, target: victim.0, mode: FireMode::Snap });
        }
        return None;
    }
    // Creep along ground nobody is watching.
    let wish = Wish {
        goal: victim.1,
        band: (1, (d - 1).max(1)),
        needs_sight: false,
        fears_sight: true,
        cover: true,
        flank: None,
    };
    if let Some(to) = better_tile(battle, me, guns, &wish) {
        return Some(Action::Move { unit: me.id, to });
    }
    // Close enough that patience stops paying: the last rush.
    if d <= 5 {
        let step = nearest_open_neighbor(battle, victim.1, me.tile, true)?;
        if step != me.tile {
            return Some(Action::Move { unit: me.id, to: step });
        }
    }
    None // lurk; let them come to the dark
}

/// Naked: the nearest enemy is close and no packmate stands between.
fn warden_exposed(battle: &Battle, me: &Unit, side: Side, prey_tile: IVec3) -> bool {
    let d = dist(me.tile, prey_tile);
    let screened = battle.living(side).any(|f| {
        f.id != me.id
            && !matches!(role_of(f), Role::Warden | Role::Lord)
            && dist(f.tile, prey_tile) < d
    });
    d <= 7 && !screened
}

/// An Overseer keeps meat between itself and the guns, and fights with the
/// mind (the psi block upstream) before the hand.
fn warden(
    battle: &Battle,
    me: &Unit,
    side: Side,
    guns: &[IVec3],
    prey_tile: IVec3,
) -> Option<Action> {
    if warden_exposed(battle, me, side, prey_tile) {
        // Exposed: fall back behind the pack.
        let wish = Wish {
            goal: prey_tile,
            band: (9, 13),
            needs_sight: false,
            fears_sight: true,
            cover: true,
            flank: None,
        };
        if let Some(to) = better_tile(battle, me, guns, &wish) {
            return Some(Action::Move { unit: me.id, to });
        }
    }
    // Screened (or cornered): fight like a skirmisher from the second rank.
    skirmisher(battle, me, side, guns, prey_tile)
}

/// A Prince holds court: it works minds from upstream (possession, terror),
/// keeps royal distance behind cover, and deigns to fight only in reach.
fn lord(
    battle: &Battle,
    me: &Unit,
    side: Side,
    guns: &[IVec3],
    prey_id: UnitId,
    prey_tile: IVec3,
) -> Option<Action> {
    if me.weapon.melee {
        return charger(battle, me, guns, prey_id, prey_tile);
    }
    let d = dist(me.tile, prey_tile);
    if !(5..=crate::battle::TERRIFY_RANGE_TILES).contains(&d) && me.tu >= 3 * STEP_TU {
        let wish = Wish {
            goal: prey_tile,
            band: (5, crate::battle::TERRIFY_RANGE_TILES),
            needs_sight: false,
            fears_sight: false,
            cover: true,
            flank: None,
        };
        if let Some(to) = better_tile(battle, me, guns, &wish) {
            return Some(Action::Move { unit: me.id, to });
        }
    }
    skirmisher(battle, me, side, guns, prey_tile)
}

/// Wings: perch unseen, and when the whole dive fits in one turn, land it
/// on the blind side.
fn flier(
    battle: &Battle,
    me: &Unit,
    side: Side,
    guns: &[IVec3],
    prey_id: UnitId,
    prey_tile: IVec3,
) -> Option<Action> {
    let strike = me.fire_cost(FireMode::Snap);
    let d = dist(me.tile, prey_tile);
    if d <= 1 {
        if strike.is_some_and(|c| me.tu >= c) {
            return Some(Action::Fire { unit: me.id, target: prey_id, mode: FireMode::Snap });
        }
        return None;
    }
    // The dive: wings pay no heed to walls, so the estimate is honest.
    let est = (d - 1) * STEP_TU + strike.unwrap_or(8);
    if me.tu >= est {
        let prey = battle.unit(prey_id);
        let mut best: Option<(i32, IVec3)> = None;
        for dy in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                let t = prey_tile + IVec3::new(dx, dy, 0);
                if !battle.tiles.is_walkable(t) || battle.unit_at(t).is_some() {
                    continue;
                }
                let mut s = dist(me.tile, t) * 2;
                if in_rear_arc(prey, t) {
                    s -= 9; // the blind side
                }
                if best.is_none_or(|(b, _)| s < b) {
                    best = Some((s, t));
                }
            }
        }
        if let Some((_, to)) = best {
            return Some(Action::Move { unit: me.id, to });
        }
    }
    // Not yet. Distant wings close openly; near ones perch out of sight.
    if d > 12 {
        let step = nearest_open_neighbor(battle, prey_tile, me.tile, true)?;
        if step == me.tile {
            return None;
        }
        return Some(Action::Move { unit: me.id, to: step });
    }
    let watched = battle
        .living(side.enemy())
        .any(|s| !s.civilian && battle.can_see(s.id, me.id));
    if watched {
        let wish = Wish {
            goal: prey_tile,
            band: ((d - 3).max(2), d.max(2)),
            needs_sight: false,
            fears_sight: true,
            cover: true,
            flank: None,
        };
        if let Some(to) = better_tile(battle, me, guns, &wish) {
            return Some(Action::Move { unit: me.id, to });
        }
    }
    None // perched, patient
}

/// A Behemoth does not path around architecture. Point it at the prey; the
/// smash pathfinder does the arithmetic.
fn breaker(battle: &Battle, me: &Unit, prey_tile: IVec3) -> Option<Action> {
    let d = dist(me.tile, prey_tile);
    if d <= 1 {
        if let Some(target) = battle.unit_at(prey_tile)
            && me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c)
        {
            return Some(Action::Fire { unit: me.id, target, mode: FireMode::Snap });
        }
        return None;
    }
    if !me.smasher {
        // A Breaker that can't break (shouldn't happen) walks like the rest.
        let step = nearest_open_neighbor(battle, prey_tile, me.tile, true)?;
        if step == me.tile {
            return None;
        }
        return Some(Action::Move { unit: me.id, to: step });
    }
    // Aim at the tile beside the prey nearest to us — wall or no wall.
    let to = prey_tile + (me.tile - prey_tile).signum();
    if to == me.tile {
        return None;
    }
    if battle.unit_at(to).is_some() {
        let step = nearest_open_neighbor(battle, prey_tile, me.tile, false)?;
        if step == me.tile {
            return None;
        }
        return Some(Action::Move { unit: me.id, to: step });
    }
    Some(Action::Move { unit: me.id, to })
}

// ----------------------------------------------------------------------
// Shared senses

/// The pack's focus: the most killable armed enemy in the side's sight —
/// hurt, lightly armored, already close to the pack's teeth.
fn pack_focus(battle: &Battle, side: Side) -> Option<UnitId> {
    battle
        .visible_enemies(side)
        .into_iter()
        .filter(|&t| !battle.unit(t).civilian)
        .min_by_key(|&t| {
            let u = battle.unit(t);
            let near = battle
                .living(side)
                .filter(|d| !d.civilian)
                .map(|d| dist(d.tile, u.tile))
                .min()
                .unwrap_or(99);
            (u.health * 2 + u.armor_front + near * 3, t.0)
        })
}

/// What THIS unit should shoot: the pack focus when it's in sight and not
/// wildly out of the way, else the nearest thing it can see.
fn choose_target(battle: &Battle, me: &Unit, side: Side) -> Option<UnitId> {
    let visible: Vec<UnitId> = battle
        .visible_enemies(side)
        .into_iter()
        .filter(|&t| battle.can_see(me.id, t))
        .collect();
    let nearest = visible
        .iter()
        .copied()
        .min_by_key(|&t| (dist(me.tile, battle.unit(t).tile), t.0))?;
    if let Some(focus) = pack_focus(battle, side)
        && visible.contains(&focus)
        && dist(me.tile, battle.unit(focus).tile)
            <= dist(me.tile, battle.unit(nearest).tile) + 4
    {
        return Some(focus);
    }
    Some(nearest)
}

/// What a unit wants from the ground it stands on.
struct Wish {
    /// The tile this unit orbits (usually its target).
    goal: IVec3,
    /// Acceptable chebyshev distance band to the goal.
    band: (i32, i32),
    /// A shooter needs to SEE the goal from where it stands.
    needs_sight: bool,
    /// A stalker pays dearly for every gun that can watch the tile.
    fears_sight: bool,
    /// Solid neighbors to duck behind are worth having.
    cover: bool,
    /// Standing in this target's rear arc is worth a detour.
    flank: Option<UnitId>,
}

/// Search the ground within this turn's legs for a tile meaningfully better
/// than the one under us. Cheap scores first, sightline raycasts only for
/// candidates still in the running — and only a strict improvement moves us.
fn better_tile(battle: &Battle, me: &Unit, guns: &[IVec3], wish: &Wish) -> Option<IVec3> {
    let reach = (me.tu / (STEP_TU * me.move_cost_mult().max(1))).clamp(1, 5);
    let cheap_score = |t: IVec3| -> i32 {
        let d = dist(t, wish.goal);
        let mut s = dist(me.tile, t) * 2;
        if d < wish.band.0 {
            s += (wish.band.0 - d) * 8;
        }
        if d > wish.band.1 {
            s += (d - wish.band.1) * 8;
        }
        if wish.cover && !hugs_cover(battle, t) {
            s += 6;
        }
        if let Some(v) = wish.flank {
            let v = battle.unit(v);
            if in_rear_arc(v, t) {
                s -= 10;
            }
        }
        // Grenades love a huddle: don't stand shoulder to shoulder.
        let huddle = battle
            .living(me.side)
            .filter(|f| f.id != me.id && dist(f.tile, t) <= 1)
            .count() as i32;
        s + huddle * 4
    };
    let full = |t: IVec3, cheap: i32| -> i32 {
        let mut s = cheap;
        if wish.needs_sight && !battle.sight_line(t, wish.goal) {
            s += 40;
        }
        if wish.fears_sight {
            s += guns
                .iter()
                .filter(|&&g| dist(g, t) <= battle.vision_tiles && battle.sight_line(g, t))
                .count() as i32
                * 10;
        }
        s
    };

    // Where we already stand, judged by the same eyes.
    let here = full(me.tile, cheap_score(me.tile));

    let mut candidates: Vec<(i32, IVec3)> = Vec::new();
    for dy in -reach..=reach {
        for dx in -reach..=reach {
            if dx == 0 && dy == 0 {
                continue;
            }
            let t = me.tile + IVec3::new(dx, dy, 0);
            if !battle.tiles.is_walkable(t) || battle.unit_at(t).is_some() {
                continue;
            }
            candidates.push((cheap_score(t), t));
        }
    }
    candidates.sort_by_key(|&(s, t)| (s, t.x, t.y));

    // Sight penalties only ever ADD, so once the cheap score can't beat the
    // best full score found (or the ground under us), stop raycasting.
    let mut best: Option<(i32, IVec3)> = None;
    for (cheap, t) in candidates {
        let bound = best.map_or(here, |(b, _)| b.min(here));
        if cheap >= bound {
            break;
        }
        let s = full(t, cheap);
        if best.is_none_or(|(b, _)| s < b) {
            best = Some((s, t));
        }
    }
    best.filter(|&(s, _)| s + 3 <= here).map(|(_, t)| t)
}

/// Is `from` behind this unit's shoulders?
fn in_rear_arc(unit: &Unit, from: IVec3) -> bool {
    let d = from - unit.tile;
    let dir = glam::Vec2::new(d.x as f32, d.y as f32).normalize_or_zero();
    let face = glam::Vec2::new(unit.facing.x as f32, unit.facing.y as f32).normalize_or_zero();
    dir.dot(face) <= -0.38
}

fn dist(a: IVec3, b: IVec3) -> i32 {
    let d = (b - a).abs();
    d.x.max(d.y).max(d.z)
}

/// Does this tile touch something solid to duck behind?
fn hugs_cover(battle: &Battle, tile: IVec3) -> bool {
    [(1, 0), (-1, 0), (0, 1), (0, -1)]
        .iter()
        .any(|&(dx, dy)| !battle.tiles.is_walkable(tile + IVec3::new(dx, dy, 0)))
}

fn nearest_open_neighbor(
    battle: &Battle,
    around: IVec3,
    from: IVec3,
    prefer_cover: bool,
) -> Option<IVec3> {
    let mut best: Option<(i32, IVec3)> = None;
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let t = around + IVec3::new(dx, dy, 0);
            if !battle.tiles.is_walkable(t) || battle.unit_at(t).is_some() {
                continue;
            }
            let mut score = dist(from, t) * 2;
            if prefer_cover && !hugs_cover(battle, t) {
                score += 3;
            }
            if best.is_none_or(|(s, _)| score < s) {
                best = Some((score, t));
            }
        }
    }
    best.map(|(_, t)| t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::battle::Action;
    use crate::scenario;

    #[test]
    fn demon_turn_acts_and_returns_play() {
        let mut b = scenario::skirmish(11);
        b.perform(Action::EndTurn).unwrap();

        let events = run_demon_turn(&mut b);
        assert_eq!(b.side_to_move, Side::Order, "play returns to the Order");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Moved { .. } | Event::Fired { .. })),
            "imps must do something on their turn: {events:?}"
        );
    }

    #[test]
    fn order_turn_acts_and_returns_play() {
        let mut b = scenario::skirmish(11);
        let events = run_order_turn(&mut b);
        assert_eq!(b.side_to_move, Side::Demons, "play passes to the demons");
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Moved { .. } | Event::Fired { .. })),
            "soldiers must do something on their turn: {events:?}"
        );
    }

    #[test]
    fn imps_close_distance_over_turns() {
        let mut b = scenario::skirmish(13);
        let gap_before = min_gap(&b);
        for _ in 0..4 {
            if b.winner.is_some() {
                break;
            }
            b.perform(Action::EndTurn).unwrap();
            run_demon_turn(&mut b);
        }
        assert!(
            min_gap(&b) < gap_before,
            "after 4 demon turns the pack should be closer ({} -> {})",
            gap_before,
            min_gap(&b)
        );
    }

    fn min_gap(b: &Battle) -> i32 {
        b.living(Side::Demons)
            .flat_map(|d| b.living(Side::Order).map(move |s| dist(d.tile, s.tile)))
            .min()
            .unwrap_or(0)
    }

    /// Park a unit in the far corner, out of sight and out of scent.
    fn park(b: &mut Battle, idx: usize, slot: i32) {
        b.units[idx].tile = IVec3::new(0, 20 + slot, 0);
    }

    #[test]
    fn the_pack_turns_its_guns_on_the_bleeding() {
        let mut b = scenario::skirmish(21);
        // Two soldiers equally close; one is nearly dead. Heavy plate on
        // both so nobody actually dies and muddies the count.
        for i in 0..2 {
            let u = &mut b.units[i];
            u.armor_front = 90;
            u.armor_side = 90;
            u.armor_rear = 90;
        }
        b.units[0].tile = IVec3::new(16, 10, 0);
        b.units[0].health = 4;
        b.units[1].tile = IVec3::new(16, 13, 0);
        park(&mut b, 2, 0);
        park(&mut b, 3, 1);
        for (i, tile) in [(4, (20, 10)), (5, (20, 11)), (6, (20, 12)), (7, (20, 13))] {
            b.units[i].tile = IVec3::new(tile.0, tile.1, 0);
        }
        b.perform(Action::EndTurn).unwrap();
        let events = run_demon_turn(&mut b);
        let (mut at_bleeder, mut at_healthy) = (0, 0);
        for e in &events {
            if let Event::Fired { target, .. } = e {
                match target.0 {
                    0 => at_bleeder += 1,
                    1 => at_healthy += 1,
                    _ => {}
                }
            }
        }
        assert!(at_bleeder > 0, "someone must shoot the bleeder: {events:?}");
        assert_eq!(at_healthy, 0, "the pack pours it all into the weak one");
    }

    #[test]
    fn wisps_will_not_be_cornered() {
        let mut b = scenario::skirmish(22);
        b.units[0].tile = IVec3::new(17, 11, 0);
        b.units[0].tu = 0; // no reaction fire to muddy the retreat
        for i in 1..4 {
            park(&mut b, i, i as i32);
        }
        b.units[4] = crate::units::Unit::bile_wisp(4, "Wisp of Envy", IVec3::new(18, 11, 0));
        for i in 5..8 {
            park(&mut b, i, i as i32);
        }
        b.perform(Action::EndTurn).unwrap();
        run_demon_turn(&mut b);
        let gap = dist(b.units[4].tile, b.units[0].tile);
        assert!(gap >= 3, "the wisp opens the gap before it lobs: {gap}");
    }

    #[test]
    fn an_overseer_will_not_stand_in_the_open() {
        let mut b = scenario::skirmish(23);
        b.units[0].tile = IVec3::new(16, 11, 0);
        b.units[0].tu = 0;
        for i in 1..4 {
            park(&mut b, i, i as i32);
        }
        b.units[5] = crate::units::Unit::overseer(5, "Overseer of Envy", IVec3::new(19, 11, 0));
        park(&mut b, 4, 4);
        park(&mut b, 6, 6);
        park(&mut b, 7, 7);
        b.perform(Action::EndTurn).unwrap();
        run_demon_turn(&mut b);
        let gap = dist(b.units[5].tile, b.units[0].tile);
        assert!(gap >= 6, "unscreened, the overseer falls back: {gap}");
    }

    #[test]
    fn hounds_leap_when_the_bite_lands() {
        let mut b = scenario::skirmish(24);
        b.units[0].tile = IVec3::new(16, 11, 0);
        b.units[0].tu = 0;
        // Plate the victim so the bite connects but never kills — a dead
        // soldier would send the hound wandering off toward the alarm.
        b.units[0].armor_front = 90;
        b.units[0].armor_side = 90;
        b.units[0].armor_rear = 90;
        for i in 1..4 {
            park(&mut b, i, i as i32);
        }
        b.units[4] = crate::units::Unit::hellhound(4, "Hound of Wrath", IVec3::new(12, 11, 0));
        for i in 5..8 {
            park(&mut b, i, i as i32);
        }
        b.perform(Action::EndTurn).unwrap();
        let events = run_demon_turn(&mut b);
        assert!(
            events
                .iter()
                .any(|e| matches!(e, Event::Fired { unit, .. } if unit.0 == 4)),
            "the whole leap fits in one turn — run AND bite: {events:?}"
        );
        assert!(dist(b.units[4].tile, b.units[0].tile) <= 1, "and it ends at the throat");
    }

    #[test]
    fn gargoyles_dive_for_the_back() {
        let mut b = scenario::skirmish(25);
        b.units[0].tile = IVec3::new(16, 11, 0);
        b.units[0].facing = IVec3::new(1, 0, 0); // watching east
        b.units[0].tu = 0;
        for i in 1..4 {
            park(&mut b, i, i as i32);
        }
        // The gargoyle starts dead ahead — where the soldier is looking.
        b.units[4] = crate::units::Unit::gargoyle(4, "Gargoyle of Wrath", IVec3::new(20, 11, 0));
        for i in 5..8 {
            park(&mut b, i, i as i32);
        }
        b.perform(Action::EndTurn).unwrap();
        run_demon_turn(&mut b);
        let g = b.units[4].tile;
        assert!(dist(g, b.units[0].tile) <= 1, "the dive lands adjacent: {g}");
        assert!(
            g.x < b.units[0].tile.x,
            "it swings around and lands on the blind side: {g}"
        );
    }

    #[test]
    fn the_breaker_does_not_use_doors() {
        let mut b = scenario::skirmish(26);
        // A soldier deep in the chapel, far from both doorways — and in
        // plate, so the fists don't end the test early.
        b.units[0].tile = IVec3::new(11, 14, 0);
        b.units[0].tu = 0;
        b.units[0].armor_front = 90;
        b.units[0].armor_side = 90;
        b.units[0].armor_rear = 90;
        for i in 1..4 {
            park(&mut b, i, i as i32);
        }
        b.units[4] = crate::units::Unit::behemoth(4, "Behemoth of Wrath", IVec3::new(8, 14, 0));
        for i in 5..8 {
            park(&mut b, i, i as i32);
        }
        let before = dist(b.units[4].tile, b.units[0].tile);
        b.perform(Action::EndTurn).unwrap();
        let events = run_demon_turn(&mut b);
        assert!(
            events.iter().any(|e| matches!(e, Event::WallSmashed { .. })),
            "masonry gives way before the Behemoth: {events:?}"
        );
        assert!(
            dist(b.units[4].tile, b.units[0].tile) < before,
            "and the straight line got shorter"
        );
    }

    #[test]
    fn the_taker_stalks_the_stray() {
        let mut b = scenario::skirmish(27);
        // A knot of three, and one who wandered north alone.
        b.units[0].tile = IVec3::new(14, 6, 0); // the stray
        b.units[1].tile = IVec3::new(14, 17, 0);
        b.units[2].tile = IVec3::new(15, 17, 0);
        b.units[3].tile = IVec3::new(14, 18, 0);
        b.units[4] = crate::units::Unit::taker(4, "The Taker", IVec3::new(19, 11, 0));
        for i in 5..8 {
            park(&mut b, i, i as i32);
        }
        for _ in 0..3 {
            if b.winner.is_some() {
                break;
            }
            // No reaction fire: this is about where the Taker chooses to go.
            for i in 0..4 {
                b.units[i].tu = 0;
            }
            b.perform(Action::EndTurn).unwrap();
            run_demon_turn(&mut b);
        }
        let to_stray = dist(b.units[4].tile, IVec3::new(14, 6, 0));
        let to_knot = dist(b.units[4].tile, IVec3::new(14, 17, 0));
        assert!(
            to_stray < to_knot,
            "the horror hunts the isolated: stray {to_stray}, knot {to_knot}"
        );
        assert!(to_stray <= 3, "and it is nearly upon them: {to_stray}");
    }
}

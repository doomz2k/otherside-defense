//! Simple side AIs: greedy but honest — they play by the same action rules
//! and TU budgets as a human player. Used by the demon side in interactive
//! play and by both sides when the campaign layer auto-resolves a battle.

use glam::IVec3;

use crate::battle::{Action, ActionError, Battle, Event};
use crate::units::{FireMode, Side, Species, UnitId};

/// Demons (and desperate soldiers) sense anything this close, seen or not.
const SCENT_TILES: i32 = 10;

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
    // Senses, not omniscience: hunt what the side can SEE, or what is close
    // enough to smell. The blind investigate noise, then drift toward where
    // the enemy came from. Crouched, quiet squads exploit exactly this.
    let seen = battle.visible_enemies(side);
    let prey = battle
        .living(side.enemy())
        .filter(|u| seen.contains(&u.id) || dist(me.tile, u.tile) <= SCENT_TILES)
        .min_by_key(|u| (dist(me.tile, u.tile), u.id.0));
    let Some(prey) = prey else {
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
    let prey_dist = dist(me.tile, prey.tile);

    // Broken creatures run from what broke them (the fearless never do).
    if me.morale < 35 && me.bravery < 80 && prey_dist <= 6 {
        let away = me.tile + (me.tile - prey.tile).signum() * 4;
        if let Some(goal) = nearest_open_neighbor(battle, away, me.tile, false)
            && goal != me.tile
        {
            return Some(Action::Move { unit: id, to: goal });
        }
    }

    // Princes seize minds outright when the strength is in them.
    if me.psi_master
        && me.tu >= me.tu_max * crate::battle::POSSESS_COST_PCT / 100
        && prey_dist <= crate::battle::TERRIFY_RANGE_TILES
        && prey.possessed == 0
    {
        return Some(Action::Possess { unit: id, target: prey.id });
    }

    // Psi talents whisper before they fight: break the bravest target.
    if me.psi
        && me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c + me.tu_max / 4)
        && prey_dist <= crate::battle::TERRIFY_RANGE_TILES
    {
        return Some(Action::Terrify { unit: id, target: prey.id });
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

    if me.weapon.melee {
        // Claws: charge the nearest prey; strike when adjacent.
        if prey_dist <= 1 {
            if me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c) {
                return Some(Action::Fire { unit: id, target: prey.id, mode: FireMode::Snap });
            }
            return None;
        }
        let goal = nearest_open_neighbor(battle, prey.tile, me.tile, false)?;
        if goal == me.tile {
            return None;
        }
        return Some(Action::Move { unit: id, to: goal });
    }

    if me.weapon.arcing {
        // Lob globs from beyond retaliation; close only when out of range.
        if prey_dist <= crate::units::ARC_RANGE_TILES {
            if me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c) {
                return Some(Action::Fire { unit: id, target: prey.id, mode: FireMode::Snap });
            }
            return None;
        }
        let goal = nearest_open_neighbor(battle, prey.tile, me.tile, true)?;
        if goal == me.tile {
            return None;
        }
        return Some(Action::Move { unit: id, to: goal });
    }

    // Gunline: shoot the nearest visible enemy while we can afford it.
    let mut visible: Vec<UnitId> = battle
        .visible_enemies(side)
        .into_iter()
        .filter(|&t| battle.can_see(id, t))
        .collect();
    visible.sort_by_key(|&t| dist(me.tile, battle.unit(t).tile));

    if let Some(&target) = visible.first() {
        if me.fire_cost(FireMode::Snap).is_some_and(|c| me.tu >= c) {
            return Some(Action::Fire { unit: id, target, mode: FireMode::Snap });
        }
        return None; // in contact but dry — hold position
    }

    // Nothing visible: hunt the last violence heard; failing that, the AI
    // falls back to omniscient pursuit so auto-battles always conclude.
    let hunt = battle.last_noise.unwrap_or(prey.tile);
    let goal = nearest_open_neighbor(battle, hunt, me.tile, true)
        .or_else(|| nearest_open_neighbor(battle, prey.tile, me.tile, true))?;
    if goal == me.tile {
        return None;
    }
    Some(Action::Move { unit: id, to: goal })
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
}

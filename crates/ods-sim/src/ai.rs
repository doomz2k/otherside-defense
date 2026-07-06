//! Demon-side AI for the first skirmish: greedy but honest — it plays by the
//! same action rules and TU budget as the player.

use glam::IVec3;

use crate::battle::{Action, ActionError, Battle, Event};
use crate::units::{FireMode, Side, UnitId};

/// Play out the demon turn and hand play back to the Order. Must be called
/// when it is the demons' turn; returns every event that occurred, ending
/// with the Order's `TurnStarted`.
pub fn run_demon_turn(battle: &mut Battle) -> Vec<Event> {
    let mut events = Vec::new();
    if battle.side_to_move != Side::Demons || battle.winner.is_some() {
        return events;
    }

    let demons: Vec<UnitId> = battle.living(Side::Demons).map(|u| u.id).collect();
    for id in demons {
        // Each iteration must spend TU or break, so this always terminates;
        // the cap is a backstop.
        for _ in 0..32 {
            if battle.winner.is_some() || !battle.unit(id).alive {
                break;
            }
            match pick_action(battle, id) {
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

fn pick_action(battle: &Battle, id: UnitId) -> Option<Action> {
    let me = battle.unit(id);

    // Shoot the nearest visible soldier while we can afford it.
    let mut visible: Vec<UnitId> = battle
        .visible_enemies(Side::Demons)
        .into_iter()
        .filter(|&t| battle.can_see(id, t))
        .collect();
    visible.sort_by_key(|&t| dist(me.tile, battle.unit(t).tile));

    if let Some(&target) = visible.first() {
        if me.tu >= me.fire_cost(FireMode::Snap) {
            return Some(Action::Fire { unit: id, target, mode: FireMode::Snap });
        }
        return None; // in contact but dry — hold position
    }

    // Nothing visible: advance toward the nearest living soldier. The AI is
    // map-omniscient for now; scent-of-prey works fine for imps.
    let prey = battle
        .living(Side::Order)
        .min_by_key(|u| dist(me.tile, u.tile))?;
    let goal = nearest_open_neighbor(battle, prey.tile, me.tile)?;
    if goal == me.tile {
        return None;
    }
    Some(Action::Move { unit: id, to: goal })
}

fn dist(a: IVec3, b: IVec3) -> i32 {
    let d = (b - a).abs();
    d.x.max(d.y).max(d.z)
}

fn nearest_open_neighbor(battle: &Battle, around: IVec3, from: IVec3) -> Option<IVec3> {
    let mut best: Option<IVec3> = None;
    for dy in -1..=1 {
        for dx in -1..=1 {
            if dx == 0 && dy == 0 {
                continue;
            }
            let t = around + IVec3::new(dx, dy, 0);
            if !battle.tiles.is_walkable(t) || battle.unit_at(t).is_some() {
                continue;
            }
            if best.is_none_or(|b| dist(from, t) < dist(from, b)) {
                best = Some(t);
            }
        }
    }
    best
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

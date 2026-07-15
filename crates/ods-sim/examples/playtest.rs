//! Headless playtest harness. Drives full rift-assault battles turn by turn
//! with the shipped Order and demon AIs, instrumenting the signals that carry
//! the X-COM "feel": fog of war at the cold open, when and how far away first
//! contact happens, whether reaction fire ever triggers, how lethal an
//! exchange is, and how long a field takes to resolve.
//!
//! Run: cargo run -p ods-sim --example playtest --release

use ods_sim::ai;
use ods_sim::battle::{Battle, Event};
use ods_sim::scenario;
use ods_sim::units::{Side, Unit};
use glam::IVec3;

fn squad() -> Vec<Unit> {
    // A full lance of six — the shape of a real sortie manifest.
    ["Vasquez", "Kowalski", "Ito", "Moreau", "Bishop", "Sund"]
        .iter()
        .enumerate()
        .map(|(i, n)| Unit::soldier(i as u32, n, IVec3::ZERO))
        .collect()
}

struct Trace {
    seed: u64,
    turns: u32,
    first_contact_turn: Option<u32>,
    first_contact_range: Option<i32>,
    // Per Order turn: how many demons the squad could actually see at the
    // moment it had to decide. 0 = fighting blind.
    seen_at_turn_start: Vec<usize>,
    aimed_shots: u32,
    aimed_hits: u32,
    reaction_shots: u32,
    reaction_hits: u32,
    noises_in_dark: u32,
    order_deaths: u32,
    demon_deaths: u32,
    panics: u32,
    winner: Option<Side>,
    order_start: usize,
    demon_start: usize,
}

fn cheb(a: IVec3, b: IVec3) -> i32 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

/// Nearest tile-distance between any living Order soldier and any demon the
/// Order can currently see. None if the squad sees nothing.
fn nearest_visible(b: &Battle) -> Option<i32> {
    let seen = b.visible_enemies(Side::Order);
    if seen.is_empty() {
        return None;
    }
    let soldiers: Vec<IVec3> = b.living(Side::Order).map(|u| u.tile).collect();
    seen.iter()
        .filter_map(|&id| {
            let d = b.units.iter().find(|u| u.id == id)?;
            soldiers.iter().map(|&s| cheb(s, d.tile)).min()
        })
        .min()
}

fn count(b: &Battle, side: Side) -> usize {
    b.living(side).count()
}

fn run(seed: u64) -> Trace {
    let mut b = scenario::incursion(seed, squad(), 8, 3);
    let order_start = count(&b, Side::Order);
    let demon_start = count(&b, Side::Demons);
    let mut t = Trace {
        seed,
        turns: 0,
        first_contact_turn: None,
        first_contact_range: None,
        seen_at_turn_start: Vec::new(),
        aimed_shots: 0,
        aimed_hits: 0,
        reaction_shots: 0,
        reaction_hits: 0,
        noises_in_dark: 0,
        order_deaths: 0,
        demon_deaths: 0,
        panics: 0,
        winner: None,
        order_start,
        demon_start,
    };

    let tally = |t: &mut Trace, evs: &[Event], b: &Battle| {
        for e in evs {
            match e {
                Event::Fired { reaction, hit, unit, .. } => {
                    let order = b.units.iter().find(|u| u.id == *unit).map(|u| u.side)
                        == Some(Side::Order);
                    if order {
                        if *reaction {
                            t.reaction_shots += 1;
                            if *hit {
                                t.reaction_hits += 1;
                            }
                        } else {
                            t.aimed_shots += 1;
                            if *hit {
                                t.aimed_hits += 1;
                            }
                        }
                    }
                }
                Event::Died { unit } => {
                    let side = b.units.iter().find(|u| u.id == *unit).map(|u| u.side);
                    match side {
                        Some(Side::Order) => t.order_deaths += 1,
                        Some(Side::Demons) => t.demon_deaths += 1,
                        None => {}
                    }
                }
                Event::NoiseInDark { .. } => t.noises_in_dark += 1,
                Event::Panicked { .. } | Event::Berserked { .. } => t.panics += 1,
                _ => {}
            }
        }
    };

    for turn in 1..=40u32 {
        t.turns = turn;
        // The cold open, and every turn after: what does the squad SEE before
        // it must act?
        let seen = b.visible_enemies(Side::Order).len();
        t.seen_at_turn_start.push(seen);
        if t.first_contact_turn.is_none() && seen > 0 {
            t.first_contact_turn = Some(turn);
            t.first_contact_range = nearest_visible(&b);
        }
        let evs = ai::run_order_turn(&mut b);
        tally(&mut t, &evs, &b);
        if let Some(w) = b.winner {
            t.winner = Some(w);
            break;
        }
        let evs = ai::run_demon_turn(&mut b);
        tally(&mut t, &evs, &b);
        if let Some(w) = b.winner {
            t.winner = Some(w);
            break;
        }
    }
    t
}

fn main() {
    let seeds: Vec<u64> = (1..=12).collect();
    let traces: Vec<Trace> = seeds.iter().map(|&s| run(s)).collect();

    println!("=== OTHERSIDE DEFENSE — headless tactical playtest ===");
    println!("Map: standard rift assault, daylight. Squad of 6 vs pack of 8 (strength 3).");
    println!("Both sides driven by the shipped AI. {} battles.\n", traces.len());

    println!(
        "{:>4} {:>6} {:>10} {:>10} {:>9} {:>10} {:>9} {:>7} {:>7} {:>8}",
        "seed", "turns", "contact@", "range", "blindT", "aim h/s", "react", "ODdead", "DMdead", "winner"
    );
    for t in &traces {
        let blind_turns = t.seen_at_turn_start.iter().take_while(|&&s| s == 0).count();
        let react = format!("{}/{}", t.reaction_hits, t.reaction_shots);
        let aim = format!("{}/{}", t.aimed_hits, t.aimed_shots);
        println!(
            "{:>4} {:>6} {:>10} {:>10} {:>9} {:>10} {:>9} {:>7} {:>7} {:>8}",
            t.seed,
            t.turns,
            t.first_contact_turn.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
            t.first_contact_range.map(|v| v.to_string()).unwrap_or_else(|| "—".into()),
            blind_turns,
            aim,
            react,
            format!("{}/{}", t.order_deaths, t.order_start),
            format!("{}/{}", t.demon_deaths, t.demon_start),
            match t.winner {
                Some(Side::Order) => "ORDER",
                Some(Side::Demons) => "DEMONS",
                None => "timeout",
            },
        );
    }

    // Aggregates — the feel signals.
    let n = traces.len() as f32;
    let avg = |f: &dyn Fn(&Trace) -> f32| traces.iter().map(|t| f(t)).sum::<f32>() / n;
    let order_wins = traces.iter().filter(|t| t.winner == Some(Side::Order)).count();
    let flawless = traces
        .iter()
        .filter(|t| t.winner == Some(Side::Order) && t.order_deaths == 0)
        .count();
    let wipes = traces.iter().filter(|t| t.order_deaths as usize >= t.order_start).count();
    let total_react: u32 = traces.iter().map(|t| t.reaction_shots).sum();
    let contacts: Vec<i32> = traces.iter().filter_map(|t| t.first_contact_range).collect();
    let contact_turns: Vec<u32> = traces.iter().filter_map(|t| t.first_contact_turn).collect();

    println!("\n--- feel signals (averaged over {} battles) ---", traces.len());
    println!(
        "first contact: turn {:.1} avg, at {:.1} tiles avg range  (vision_tiles is the cap)",
        contact_turns.iter().sum::<u32>() as f32 / contact_turns.len().max(1) as f32,
        contacts.iter().sum::<i32>() as f32 / contacts.len().max(1) as f32,
    );
    println!(
        "cold-open: {:.1} demons visible on turn 1 before anyone moves (0 = true fog)",
        avg(&|t| t.seen_at_turn_start.first().copied().unwrap_or(0) as f32),
    );
    println!("battle length: {:.1} turns avg", avg(&|t| t.turns as f32));
    println!(
        "reaction fire: {} shots across all battles ({:.1}/battle)",
        total_react,
        total_react as f32 / n
    );
    println!(
        "lethality to the squad: {:.1} deaths/battle; {} wipes",
        avg(&|t| t.order_deaths as f32),
        wipes
    );
    println!(
        "outcomes: Order won {}/{}  ({} of them flawless, no dead)",
        order_wins,
        traces.len(),
        flawless
    );
    println!(
        "noise-in-dark cues: {:.1}/battle",
        avg(&|t| t.noises_in_dark as f32)
    );
}

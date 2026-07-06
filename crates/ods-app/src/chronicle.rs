//! Headless modes: the smoke test and the text-mode campaign chronicle.
//! `narrate` is shared with the Geoscape UI's event log.

use ods_geo::{Campaign, GeoEvent, Project};
use ods_sim::units::Side;
use ods_sim::{ai, scenario};
use ods_sim::battle::Action;

pub fn narrate(c: &Campaign, event: &GeoEvent) -> String {
    use GeoEvent as E;
    let stamp = format!("[m{} d{}]", c.month, c.day);
    match event {
        E::FacilityComplete { facility } => {
            format!("{stamp} construction complete: {}", facility.name())
        }
        E::ResearchComplete { project } => {
            format!("{stamp} the Codex yields: {}", project.name())
        }
        E::SoldierRecovered { name } => format!("{stamp} {name} returns to duty"),
        E::RiftDetected { kind, region, days_left, .. } => format!(
            "{stamp} augurs scream: {} in {} ({days_left} days to act)",
            kind.name(),
            region.name()
        ),
        E::RiftExpired { kind, region, penalty, .. } => format!(
            "{stamp} the {} in {} runs its course unopposed (-{penalty})",
            kind.name(),
            region.name()
        ),
        E::NestFounded { region, .. } => {
            format!("{stamp} !!! a nest takes root in {}", region.name())
        }
        E::RegionInfiltrated { region } => {
            format!("{stamp} !!! cultists seize power in {}", region.name())
        }
        E::ReckoningRepelled { demons_slain, dead } => format!(
            "{stamp} !!! RECKONING: demons breached the chapterhouse — repelled \
             ({demons_slain} slain, {dead} defenders lost)"
        ),
        E::MonthlyReport { month, score, income, expenses, funds } => format!(
            "{stamp} === month {month} report: score {score}, income {income}k, \
             expenses {expenses}k, treasury {funds}k | salvage: {} brimstone {} hellsteel ===",
            c.brimstone, c.hellsteel
        ),
        E::ManufactureComplete { item } => {
            format!("{stamp} the workshop delivers: {}", item.name())
        }
        E::WardSkirmish { name } => {
            format!("{stamp} {name} is carried back bloodied from the picket line")
        }
        E::FacilityWrecked { facility } => {
            format!("{stamp} the fighting wrecked the {}", facility.name())
        }
        E::RequestIssued { region, needed, reward } => format!(
            "{stamp} the council demands: banish {needed} rift(s) in {} ({reward}k)",
            region.name()
        ),
        E::RequestFulfilled { reward } => {
            format!("{stamp} the council pays its debt: +{reward}k")
        }
        E::RequestFailed { region } => {
            format!("{stamp} {} despairs of us — the demand went unmet", region.name())
        }
        E::MarketShift { brimstone, hellsteel } => format!(
            "{stamp} the reliquaries repost prices: brimstone {brimstone}k, hellsteel {hellsteel}k"
        ),
        E::RegionPanicking { region, panic } => format!(
            "{stamp} !!! {} is in open panic ({panic}) — patrons flee, terror feeds",
            region.name()
        ),
        E::ChapterhouseLost { region } => {
            format!("{stamp} !!! the chapterhouse in {} is overrun and lost", region.name())
        }
        E::SortieArrived { region, .. } => {
            format!("{stamp} the zeppelin sets down in {} — squad on site", region.name())
        }
        E::SortieFought { region, victory, demons_slain, dead } => {
            if *victory {
                format!(
                    "{stamp} sortie in {}: VICTORY — {demons_slain} slain, {dead} lost",
                    region.name()
                )
            } else {
                format!(
                    "{stamp} sortie in {}: REPELLED — {dead} lost, the rift holds",
                    region.name()
                )
            }
        }
        E::SortieRecalled { region } => format!(
            "{stamp} the rift in {} closed before the squad landed — turning for home",
            region.name()
        ),
        E::CampaignOver { outcome } => match outcome {
            ods_geo::CampaignOutcome::Victory => {
                format!("{stamp} ### THE NAME IS BROKEN — THE ORDER PREVAILS ###")
            }
            _ => format!("{stamp} ### THE ORDER FALLS: {outcome:?} ###"),
        },
    }
}

/// The old smoke test, kept for CI and cloud sessions with no display.
pub fn headless_smoke_test() -> anyhow::Result<()> {
    let mut battle = scenario::skirmish(42);
    println!("skirmish begins: turn {}, {:?} to move", battle.turn, battle.side_to_move);
    for _ in 0..30 {
        if battle.winner.is_some() {
            break;
        }
        battle.perform(Action::EndTurn).ok();
        let events = ai::run_demon_turn(&mut battle);
        println!("demon turn: {} events", events.len());
    }
    println!(
        "after 30 turns: soldiers {}, imps {}",
        battle.living(Side::Order).count(),
        battle.living(Side::Demons).count()
    );
    Ok(())
}

/// Headless campaign: a simple commander policy plays N months and narrates.
/// Every assault is a real auto-resolved Battlescape fight.
pub fn campaign_chronicle(months: u32) -> anyhow::Result<()> {
    let mut c = Campaign::new(1999);
    let mut research_queue = vec![
        Project::RiftAugury,
        Project::BlessedArms,
        Project::HellsteelPlate,
        Project::HellfireLance,
    ];

    println!("== The Order convenes. {} soldiers sworn in. ==", c.soldiers.len());
    for _day in 0..months * 30 {
        if c.over.is_some() {
            break;
        }
        if c.research.active.is_none()
            && let Some(&next) = research_queue.first()
            && c.start_research(next).is_ok()
        {
            research_queue.remove(0);
            println!("[m{} d{}] research begins: {}", c.month, c.day, next.name());
        }
        if c.soldiers.len() < 8 && c.funds > 500 {
            let (m, d) = (c.month, c.day);
            if let Ok(s) = c.hire_soldier() {
                println!("[m{m} d{d}] recruited {}", s.name);
            }
        }

        for event in c.advance_day() {
            println!("{}", narrate(&c, &event));
            if let GeoEvent::RiftDetected { id, kind, region, .. } = event {
                if c.soldiers.iter().filter(|s| s.is_fit()).count() < 4 {
                    println!("    >> too few fit soldiers to assault — holding");
                    continue;
                }
                match c.assault_rift(id) {
                    Ok(r) if r.victory => println!(
                        "    >> BANISHED the {} in {} ({} demons slain, {} lost, {} turns)",
                        kind.name(),
                        region.name(),
                        r.demons_slain,
                        r.dead.len(),
                        r.turns
                    ),
                    Ok(r) => println!(
                        "    >> REPELLED at the {} in {} ({} lost) — the rift holds",
                        kind.name(),
                        region.name(),
                        r.dead.len()
                    ),
                    // No chapterhouse there: put the squad on the zeppelin.
                    Err(ods_geo::GeoError::NotOnSite) => match c.dispatch_squad(id, false) {
                        Ok(days) => println!(
                            "    >> squad dispatched to {} — {days} day(s) of flight",
                            region.name()
                        ),
                        Err(e) => println!("    >> cannot dispatch: {e:?}"),
                    },
                    Err(e) => println!("    >> cannot assault: {e:?}"),
                }
            }
        }
        if let Some(nest_id) = c.nests.first().map(|n| n.id)
            && let Ok(r) = c.raze_nest(nest_id)
            && r.victory
        {
            println!("    >> nest RAZED ({} demons slain)", r.demons_slain);
        }
    }

    println!(
        "\n== Chronicle ends: month {}, funds {}k, {} soldiers, {} nests standing, outcome: {} ==",
        c.month,
        c.funds,
        c.soldiers.len(),
        c.nests.len(),
        match c.over {
            None => "the Order fights on".to_string(),
            Some(o) => format!("{o:?}"),
        }
    );
    Ok(())
}

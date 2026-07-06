//! The Geoscape and main-menu screens. The spinning globe renders beneath
//! transparent panels; everything strategic is managed here.

use ods_geo::{
    Campaign, Difficulty, Facility, GRID, ManufactureItem, MissionKind, Project,
};

use crate::chronicle::narrate;
use crate::{Core, SAVE_PATH, Screen};

/// What the geoscape UI asked the app shell to do.
pub enum GeoAction {
    None,
    LeadMission(MissionKind),
}

impl Core {
    pub fn menu_ui(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space(60.0);
                ui.label(egui::RichText::new("OTHERSIDE DEFENSE").size(34.0).strong());
                ui.label("The rifts are opening. The Order answers.");
                ui.add_space(20.0);

                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() / 2.0 - 130.0);
                    for d in Difficulty::ALL {
                        ui.selectable_value(&mut self.difficulty_choice, d, d.name());
                    }
                });
                ui.add_space(12.0);

                if ui.button(egui::RichText::new("New campaign").size(18.0)).clicked() {
                    self.campaign =
                        Some(Campaign::new_with(seed_from_clock(), self.difficulty_choice));
                    self.log = vec!["The Order convenes.".to_string()];
                    self.enter_geoscape();
                }
                ui.add_space(8.0);
                let has_save = std::path::Path::new(SAVE_PATH).exists();
                if ui
                    .add_enabled(
                        has_save,
                        egui::Button::new(egui::RichText::new("Load campaign").size(18.0)),
                    )
                    .clicked()
                {
                    match std::fs::read_to_string(SAVE_PATH)
                        .map_err(|e| e.to_string())
                        .and_then(|s| Campaign::load_from_str(&s).map_err(|e| e.to_string()))
                    {
                        Ok(c) => {
                            self.log = vec![format!(
                                "Campaign restored: month {}, day {}, {}k in the treasury.",
                                c.month, c.day, c.funds
                            )];
                            self.campaign = Some(c);
                            self.enter_geoscape();
                        }
                        Err(e) => self.status = Some(format!("load failed: {e}")),
                    }
                }
                ui.add_space(8.0);
                if ui.button(egui::RichText::new("Quick skirmish").size(18.0)).clicked() {
                    self.start_skirmish();
                }
                if let Some(status) = &self.status {
                    ui.add_space(12.0);
                    ui.colored_label(egui::Color32::LIGHT_RED, status);
                }
            });
        });
    }

    pub fn geoscape_ui(&mut self, ctx: &egui::Context) -> GeoAction {
        let mut action = GeoAction::None;
        let Some(c) = &mut self.campaign else {
            self.screen = Screen::Menu;
            return action;
        };

        // ------------------------------------------------------ top bar
        egui::TopBottomPanel::top("geo-top").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.strong(format!(
                    "Month {} — Day {} [{}]",
                    c.month,
                    c.day,
                    c.difficulty.name()
                ));
                ui.separator();
                ui.label(format!("Treasury {}k", c.funds));
                ui.label(format!("Score {}", c.month_score));
                ui.label(format!("🜏 {}", c.brimstone));
                ui.label(format!("⛓ {}", c.hellsteel));
                ui.label(format!("🧨 {}", c.grenade_stock));
                ui.label(format!("✚ {}", c.dressing_stock));
                ui.label(format!(
                    "⛓ cells: {}g/{}o",
                    c.prisoners.grunts, c.prisoners.overseers
                ));
                ui.separator();
                let alive = c.over.is_none();
                if ui.add_enabled(alive, egui::Button::new("▶ Day")).clicked() {
                    let events = c.advance_day();
                    for e in &events {
                        self.log.push(narrate(c, e));
                    }
                }
                if ui.add_enabled(alive, egui::Button::new("⏩ Week")).clicked() {
                    for _ in 0..7 {
                        if c.over.is_some() {
                            break;
                        }
                        let events = c.advance_day();
                        for e in &events {
                            self.log.push(narrate(c, e));
                        }
                    }
                }
                ui.separator();
                if ui.button("💾 Save").clicked() {
                    match std::fs::write(SAVE_PATH, c.save_to_string()) {
                        Ok(()) => self.log.push(format!("Campaign saved to {SAVE_PATH}.")),
                        Err(e) => self.log.push(format!("save failed: {e}")),
                    }
                }
                if ui.button("Menu").clicked() {
                    self.screen = Screen::Menu;
                }
            });
        });

        // ------------------------------------------------- operations
        egui::SidePanel::left("geo-ops").default_width(360.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Incursions");
                let fit = c.soldiers.iter().filter(|s| s.is_fit()).count();
                ui.label(format!("{fit} soldiers fit for duty"));
                ui.separator();

                let rifts: Vec<_> = c
                    .rifts
                    .iter()
                    .filter(|r| r.detected)
                    .map(|r| (r.id, r.kind, r.region, r.days_left, r.is_stabilized()))
                    .collect();
                if rifts.is_empty() {
                    ui.label("No detected rifts. The augurs keep watch.");
                }
                for (id, kind, region, days_left, stabilized) in rifts {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "{} in {} — {days_left}d{}",
                            kind.name(),
                            region.name(),
                            if stabilized { " (DUG IN)" } else { " (unstable)" }
                        ));
                        if ui.button("⚔ Lead").clicked() {
                            action = GeoAction::LeadMission(MissionKind::Rift(id));
                        }
                        if ui.button("🎲 Auto").clicked() {
                            match c.assault_rift(id) {
                                Ok(r) => self.log.push(report_line("assault", r)),
                                Err(e) => self.log.push(format!("cannot assault: {e:?}")),
                            }
                        }
                    });
                }

                ui.add_space(6.0);
                ui.heading("Nests");
                let nests: Vec<_> = c.nests.iter().map(|n| (n.id, n.region)).collect();
                if nests.is_empty() {
                    ui.label("No standing nests.");
                }
                for (id, region) in nests {
                    ui.horizontal(|ui| {
                        ui.label(format!("nest in {}", region.name()));
                        if ui.button("⚔ Lead").clicked() {
                            action = GeoAction::LeadMission(MissionKind::Nest(id));
                        }
                        if ui.button("🎲 Auto").clicked() {
                            match c.raze_nest(id) {
                                Ok(r) => self.log.push(report_line("raze", r)),
                                Err(e) => self.log.push(format!("cannot raze: {e:?}")),
                            }
                        }
                    });
                }

                // The endgame, once the Name is known.
                if c.research.is_complete(Project::NameOfTheEnemy) && c.over.is_none() {
                    ui.add_space(8.0);
                    ui.heading("The Name is known");
                    ui.label(format!(
                        "Open the way to the Otherside ({} brimstone).",
                        ods_geo::FINAL_ASSAULT_BRIMSTONE
                    ));
                    if c.sanctum_open {
                        ui.colored_label(
                            egui::Color32::GOLD,
                            "The breach holds. The sanctum waits.",
                        );
                        if ui
                            .button(egui::RichText::new("⚔ INTO THE SANCTUM").strong())
                            .clicked()
                        {
                            action = GeoAction::LeadMission(MissionKind::FinalSanctum);
                        }
                    } else if ui
                        .button(egui::RichText::new("⚔ THE FINAL ASSAULT").strong())
                        .clicked()
                    {
                        action = GeoAction::LeadMission(MissionKind::FinalAssault);
                    }
                }

                ui.add_space(6.0);
                ui.heading("Market");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(c.brimstone > 0, egui::Button::new("Sell 🜏 (15k)"))
                        .clicked()
                    {
                        let _ = c.sell_brimstone(1);
                    }
                    if ui
                        .add_enabled(c.hellsteel > 0, egui::Button::new("Sell ⛓ (5k)"))
                        .clicked()
                    {
                        let _ = c.sell_hellsteel(1);
                    }
                });

                ui.add_space(6.0);
                egui::CollapsingHeader::new("Roster & loadouts")
                    .default_open(true)
                    .show(ui, |ui| {
                        egui::Grid::new("roster").striped(true).show(ui, |ui| {
                            for h in ["Name", "Rank", "TU", "HP", "Acc", "K", "🧨", "✚", "Status"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for s in &mut c.soldiers {
                                ui.label(&s.name);
                                ui.label(s.rank());
                                ui.label(s.stats.tu.to_string());
                                ui.label(s.stats.health.to_string());
                                ui.label(s.stats.accuracy.to_string());
                                ui.label(s.kills.to_string());
                                ui.horizontal(|ui| {
                                    if ui.small_button("-").clicked() && s.grenades_loadout > 0 {
                                        s.grenades_loadout -= 1;
                                    }
                                    ui.label(s.grenades_loadout.to_string());
                                    if ui.small_button("+").clicked() && s.grenades_loadout < 4 {
                                        s.grenades_loadout += 1;
                                    }
                                });
                                ui.horizontal(|ui| {
                                    if ui.small_button("-").clicked() && s.dressings_loadout > 0 {
                                        s.dressings_loadout -= 1;
                                    }
                                    ui.label(s.dressings_loadout.to_string());
                                    if ui.small_button("+").clicked() && s.dressings_loadout < 4 {
                                        s.dressings_loadout += 1;
                                    }
                                });
                                if s.is_fit() {
                                    ui.label("fit");
                                } else {
                                    ui.colored_label(
                                        egui::Color32::LIGHT_RED,
                                        format!("{}d", s.recovery_days),
                                    );
                                }
                                ui.end_row();
                            }
                        });
                        ui.label("Heavy packs (>5 items) cost 4 TU in the field.");
                    });

                if !c.memorial.is_empty() {
                    egui::CollapsingHeader::new(format!("Memorial ({})", c.memorial.len()))
                        .show(ui, |ui| {
                            for f in c.memorial.iter().rev() {
                                ui.label(format!(
                                    "{} {} — m{}, {} missions, {} kills — fell at {}",
                                    f.rank, f.name, f.month, f.missions, f.kills, f.cause
                                ));
                            }
                        });
                }
            });
        });

        // ------------------------------------------------- chapterhouses
        egui::SidePanel::right("geo-base").default_width(360.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, b) in c.bases.iter().enumerate() {
                        ui.selectable_value(&mut self.selected_base, i, b.region.name());
                    }
                });
                self.selected_base = self.selected_base.min(c.bases.len() - 1);
                let bi = self.selected_base;
                ui.heading(format!("Chapterhouse — {}", c.bases[bi].region.name()));

                ui.horizontal_wrapped(|ui| {
                    for f in Facility::BUILDABLE {
                        ui.selectable_value(
                            &mut self.build_choice,
                            f,
                            format!("{} ({}k)", f.name(), f.cost()),
                        );
                    }
                });
                ui.add_space(4.0);

                egui::Grid::new("base-grid").spacing([2.0, 2.0]).show(ui, |ui| {
                    for y in 0..GRID {
                        for x in 0..GRID {
                            let cell = c.bases[bi].facility_at(x, y);
                            let label = match cell {
                                Some((f, true)) => {
                                    f.name().chars().next().unwrap_or('?').to_string()
                                }
                                Some((_, false)) => "⏳".to_string(),
                                None => "·".to_string(),
                            };
                            let button = egui::Button::new(label).min_size(egui::vec2(30.0, 30.0));
                            let resp = ui.add(button);
                            let resp = match cell {
                                Some((f, true)) => resp.on_hover_text(f.name()),
                                Some((f, false)) => {
                                    resp.on_hover_text(format!("{} (building)", f.name()))
                                }
                                None => resp.on_hover_text("empty"),
                            };
                            if resp.clicked()
                                && cell.is_none()
                                && let Err(e) = c.start_build(bi, self.build_choice, x, y)
                            {
                                self.log.push(format!("cannot build: {e:?}"));
                            }
                        }
                        ui.end_row();
                    }
                });

                // Founding new chapterhouses.
                ui.add_space(4.0);
                ui.menu_button(
                    format!("Found chapterhouse ({}k)…", ods_geo::CHAPTERHOUSE_COST),
                    |ui| {
                        for region in ods_geo::Region::ALL {
                            if c.bases.iter().any(|b| b.region == region) {
                                continue;
                            }
                            if ui.button(region.name()).clicked() {
                                match c.found_chapterhouse(region) {
                                    Ok(()) => self.log.push(format!(
                                        "A new chapterhouse rises in {}.",
                                        region.name()
                                    )),
                                    Err(e) => self.log.push(format!("cannot found: {e:?}")),
                                }
                                ui.close();
                            }
                        }
                    },
                );

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if ui
                        .button(format!("Soldier ({}k)", ods_geo::SOLDIER_HIRE_COST))
                        .clicked()
                    {
                        match c.hire_soldier() {
                            Ok(s) => {
                                let line = format!("Recruited {}.", s.name);
                                self.log.push(line);
                            }
                            Err(e) => self.log.push(format!("cannot hire: {e:?}")),
                        }
                    }
                    if ui
                        .button(format!("Occultist ({}k)", ods_geo::OCCULTIST_HIRE_COST))
                        .clicked()
                    {
                        match c.hire_occultist() {
                            Ok(()) => self.log.push("An occultist joins.".to_string()),
                            Err(e) => self.log.push(format!("cannot hire: {e:?}")),
                        }
                    }
                    if ui
                        .button(format!("Artificer ({}k)", ods_geo::ARTIFICER_HIRE_COST))
                        .clicked()
                    {
                        match c.hire_artificer() {
                            Ok(()) => self.log.push("An artificer joins.".to_string()),
                            Err(e) => self.log.push(format!("cannot hire: {e:?}")),
                        }
                    }
                });
                ui.label(format!(
                    "{} soldiers, {} occultists, {} artificers / {} beds",
                    c.soldiers.len(),
                    c.occultists,
                    c.artificers,
                    c.quarters_capacity()
                ));

                ui.add_space(6.0);
                ui.heading("Forbidden Codex");
                match &c.research.active {
                    Some((project, left)) => {
                        let done = project.cost().saturating_sub(*left) as f32;
                        ui.add(
                            egui::ProgressBar::new(done / project.cost() as f32)
                                .text(format!("{} — {left} pts left", project.name())),
                        );
                    }
                    None => {
                        ui.label("The scriptorium is idle.");
                    }
                }
                for project in Project::ALL {
                    if c.research.is_complete(project) {
                        ui.label(format!("✓ {}", project.name()));
                        continue;
                    }
                    let (brim, steel) = project.materials();
                    let (grunts, overseers) = project.prisoners();
                    let mut needs = String::new();
                    if brim + steel > 0 {
                        needs.push_str(&format!(" +{brim}🜏 {steel}⛓"));
                    }
                    if grunts + overseers > 0 {
                        needs.push_str(&format!(" +{grunts}g/{overseers}o bound"));
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Start").clicked()
                            && let Err(e) = c.start_research(project)
                        {
                            self.log.push(format!("cannot research: {e:?}"));
                        }
                        ui.label(format!("{} ({}pts{needs})", project.name(), project.cost()));
                    });
                }

                ui.add_space(6.0);
                ui.heading("Workshop");
                match &c.manufacture {
                    Some((item, left)) => {
                        let done = item.cost().saturating_sub(*left) as f32;
                        ui.add(
                            egui::ProgressBar::new(done / item.cost() as f32)
                                .text(format!("{} — {left} left", item.name())),
                        );
                    }
                    None => {
                        ui.label(if c.workshop_capacity() == 0 {
                            "No workshop built."
                        } else {
                            "The benches are idle."
                        });
                    }
                }
                for item in ManufactureItem::ALL {
                    let (brim, steel) = item.materials();
                    ui.horizontal(|ui| {
                        if ui.button("Make").clicked()
                            && let Err(e) = c.start_manufacture(item)
                        {
                            self.log.push(format!("cannot make: {e:?}"));
                        }
                        ui.label(format!("{} ({}pts, {brim}🜏 {steel}⛓)", item.name(), item.cost()));
                    });
                }
            });
        });

        // ------------------------------------------------------- log
        egui::TopBottomPanel::bottom("geo-log")
            .default_height(130.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for line in &self.log {
                        ui.label(line);
                    }
                });
            });

        // ------------------------------------------- the world itself
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new(
                        "right-drag: turn the world · scroll: zoom · click: inspect region",
                    )
                    .weak()
                    .small(),
                );
            });

        if let Some(region) = self.selected_region {
            egui::Window::new(region.name())
                .anchor(egui::Align2::LEFT_BOTTOM, [12.0, -160.0])
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label(format!(
                        "Monthly funding: {}k",
                        c.region_funding.get(&region).copied().unwrap_or(0)
                    ));
                    let rifts_here = c
                        .rifts
                        .iter()
                        .filter(|r| r.detected && r.region == region)
                        .count();
                    let base_hint = if c.bases.iter().any(|b| b.region == region) {
                        " (chapterhouse here)"
                    } else {
                        ""
                    };
                    ui.label(format!("Detected rifts: {rifts_here}{base_hint}"));
                    ui.label(format!(
                        "Standing nests: {}",
                        c.nests.iter().filter(|n| n.region == region).count()
                    ));
                    if ui.button("Close").clicked() {
                        self.selected_region = None;
                    }
                });
        }

        // -------------------------------------------------- game over/won
        if let Some(outcome) = c.over {
            let (title, body) = match outcome {
                ods_geo::CampaignOutcome::Victory => (
                    "THE NAME IS BROKEN",
                    "The arch-demon is unmade in its own realm. The rifts close.\nEarth endures — because of them. Every name on the wall mattered.",
                ),
                _ => (
                    "The Order falls",
                    "The rifts widen unopposed. Earth's chronicle ends here.",
                ),
            };
            egui::Window::new(title)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(format!("{outcome:?}")).size(20.0).strong());
                    ui.label(body);
                    if ui.button("Back to menu").clicked() {
                        self.screen = Screen::Menu;
                        self.campaign = None;
                    }
                });
        }

        action
    }
}

fn report_line(what: &str, r: ods_geo::BattleReport) -> String {
    let captures = r.captured_grunts + r.captured_overseers;
    if r.victory {
        format!(
            "{what}: VICTORY — {} demons slain{}, {} soldiers lost, {} turns",
            r.demons_slain,
            if captures > 0 { format!(", {captures} bound") } else { String::new() },
            r.dead.len(),
            r.turns
        )
    } else {
        format!(
            "{what}: REPELLED — {} soldiers lost, the enemy holds the field",
            r.dead.len()
        )
    }
}

fn seed_from_clock() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(1999)
}

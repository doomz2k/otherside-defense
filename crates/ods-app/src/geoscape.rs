//! The Geoscape and main-menu screens (pure egui — no 3D scene behind them).

use ods_geo::{Campaign, Facility, MissionKind, Project, GRID};

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
                ui.add_space(30.0);

                if ui.button(egui::RichText::new("New campaign").size(18.0)).clicked() {
                    self.campaign = Some(Campaign::new(seed_from_clock()));
                    self.log = vec!["The Order convenes.".to_string()];
                    self.enter_geoscape();
                }
                ui.add_space(8.0);
                let has_save = std::path::Path::new(SAVE_PATH).exists();
                if ui
                    .add_enabled(has_save, egui::Button::new(egui::RichText::new("Load campaign").size(18.0)))
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
                ui.strong(format!("Month {} — Day {}", c.month, c.day));
                ui.separator();
                ui.label(format!("Treasury {}k", c.funds));
                ui.label(format!("Score {}", c.month_score));
                ui.label(format!("🜏 {} brimstone", c.brimstone));
                ui.label(format!("⛓ {} hellsteel", c.hellsteel));
                ui.separator();
                let alive = c.over.is_none();
                if ui.add_enabled(alive, egui::Button::new("▶ Advance day")).clicked() {
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
        egui::SidePanel::left("geo-ops").default_width(340.0).show(ctx, |ui| {
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
                        "{} in {} — {days_left}d left{}",
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

            ui.add_space(6.0);
            ui.heading("Market");
            ui.horizontal(|ui| {
                if ui.add_enabled(c.brimstone > 0, egui::Button::new("Sell brimstone (15k)")).clicked() {
                    let _ = c.sell_brimstone(1);
                }
                if ui.add_enabled(c.hellsteel > 0, egui::Button::new("Sell hellsteel (5k)")).clicked() {
                    let _ = c.sell_hellsteel(1);
                }
            });

            ui.add_space(6.0);
            egui::CollapsingHeader::new("Roster")
                .default_open(true)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        egui::Grid::new("roster").striped(true).show(ui, |ui| {
                            for h in ["Name", "TU", "HP", "Acc", "Msn", "Kills", "Status"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for s in &c.soldiers {
                                ui.label(&s.name);
                                ui.label(s.stats.tu.to_string());
                                ui.label(s.stats.health.to_string());
                                ui.label(s.stats.accuracy.to_string());
                                ui.label(s.missions.to_string());
                                ui.label(s.kills.to_string());
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
                    });
                });
        });

        // ------------------------------------------------- chapterhouse
        egui::SidePanel::right("geo-base").default_width(340.0).show(ctx, |ui| {
            ui.heading(format!("Chapterhouse — {}", c.base.region.name()));
            ui.label("Click an empty cell to build the selected facility.");
            ui.horizontal_wrapped(|ui| {
                for f in [
                    Facility::Quarters,
                    Facility::AugurArray,
                    Facility::Library,
                    Facility::Infirmary,
                ] {
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
                        let cell = c.base.facility_at(x, y);
                        let label = match cell {
                            Some((f, true)) => f.name().chars().next().unwrap_or('?').to_string(),
                            Some((_, false)) => "⏳".to_string(),
                            None => "·".to_string(),
                        };
                        let button = egui::Button::new(label).min_size(egui::vec2(30.0, 30.0));
                        let resp = ui.add(button);
                        let resp = match cell {
                            Some((f, true)) => resp.on_hover_text(f.name()),
                            Some((f, false)) => resp.on_hover_text(format!("{} (building)", f.name())),
                            None => resp.on_hover_text("empty"),
                        };
                        if resp.clicked()
                            && cell.is_none()
                            && let Err(e) = c.start_build(self.build_choice, x, y)
                        {
                            self.log.push(format!("cannot build: {e:?}"));
                        }
                    }
                    ui.end_row();
                }
            });

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui.button(format!("Hire soldier ({}k)", ods_geo::SOLDIER_HIRE_COST)).clicked() {
                    match c.hire_soldier() {
                        Ok(s) => {
                            let line = format!("Recruited {}.", s.name);
                            self.log.push(line);
                        }
                        Err(e) => self.log.push(format!("cannot hire: {e:?}")),
                    }
                }
                if ui.button(format!("Hire occultist ({}k)", ods_geo::OCCULTIST_HIRE_COST)).clicked() {
                    match c.hire_occultist() {
                        Ok(()) => self.log.push("An occultist joins the Order.".to_string()),
                        Err(e) => self.log.push(format!("cannot hire: {e:?}")),
                    }
                }
            });
            ui.label(format!(
                "{} soldiers, {} occultists / {} beds",
                c.soldiers.len(),
                c.occultists,
                c.base.quarters_capacity()
            ));

            ui.add_space(6.0);
            ui.heading("Forbidden Codex");
            match &c.research.active {
                Some((project, left)) => {
                    let done = project.cost().saturating_sub(*left) as f32;
                    ui.add(
                        egui::ProgressBar::new(done / project.cost() as f32)
                            .text(format!("{} — {left} points left", project.name())),
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
                let materials = if brim + steel > 0 {
                    format!(" + {brim} brimstone, {steel} hellsteel")
                } else {
                    String::new()
                };
                ui.horizontal(|ui| {
                    if ui.button("Start").clicked()
                        && let Err(e) = c.start_research(project)
                    {
                        self.log.push(format!("cannot research: {e:?}"));
                    }
                    ui.label(format!("{} ({} pts{materials})", project.name(), project.cost()));
                });
            }
        });

        // ------------------------------------------------------- log
        egui::TopBottomPanel::bottom("geo-log")
            .default_height(140.0)
            .show(ctx, |ui| {
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for line in &self.log {
                        ui.label(line);
                    }
                });
            });

        // ------------------------------------------- the world itself
        // Transparent center: the spinning globe renders underneath.
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("right-drag: turn the world · scroll: zoom · click: inspect region")
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
                    let hidden_hint = if c.base.region == region {
                        " (chapterhouse here)"
                    } else {
                        ""
                    };
                    ui.label(format!("Detected rifts: {rifts_here}{hidden_hint}"));
                    ui.label(format!(
                        "Standing nests: {}",
                        c.nests.iter().filter(|n| n.region == region).count()
                    ));
                    if ui.button("Close").clicked() {
                        self.selected_region = None;
                    }
                });
        }

        // -------------------------------------------------- game over
        if let Some(outcome) = c.over {
            egui::Window::new("The Order falls")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label(egui::RichText::new(format!("{outcome:?}")).size(20.0).strong());
                    ui.label("The rifts widen unopposed. Earth's chronicle ends here.");
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
    if r.victory {
        format!(
            "{what}: VICTORY — {} demons slain, {} soldiers lost, {} turns",
            r.demons_slain,
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

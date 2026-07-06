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
        // The voxel diorama smoulders behind a transparent panel; the menu
        // itself sits on a parchment-dark card.
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ctx, |ui| {
            let card = egui::Frame::new()
                .fill(egui::Color32::from_rgba_premultiplied(14, 8, 10, 210))
                .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 96, 48)))
                .corner_radius(egui::CornerRadius::same(4))
                .inner_margin(egui::Margin::symmetric(28, 18));
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                card.show(ui, |ui| self.menu_card(ui));
            });
        });

        if self.show_options {
            let mut open = true;
            egui::Window::new("Options")
                .open(&mut open)
                .anchor(egui::Align2::RIGHT_TOP, [-16.0, 16.0])
                .resizable(false)
                .show(ctx, |ui| {
                    ui.label("Volume");
                    if ui
                        .add(egui::Slider::new(&mut self.volume, 0.0..=1.0).show_value(false))
                        .changed()
                    {
                        let volume = self.volume;
                        if let Some(a) = self.audio_mut() {
                            a.set_volume(volume);
                        }
                    }
                    ui.label("Camera sensitivity");
                    ui.add(egui::Slider::new(&mut self.cam_sense, 0.3..=2.5).show_value(false));
                    ui.label(
                        egui::RichText::new(
                            "Applies to right-drag orbiting on both the globe and the field.",
                        )
                        .weak()
                        .small(),
                    );
                });
            self.show_options = open;
        }
    }

    fn menu_card(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("OTHERSIDE DEFENSE").size(34.0).strong());
                ui.label("The rifts are opening. The Order answers.");
                ui.add_space(20.0);

                ui.horizontal(|ui| {
                    ui.add_space(ui.available_width() / 2.0 - 160.0);
                    for d in Difficulty::ALL {
                        ui.selectable_value(&mut self.difficulty_choice, d, d.name());
                    }
                    ui.checkbox(&mut self.ironman_choice, "Ironman");
                });
                ui.add_space(12.0);

                if ui.button(egui::RichText::new("New campaign").size(18.0)).clicked() {
                    let mut c = Campaign::new_with(seed_from_clock(), self.difficulty_choice);
                    c.ironman = self.ironman_choice;
                    self.campaign = Some(c);
                    self.log = vec!["The Order convenes.".to_string()];
                    self.enter_geoscape();
                }
                ui.add_space(8.0);
                let mut candidates: Vec<(String, String)> =
                    vec![(SAVE_PATH.to_string(), "quicksave".to_string())];
                for slot in 1..=3usize {
                    candidates.push((crate::slot_path(slot), format!("slot {slot}")));
                }
                candidates.push((crate::AUTOSAVE_PATH.to_string(), "autosave".to_string()));
                for (path, label) in candidates {
                    if !std::path::Path::new(&path).exists() {
                        continue;
                    }
                    if ui
                        .button(egui::RichText::new(format!("Load {label}")).size(16.0))
                        .clicked()
                    {
                        match std::fs::read_to_string(&path)
                            .map_err(|e| e.to_string())
                            .and_then(|s| Campaign::load_from_str(&s).map_err(|e| e.to_string()))
                        {
                            Ok(c) => {
                                self.log = vec![format!(
                                    "Campaign restored ({label}): month {}, day {}, {}k banked.",
                                    c.month, c.day, c.funds
                                )];
                                self.campaign = Some(c);
                                self.enter_geoscape();
                            }
                            Err(e) => self.status = Some(format!("load failed: {e}")),
                        }
                    }
                }
                ui.add_space(8.0);
                if ui.button(egui::RichText::new("Quick skirmish").size(18.0)).clicked() {
                    self.start_skirmish();
                }
                ui.add_space(4.0);
                if ui.button("Options").clicked() {
                    self.show_options = !self.show_options;
                }
                if let Some(status) = &self.status {
                    ui.add_space(12.0);
                    ui.colored_label(egui::Color32::LIGHT_RED, status);
                }
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
                let mut advanced = false;
                if ui.add_enabled(alive, egui::Button::new("▶ Day")).clicked() {
                    let events = c.advance_day();
                    for e in &events {
                        self.log.push(narrate(c, e));
                    }
                    advanced = true;
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
                    advanced = true;
                }
                if advanced {
                    // The world remembers, whether or not you asked it to.
                    let _ = std::fs::write(crate::AUTOSAVE_PATH, c.save_to_string());
                }
                ui.separator();
                if c.ironman {
                    ui.label("IRONMAN — the autosave is the only record");
                } else {
                    if ui.button("💾 Quick").clicked() {
                        match std::fs::write(SAVE_PATH, c.save_to_string()) {
                            Ok(()) => self.log.push("Saved.".to_string()),
                            Err(e) => self.log.push(format!("save failed: {e}")),
                        }
                    }
                    for slot in 1..=3usize {
                        if ui.button(format!("S{slot}")).clicked() {
                            match std::fs::write(crate::slot_path(slot), c.save_to_string()) {
                                Ok(()) => self.log.push(format!("Saved to slot {slot}.")),
                                Err(e) => self.log.push(format!("save failed: {e}")),
                            }
                        }
                    }
                }
                ui.separator();
                if ui.button("📖 Bestiary").clicked() {
                    self.show_codex = !self.show_codex;
                }
                if ui.button("📜 Ledger").clicked() {
                    self.show_stats = !self.show_stats;
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
                    let local = c.bases.iter().any(|b| b.region == region);
                    let sortie = c.sorties.iter().find(|s| s.rift_id == id).copied();
                    ui.horizontal_wrapped(|ui| {
                        ui.label(format!(
                            "{} in {} ({}) — {days_left}d{}",
                            kind.name(),
                            region.name(),
                            region.biome().name(),
                            if stabilized { " (DUG IN)" } else { " (unstable)" }
                        ));
                        match sortie {
                            // In the air: nothing to do but watch the clock.
                            Some(s) if s.days_left > 0 => {
                                ui.colored_label(
                                    egui::Color32::LIGHT_BLUE,
                                    format!("🛫 squad en route — {}d", s.days_left),
                                );
                            }
                            // Boots on the ground, holding for the order.
                            Some(_) => {
                                ui.colored_label(egui::Color32::LIGHT_GREEN, "on site");
                                if ui.button("⚔ Lead").clicked() {
                                    action = GeoAction::LeadMission(MissionKind::Rift(id));
                                }
                                if ui.button("🎲 Auto").clicked() {
                                    match c.assault_rift(id) {
                                        Ok(r) => self.log.push(report_line("assault", r)),
                                        Err(e) => {
                                            self.log.push(format!("cannot assault: {e:?}"))
                                        }
                                    }
                                }
                            }
                            // Local rifts strike same-day; distant ones fly.
                            None if local => {
                                if ui.button("⚔ Lead").clicked() {
                                    action = GeoAction::LeadMission(MissionKind::Rift(id));
                                }
                                if ui.button("🎲 Auto").clicked() {
                                    match c.assault_rift(id) {
                                        Ok(r) => self.log.push(report_line("assault", r)),
                                        Err(e) => {
                                            self.log.push(format!("cannot assault: {e:?}"))
                                        }
                                    }
                                }
                            }
                            None => {
                                let eta = c.travel_days(id).unwrap_or(0);
                                if ui
                                    .button(format!("🛫 Fly & lead ({eta}d)"))
                                    .on_hover_text(
                                        "no chapterhouse in the region: the squad flies out \
                                         and holds on arrival for your order",
                                    )
                                    .clicked()
                                    && let Err(e) = c.dispatch_squad(id, true)
                                {
                                    self.log.push(format!("cannot dispatch: {e:?}"));
                                }
                                if ui
                                    .button(format!("🛫 Fly & auto ({eta}d)"))
                                    .on_hover_text("auto-resolves the day the squad lands")
                                    .clicked()
                                    && let Err(e) = c.dispatch_squad(id, false)
                                {
                                    self.log.push(format!("cannot dispatch: {e:?}"));
                                }
                            }
                        }
                        if ui
                            .button("🛡 Ward")
                            .on_hover_text("post a fit soldier: the rift cannot dig in, but the picket line is dangerous")
                            .clicked()
                        {
                            let volunteer =
                                c.soldiers.iter().position(|s| s.is_fit());
                            match volunteer {
                                Some(i) => {
                                    let outcome = c.assign_ward(i, id);
                                    match outcome {
                                        Ok(()) => self.log.push(format!(
                                            "{} takes the picket line.",
                                            c.soldiers[i].name
                                        )),
                                        Err(e) => self.log.push(format!("cannot ward: {e:?}")),
                                    }
                                }
                                None => self.log.push("nobody fit to stand the line".to_string()),
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

                if let Some(req) = &c.request {
                    ui.add_space(6.0);
                    ui.heading("Council demand");
                    ui.label(format!(
                        "Banish {} rift(s) in {} this month ({}/{}) — {}k",
                        req.needed,
                        req.region.name(),
                        req.done,
                        req.needed,
                        req.reward
                    ));
                }

                ui.add_space(6.0);
                ui.heading("Market");
                ui.label("Reliquary prices shift with the month's fortunes.");
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(
                            c.brimstone > 0,
                            egui::Button::new(format!("Sell 🜏 ({}k)", c.brim_price)),
                        )
                        .clicked()
                    {
                        let _ = c.sell_brimstone(1);
                    }
                    if ui
                        .add_enabled(
                            c.hellsteel > 0,
                            egui::Button::new(format!("Sell ⛓ ({}k)", c.steel_price)),
                        )
                        .clicked()
                    {
                        let _ = c.sell_hellsteel(1);
                    }
                });

                ui.add_space(6.0);
                egui::CollapsingHeader::new("Roster & loadouts")
                    .default_open(true)
                    .show(ui, |ui| {
                        let lance_ok = c.research.is_complete(Project::HellfireLance);
                        let mut lance_toggle: Option<(usize, bool)> = None;
                        let mut transfer: Option<usize> = None;
                        egui::Grid::new("roster").striped(true).show(ui, |ui| {
                            for h in ["Name", "Rank", "Quirk", "TU", "HP", "Acc", "K", "🧨", "✚", "Lance", "Status"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for (si, s) in c.soldiers.iter_mut().enumerate() {
                                let mut tag = String::new();
                                if !s.scars.is_empty() {
                                    tag.push('*');
                                }
                                if !s.lost_parts.is_empty() {
                                    tag.push('†');
                                }
                                if tag.is_empty() {
                                    ui.label(&s.name);
                                } else {
                                    let lost: Vec<&str> =
                                        s.lost_parts.iter().map(|p| p.name()).collect();
                                    let mut hover =
                                        format!("{} lasting scar(s)", s.scars.len());
                                    if !lost.is_empty() {
                                        hover.push_str(&format!("; lost: {}", lost.join(", ")));
                                    }
                                    ui.label(format!("{}{tag}", s.name)).on_hover_text(hover);
                                }
                                ui.label(s.rank());
                                ui.label(s.quirk.map_or("–", |q| q.name()));
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
                                if lance_ok {
                                    let label = if s.has_lance { "🔥" } else { "–" };
                                    if ui.small_button(label).clicked() {
                                        lance_toggle = Some((si, !s.has_lance));
                                    }
                                } else {
                                    ui.label("–");
                                }
                                if s.warding.is_some() {
                                    ui.colored_label(egui::Color32::YELLOW, "warding");
                                } else if s.is_fit() {
                                    if ui.small_button(format!("fit @{}", s.home)).clicked() {
                                        transfer = Some(si);
                                    }
                                } else {
                                    ui.colored_label(
                                        egui::Color32::LIGHT_RED,
                                        format!("{}d", s.recovery_days),
                                    );
                                }
                                ui.end_row();
                            }
                        });
                        if let Some((si, take)) = lance_toggle
                            && let Err(e) = c.assign_lance(si, take)
                        {
                            self.log.push(format!("cannot assign lance: {e:?}"));
                        }
                        if let Some(si) = transfer {
                            let next = (c.soldiers[si].home + 1) % c.bases.len();
                            if next != c.soldiers[si].home {
                                match c.transfer_soldier(si, next) {
                                    Ok(()) => self.log.push(format!(
                                        "{} takes the road to {}.",
                                        c.soldiers[si].name,
                                        c.bases[next].region.name()
                                    )),
                                    Err(e) => self.log.push(format!("cannot transfer: {e:?}")),
                                }
                            }
                        }
                        ui.label(format!(
                            "Heavy packs (>5 items) cost 4 TU. Lances in armoury: {}.",
                            c.lance_stock
                        ));
                        ui.label("Click a fit soldier's @base tag to rotate their station.");
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
                    let panic = c.region_panic.get(&region).copied().unwrap_or(0);
                    let (mood, color) = match panic {
                        0..20 => ("wary", egui::Color32::LIGHT_GREEN),
                        20..ods_geo::PANIC_BREAKPOINT => ("fearful", egui::Color32::YELLOW),
                        _ => ("PANICKING", egui::Color32::LIGHT_RED),
                    };
                    ui.colored_label(color, format!("Populace: {mood} (dread {panic})"));
                    if ui.button("Close").clicked() {
                        self.selected_region = None;
                    }
                });
        }

        // ------------------------------------------------ bestiary codex
        if self.show_codex {
            let mut open = true;
            egui::Window::new("Bestiary of the Otherside")
                .open(&mut open)
                .default_width(420.0)
                .show(ctx, |ui| {
                    ui.label(
                        "Field reports describe what the squads have met. Take a specimen \
                         alive and the occultists open it up — anatomy and all.",
                    );
                    ui.separator();
                    egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                        for species in ods_sim::units::Species::DEMONS {
                            let seen = c.codex_seen.contains(&species);
                            let captured = c.codex_captured.contains(&species);
                            if !seen {
                                ui.label(
                                    egui::RichText::new("??? — unencountered")
                                        .weak()
                                        .italics(),
                                );
                                ui.separator();
                                continue;
                            }
                            ui.horizontal(|ui| {
                                ui.strong(species.name());
                                if captured {
                                    ui.colored_label(egui::Color32::GOLD, "⛓ dissected");
                                }
                            });
                            ui.label(bestiary_lore(species));
                            if captured {
                                let parts: Vec<&str> = species
                                    .body_parts()
                                    .iter()
                                    .map(|p| p.name())
                                    .collect();
                                ui.label(format!("Anatomy: {}", parts.join(", ")));
                            } else {
                                ui.label(
                                    egui::RichText::new(
                                        "Bind one alive to learn its anatomy.",
                                    )
                                    .weak(),
                                );
                            }
                            ui.separator();
                        }
                    });
                });
            self.show_codex = open;
        }

        // ------------------------------------------------ campaign ledger
        if self.show_stats {
            let mut open = true;
            egui::Window::new("The Order's Ledger")
                .open(&mut open)
                .show(ctx, |ui| {
                    let s = c.stats;
                    egui::Grid::new("ledger").striped(true).show(ui, |ui| {
                        ui.label("Missions won / lost");
                        ui.label(format!("{} / {}", s.missions_won, s.missions_lost));
                        ui.end_row();
                        ui.label("Rifts banished");
                        ui.label(s.rifts_banished.to_string());
                        ui.end_row();
                        ui.label("Nests razed");
                        ui.label(s.nests_razed.to_string());
                        ui.end_row();
                        ui.label("Reckonings repelled");
                        ui.label(s.reckonings_repelled.to_string());
                        ui.end_row();
                        ui.label("Demons slain / bound");
                        ui.label(format!("{} / {}", s.demons_slain, s.demons_captured));
                        ui.end_row();
                        ui.label("Soldiers lost / hired");
                        ui.label(format!("{} / {}", s.soldiers_lost, s.soldiers_hired));
                        ui.end_row();
                        ui.label("Civilians saved / lost");
                        ui.label(format!("{} / {}", s.civilians_saved, s.civilians_dead));
                        ui.end_row();
                        ui.label("Shots fired / hit");
                        let pct = if s.shots_fired > 0 {
                            format!(" ({}%)", s.shots_hit * 100 / s.shots_fired)
                        } else {
                            String::new()
                        };
                        ui.label(format!("{} / {}{pct}", s.shots_fired, s.shots_hit));
                        ui.end_row();
                        ui.label("Breeds catalogued");
                        ui.label(format!(
                            "{} seen, {} dissected",
                            c.codex_seen.len(),
                            c.codex_captured.len()
                        ));
                        ui.end_row();
                    });
                });
            self.show_stats = open;
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

/// What the field reports say about each breed, once it has been met.
fn bestiary_lore(species: ods_sim::units::Species) -> &'static str {
    use ods_sim::units::Species as S;
    match species {
        S::Soldier => "One of ours.",
        S::Imp => {
            "The rabble of the Otherside. Weak alone, endless together; their \
             hellspit burns at range. Aim for the horns — they steer by them."
        }
        S::Overseer => {
            "A pack-driver. It does not merely fight: it reaches into minds and \
             squeezes. Kill it first and the rabble it drives loses its nerve."
        }
        S::Hellhound => {
            "Fast, thick-hided, and always closing. It takes a wall of reaction \
             fire to drop one mid-pounce. Never let it reach the line."
        }
        S::BileWisp => {
            "A floating gut full of acid. It lobs its bile clean over cover, \
             and bursts wetly when shot — keep your distance twice over."
        }
        S::Taker => {
            "The horror the survivors won't describe. One touch of its claws \
             and a soldier is not killed but Taken — and rises as a Husk."
        }
        S::Husk => {
            "What is left when a Taker is finished. Slow, unafraid, and \
             wearing a face from the memorial wall. Grant them rest."
        }
        S::Prince => {
            "A lord of the Otherside. Its will is a weapon: possession, terror, \
             and a court of lesser breeds. It has never known fear — teach it."
        }
        S::Gargoyle => {
            "A winged skirmisher that perches where it pleases and dives where \
             it hurts. The stone hide turns rifle fire; the wings do not."
        }
        S::Behemoth => {
            "A siege-beast. Walls are a suggestion to it; the chapterhouse's \
             own masonry becomes its rubble. Bring the lances or bring nothing."
        }
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

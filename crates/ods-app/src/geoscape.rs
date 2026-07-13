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
    /// Walk into the chapterhouse: the Basescape diorama.
    EnterBase,
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
                for n in 1..=3usize {
                    candidates.push((
                        crate::autosave_history_path(n),
                        format!("autosave −{n} day(s)"),
                    ));
                }
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
                if c.blood_moon.is_some() {
                    ui.colored_label(
                        egui::Color32::from_rgb(255, 60, 50),
                        egui::RichText::new("🌑 BLOOD MOON").strong(),
                    )
                    .on_hover_text("the packs come stronger; the salvage comes double");
                }
            });
        });

        // ---------------------------------------------- command sidebar
        // The right rail, 1994-style: the calendar, the clock, and every
        // desk the commander can be called to.
        egui::SidePanel::right("geo-command").exact_width(200.0).show(ctx, |ui| {
            ui.add_space(8.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(format!("Month {}", c.month)).size(16.0));
                ui.label(egui::RichText::new(format!("Day {}", c.day)).size(30.0).strong());
                ui.label(egui::RichText::new(c.difficulty.name()).weak().small());
            });
            ui.add_space(6.0);
            ui.separator();

            let alive = c.over.is_none();
            let sky_held = c.interception.is_some();
            ui.label("Time");
            ui.horizontal(|ui| {
                for (speed, label, hint) in [
                    (0u8, "⏸", "hold the clock"),
                    (1, "▶", "a day each twelve seconds"),
                    (2, "▶▶", "a day every three seconds"),
                    (3, "▶▶▶", "days streak past"),
                ] {
                    if ui
                        .add_enabled(
                            alive && !sky_held,
                            egui::Button::selectable(self.geo_speed == speed, label),
                        )
                        .on_hover_text(hint)
                        .clicked()
                    {
                        self.geo_speed = speed;
                    }
                }
            });
            ui.add(
                egui::ProgressBar::new(self.day_progress)
                    .desired_height(6.0)
                    .fill(egui::Color32::from_rgb(150, 120, 60)),
            );
            if sky_held {
                ui.colored_label(
                    egui::Color32::from_rgb(230, 120, 90),
                    "the sky fight holds the clock",
                );
            }
            let mut advanced = false;
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(alive && !sky_held, egui::Button::new("▶ Day"))
                    .clicked()
                {
                    let events = c.advance_day();
                    for e in &events {
                        self.log.push(narrate(c, e));
                    }
                    advanced = true;
                }
                if ui
                    .add_enabled(alive && !sky_held, egui::Button::new("⏩ Week"))
                    .clicked()
                {
                    for _ in 0..7 {
                        if c.over.is_some() || c.interception.is_some() {
                            break;
                        }
                        let events = c.advance_day();
                        for e in &events {
                            self.log.push(narrate(c, e));
                        }
                    }
                    advanced = true;
                }
            });
            if advanced {
                self.day_progress = 0.0;
                // The world remembers, whether or not you asked it to.
                crate::write_autosave(c);
            }
            ui.separator();

            let wide = [ui.available_width(), 24.0];
            if ui.add_sized(wide, egui::Button::new("🏰 Chapterhouses")).clicked() {
                action = GeoAction::EnterBase;
            }
            if ui
                .add_sized(wide, egui::Button::selectable(self.show_codex, "📖 Bestiary"))
                .clicked()
            {
                self.show_codex = !self.show_codex;
            }
            if ui
                .add_sized(wide, egui::Button::selectable(self.show_stats, "📜 Ledger"))
                .clicked()
            {
                self.show_stats = !self.show_stats;
            }
            if ui
                .add_sized(wide, egui::Button::selectable(self.show_options, "⚙ Options"))
                .clicked()
            {
                self.show_options = !self.show_options;
            }
            ui.separator();

            if c.ironman {
                ui.label(
                    egui::RichText::new("IRONMAN — the autosave is the only record")
                        .weak()
                        .small(),
                );
            } else {
                ui.horizontal(|ui| {
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
                });
            }
            if ui.add_sized(wide, egui::Button::new("Menu")).clicked() {
                self.screen = Screen::Menu;
            }
        });

        // ------------------------------------------------- the dogfight
        // Gargoyles on a led sortie's wind: the commander flies the
        // exchange from the gondola guns, one order per round.
        if let Some(it) = c.interception {
            egui::Window::new("⚔ GARGOYLES ON THE WIND")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, -40.0])
                .show(ctx, |ui| {
                    ui.label(format!(
                        "The pack found the sortie over {} — round {}",
                        it.region.name(),
                        it.round + 1
                    ));
                    ui.add(
                        egui::ProgressBar::new((it.envelope.max(0) as f32) / 100.0)
                            .text(format!("envelope {}%", it.envelope.max(0)))
                            .fill(if it.envelope > 50 {
                                egui::Color32::from_rgb(120, 160, 120)
                            } else {
                                egui::Color32::from_rgb(200, 90, 70)
                            }),
                    );
                    ui.label(format!(
                        "gargoyles on the wing: {}   downed: {}",
                        it.gargoyles, it.downed
                    ));
                    let reach = match it.range {
                        0..=3 => " — point blank, claws and muzzles",
                        4..=5 => " — in reach of guns and claws both",
                        6 => " — the guns just bite",
                        _ => " — out of reach, closing costs time",
                    };
                    ui.label(format!("range: {} span(s){reach}", it.range));
                    ui.add_space(6.0);
                    let mut order: Option<bool> = None;
                    ui.horizontal(|ui| {
                        if ui
                            .button(egui::RichText::new("🔫 Press the attack").strong())
                            .on_hover_text("close and work the guns — and give the claws a steady perch")
                            .clicked()
                        {
                            order = Some(true);
                        }
                        if ui
                            .button("🌀 Run for cloud")
                            .on_hover_text("open the range; a running target is a poor perch, and clouds hide ships")
                            .clicked()
                        {
                            order = Some(false);
                        }
                    });
                    if let Some(press) = order {
                        let rep = c.intercept_round(press);
                        let mut line = String::from("gun deck: ");
                        line.push_str(&if rep.downed > 0 {
                            format!("{} gargoyle(s) knocked burning off the wind", rep.downed)
                        } else {
                            "the volleys go wide".to_string()
                        });
                        if rep.envelope_hit > 0 {
                            line.push_str(&format!(
                                "; claws take {}% of the envelope",
                                rep.envelope_hit
                            ));
                        }
                        self.log.push(line);
                        if let Some(outcome) = rep.outcome {
                            self.log.push(
                                match outcome {
                                    ods_geo::SkyHuntOutcome::Repelled => {
                                        "the sky is ours. The sortie flies on."
                                    }
                                    ods_geo::SkyHuntOutcome::Bloodied => {
                                        "won clear — torn and listing. The squad lands bleeding."
                                    }
                                    ods_geo::SkyHuntOutcome::TurnedBack => {
                                        "the envelope gives. The zeppelin limps for home."
                                    }
                                }
                                .to_string(),
                            );
                        }
                    }
                });
        }

        // ------------------------------------------------- operations
        egui::SidePanel::left("geo-ops").default_width(360.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Incursions");
                let fit = c.soldiers.iter().filter(|s| s.is_fit()).count();
                ui.label(format!("{fit} soldiers fit for duty"));
                ui.horizontal_wrapped(|ui| {
                    ui.label("Answering:");
                    for (i, name) in ods_geo::SQUAD_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut c.active_squad, i as u8, *name);
                    }
                });
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
                        let mut squad_rotate: Option<usize> = None;
                        egui::Grid::new("roster").striped(true).show(ui, |ui| {
                            for h in ["Name", "Rank", "Quirk", "Squad", "Mind", "TU", "HP", "Acc", "K", "🧨", "✚", "Lance", "Status"] {
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
                                {
                                    let tag = ods_geo::SQUAD_NAMES[s.squad as usize];
                                    let short: String = tag.chars().take(4).collect();
                                    if ui
                                        .small_button(short)
                                        .on_hover_text(format!("standing squad: {tag} (click to rotate)"))
                                        .clicked()
                                    {
                                        squad_rotate = Some(si);
                                    }
                                }
                                let mind_color = match s.sanity {
                                    0..=20 => egui::Color32::from_rgb(220, 60, 60),
                                    21..=50 => egui::Color32::from_rgb(230, 180, 70),
                                    _ => egui::Color32::from_rgb(140, 200, 140),
                                };
                                let mind = ui.colored_label(mind_color, s.sanity.to_string());
                                if let Some(phobia) = s.phobia {
                                    mind.on_hover_text(format!("phobia: {}", phobia.name()));
                                }
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
                                if s.is_broken() {
                                    ui.colored_label(egui::Color32::from_rgb(220, 60, 60), "broken")
                                        .on_hover_text(
                                            "sanity gone: unfit until it climbs past 20 \
                                             (a Chapel mends minds three times as fast)",
                                        );
                                } else if s.warding.is_some() {
                                    ui.colored_label(egui::Color32::YELLOW, "warding");
                                } else if s.aboard.is_some() {
                                    ui.colored_label(egui::Color32::LIGHT_BLUE, "aboard");
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
                        if let Some(si) = squad_rotate {
                            c.soldiers[si].squad =
                                (c.soldiers[si].squad + 1) % ods_geo::SQUAD_NAMES.len() as u8;
                        }
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

                egui::CollapsingHeader::new("Armoury assignments").show(ui, |ui| {
                    ui.label(format!(
                        "stock — arbalest {} · censer {} · hammer {} · mortar {} | \
                         blades {} · circlets {} | plate {} · aegis {}",
                        c.weapon_stock.get("arbalest").copied().unwrap_or(0),
                        c.weapon_stock.get("censer").copied().unwrap_or(0),
                        c.weapon_stock.get("ram_hammer").copied().unwrap_or(0),
                        c.weapon_stock.get("salt_mortar").copied().unwrap_or(0),
                        c.blade_stock,
                        c.circlet_stock,
                        c.plate_stock,
                        c.aegis_stock,
                    ));
                    let mut act: Option<(usize, u8)> = None;
                    egui::Grid::new("armoury").striped(true).show(ui, |ui| {
                        for h in ["Name", "Weapon", "Blade", "Circlet", "Armor", "Relic"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for (si, s) in c.soldiers.iter().enumerate() {
                            if ui
                                .small_button(&s.name)
                                .on_hover_text("open the armoury mirror: paper doll, identity")
                                .clicked()
                            {
                                self.equip_for = Some(si);
                            }
                            if ui.small_button(s.weapon_key.replace('_', " ")).clicked() {
                                act = Some((si, 0));
                            }
                            if ui.small_button(if s.has_blade { "🗡" } else { "–" }).clicked() {
                                act = Some((si, 1));
                            }
                            if ui.small_button(if s.has_circlet { "◎" } else { "–" }).clicked() {
                                act = Some((si, 2));
                            }
                            if ui.small_button(s.armor.name()).clicked() {
                                act = Some((si, 3));
                            }
                            match &s.relic {
                                Some(r) => {
                                    if ui
                                        .small_button(&r.name)
                                        .on_hover_text(r.affix.describe())
                                        .clicked()
                                    {
                                        act = Some((si, 4)); // return it
                                    }
                                }
                                None => {
                                    ui.menu_button("–", |ui| {
                                        for (ri, r) in c.relic_pool.iter().enumerate() {
                                            if ui
                                                .button(format!(
                                                    "{} ({})",
                                                    r.name,
                                                    r.affix.describe()
                                                ))
                                                .clicked()
                                            {
                                                act = Some((si, 10 + ri as u8));
                                                ui.close();
                                            }
                                        }
                                        if c.relic_pool.is_empty() {
                                            ui.label("the reliquary is bare");
                                        }
                                    });
                                }
                            }
                            ui.end_row();
                        }
                    });
                    if let Some((si, what)) = act {
                        let outcome = match what {
                            0 => c.cycle_weapon(si).map(|_| ()),
                            1 => c.toggle_blade(si),
                            2 => c.toggle_circlet(si),
                            3 => c.cycle_armor(si).map(|_| ()),
                            4 => c.assign_relic(si, None),
                            n => c.assign_relic(si, Some((n - 10) as usize)),
                        };
                        if let Err(e) = outcome {
                            self.log.push(format!("cannot issue: {e:?}"));
                        }
                    }
                });

                let maimed: Vec<usize> = c
                    .soldiers
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| !s.lost_parts.is_empty())
                    .map(|(i, _)| i)
                    .collect();
                if !maimed.is_empty() {
                    egui::CollapsingHeader::new(format!(
                        "Infirmary — the maimed ({}) | limbs {} · grafts {}",
                        maimed.len(),
                        c.limb_stock,
                        c.graft_stock
                    ))
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.label(
                            "Hellsteel limbs restore what was lost. Flesh grafts restore \
                             it better — and cost the wearer a piece of their sleep.",
                        );
                        let mut fit: Option<(usize, bool)> = None;
                        for &i in &maimed {
                            let s = &c.soldiers[i];
                            ui.horizontal(|ui| {
                                let lost: Vec<&str> =
                                    s.lost_parts.iter().map(|p| p.name()).collect();
                                ui.label(format!("{} — lost: {}", s.name, lost.join(", ")));
                                if ui
                                    .add_enabled(c.limb_stock > 0, egui::Button::new("🦾 Fit limb"))
                                    .clicked()
                                {
                                    fit = Some((i, false));
                                }
                                if ui
                                    .add_enabled(c.graft_stock > 0, egui::Button::new("🩸 Graft"))
                                    .clicked()
                                {
                                    fit = Some((i, true));
                                }
                            });
                        }
                        if let Some((i, graft)) = fit {
                            match c.fit_replacement(i, graft) {
                                Ok(()) => self.log.push(format!(
                                    "{} is made whole{}",
                                    c.soldiers[i].name,
                                    if graft { " — with something that was never theirs." } else { "." }
                                )),
                                Err(e) => self.log.push(format!("cannot fit: {e:?}")),
                            }
                        }
                    });
                }

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

        // Under a blood moon the whole sky is a wound.
        if c.blood_moon.is_some() {
            let painter = ctx.layer_painter(egui::LayerId::new(
                egui::Order::Background,
                egui::Id::new("blood-moon-wash"),
            ));
            painter.rect_filled(
                ctx.viewport_rect(),
                0.0,
                egui::Color32::from_rgba_unmultiplied(140, 10, 10, 26),
            );
        }

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
                    if c.corrupted_patrons.contains(&region) {
                        ui.colored_label(
                            egui::Color32::from_rgb(230, 80, 200),
                            format!("{} serves the other side", region.patron()),
                        );
                        ui.horizontal(|ui| {
                            if ui.button("⚔ Lead the purge").clicked() {
                                action = GeoAction::LeadMission(MissionKind::Purge(region));
                            }
                            if ui.button("🎲 Auto").clicked() {
                                match c.purge_patron(region) {
                                    Ok(r) => self.log.push(report_line("purge", r)),
                                    Err(e) => self.log.push(format!("cannot purge: {e:?}")),
                                }
                            }
                        });
                    } else {
                        ui.label(format!("Patron: {}", region.patron()));
                    }
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
                            let slain = c.codex_slain.contains(&species);
                            ui.horizontal(|ui| {
                                ui.strong(species.name());
                                if slain {
                                    ui.colored_label(egui::Color32::from_rgb(200, 90, 70), "☠ necropsied");
                                }
                                if captured {
                                    ui.colored_label(egui::Color32::GOLD, "⛓ dissected");
                                }
                            });
                            ui.label(bestiary_lore(species));
                            if slain {
                                let key = species.name().to_lowercase().replace(' ', "_");
                                if let Some(d) = ods_sim::data::species().get(&key) {
                                    ui.label(format!(
                                        "Necropsy: {} TU · {} HP · armor {}/{}/{}",
                                        d.tu, d.health, d.armor.0, d.armor.1, d.armor.2
                                    ));
                                }
                            }
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

        // ------------------------------------------- the armoury mirror
        // A soldier stands before the glass: identity above, the paper
        // doll below, every slot clickable.
        if let Some(si) = self.equip_for {
            if si >= c.soldiers.len() {
                self.equip_for = None;
            } else {
                let mut open = true;
                egui::Window::new("The Armoury Mirror")
                    .open(&mut open)
                    .default_width(300.0)
                    .resizable(false)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("Name");
                            ui.text_edit_singleline(&mut c.soldiers[si].name);
                        });
                        ui.horizontal(|ui| {
                            ui.label("Callsign");
                            ui.add(
                                egui::TextEdit::singleline(&mut c.soldiers[si].callsign)
                                    .hint_text("what the squad shouts"),
                            );
                        });
                        ui.separator();
                        paper_doll(ui, c, si, &mut self.log);
                    });
                if !open {
                    self.equip_for = None;
                }
            }
        }

        // -------------------------------------------- after-action debrief
        if let Some(d) = &self.debrief {
            let mut dismiss = false;
            egui::Window::new(if d.victory {
                "AFTER ACTION — THE FIELD HELD"
            } else {
                "AFTER ACTION — REPELLED"
            })
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, -30.0])
            .show(ctx, |ui| {
                ui.strong(&d.label);
                ui.label(format!(
                    "{} turns · {} demons slain{}{}",
                    d.turns,
                    d.demons_slain,
                    if d.captures > 0 {
                        format!(" · {} bound alive", d.captures)
                    } else {
                        String::new()
                    },
                    if d.civilians.0 + d.civilians.1 > 0 {
                        format!(" · civilians {} saved / {} lost", d.civilians.0, d.civilians.1)
                    } else {
                        String::new()
                    },
                ));
                if !d.fallen.is_empty() {
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(200, 90, 80),
                        "The Wall remembers:",
                    );
                    for name in &d.fallen {
                        ui.label(format!("  ✝ {name}"));
                    }
                }
                if !d.commendations.is_empty() {
                    ui.separator();
                    ui.colored_label(
                        egui::Color32::from_rgb(230, 200, 120),
                        "Commendations:",
                    );
                    for line in &d.commendations {
                        ui.label(format!("  {line}"));
                    }
                }
                ui.add_space(6.0);
                if ui.button("Dismiss").clicked() {
                    dismiss = true;
                }
            });
            if dismiss {
                self.debrief = None;
            }
        }

        // -------------------------------------------------- game over/won
        let (mut wants_chronicle, mut wants_dawn, mut wants_menu) = (false, false, false);
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
                    if ui
                        .button("📜 Write the chronicle")
                        .on_hover_text("the whole war, month by month, as a document")
                        .clicked()
                    {
                        wants_chronicle = true;
                    }
                    if outcome == ods_geo::CampaignOutcome::Victory
                        && ui
                            .button(egui::RichText::new("🌅 THE SECOND DAWN").strong())
                            .on_hover_text(
                                "keep fighting: the veil stays cracked, hell comes harder, \
                                 and the Ledger becomes the scoreboard",
                            )
                            .clicked()
                    {
                        wants_dawn = true;
                    }
                    if ui.button("Back to menu").clicked() {
                        wants_menu = true;
                    }
                });
            if wants_chronicle {
                let path = format!("chronicle-m{}.md", c.month);
                match std::fs::write(&path, write_chronicle(c, &self.log)) {
                    Ok(()) => self.log.push(format!("The chronicle is written: {path}")),
                    Err(e) => self.log.push(format!("cannot write chronicle: {e}")),
                }
            }
            if wants_dawn && c.second_dawn().is_ok() {
                self.log.push("The Second Dawn. The war goes on, harder.".to_string());
            }
            if wants_menu {
                self.screen = Screen::Menu;
                self.campaign = None;
            }
        }

        action
    }

    /// The Basescape screen: the diorama orbits behind, the desk sits on
    /// the right. Returns true when the commander walks back out.
    pub fn base_ui(&mut self, ctx: &egui::Context) -> bool {
        let mut back = false;
        let Some(c) = &mut self.campaign else {
            self.screen = Screen::Menu;
            return false;
        };

        egui::TopBottomPanel::top("base-top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("⬅ Geoscape").clicked() {
                    back = true;
                }
                ui.separator();
                let bi = self.selected_base.min(c.bases.len() - 1);
                ui.strong(format!(
                    "Chapterhouse — {}   ·   Treasury {}k   ·   🜏 {}   ⛓ {}",
                    c.bases[bi].region.name(),
                    c.funds,
                    c.brimstone,
                    c.hellsteel
                ));
            });
        });

        egui::SidePanel::right("base-desk").default_width(390.0).show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                chapterhouse_panel(
                    ui,
                    c,
                    &mut self.selected_base,
                    &mut self.build_choice,
                    &mut self.log,
                    &mut self.base_dirty,
                );
            });
        });

        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ctx, |ui| {
                ui.label(
                    egui::RichText::new("right-drag: walk around the halls · scroll: zoom")
                        .weak()
                        .small(),
                );
            });
        back
    }
}

/// What the field reports say about each breed, once it has been met.
pub(crate) fn bestiary_lore(species: ods_sim::units::Species) -> &'static str {
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

/// Assemble the war's written history from the campaign and the log:
/// the record an ironman run leaves behind.
fn write_chronicle(c: &Campaign, log: &[String]) -> String {
    let mut out = String::new();
    out.push_str("# The Chronicle of the War of the Otherside\n\n");
    out.push_str(&format!(
        "*{} months under arms — difficulty {}{}.*\n\n",
        c.month,
        c.difficulty.name(),
        if c.ironman { ", ironman" } else { "" }
    ));
    let s = c.stats;
    out.push_str("## The Ledger\n\n");
    out.push_str(&format!(
        "- Missions won / lost: {} / {}\n- Rifts banished: {}\n- Nests razed: {}\n         - Reckonings repelled: {}\n- Demons slain / bound: {} / {}\n         - Soldiers lost / hired: {} / {}\n- Civilians saved / lost: {} / {}\n\n",
        s.missions_won,
        s.missions_lost,
        s.rifts_banished,
        s.nests_razed,
        s.reckonings_repelled,
        s.demons_slain,
        s.demons_captured,
        s.soldiers_lost,
        s.soldiers_hired,
        s.civilians_saved,
        s.civilians_dead,
    ));
    if let Some(n) = &c.nemesis {
        out.push_str(&format!(
            "**{} still walks**, {} escapes to its name.\n\n",
            n.name, n.escapes
        ));
    }
    if !c.memorial.is_empty() {
        out.push_str("## The Wall of the Fallen\n\n");
        for f in &c.memorial {
            out.push_str(&format!(
                "- {} {}, month {}: {} missions, {} kills — fell at {}\n",
                f.rank, f.name, f.month, f.missions, f.kills, f.cause
            ));
        }
        out.push('\n');
    }
    out.push_str("## The Record, Day by Day\n\n");
    for line in log {
        out.push_str(&format!("{line}\n\n"));
    }
    out
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

/// The chapterhouse desk: build grid, founding, hiring, drills, the Codex,
/// and the Workshop. Shared by the Basescape's side panel.
fn chapterhouse_panel(
    ui: &mut egui::Ui,
    c: &mut Campaign,
    selected_base: &mut usize,
    build_choice: &mut Facility,
    log: &mut Vec<String>,
    base_dirty: &mut bool,
) {
    let before = (*selected_base, snapshot(c, *selected_base));
                ui.horizontal_wrapped(|ui| {
                    for (i, b) in c.bases.iter().enumerate() {
                        ui.selectable_value(selected_base, i, b.region.name());
                    }
                });
                *selected_base = (*selected_base).min(c.bases.len() - 1);
                let bi = *selected_base;
                ui.heading(format!("Chapterhouse — {}", c.bases[bi].region.name()));

                ui.horizontal_wrapped(|ui| {
                    for f in Facility::BUILDABLE {
                        ui.selectable_value(
                            build_choice,
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
                                && let Err(e) = c.start_build(bi, *build_choice, x, y)
                            {
                                log.push(format!("cannot build: {e:?}"));
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
                                    Ok(()) => log.push(format!(
                                        "A new chapterhouse rises in {}.",
                                        region.name()
                                    )),
                                    Err(e) => log.push(format!("cannot found: {e:?}")),
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
                                log.push(line);
                            }
                            Err(e) => log.push(format!("cannot hire: {e:?}")),
                        }
                    }
                    if ui
                        .button(format!("Occultist ({}k)", ods_geo::OCCULTIST_HIRE_COST))
                        .clicked()
                    {
                        match c.hire_occultist() {
                            Ok(()) => log.push("An occultist joins.".to_string()),
                            Err(e) => log.push(format!("cannot hire: {e:?}")),
                        }
                    }
                    if ui
                        .button(format!("Artificer ({}k)", ods_geo::ARTIFICER_HIRE_COST))
                        .clicked()
                    {
                        match c.hire_artificer() {
                            Ok(()) => log.push("An artificer joins.".to_string()),
                            Err(e) => log.push(format!("cannot hire: {e:?}")),
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

                if c.bases.iter().any(|b| b.count_active(Facility::TrainingGround) > 0) {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("Drills:");
                        for f in ods_geo::Focus::ALL {
                            ui.selectable_value(&mut c.training_focus, f, f.name());
                        }
                    });
                }

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
                            log.push(format!("cannot research: {e:?}"));
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
                            log.push(format!("cannot make: {e:?}"));
                        }
                        ui.label(format!("{} ({}pts, {brim}🜏 {steel}⛓)", item.name(), item.cost()));
                    });
                }
    // Anything that changed the halls redraws the diorama.
    if before.0 != *selected_base || before.1 != snapshot(c, *selected_base) {
        *base_dirty = true;
    }
}

/// A cheap fingerprint of a base's construction state.
fn snapshot(c: &Campaign, bi: usize) -> (usize, usize) {
    let bi = bi.min(c.bases.len() - 1);
    let cells = c.bases[bi].occupied_cells().len();
    (c.bases.len(), cells)
}

/// The paper doll: a painted silhouette wearing what the soldier wears,
/// with a clickable slot beside each piece of it.
fn paper_doll(ui: &mut egui::Ui, c: &mut Campaign, si: usize, log: &mut Vec<String>) {
    use ods_geo::ArmorTier;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(270.0, 210.0), egui::Sense::hover());
    let paint = ui.painter_at(rect);
    let s = &c.soldiers[si];
    let cx = rect.center().x;
    let top = rect.min.y + 6.0;

    let armor_color = match s.armor {
        ArmorTier::Vestments => egui::Color32::from_rgb(88, 74, 58),
        ArmorTier::Plate => egui::Color32::from_rgb(120, 126, 138),
        ArmorTier::Aegis => egui::Color32::from_rgb(168, 142, 70),
    };
    let skin = egui::Color32::from_rgb(196, 160, 130);
    let cloth = egui::Color32::from_rgb(60, 52, 46);

    // Head, and the circlet's teal ring when worn.
    paint.circle_filled(egui::pos2(cx, top + 20.0), 14.0, skin);
    if s.has_circlet {
        paint.circle_stroke(
            egui::pos2(cx, top + 14.0),
            15.0,
            egui::Stroke::new(2.5, egui::Color32::from_rgb(40, 220, 195)),
        );
    }
    // Torso in the armor's color; pauldron nubs when armored.
    paint.rect_filled(
        egui::Rect::from_center_size(egui::pos2(cx, top + 66.0), egui::vec2(46.0, 56.0)),
        4.0,
        armor_color,
    );
    if s.armor != ArmorTier::Vestments {
        for side in [-1.0f32, 1.0] {
            paint.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(cx + side * 27.0, top + 44.0),
                    egui::vec2(12.0, 10.0),
                ),
                3.0,
                armor_color,
            );
        }
    }
    // Arms and legs.
    for side in [-1.0f32, 1.0] {
        paint.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(cx + side * 31.0, top + 72.0),
                egui::vec2(9.0, 44.0),
            ),
            3.0,
            cloth,
        );
        paint.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(cx + side * 11.0, top + 122.0),
                egui::vec2(12.0, 52.0),
            ),
            3.0,
            cloth,
        );
    }
    // The weapon along the right arm; the blade at the left hip.
    paint.rect_filled(
        egui::Rect::from_center_size(egui::pos2(cx + 42.0, top + 70.0), egui::vec2(6.0, 62.0)),
        2.0,
        egui::Color32::from_rgb(50, 44, 38),
    );
    if s.has_blade {
        paint.rect_filled(
            egui::Rect::from_center_size(egui::pos2(cx - 36.0, top + 96.0), egui::vec2(4.0, 26.0)),
            1.0,
            egui::Color32::from_rgb(200, 200, 190),
        );
    }
    // Relic charm over the heart.
    if s.relic.is_some() {
        paint.circle_filled(egui::pos2(cx - 12.0, top + 52.0), 4.5, egui::Color32::GOLD);
    }

    // The slots down each margin, clicking straight into the armoury.
    let mut act: Option<u8> = None;
    let slot = |ui: &mut egui::Ui, y: f32, right: bool, label: String, hover: &str| -> bool {
        let w = 86.0;
        let x = if right { rect.max.x - w - 2.0 } else { rect.min.x + 2.0 };
        ui.put(
            egui::Rect::from_min_size(egui::pos2(x, y), egui::vec2(w, 20.0)),
            egui::Button::new(egui::RichText::new(label).small()),
        )
        .on_hover_text(hover)
        .clicked()
    };
    let s = &c.soldiers[si];
    if slot(ui, top + 8.0, false, format!("◎ {}", if s.has_circlet { "circlet" } else { "—" }), "warded circlet: takes one psi blow") {
        act = Some(2);
    }
    if slot(ui, top + 44.0, false, format!("🛡 {}", s.armor.name()), "cycle armor tier") {
        act = Some(3);
    }
    if slot(
        ui,
        top + 80.0,
        false,
        match &s.relic {
            Some(r) => format!("★ {}", r.name),
            None => "★ relic —".to_string(),
        },
        "take the first relic from the reliquary / return this one",
    ) {
        act = Some(4);
    }
    if slot(ui, top + 8.0, true, format!("⚔ {}", s.weapon_key.replace('_', " ")), "cycle issued weapon") {
        act = Some(0);
    }
    if slot(ui, top + 44.0, true, format!("🗡 {}", if s.has_blade { "blade" } else { "—" }), "consecrated blade: ripostes melee") {
        act = Some(1);
    }

    if let Some(what) = act {
        let outcome = match what {
            0 => c.cycle_weapon(si).map(|_| ()),
            1 => c.toggle_blade(si),
            2 => c.toggle_circlet(si),
            3 => c.cycle_armor(si).map(|_| ()),
            _ => {
                if c.soldiers[si].relic.is_some() {
                    c.assign_relic(si, None)
                } else if c.relic_pool.is_empty() {
                    log.push("the reliquary is bare".to_string());
                    Ok(())
                } else {
                    c.assign_relic(si, Some(0))
                }
            }
        };
        if let Err(e) = outcome {
            log.push(format!("cannot issue: {e:?}"));
        }
    }
}

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
        // Embers rise off the diorama behind everything, and a slow sigil
        // ring turns behind the title.
        let screen = ctx.viewport_rect();
        let painter = ctx.layer_painter(egui::LayerId::new(
            egui::Order::Background,
            egui::Id::new("title-dressing"),
        ));
        let t = self.clock;
        let hash = |i: u32| -> f32 {
            let mut h = i.wrapping_mul(2654435761).wrapping_add(0x9E3779B9);
            h ^= h >> 15;
            ((h.wrapping_mul(2246822519) >> 9) & 1023) as f32 / 1023.0
        };
        for i in 0..44u32 {
            let hx = hash(i);
            let speed = 14.0 + hash(i + 100) * 26.0;
            let cycle = screen.height() + 40.0;
            let y = screen.max.y - ((t * speed + hash(i + 200) * cycle) % cycle);
            let x = screen.min.x
                + hx * screen.width()
                + (t * (0.6 + hash(i + 300)) + i as f32).sin() * 9.0;
            let flicker = 0.4 + 0.6 * ((t * 3.0 + i as f32 * 1.7).sin() * 0.5 + 0.5);
            let a = (flicker * 130.0) as u8;
            painter.circle_filled(
                egui::pos2(x, y),
                1.0 + hash(i + 400) * 1.8,
                egui::Color32::from_rgba_unmultiplied(255, 130 + (hash(i + 500) * 60.0) as u8, 40, a),
            );
        }
        // The sigil: two counter-turning rings high behind the card.
        let sigil_c = egui::pos2(screen.center().x, screen.min.y + 108.0);
        let pulse = 0.5 + 0.5 * (t * 1.1).sin();
        painter.circle_stroke(
            sigil_c,
            64.0 + pulse * 3.0,
            egui::Stroke::new(1.5, egui::Color32::from_rgba_unmultiplied(190, 40, 30, 90)),
        );
        painter.circle_stroke(
            sigil_c,
            52.0 - pulse * 3.0,
            egui::Stroke::new(1.0, egui::Color32::from_rgba_unmultiplied(150, 30, 24, 70)),
        );
        for k in 0..5 {
            let a = t * 0.35 + k as f32 * std::f32::consts::TAU / 5.0;
            let p = sigil_c + egui::vec2(a.cos(), a.sin()) * 58.0;
            painter.circle_filled(p, 2.2, egui::Color32::from_rgba_unmultiplied(220, 60, 40, 120));
        }

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

        // The build wears its number quietly.
        painter.text(
            screen.max - egui::vec2(10.0, 8.0),
            egui::Align2::RIGHT_BOTTOM,
            format!("v{}", env!("CARGO_PKG_VERSION")),
            egui::FontId::proportional(11.0),
            egui::Color32::from_gray(110),
        );
    }

    fn menu_card(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
                // The wordmark, in the house pixel face.
                let px = 4.0;
                let w = crate::pixfont::width("OTHERSIDE DEFENSE", px);
                let (rect, _) = ui.allocate_exact_size(
                    egui::vec2(w + 2.0 * px, 8.0 * px),
                    egui::Sense::hover(),
                );
                crate::pixfont::draw_centered(
                    &ui.painter_at(rect.expand(8.0)),
                    rect.center(),
                    px,
                    egui::Color32::from_rgb(226, 184, 96),
                    "OTHERSIDE DEFENSE",
                );
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

                // Continue: straight back into the newest record on disk.
                let newest = {
                    let mut paths = vec![
                        SAVE_PATH.to_string(),
                        crate::AUTOSAVE_PATH.to_string(),
                    ];
                    for slot in 1..=3usize {
                        paths.push(crate::slot_path(slot));
                        paths.push(crate::autosave_history_path(slot));
                    }
                    paths
                        .into_iter()
                        .filter_map(|p| {
                            let m = std::fs::metadata(&p).ok()?.modified().ok()?;
                            Some((p, m))
                        })
                        .max_by_key(|(_, m)| *m)
                        .map(|(p, _)| p)
                };
                if let Some(path) = newest
                    && ui
                        .button(egui::RichText::new("Continue the war").size(18.0).strong())
                        .clicked()
                {
                    match std::fs::read_to_string(&path)
                        .map_err(|e| e.to_string())
                        .and_then(|s| Campaign::load_from_str(&s).map_err(|e| e.to_string()))
                    {
                        Ok(c) => {
                            self.log = vec![format!(
                                "The war resumes: month {}, day {}, {}k banked.",
                                c.month, c.day, c.funds
                            )];
                            self.campaign = Some(c);
                            self.enter_geoscape();
                        }
                        Err(e) => self.status = Some(format!("load failed: {e}")),
                    }
                }
                ui.add_space(6.0);
                if ui.button(egui::RichText::new("New campaign").size(18.0)).clicked() {
                    self.maybe_hint(
                        "start",
                        "The Geoscape: run time with the sidebar (it pauses itself when \
                         anything happens). The augurs only see rifts in regions they \
                         watch — coverage is life.",
                    );
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
                    // The record card: what this save actually holds.
                    let desc = std::fs::read_to_string(&path)
                        .ok()
                        .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                        .map(|v| {
                            let soldiers =
                                v["soldiers"].as_array().map(|a| a.len()).unwrap_or(0);
                            format!(
                                "month {} · {}k · {} soldiers · {}{}",
                                v["month"],
                                v["funds"],
                                soldiers,
                                v["difficulty"].as_str().unwrap_or("?"),
                                if v["ironman"].as_bool().unwrap_or(false) {
                                    " · IRONMAN"
                                } else {
                                    ""
                                }
                            )
                        })
                        .unwrap_or_else(|| "unreadable record".to_string());
                    ui.horizontal(|ui| {
                        if ui
                            .button(egui::RichText::new(format!("Load {label}")).size(16.0))
                            .on_hover_text(&desc)
                            .clicked()
                        {
                            match std::fs::read_to_string(&path)
                                .map_err(|e| e.to_string())
                                .and_then(|s| {
                                    Campaign::load_from_str(&s).map_err(|e| e.to_string())
                                }) {
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
                        ui.label(egui::RichText::new(&desc).weak().small());
                        let staged = self.pending_delete.as_deref() == Some(path.as_str());
                        let del_label = if staged { "Certain?" } else { "🗑" };
                        if ui
                            .small_button(del_label)
                            .on_hover_text("delete this record")
                            .clicked()
                        {
                            if staged {
                                let _ = std::fs::remove_file(&path);
                                self.pending_delete = None;
                            } else {
                                self.pending_delete = Some(path.clone());
                            }
                        }
                    });
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

    // ------------------------------------------------------------------
    // The desk unfolds: what used to crowd the war room gets a full
    // screen each — muster rolls, the forge, the bestiary, the mirror.

    fn desk_top(&mut self, ctx: &egui::Context, title: &str, back: Screen) {
        egui::TopBottomPanel::top("desk-screen-top").show(ctx, |ui| {
            ui.horizontal(|ui| {
                if ui.button("← Back").clicked() {
                    self.screen = back;
                }
                ui.heading(title);
            });
        });
    }

    pub fn codex_screen(&mut self, ctx: &egui::Context) {
        self.desk_top(ctx, "Bestiary of the Otherside", Screen::Geoscape);
        egui::CentralPanel::default().frame(desk_fill()).show(ctx, |ui| {
            let Some(c) = &mut self.campaign else { return };
            ui.label(
                "Field reports describe what the squads have met. Take a specimen \
                 alive and the occultists open it up — anatomy and all.",
            );
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_max_width(640.0);
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
                            if slain {
                                // The plate: the specimen as the carvers
                                // recorded it, slowly turned.
                                let yaw = ui.input(|i| i.time) as f32 * 0.4;
                                crate::portraits::draw_figure_iso(ui, species, None, 110.0, yaw);
                                ui.ctx().request_repaint();
                            }
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
    }

    pub fn roster_screen(&mut self, ctx: &egui::Context) {
        self.desk_top(ctx, "Muster rolls & armoury", Screen::Geoscape);
        egui::CentralPanel::default().frame(desk_fill()).show(ctx, |ui| {
            egui::ScrollArea::both().show(ui, |ui| {
                let Some(c) = &mut self.campaign else { return };
                ui.add_space(6.0);
                egui::CollapsingHeader::new("Roster & loadouts")
                    .default_open(true)
                    .show(ui, |ui| {
                        let lance_ok = c.research.is_complete(Project::HellfireLance);
                        let mut lance_toggle: Option<(usize, bool)> = None;
                        let mut transfer: Option<usize> = None;
                        let mut squad_rotate: Option<usize> = None;
                        // Sorting and density are the commander's choice.
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut self.roster_compact, "compact")
                                .on_hover_text("vitals only: hide kit and skill columns");
                            if ui
                                .small_button("issue standard kit")
                                .on_hover_text(
                                    "2 charges, 2 dressings, 2 blessed magazines, all hands",
                                )
                                .clicked()
                            {
                                for s in c.soldiers.iter_mut() {
                                    s.grenades_loadout = 2;
                                    s.dressings_loadout = 2;
                                    s.mags_loadout = 2;
                                    s.mag_pref = ods_sim::units::MagKind::Blessed;
                                }
                                self.log.push(
                                    "The quartermaster issues the standard kit to all hands."
                                        .to_string(),
                                );
                            }
                            if self.roster_sort.is_some() && ui.small_button("muster order").clicked()
                            {
                                self.roster_sort = None;
                            }
                        });
                        let compact = self.roster_compact;
                        // Display order: click a sortable header to reorder.
                        let mut order: Vec<usize> = (0..c.soldiers.len()).collect();
                        if let Some((col, asc)) = self.roster_sort {
                            order.sort_by_key(|&i| {
                                let s = &c.soldiers[i];
                                match col {
                                    0 => s.sanity as i64,
                                    1 => s.stats.tu as i64,
                                    2 => s.stats.health as i64,
                                    3 => s.stats.accuracy as i64,
                                    4 => s.kills as i64,
                                    _ => s.missions as i64,
                                }
                            });
                            if !asc {
                                order.reverse();
                            }
                        }
                        let sort_state = &mut self.roster_sort;
                        // The full table is wider than the desk: scroll it
                        // sideways rather than let it shove the panel open.
                        egui::ScrollArea::horizontal().id_salt("roster-h").show(ui, |ui| {
                        egui::Grid::new("roster").striped(true).show(ui, |ui| {
                            let mut header =
                                |ui: &mut egui::Ui, label: &str, col: Option<usize>| {
                                    let arrow = match (col, *sort_state) {
                                        (Some(c1), Some((c2, true))) if c1 == c2 => " ▲",
                                        (Some(c1), Some((c2, false))) if c1 == c2 => " ▼",
                                        _ => "",
                                    };
                                    let resp = ui.add(
                                        egui::Label::new(
                                            egui::RichText::new(format!("{label}{arrow}"))
                                                .strong(),
                                        )
                                        .sense(egui::Sense::click()),
                                    );
                                    if let Some(c1) = col
                                        && resp
                                            .on_hover_text("click to sort")
                                            .clicked()
                                    {
                                        *sort_state = match *sort_state {
                                            Some((c2, true)) if c2 == c1 => Some((c1, false)),
                                            Some((c2, false)) if c2 == c1 => None,
                                            _ => Some((c1, true)),
                                        };
                                    }
                                };
                            header(ui, "Name", None);
                            header(ui, "Rank", Some(5));
                            if !compact {
                                header(ui, "Quirk", None);
                                header(ui, "Squad", None);
                            }
                            header(ui, "Mind", Some(0));
                            header(ui, "TU", Some(1));
                            if !compact {
                                header(ui, "Sta", None);
                            }
                            header(ui, "HP", Some(2));
                            header(ui, "Fir", Some(3));
                            if !compact {
                                header(ui, "Thr", None);
                                header(ui, "Mel", None);
                                header(ui, "Str", None);
                            }
                            header(ui, "K", Some(4));
                            if !compact {
                                header(ui, "🧨", None);
                                header(ui, "✚", None);
                                header(ui, "Lance", None);
                            }
                            header(ui, "Status", None);
                            ui.end_row();
                            for si in order {
                                let s = &mut c.soldiers[si];
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
                                {
                                    let rank = ui.label(s.rank());
                                    if let Some(calling) = ods_geo::calling_from(&s.deeds) {
                                        rank.on_hover_text(format!(
                                            "called {} — {}",
                                            calling.name(),
                                            calling.blurb()
                                        ));
                                    }
                                }
                                if !compact {
                                    ui.label(s.quirk.map_or("–", |q| q.name()));
                                    let tag = ods_geo::SQUAD_NAMES[s.squad as usize];
                                    let short: String = tag.chars().take(4).collect();
                                    if ui
                                        .small_button(short)
                                        .on_hover_text(format!(
                                            "standing squad: {tag} (click to rotate)"
                                        ))
                                        .clicked()
                                    {
                                        squad_rotate = Some(si);
                                    }
                                }
                                let mind_color = match (s.sanity, self.colorblind) {
                                    (0..=20, false) => egui::Color32::from_rgb(220, 60, 60),
                                    (0..=20, true) => egui::Color32::from_rgb(235, 140, 30),
                                    (21..=50, _) => egui::Color32::from_rgb(230, 180, 70),
                                    (_, false) => egui::Color32::from_rgb(140, 200, 140),
                                    (_, true) => egui::Color32::from_rgb(120, 170, 235),
                                };
                                let mind = ui.colored_label(mind_color, s.sanity.to_string());
                                if let Some(phobia) = s.phobia {
                                    mind.on_hover_text(format!("phobia: {}", phobia.name()));
                                }
                                ui.label(s.stats.tu.to_string());
                                if !compact {
                                    ui.label(s.stats.stamina.to_string());
                                }
                                ui.label(s.stats.health.to_string());
                                ui.label(s.stats.accuracy.to_string())
                                    .on_hover_text("firing accuracy");
                                if !compact {
                                    ui.label(s.stats.throwing.to_string())
                                        .on_hover_text("throwing accuracy");
                                    ui.label(s.stats.melee.to_string())
                                        .on_hover_text("melee accuracy");
                                    ui.label(s.stats.strength.to_string());
                                }
                                ui.label(s.kills.to_string());
                                if !compact {
                                    ui.horizontal(|ui| {
                                        if ui.small_button("-").clicked()
                                            && s.grenades_loadout > 0
                                        {
                                            s.grenades_loadout -= 1;
                                        }
                                        ui.label(s.grenades_loadout.to_string());
                                        if ui.small_button("+").clicked()
                                            && s.grenades_loadout < 4
                                        {
                                            s.grenades_loadout += 1;
                                        }
                                    });
                                    ui.horizontal(|ui| {
                                        if ui.small_button("-").clicked()
                                            && s.dressings_loadout > 0
                                        {
                                            s.dressings_loadout -= 1;
                                        }
                                        ui.label(s.dressings_loadout.to_string());
                                        if ui.small_button("+").clicked()
                                            && s.dressings_loadout < 4
                                        {
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
                            "Heavy packs slow the hand (capacity 2 + Str/8; two magazines ride \
                             as one item). Lances: {}. Blessed magazines: {}.",
                            c.lance_stock,
                            c.quarrel_stock
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
                    egui::ScrollArea::horizontal().id_salt("armoury-h").show(ui, |ui| {
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
                                self.screen = Screen::Equip;
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
            });
        });
    }

    pub fn forge_screen(&mut self, ctx: &egui::Context) {
        self.desk_top(ctx, "The Forge", Screen::Geoscape);
        egui::CentralPanel::default().frame(desk_fill()).show(ctx, |ui| {
            let Some(c) = &mut self.campaign else { return };
            ui.label(format!(
                "Stores: {} brimstone · {} hellsteel | quarrels {} · cold iron {} · salt {}",
                c.brimstone, c.hellsteel, c.quarrel_stock, c.coldiron_stock, c.salt_stock,
            ));
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.set_max_width(720.0);
                for bi in 0..c.bases.len() {
                    ui.heading(format!("{} — workshop", c.bases[bi].region.name()));
                    match c.jobs.iter().find(|j| j.base == bi) {
                        Some(j) => {
                            let done = j.item.cost().saturating_sub(j.left) as f32;
                            ui.add(
                                egui::ProgressBar::new(done / j.item.cost() as f32)
                                    .text(format!("{} — {} left", j.item.name(), j.left)),
                            );
                        }
                        None => {
                            ui.label(if c.bases[bi].workshop_capacity() == 0 {
                                "No workshop in this house."
                            } else if c.bases[bi].artificers == 0 {
                                "Benches stand ready; no smiths posted here."
                            } else {
                                "The benches are idle."
                            });
                        }
                    }
                    if c.bases[bi].workshop_capacity() > 0 {
                        for item in ManufactureItem::ALL {
                            // Unresearched patterns don't exist here either.
                            if item
                                .required_research()
                                .is_some_and(|p| !c.research.is_complete(p))
                            {
                                continue;
                            }
                            let (brim, steel) = item.materials();
                            ui.horizontal(|ui| {
                                if ui.button("Make").clicked()
                                    && let Err(e) = c.start_manufacture(bi, item)
                                {
                                    self.log.push(format!("cannot make: {e:?}"));
                                }
                                ui.label(format!(
                                    "{} ({}pts, {brim}🜏 {steel}⛓)",
                                    item.name(),
                                    item.cost()
                                ))
                                .on_hover_text(item_lore(item.name()));
                            });
                        }
                    }
                    ui.add_space(8.0);
                    ui.separator();
                }
            });
        });
    }

    pub fn equip_screen(&mut self, ctx: &egui::Context) {
        self.desk_top(ctx, "The Armoury Mirror", Screen::Roster);
        let Some(si) = self.equip_for else {
            self.screen = Screen::Roster;
            return;
        };
        if self.campaign.as_ref().is_none_or(|c| si >= c.soldiers.len()) {
            self.equip_for = None;
            self.screen = Screen::Roster;
            return;
        }
        let (species, weapon_key) = self
            .campaign
            .as_ref()
            .map(|c| {
                (
                    ods_sim::units::Species::Soldier,
                    c.soldiers[si].weapon_key.clone(),
                )
            })
            .unwrap_or((ods_sim::units::Species::Soldier, String::new()));
        egui::CentralPanel::default().frame(desk_fill()).show(ctx, |ui| {
            ui.horizontal_top(|ui| {
                // The soldier in the glass: their carved figure, slowly
                // turning, carrying the weapon on their sheet.
                ui.vertical(|ui| {
                    ui.set_width(240.0);
                    let yaw = ui.input(|i| i.time) as f32 * 0.6;
                    crate::portraits::draw_figure_iso(
                        ui,
                        species,
                        Some(&weapon_key),
                        220.0,
                        yaw,
                    );
                    ui.ctx().request_repaint();
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| {
                    ui.set_max_width(420.0);
                    self.mirror_body(ui, si);
                });
            });
        });
    }

    /// One soldier before the glass: identity, stats, kit, paper doll.
    fn mirror_body(&mut self, ui: &mut egui::Ui, si: usize) {
        let Some(c) = &mut self.campaign else { return };
        ui.vertical_centered(|ui| {
                            crate::portraits::draw(
                                ui,
                                crate::portraits::seed_of(&c.soldiers[si].name),
                                48.0,
                                c.soldiers[si].scars.len(),
                            );
                        });
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
                        stat_bars(ui, &c.soldiers[si]);
                        ui.separator();
                        inventory_grid(
                            ui,
                            c,
                            si,
                            &mut self.presets,
                            &mut self.preset_name,
                        );
                        ui.horizontal(|ui| {
                            use ods_sim::units::MagKind;
                            ui.label("Pressed with");
                            let pref = &mut c.soldiers[si].mag_pref;
                            for (kind, hint) in [
                                (MagKind::Blessed, "the standard: consecrated shot"),
                                (
                                    MagKind::ColdIron,
                                    "+4 power — old iron, older grudges",
                                ),
                                (
                                    MagKind::Salt,
                                    "-4 power, +6 stun a hit — for taking them breathing",
                                ),
                            ] {
                                if ui
                                    .selectable_label(*pref == kind, kind.name())
                                    .on_hover_text(hint)
                                    .clicked()
                                {
                                    *pref = kind;
                                }
                            }
                            ui.label(
                                egui::RichText::new(format!(
                                    "(iron {}, salt {})",
                                    c.coldiron_stock, c.salt_stock
                                ))
                                .weak()
                                .small(),
                            );
                        });
                        if let Some(calling) =
                            ods_geo::calling_from(&c.soldiers[si].deeds)
                        {
                            ui.label(
                                egui::RichText::new(format!("☩ {}", calling.name()))
                                    .color(egui::Color32::from_rgb(230, 200, 120)),
                            )
                            .on_hover_text(calling.blurb());
                        }
                        if c.soldiers[si].confessor {
                            ui.label(
                                egui::RichText::new("🕯 Confessor")
                                    .color(egui::Color32::from_rgb(190, 160, 230)),
                            )
                            .on_hover_text(
                                "anointed under the Rites: Steady and Dread in the field, \
                                 and the channel burns its keeper",
                            );
                        } else if c.research.is_complete(ods_geo::Project::RitesOfConfession)
                            && ui
                                .small_button("Anoint Confessor")
                                .on_hover_text(
                                    "requires a Sanctum at their house and a whole mind \
                                     (sanity 60+)",
                                )
                                .clicked()
                        {
                            match c.anoint_confessor(si) {
                                Ok(()) => self.log.push(format!(
                                    "{} kneels in the Sanctum and rises a Confessor.",
                                    c.soldiers[si].name
                                )),
                                Err(e) => self.log.push(format!("cannot anoint: {e:?}")),
                            }
                        }
                        ui.separator();
                        paper_doll(ui, c, si, &mut self.log);
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
                use crate::icons::{self, Icon};
                ui.strong(format!("M{} · D{}", c.month, c.day));
                ui.separator();
                ui.label(format!("Treasury {}k", c.funds));
                ui.label(format!("Score {}", c.month_score));
                let crises = c.rifts.iter().filter(|r| r.detected).count() + c.nests.len();
                if crises > 0 {
                    ui.colored_label(
                        if crises >= 3 {
                            egui::Color32::from_rgb(230, 100, 70)
                        } else {
                            egui::Color32::from_rgb(230, 180, 70)
                        },
                        format!("⚠ {crises} crisis(es)"),
                    );
                }
                ui.label(format!("🛩 {}/{}", c.sorties.len(), c.zeppelins))
                    .on_hover_text("sorties aloft / zeppelins");
                if c.heresy > 0 {
                    ui.colored_label(
                        if c.heresy >= 25 {
                            egui::Color32::from_rgb(220, 60, 60)
                        } else {
                            egui::Color32::from_rgb(230, 180, 70)
                        },
                        format!("☿ heresy {}", c.heresy),
                    )
                    .on_hover_text("the council reads its ledger at every month's end");
                }
                ui.separator();
                icons::stat(ui, Icon::Brimstone, c.brimstone.to_string(), "brimstone salvage");
                icons::stat(ui, Icon::Hellsteel, c.hellsteel.to_string(), "hellsteel salvage");
                icons::stat(ui, Icon::Charge, c.grenade_stock.to_string(), "hellfire charges in stores");
                icons::stat(ui, Icon::Dressing, c.dressing_stock.to_string(), "field dressings in stores");
                icons::stat(
                    ui,
                    Icon::Cells,
                    format!("{}g/{}o", c.prisoners.grunts, c.prisoners.overseers),
                    "bound demons in the cells (grunts/overseers)",
                );
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
                let day_color = if self.pause_flash > 0.0 {
                    // The clock just stopped for something: it says so.
                    let t = (self.pause_flash * 6.0).sin().abs();
                    egui::Color32::from_rgb(200 + (t * 55.0) as u8, 170, 80)
                } else {
                    egui::Color32::from_rgb(214, 202, 178)
                };
                ui.label(
                    egui::RichText::new(format!("Day {}", c.day))
                        .size(30.0)
                        .strong()
                        .color(day_color),
                );
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
                        self.run_to_event = false;
                    }
                }
                if ui
                    .add_enabled(
                        alive && !sky_held,
                        egui::Button::selectable(self.run_to_event, "⏭"),
                    )
                    .on_hover_text("run the clock until something happens, then stop")
                    .clicked()
                {
                    self.run_to_event = !self.run_to_event;
                    self.geo_speed = if self.run_to_event { 3 } else { 0 };
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
            let mut goto = None;
            if ui.add_sized(wide, egui::Button::new("🛡 Muster rolls")).clicked() {
                goto = Some(Screen::Roster);
            }
            if ui.add_sized(wide, egui::Button::new("⚒ The Forge")).clicked() {
                goto = Some(Screen::Forge);
            }
            if ui.add_sized(wide, egui::Button::new("📖 Bestiary")).clicked() {
                goto = Some(Screen::Codex);
            }
            if let Some(screen) = goto {
                // The desk screens hold the clock while they're open.
                self.geo_speed = 0;
                self.run_to_event = false;
                self.screen = screen;
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
                            Ok(()) => {
                                self.log.push("Saved.".to_string());
                                if let Some(a) = &self.audio {
                                    a.play(crate::audio::Sound::SaveChime);
                                }
                            }
                            Err(e) => self.log.push(format!("save failed: {e}")),
                        }
                    }
                    for slot in 1..=3usize {
                        if ui.button(format!("S{slot}")).clicked() {
                            match std::fs::write(crate::slot_path(slot), c.save_to_string()) {
                                Ok(()) => {
                                    self.log.push(format!("Saved to slot {slot}."));
                                    if let Some(a) = &self.audio {
                                        a.play(crate::audio::Sound::SaveChime);
                                    }
                                }
                                Err(e) => self.log.push(format!("save failed: {e}")),
                            }
                        }
                    }
                });
            }
            if ui.add_sized(wide, egui::Button::new("Menu")).clicked() {
                self.confirm_menu = true;
            }
        });

        // Leaving the war table wants a nod (and offers the quicksave).
        if self.confirm_menu {
            let mut close = false;
            egui::Window::new("Leave the war table?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
                .show(ctx, |ui| {
                    ui.label("Anything unsaved stays behind.");
                    ui.horizontal(|ui| {
                        if ui.button("Save & leave").clicked() {
                            match std::fs::write(SAVE_PATH, c.save_to_string()) {
                                Ok(()) => {
                                    self.screen = Screen::Menu;
                                }
                                Err(e) => self.log.push(format!("save failed: {e}")),
                            }
                            close = true;
                        }
                        if ui.button("Leave").clicked() {
                            self.screen = Screen::Menu;
                            close = true;
                        }
                        if ui.button("Stay").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.confirm_menu = false;
            }
        }

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
        // Cap the width: the ops desk must never bury the world behind it.
        egui::SidePanel::left("geo-ops")
            .default_width(360.0)
            .max_width(430.0)
            .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                let open_now = c.rifts.iter().filter(|r| r.detected).count();
                ui.heading(format!("War room — {open_now} incursion(s)"));
                let fit = c.soldiers.iter().filter(|s| s.is_fit()).count();
                ui.label(format!("{fit} soldiers fit for duty"));
                ui.horizontal_wrapped(|ui| {
                    ui.label("Answering:");
                    for (i, name) in ods_geo::SQUAD_NAMES.iter().enumerate() {
                        ui.selectable_value(&mut c.active_squad, i as u8, *name);
                    }
                });
                ui.separator();

                let mut rifts: Vec<_> = c
                    .rifts
                    .iter()
                    .filter(|r| r.detected)
                    .map(|r| {
                        (
                            r.id,
                            r.kind,
                            r.region,
                            r.days_left,
                            r.is_stabilized(),
                            r.lat,
                            r.lon,
                            r.effective_garrison(),
                        )
                    })
                    .collect();
                // The shortest fuse burns at the top of the queue.
                rifts.sort_by_key(|r| r.3);
                if rifts.is_empty() {
                    ui.label("No detected rifts. The augurs keep watch.");
                }
                for (id, kind, region, days_left, stabilized, lat, lon, garrison) in rifts {
                    let local = c.bases.iter().any(|b| b.region == region);
                    let sortie = c.sorties.iter().find(|s| s.rift_id == id).copied();
                    ui.horizontal_wrapped(|ui| {
                        if ui
                            .small_button("⌖")
                            .on_hover_text("swing the globe to it")
                            .clicked()
                        {
                            self.geo_swing =
                                Some((lon.to_radians(), lat.to_radians().clamp(0.15, 1.2)));
                        }
                        let line = format!(
                            "{} in {} — {days_left}d · ~{garrison} demons{}",
                            kind.name(),
                            region.name(),
                            if stabilized { " (DUG IN)" } else { " (unstable)" }
                        );
                        let urgency = match days_left {
                            0..=1 => egui::Color32::from_rgb(230, 90, 70),
                            2 => egui::Color32::from_rgb(230, 180, 70),
                            _ => ui.visuals().text_color(),
                        };
                        ui.colored_label(urgency, line)
                            .on_hover_text(format!("{} country", region.biome().name()));
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
                                    self.pending_launch = Some(MissionKind::Rift(id));
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
                                    self.pending_launch = Some(MissionKind::Rift(id));
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
                            self.pending_launch = Some(MissionKind::Nest(id));
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
                            self.pending_launch = Some(MissionKind::FinalSanctum);
                        }
                    } else if ui
                        .button(egui::RichText::new("⚔ THE FINAL ASSAULT").strong())
                        .clicked()
                    {
                        self.pending_launch = Some(MissionKind::FinalAssault);
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
                if ui.button("🛡 Muster rolls & armoury").clicked() {
                    self.geo_speed = 0;
                    self.run_to_event = false;
                    self.screen = Screen::Roster;
                }

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
                ui.horizontal(|ui| {
                    for (i, label) in
                        [(0u8, "All"), (1, "Battles"), (2, "Council"), (3, "Augurs")]
                    {
                        if ui.selectable_label(self.log_filter == i, label).clicked() {
                            self.log_filter = i;
                        }
                    }
                    ui.label(
                        egui::RichText::new("click a line naming a region to swing to it")
                            .weak()
                            .small(),
                    );
                });
                let keep = |line: &str| -> bool {
                    match self.log_filter {
                        1 => {
                            line.contains("VICTORY")
                                || line.contains("REPELLED")
                                || line.contains("RECKONING")
                                || line.contains("sortie")
                        }
                        2 => {
                            line.contains("council")
                                || line.contains("INQUISITION")
                                || line.contains("month")
                                || line.contains("demand")
                        }
                        3 => {
                            line.contains("augur")
                                || line.contains("rift")
                                || line.contains("RIFT")
                                || line.contains("quiet")
                                || line.contains("MOON")
                        }
                        _ => true,
                    }
                };
                let mut swing: Option<ods_geo::Region> = None;
                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for line in self.log.iter().filter(|l| keep(l)) {
                        let resp = ui.add(
                            egui::Label::new(
                                egui::RichText::new(line).color(log_color(line)),
                            )
                            .sense(egui::Sense::click()),
                        );
                        if resp.clicked()
                            && let Some(region) =
                                ods_geo::Region::ALL.iter().find(|r| line.contains(r.name()))
                        {
                            swing = Some(*region);
                        }
                    }
                });
                if let Some(region) = swing {
                    let (lat, lon) = region.centroid();
                    self.geo_swing =
                        Some((lon.to_radians(), lat.to_radians().clamp(0.15, 1.2)));
                }
            });

        // Hover cards: the globe answers before it is asked.
        if !ctx.is_pointer_over_area() {
            let ppp = ctx.pixels_per_point();
            let (w, h) = self.renderer.size();
            let (origin, dir) =
                self.geo_camera.screen_ray(self.cursor.0, self.cursor.1, w, h);
            if let Some(region) = crate::globe::pick_region(origin, dir) {
                let rifts: Vec<_> = c
                    .rifts
                    .iter()
                    .filter(|r| r.detected && r.region == region)
                    .collect();
                let nests = c.nests.iter().filter(|n| n.region == region).count();
                if !rifts.is_empty() || nests > 0 {
                    egui::Area::new(egui::Id::new("globe-hover"))
                        .fixed_pos(egui::pos2(
                            self.cursor.0 / ppp + 14.0,
                            self.cursor.1 / ppp + 10.0,
                        ))
                        .show(ctx, |ui| {
                            egui::Frame::window(ui.style()).show(ui, |ui| {
                                ui.strong(region.name());
                                for r in &rifts {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{} — {}d{} · ~{} demons",
                                            r.kind.name(),
                                            r.days_left,
                                            if r.is_stabilized() { " (DUG IN)" } else { "" },
                                            r.effective_garrison()
                                        ))
                                        .small(),
                                    );
                                }
                                if nests > 0 {
                                    ui.label(
                                        egui::RichText::new(format!(
                                            "{nests} standing nest(s)"
                                        ))
                                        .small(),
                                    );
                                }
                                if let Some(r) = rifts.first()
                                    && let Ok(days) = c.travel_days(r.id)
                                {
                                    ui.label(
                                        egui::RichText::new(if days == 0 {
                                            "local: strikes roll same-day".to_string()
                                        } else {
                                            format!("travel: {days}d by zeppelin")
                                        })
                                        .weak()
                                        .small(),
                                    );
                                }
                            });
                        });
                }
            }
        }

        // ------------------------------------------- the muster sheet
        // Every led mission passes the sheet: who answers, and what they
        // are missing, before boots leave the ground.
        if let Some(kind) = self.pending_launch {
            let mut go = false;
            let mut stay = false;
            egui::Window::new("THE MUSTER SHEET")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, -20.0])
                .show(ctx, |ui| {
                    ui.strong(format!("Operation: {}", kind.label().to_uppercase()));
                    let want = c.active_squad;
                    let mut squad: Vec<usize> = c
                        .soldiers
                        .iter()
                        .enumerate()
                        .filter(|(_, s)| s.is_fit() && want != 0 && s.squad == want)
                        .map(|(i, _)| i)
                        .take(6)
                        .collect();
                    for (i, s) in c.soldiers.iter().enumerate() {
                        if squad.len() >= 6 {
                            break;
                        }
                        if s.is_fit() && !squad.contains(&i) {
                            squad.push(i);
                        }
                    }
                    let mut mags_wanted = 0;
                    for &i in &squad {
                        let s = &c.soldiers[i];
                        let mut flags: Vec<&str> = Vec::new();
                        if s.mags_loadout == 0 {
                            flags.push("NO SPARE MAGS");
                        }
                        if s.dressings_loadout == 0 {
                            flags.push("no dressings");
                        }
                        if s.sanity < 40 {
                            flags.push("mind frayed");
                        }
                        mags_wanted += s.mags_loadout;
                        if flags.is_empty() {
                            ui.label(format!("{} — ready", s.name));
                        } else {
                            ui.colored_label(
                                egui::Color32::from_rgb(230, 180, 70),
                                format!("{} — {}", s.name, flags.join(", ")),
                            );
                        }
                    }
                    if squad.is_empty() {
                        ui.colored_label(
                            egui::Color32::from_rgb(220, 60, 60),
                            "Nobody is fit to muster.",
                        );
                    } else if squad.len() < 6 {
                        ui.colored_label(
                            egui::Color32::from_rgb(230, 180, 70),
                            format!("Understrength: {} of 6.", squad.len()),
                        );
                    }
                    if mags_wanted > c.quarrel_stock {
                        ui.colored_label(
                            egui::Color32::from_rgb(230, 180, 70),
                            format!(
                                "The stores cannot fill every belt ({} wanted, {} pressed).",
                                mags_wanted, c.quarrel_stock
                            ),
                        );
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        if !squad.is_empty()
                            && ui.button(egui::RichText::new("⚔ Launch").strong()).clicked()
                        {
                            go = true;
                        }
                        if ui.button("Stand down").clicked() {
                            stay = true;
                        }
                    });
                });
            if go {
                action = GeoAction::LeadMission(kind);
                self.pending_launch = None;
            } else if stay {
                self.pending_launch = None;
            }
        }

        // ------------------------------------- demolition wants a nod
        if let Some((bi, x, y)) = self.pending_demolish {
            let name = c.bases.get(bi).and_then(|b| b.facility_at(x, y)).map(|(f, _)| f.name());
            let mut done = false;
            egui::Window::new("Tear it down?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 20.0])
                .show(ctx, |ui| {
                    match name {
                        Some(n) => ui.label(format!(
                            "The {n} comes down for a quarter of its stone. This is not \
                             a scaffold — it is a finished hall."
                        )),
                        None => ui.label("Nothing stands there anymore."),
                    };
                    ui.horizontal(|ui| {
                        if name.is_some() && ui.button("Demolish").clicked() {
                            match c.demolish_facility(bi, x, y) {
                                Ok((f, refund)) => {
                                    self.log.push(format!(
                                        "{} torn down; {refund}k reclaimed in stone.",
                                        f.name()
                                    ));
                                    self.base_dirty = true;
                                }
                                Err(e) => self.log.push(format!("cannot demolish: {e:?}")),
                            }
                            done = true;
                        }
                        if ui.button("Let it stand").clicked() {
                            done = true;
                        }
                    });
                });
            if done {
                self.pending_demolish = None;
            }
        }

        // Transient notices, top-right, fading as they age.
        if !self.toasts.is_empty() {
            egui::Area::new(egui::Id::new("toasts"))
                .anchor(egui::Align2::RIGHT_TOP, [-12.0, 40.0])
                .show(ctx, |ui| {
                    for (text, ttl) in &self.toasts {
                        let alpha = (*ttl * 255.0 / 2.0).clamp(0.0, 220.0) as u8;
                        egui::Frame::window(ui.style())
                            .fill(egui::Color32::from_rgba_unmultiplied(24, 22, 20, alpha))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(text).small());
                            });
                    }
                });
        }

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
                                self.pending_launch = Some(MissionKind::Purge(region));
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
                    "Chapterhouse — {}   ·   Treasury {}k   ·   🜏 {}/{}   ⛓ {}/{}",
                    c.bases[bi].region.name(),
                    c.funds,
                    c.brimstone,
                    c.store_capacity(),
                    c.hellsteel,
                    c.store_capacity()
                ));
            });
        });

        egui::SidePanel::right("base-desk")
            .default_width(390.0)
            .max_width(460.0)
            .show(ctx, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                chapterhouse_panel(
                    ui,
                    c,
                    &mut self.selected_base,
                    &mut self.build_choice,
                    &mut self.log,
                    &mut self.base_dirty,
                    &mut self.desk_tab,
                    &mut self.pending_demolish,
                    &mut self.relic_confirm,
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

        if let Some((bi, x, y)) = self.pending_demolish {
            let name = c.bases.get(bi).and_then(|b| b.facility_at(x, y)).map(|(f, _)| f.name());
            let mut done = false;
            egui::Window::new("Tear it down?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, [0.0, 20.0])
                .show(ctx, |ui| {
                    match name {
                        Some(n) => ui.label(format!(
                            "The {n} comes down for a quarter of its stone."
                        )),
                        None => ui.label("Nothing stands there anymore."),
                    };
                    ui.horizontal(|ui| {
                        if name.is_some() && ui.button("Demolish").clicked() {
                            match c.demolish_facility(bi, x, y) {
                                Ok((f, refund)) => {
                                    self.log.push(format!(
                                        "{} torn down; {refund}k reclaimed in stone.",
                                        f.name()
                                    ));
                                    self.base_dirty = true;
                                }
                                Err(e) => self.log.push(format!("cannot demolish: {e:?}")),
                            }
                            done = true;
                        }
                        if ui.button("Let it stand").clicked() {
                            done = true;
                        }
                    });
                });
            if done {
                self.pending_demolish = None;
            }
        }
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
            "{what}: VICTORY — {} demons slain{}, {} soldiers lost, {} turns{}",
            r.demons_slain,
            if captures > 0 { format!(", {captures} bound") } else { String::new() },
            r.dead.len(),
            r.turns,
            if r.recovered.is_empty() {
                String::new()
            } else {
                format!(" — {} forged weapon(s) came home off the field", r.recovered.len())
            } + &if r.escaped > 0 {
                format!(" — {} fled to tell of it", r.escaped)
            } else {
                String::new()
            } + &if r.executed > 0 {
                format!(" — {} put down where they lay", r.executed)
            } else {
                String::new()
            }
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
#[allow(clippy::too_many_arguments)] // the commander's desk has many drawers
fn chapterhouse_panel(
    ui: &mut egui::Ui,
    c: &mut Campaign,
    selected_base: &mut usize,
    build_choice: &mut Facility,
    log: &mut Vec<String>,
    base_dirty: &mut bool,
    tab: &mut u8,
    pending_demolish: &mut Option<(usize, usize, usize)>,
    relic_confirm: &mut bool,
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
                ui.horizontal(|ui| {
                    for (i, label) in
                        [(0u8, "Halls"), (1, "Codex"), (2, "Forge"), (3, "Broker")]
                    {
                        if ui.selectable_label(*tab == i, label).clicked() {
                            *tab = i;
                        }
                    }
                });
                ui.separator();
                if *tab == 0 {

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

                let linked: std::collections::HashSet<(usize, usize)> =
                    c.bases[bi].linked_cells().into_iter().collect();
                egui::Grid::new("base-grid").spacing([2.0, 2.0]).show(ui, |ui| {
                    for y in 0..GRID {
                        for x in 0..GRID {
                            let cell = c.bases[bi].facility_at(x, y);
                            let days = c.bases[bi].build_days_left(x, y);
                            let legal = cell.is_none() && c.bases[bi].touches(x, y);
                            let cut_off = cell.is_some() && !linked.contains(&(x, y));
                            let label = match (cell, days) {
                                (Some(_), Some(d)) => egui::RichText::new(format!("{d}"))
                                    .color(egui::Color32::from_rgb(230, 190, 90)),
                                (Some((f, _)), None) => {
                                    let ch =
                                        f.name().chars().next().unwrap_or('?').to_string();
                                    if cut_off {
                                        egui::RichText::new(ch)
                                            .color(egui::Color32::from_rgb(220, 90, 70))
                                    } else {
                                        egui::RichText::new(ch)
                                    }
                                }
                                (None, _) => {
                                    if legal {
                                        egui::RichText::new("+").weak()
                                    } else {
                                        egui::RichText::new(" ").weak()
                                    }
                                }
                            };
                            let button = egui::Button::new(label).min_size(egui::vec2(30.0, 30.0));
                            let resp = ui.add(button);
                            let resp = match cell {
                                Some((f, true)) => resp.on_hover_text(format!(
                                    "{}{}\nright-click: demolish (refund {}k)",
                                    f.name(),
                                    if cut_off {
                                        " — CUT OFF from the gate: it will not \
                                         answer a Reckoning"
                                    } else {
                                        ""
                                    },
                                    f.cost() / 4
                                )),
                                Some((f, false)) => resp.on_hover_text(format!(
                                    "{} — {} day(s) of works left\nright-click: cancel \
                                     (refund {}k)",
                                    f.name(),
                                    days.unwrap_or(0),
                                    f.cost() / 2
                                )),
                                None if legal => resp.on_hover_text(format!(
                                    "build {} here ({}k)",
                                    build_choice.name(),
                                    build_choice.cost()
                                )),
                                None => resp.on_hover_text(
                                    "too far from the halls: new walls grow from old walls",
                                ),
                            };
                            if resp.clicked()
                                && cell.is_none()
                                && let Err(e) = c.start_build(bi, *build_choice, x, y)
                            {
                                log.push(format!("cannot build: {e:?}"));
                            }
                            if resp.secondary_clicked() && cell.is_some() {
                                if days.is_none() {
                                    // A finished hall waits for the nod.
                                    *pending_demolish = Some((bi, x, y));
                                } else {
                                    // Canceling a scaffold is cheap and instant.
                                    match c.demolish_facility(bi, x, y) {
                                        Ok((f, refund)) => log.push(format!(
                                            "{} works canceled; {refund}k reclaimed.",
                                            f.name()
                                        )),
                                        Err(e) => log.push(format!("cannot demolish: {e:?}")),
                                    }
                                }
                            }
                        }
                        ui.end_row();
                    }
                });
                let total_upkeep: i64 = c.bases.iter().map(|b| b.maintenance()).sum();
                ui.label(
                    egui::RichText::new(format!(
                        "Upkeep: {}k/mo this house · {}k/mo across the Order",
                        c.bases[bi].maintenance(),
                        total_upkeep
                    ))
                    .weak()
                    .small(),
                );
                let stationed = c.soldiers.iter().filter(|s| s.home == bi).count();
                ui.label(format!(
                    "Stationed here: {stationed} soldier(s) · {} scholar(s) · {} smith(s)",
                    c.bases[bi].occultists, c.bases[bi].artificers
                ));
                if c.bases.len() > 1 {
                    let next = (bi + 1) % c.bases.len();
                    let next_name = c.bases[next].region.name();
                    ui.horizontal(|ui| {
                        if ui
                            .small_button("scholar →")
                            .on_hover_text(format!("repost one scholar to {next_name}"))
                            .clicked()
                            && let Err(e) = c.move_staff(bi, next, false)
                        {
                            log.push(format!("cannot repost: {e:?}"));
                        }
                        if ui
                            .small_button("smith →")
                            .on_hover_text(format!("repost one smith to {next_name}"))
                            .clicked()
                            && let Err(e) = c.move_staff(bi, next, true)
                        {
                            log.push(format!("cannot repost: {e:?}"));
                        }
                    });
                }

                // The fleet: hulls, sorties aloft, and how they fly.
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    ui.label(format!(
                        "Fleet: {} zeppelin(s), {} aloft",
                        c.zeppelins,
                        c.sorties.len()
                    ));
                    if c.zeppelins < ods_geo::MAX_ZEPPELINS
                        && ui
                            .button(format!("Commission ({}k)", ods_geo::ZEPPELIN_COST))
                            .on_hover_text("another hull: another sortie in the air at once")
                            .clicked()
                    {
                        match c.commission_zeppelin() {
                            Ok(()) => log.push(
                                "A new envelope rises from the yards, blessed and named.".into(),
                            ),
                            Err(e) => log.push(format!("cannot commission: {e:?}")),
                        }
                    }
                    ui.separator();
                    ui.label("Sorties fly:");
                    if ui
                        .selectable_label(c.posture == ods_geo::Posture::Bold, "Bold")
                        .on_hover_text("the fast winds: full speed, full risk")
                        .clicked()
                    {
                        c.posture = ods_geo::Posture::Bold;
                    }
                    if ui
                        .selectable_label(c.posture == ods_geo::Posture::Cautious, "Cautious")
                        .on_hover_text("hug the cloud: +1 day of travel, half the sky-hunts")
                        .clicked()
                    {
                        c.posture = ods_geo::Posture::Cautious;
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
                        match c.hire_soldier(bi) {
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
                        match c.hire_occultist(bi) {
                            Ok(()) => log.push(format!(
                                "An occultist takes rooms at {}.",
                                c.bases[bi].region.name()
                            )),
                            Err(e) => log.push(format!("cannot hire: {e:?}")),
                        }
                    }
                    if ui
                        .button(format!("Artificer ({}k)", ods_geo::ARTIFICER_HIRE_COST))
                        .clicked()
                    {
                        match c.hire_artificer(bi) {
                            Ok(()) => log.push(format!(
                                "An artificer takes rooms at {}.",
                                c.bases[bi].region.name()
                            )),
                            Err(e) => log.push(format!("cannot hire: {e:?}")),
                        }
                    }
                });
                ui.label(format!(
                    "{} soldiers, {} occultists, {} artificers / {} beds",
                    c.soldiers.len(),
                    c.occultist_count(),
                    c.artificer_count(),
                    c.quarters_capacity()
                ));

                if c.bases[bi].count_active(Facility::TrainingGround) > 0 {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label("This house drills:");
                        for f in ods_geo::Focus::ALL {
                            ui.selectable_value(&mut c.bases[bi].focus, f, f.name());
                        }
                    });
                }

                } // halls
                if *tab == 1 {
                ui.add_space(6.0);
                ui.heading("Forbidden Codex");
                {
                    let seated: u32 = c
                        .bases
                        .iter()
                        .map(|b| b.occultists.min(b.library_capacity() as u32))
                        .sum();
                    ui.label(
                        egui::RichText::new(format!(
                            "{seated} scholar(s) seated at lecterns across the Order"
                        ))
                        .weak()
                        .small(),
                    );
                }
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
                    // What the Order can't reach yet, it can't see: locked
                    // chains and capture-gated studies stay off the docket.
                    if !project.unlocked(&c.research, &c.codex_captured) {
                        continue;
                    }
                    let (brim, steel) = project.materials();
                    let (grunts, overseers) = project.prisoners();
                    let mut needs = String::new();
                    if let Some(breed) = project.requires_capture() {
                        needs.push_str(&format!(" +a living {:?}", breed));
                    }
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
                        ui.label(format!("{} ({}pts{needs})", project.name(), project.cost()))
                            .on_hover_text(project_lore(project.name()));
                    });
                }

                } // codex
                if *tab == 2 {
                ui.add_space(6.0);
                ui.heading(format!("Workshop — {}", c.bases[bi].region.name()));
                match c.jobs.iter().find(|j| j.base == bi) {
                    Some(j) => {
                        let done = j.item.cost().saturating_sub(j.left) as f32;
                        ui.add(
                            egui::ProgressBar::new(done / j.item.cost() as f32)
                                .text(format!("{} — {} left", j.item.name(), j.left)),
                        );
                    }
                    None => {
                        ui.label(if c.bases[bi].workshop_capacity() == 0 {
                            "No workshop in this house."
                        } else if c.bases[bi].artificers == 0 {
                            "Benches stand ready; no smiths posted here."
                        } else {
                            "The benches are idle."
                        });
                    }
                }
                for j in &c.jobs {
                    if j.base != bi && j.base < c.bases.len() {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} forges {} ({} left)",
                                c.bases[j.base].region.name(),
                                j.item.name(),
                                j.left
                            ))
                            .weak()
                            .small(),
                        );
                    }
                }
                for item in ManufactureItem::ALL {
                    // Unresearched patterns don't exist as far as the
                    // benches know.
                    if item
                        .required_research()
                        .is_some_and(|p| !c.research.is_complete(p))
                    {
                        continue;
                    }
                    let (brim, steel) = item.materials();
                    ui.horizontal(|ui| {
                        if ui.button("Make").clicked()
                            && let Err(e) = c.start_manufacture(bi, item)
                        {
                            log.push(format!("cannot make: {e:?}"));
                        }
                        ui.label(format!("{} ({}pts, {brim}🜏 {steel}⛓)", item.name(), item.cost()))
                            .on_hover_text(item_lore(item.name()));
                    });
                }
    } // forge
    if *tab == 3 {
    ui.add_space(6.0);
    ui.heading("The Shadow Broker");
    let heresy_color = match c.heresy {
        0..=9 => egui::Color32::GRAY,
        10..=24 => egui::Color32::from_rgb(230, 180, 70),
        _ => egui::Color32::from_rgb(220, 60, 60),
    };
    ui.colored_label(heresy_color, format!("Heresy: {} — the council reads its ledger", c.heresy))
        .on_hover_text(
            "grafts, dark bargains, and prisoners fed to the Codex all leave marks. \
             Each ten marks tithes 5% of funding (to 20%); past twenty-five the \
             Inquisition arrives. One mark of penance clears each month.",
        );
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(c.brimstone >= 10, egui::Button::new("Sell 10 🜏 (dark)"))
            .on_hover_text("half again the reliquary price; +2 heresy")
            .clicked()
        {
            match c.dark_sell_brimstone(10) {
                Ok(gained) => log.push(format!(
                    "Ten crates leave by the night road. +{gained}k, and a mark in a ledger."
                )),
                Err(e) => log.push(format!("no deal: {e:?}")),
            }
        }
        if ui
            .add_enabled(c.prisoners.grunts > 0, egui::Button::new("Sell a bound grunt (60k)"))
            .on_hover_text("alive, to people who should not have one; +3 heresy")
            .clicked()
        {
            match c.dark_sell_prisoner(false) {
                Ok(_) => log.push("A crate that scratches leaves by the night road.".into()),
                Err(e) => log.push(format!("no deal: {e:?}")),
            }
        }
        if ui
            .add_enabled(
                c.prisoners.overseers > 0,
                egui::Button::new("Sell a bound overseer (140k)"),
            )
            .on_hover_text("it will remember every face it saw; +3 heresy")
            .clicked()
        {
            match c.dark_sell_prisoner(true) {
                Ok(_) => log.push("A crate that whispers leaves by the night road.".into()),
                Err(e) => log.push(format!("no deal: {e:?}")),
            }
        }
        let relic_label = if *relic_confirm {
            "Certain? It does not come back"
        } else {
            "Sell a relic (120k)"
        };
        if ui
            .add_enabled(!c.relic_pool.is_empty(), egui::Button::new(relic_label))
            .on_hover_text("something old and holy, gone quietly abroad; +2 heresy")
            .clicked()
        {
            if *relic_confirm {
                match c.dark_sell_relic() {
                    Ok(_) => log.push("A reliquary case leaves by the night road.".into()),
                    Err(e) => log.push(format!("no deal: {e:?}")),
                }
                *relic_confirm = false;
            } else {
                *relic_confirm = true;
            }
        }
    });

    } // broker

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

/// The Apocalypse sheet: ten labelled bars, one soldier. Bars are scaled
/// against the highest value the campaign can ever grow a stat to, so a
/// full bar means "this cannot get better".
fn stat_bars(ui: &mut egui::Ui, s: &ods_geo::Soldier) {
    let rows: [(&str, i32, i32, egui::Color32); 10] = [
        ("Time Units", s.stats.tu, 65, egui::Color32::from_rgb(212, 178, 90)),
        ("Stamina", s.stats.stamina, 80, egui::Color32::from_rgb(225, 220, 130)),
        ("Health", s.stats.health, 40, egui::Color32::from_rgb(205, 75, 65)),
        ("Bravery", s.stats.bravery, 95, egui::Color32::from_rgb(190, 125, 215)),
        ("Reactions", s.stats.reactions, 90, egui::Color32::from_rgb(120, 195, 220)),
        ("Firing Accuracy", s.stats.accuracy, 95, egui::Color32::from_rgb(125, 200, 125)),
        ("Throwing Accuracy", s.stats.throwing, 90, egui::Color32::from_rgb(105, 175, 105)),
        ("Melee Accuracy", s.stats.melee, 90, egui::Color32::from_rgb(90, 155, 95)),
        ("Strength", s.stats.strength, 60, egui::Color32::from_rgb(220, 150, 85)),
        ("Sanity", s.sanity, 100, egui::Color32::from_rgb(160, 165, 230)),
    ];
    // The founding sheet, for growth-at-a-glance on hover.
    let base = s.base_stats;
    let base_of = |label: &str| -> Option<i32> {
        let b = base?;
        Some(match label {
            "Time Units" => b.tu,
            "Stamina" => b.stamina,
            "Health" => b.health,
            "Bravery" => b.bravery,
            "Reactions" => b.reactions,
            "Firing Accuracy" => b.accuracy,
            "Throwing Accuracy" => b.throwing,
            "Melee Accuracy" => b.melee,
            "Strength" => b.strength,
            _ => return None,
        })
    };
    for (label, value, max, color) in rows {
        ui.horizontal(|ui| {
            ui.add_sized(
                [118.0, 12.0],
                egui::Label::new(egui::RichText::new(label).small()),
            );
            let value_label = ui.add_sized(
                [22.0, 12.0],
                egui::Label::new(egui::RichText::new(value.to_string()).small().strong()),
            );
            if let Some(b) = base_of(label) {
                let delta = value - b;
                if delta != 0 {
                    value_label.on_hover_text(format!(
                        "{}{delta} since recruitment (was {b})",
                        if delta > 0 { "+" } else { "" }
                    ));
                }
            }
            let (rect, _) = ui.allocate_exact_size(egui::vec2(110.0, 7.0), egui::Sense::hover());
            let paint = ui.painter_at(rect);
            paint.rect_filled(rect, 1.0, egui::Color32::from_gray(38));
            let frac = (value.max(0) as f32 / max as f32).min(1.0);
            if frac > 0.0 {
                let mut fill = rect;
                fill.set_width(rect.width() * frac);
                paint.rect_filled(fill, 1.0, color);
            }
        });
    }
}

/// One cell of the kit grid: an item where it rides, or an empty slot.
fn kit_cell(ui: &mut egui::Ui, icon: &str, hover: &str, in_pack: bool) {
    let text = if icon.is_empty() {
        egui::RichText::new("·").weak()
    } else if in_pack {
        egui::RichText::new(icon).size(15.0).weak()
    } else {
        egui::RichText::new(icon).size(15.0)
    };
    let cell = ui.add_sized([30.0, 28.0], egui::Button::new(text));
    if !hover.is_empty() {
        cell.on_hover_text(hover);
    }
}

/// The kit, laid out the old way: hands, belt, pack. The belt's three
/// slots are the first three consumable uses at the plain price; below
/// them, everything else rides in the pack at +6 TU a fetch. Strength
/// carries the weight — overload it and the hands slow.
fn inventory_grid(
    ui: &mut egui::Ui,
    c: &mut Campaign,
    si: usize,
    presets: &mut Vec<(String, u32, u32, u32, String)>,
    preset_name: &mut String,
) {
    let s = &c.soldiers[si];

    // Hands: what the paper doll below actually assigns.
    ui.label(egui::RichText::new("HANDS").weak().small());
    ui.horizontal(|ui| {
        let main = if s.has_lance {
            "hellfire lance".to_string()
        } else {
            s.weapon_key.replace('_', " ")
        };
        ui.add_sized([132.0, 30.0], egui::Button::new(format!("⚔ {main}")))
            .on_hover_text("the issued weapon — cycle it in the paper doll below");
        let off = if s.has_blade { "consecrated blade" } else { "—" };
        ui.add_sized([132.0, 30.0], egui::Button::new(format!("🗡 {off}")))
            .on_hover_text("the sidearm at the hip — toggled below, drawn with [I] in the field");
    });

    // The consumables, in carry order: what the hands find first.
    let mut items: Vec<(&str, String)> = Vec::new();
    for _ in 0..s.grenades_loadout {
        items.push(("🧨", "hellfire charge".to_string()));
    }
    for _ in 0..s.dressings_loadout {
        items.push(("✚", "field dressing".to_string()));
    }
    for _ in 0..s.mags_loadout {
        items.push(("▤", format!("{} magazine", s.mag_pref.name())));
    }
    items.push(("✦", "witchfire flare (standard issue)".to_string()));
    items.push(("✦", "witchfire flare (standard issue)".to_string()));

    ui.label(
        egui::RichText::new("BELT — the three at hand, plain price").weak().small(),
    );
    ui.horizontal(|ui| {
        for slot in 0..3 {
            match items.get(slot) {
                Some((icon, name)) => kit_cell(ui, icon, name, false),
                None => kit_cell(ui, "", "an empty belt loop", false),
            }
        }
    });

    ui.label(egui::RichText::new("PACK — +6 TU a fetch").weak().small());
    let rest = if items.len() > 3 { &items[3..] } else { &[] };
    let rows = (rest.len() + 4) / 4; // at least one row of empties
    for row in 0..rows.max(1) {
        ui.horizontal(|ui| {
            for col in 0..4 {
                match rest.get(row * 4 + col) {
                    Some((icon, name)) => {
                        kit_cell(ui, icon, &format!("{name} — fetched at +6 TU"), true)
                    }
                    None => kit_cell(ui, "", "empty pack space", true),
                }
            }
        });
    }
    ui.label(
        egui::RichText::new("webbing: 1 smoke, 1 ward kit — always at hand")
            .weak()
            .small(),
    );

    // The back decides where "over" begins.
    let load = s.grenades_loadout + s.dressings_loadout + s.mags_loadout / 2;
    let capacity = 2 + s.stats.strength as u32 / 8;
    let mule = s.quirk == Some(ods_geo::Quirk::PackMule);
    let line = format!(
        "Load {load} / {capacity}{}",
        if load > capacity && mule {
            " — over, but born to haul"
        } else if load > capacity {
            " — OVER: the hands slow (−4 TU)"
        } else {
            ""
        }
    );
    ui.label(if load > capacity && !mule {
        egui::RichText::new(line).color(egui::Color32::from_rgb(230, 120, 70))
    } else {
        egui::RichText::new(line).weak()
    })
    .on_hover_text(
        "capacity is 2 + Strength/8; charges and dressings weigh one \
         apiece, two magazines ride as one",
    );

    // Presets: a kit worth keeping gets a name.
    ui.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new("Presets:").weak().small());
        let mut apply: Option<usize> = None;
        let mut delete: Option<usize> = None;
        for (i, (name, ..)) in presets.iter().enumerate() {
            let resp = ui.small_button(name).on_hover_text(
                "click to apply · right-click to forget",
            );
            if resp.clicked() {
                apply = Some(i);
            }
            if resp.secondary_clicked() {
                delete = Some(i);
            }
        }
        if let Some(i) = apply {
            let (_, g, d, m, kind) = presets[i].clone();
            let s = &mut c.soldiers[si];
            s.grenades_loadout = g;
            s.dressings_loadout = d;
            s.mags_loadout = m;
            s.mag_pref = match kind.as_str() {
                "cold iron" => ods_sim::units::MagKind::ColdIron,
                "salt" => ods_sim::units::MagKind::Salt,
                _ => ods_sim::units::MagKind::Blessed,
            };
        }
        if let Some(i) = delete {
            presets.remove(i);
        }
        ui.add(
            egui::TextEdit::singleline(preset_name)
                .desired_width(70.0)
                .hint_text("name"),
        );
        if ui.small_button("save kit").clicked() && !preset_name.trim().is_empty() {
            let s = &c.soldiers[si];
            presets.push((
                preset_name.trim().to_string(),
                s.grenades_loadout,
                s.dressings_loadout,
                s.mags_loadout,
                s.mag_pref.name().to_string(),
            ));
            preset_name.clear();
        }
    });

    // The quartermaster's counters.
    ui.horizontal(|ui| {
        let s = &mut c.soldiers[si];
        for (icon, count, max, stock, hover) in [
            (
                "🧨",
                &mut s.grenades_loadout,
                4u32,
                c.grenade_stock,
                "hellfire charges",
            ),
            ("✚", &mut s.dressings_loadout, 4, c.dressing_stock, "field dressings"),
            ("▤", &mut s.mags_loadout, 4, c.quarrel_stock, "spare magazines"),
        ] {
            if ui.small_button("−").clicked() && *count > 0 {
                *count -= 1;
            }
            ui.label(format!("{icon}{count}"))
                .on_hover_text(format!("{hover} ({stock} in stores)"));
            if ui.small_button("+").clicked() && *count < max {
                *count += 1;
            }
            ui.add_space(6.0);
        }
    });
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
    if slot(ui, top + 44.0, true, format!("🗡 {}", if s.has_blade { "blade" } else { "—" }), "consecrated blade: free riposte, and a drawable sidearm in the field [I]") {
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

/// The log wears its severity: alarms red, milestones gold, reports plain.
pub(crate) fn log_color(line: &str) -> egui::Color32 {
    if line.contains("!!!") || line.contains("###") {
        egui::Color32::from_rgb(225, 110, 90)
    } else if line.contains("***") {
        egui::Color32::from_rgb(225, 190, 110)
    } else if line.contains("===") || line.contains("VICTORY") {
        egui::Color32::from_rgb(200, 200, 160)
    } else {
        egui::Color32::from_rgb(200, 190, 170)
    }
}


/// What the scriptorium would tell you about a project, by its name.
fn project_lore(name: &str) -> &'static str {
    match name {
        "Rift Augury" => "Teach the augur arrays to smell reality thinning: rifts are detected sooner and more often.",
        "Interrogation" => "Bound demons can be made to talk. What they say sharpens detection and opens darker questions.",
        "Blessed Arms" => "Consecrate the armoury's powder and shot: every issued weapon bites harder.",
        "Hellsteel Plate" => "Armor beaten from the enemy's own hide. Every soldier deploys tougher.",
        "Hellfire Lance" => "A forged siege weapon that fires what hell fires back. The workshop can build them once this is known.",
        "Escort Gondola" => "Guns and armor for the zeppelin's gondola: sky-hunts are met with volleys instead of luck.",
        "The Herald's Confession" => "The captured overseer breaks. What it confesses points at the throne behind the rifts.",
        "The Name of the Enemy" => "The last question. Knowing the Name opens the way to the final assault.",
        _ => "The occultists will not say more until the work is done.",
    }
}

/// What the workshop foreman would tell you about an order, by its name.
fn item_lore(name: &str) -> &'static str {
    match name {
        "Hellfire Charges" => "Four thrown charges for the squad stores. The answer to walls, packs, and doubt.",
        "Field Dressings" => "Four dressings for the stores: staunched wounds, soldiers who come home.",
        "Trade Arms" => "Rifles for the open market. Turns bench time into treasury.",
        "Forge Lance" => "One hellfire lance, if the research is known. A soldier carrying one IS the plan.",
        "Hellsteel Limb" => "A replacement for what the war took. Fitted in the infirmary.",
        "Flesh Graft" => "A living replacement, better than the original — and it whispers to its wearer.",
        "Mount Trophy" => "A slain breed mounted in the halls: the garrison remembers who wins.",
        "Forge Arbalest" => "The silent option: a consecrated arbalest that raises no alarm.",
        "Forge Censer" => "A fire-throwing censer. Burns cover, burns fog, burns them.",
        "Forge Ram Hammer" => "A breaching hammer that cracks masonry through the demon it hits.",
        "Forge Salt Mortar" => "Salt-shot that stuns instead of kills: for the ones wanted alive.",
        _ => "The benches know their business.",
    }
}

/// The opaque desk background every full screen sits on.
fn desk_fill() -> egui::Frame {
    egui::Frame::new()
        .fill(egui::Color32::from_rgb(19, 15, 12)) // old paper over stone
        .stroke(egui::Stroke::new(1.0, egui::Color32::from_rgb(110, 88, 55)))
        .inner_margin(egui::Margin::same(18))
}

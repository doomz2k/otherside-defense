//! The Order's look: candle-lit parchment on near-black stone, gold leaf
//! for what matters. Applied once at startup; every screen inherits it.

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;

    v.dark_mode = true;
    v.override_text_color = Some(egui::Color32::from_rgb(214, 202, 178)); // parchment
    v.panel_fill = egui::Color32::from_rgba_premultiplied(16, 10, 12, 235); // stone
    v.window_fill = egui::Color32::from_rgb(22, 14, 16);
    v.window_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(120, 96, 48));
    v.faint_bg_color = egui::Color32::from_rgb(30, 20, 22);
    v.extreme_bg_color = egui::Color32::from_rgb(10, 6, 8);
    v.hyperlink_color = egui::Color32::from_rgb(220, 170, 80);
    v.warn_fg_color = egui::Color32::from_rgb(230, 160, 60);
    v.error_fg_color = egui::Color32::from_rgb(220, 80, 60);
    v.selection.bg_fill = egui::Color32::from_rgb(96, 24, 20);
    v.selection.stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(230, 190, 100));

    // Widgets: iron fittings, gold when touched.
    let iron = egui::Color32::from_rgb(38, 28, 26);
    let iron_lit = egui::Color32::from_rgb(56, 40, 34);
    v.widgets.noninteractive.bg_fill = iron;
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(70, 54, 40));
    v.widgets.inactive.bg_fill = iron;
    v.widgets.inactive.weak_bg_fill = iron;
    v.widgets.hovered.bg_fill = iron_lit;
    v.widgets.hovered.weak_bg_fill = iron_lit;
    v.widgets.hovered.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(200, 160, 80));
    v.widgets.active.bg_fill = egui::Color32::from_rgb(84, 30, 24);
    v.widgets.active.weak_bg_fill = egui::Color32::from_rgb(84, 30, 24);
    v.widgets.active.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(230, 190, 100));

    // Sharper masonry: barely-rounded corners everywhere.
    let corner = egui::CornerRadius::same(2);
    v.widgets.noninteractive.corner_radius = corner;
    v.widgets.inactive.corner_radius = corner;
    v.widgets.hovered.corner_radius = corner;
    v.widgets.active.corner_radius = corner;
    v.widgets.open.corner_radius = corner;
    v.window_corner_radius = egui::CornerRadius::same(3);
    v.menu_corner_radius = egui::CornerRadius::same(3);

    ctx.set_style(style);
}

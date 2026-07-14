//! The Order's look: candle-lit parchment on near-black stone, gold leaf
//! for what matters. Applied once at startup; every screen inherits it.

pub fn apply(ctx: &egui::Context) {
    let mut style = (*ctx.style()).clone();
    let v = &mut style.visuals;

    v.dark_mode = true;
    v.override_text_color = Some(egui::Color32::from_rgb(214, 202, 178)); // parchment
    // Furniture, not glass: every panel is solid cabinetry. The world is
    // seen through the viewport, never through the desk.
    v.panel_fill = egui::Color32::from_rgb(17, 13, 12); // stone, opaque
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
    // Bronze joinery: separators and frames read as fittings, not hairlines.
    v.widgets.noninteractive.bg_stroke = egui::Stroke::new(1.0, egui::Color32::from_rgb(96, 74, 46));
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

    // Crisp period cabinetry: no soft drop shadows anywhere.
    v.window_shadow.color = egui::Color32::TRANSPARENT;
    v.popup_shadow.color = egui::Color32::TRANSPARENT;

    // One typographic scale for the whole war office.
    use egui::{FontFamily, FontId, TextStyle};
    style.text_styles = [
        (TextStyle::Heading, FontId::new(18.0, FontFamily::Proportional)),
        (TextStyle::Body, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Monospace, FontId::new(12.5, FontFamily::Monospace)),
        (TextStyle::Button, FontId::new(13.5, FontFamily::Proportional)),
        (TextStyle::Small, FontId::new(10.5, FontFamily::Proportional)),
    ]
    .into();
    style.spacing.item_spacing = egui::vec2(6.0, 4.0);
    style.spacing.button_padding = egui::vec2(8.0, 3.0);

    ctx.set_style(style);
}

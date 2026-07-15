//! The Order's look: candle-lit parchment on near-black stone, gold leaf
//! for what matters. Applied once at startup; every screen inherits it.

/// The Order's own hands: a 17th-century English print face for what
/// speaks (headings, banners, the map), and its small-caps cut for
/// inscriptions. Body text stays plain for the sake of dense tables.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "fell".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/IMFellEnglish-Regular.ttf"
        ))),
    );
    fonts.font_data.insert(
        "fell-sc".into(),
        std::sync::Arc::new(egui::FontData::from_static(include_bytes!(
            "../assets/fonts/IMFellEnglishSC-Regular.ttf"
        ))),
    );
    fonts
        .families
        .insert(egui::FontFamily::Name("fell".into()), vec!["fell".into()]);
    fonts
        .families
        .insert(egui::FontFamily::Name("fell-sc".into()), vec!["fell-sc".into()]);
    ctx.set_fonts(fonts);
}

/// The display face at a size: banners, headings, region names.
pub fn display(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("fell-sc".into()))
}

/// The reading face at a size: lore, briefings, the chronicle.
pub fn reading(size: f32) -> egui::FontId {
    egui::FontId::new(size, egui::FontFamily::Name("fell".into()))
}

pub fn apply(ctx: &egui::Context) {
    install_fonts(ctx);
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
        (
            TextStyle::Heading,
            FontId::new(20.0, FontFamily::Name("fell-sc".into())),
        ),
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

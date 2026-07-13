//! A drawn pixel-icon set: no emoji, no platform font roulette. Every icon
//! is a few painter primitives at whatever size the UI asks for.

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    /// Brimstone: a sulfur-yellow triangle over a coal.
    Brimstone,
    /// Hellsteel: two interlocked chain links.
    Hellsteel,
    /// A hellfire charge: a bomb with a lit fuse.
    Charge,
    /// A field dressing: the mending cross.
    Dressing,
    /// The cells: prison bars.
    Cells,
    /// A consecrated blade.
    Blade,
    /// A soldier kneeling: the low wedge.
    Kneel,
    /// Bleeding: a falling drop.
    Blood,
    /// A seized mind: the violet eye.
    Eye,
    /// Down/unconscious: the grey cross-out.
    Down,
    /// A witchfire flare.
    Flare,
}

/// Paint an icon into an allocated square and return its response.
pub fn draw(ui: &mut egui::Ui, icon: Icon, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint(ui.painter(), rect, icon);
    resp
}

/// An icon with a number beside it — the resource-bar staple.
pub fn stat(ui: &mut egui::Ui, icon: Icon, text: impl Into<String>, hover: &str) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        draw(ui, icon, 13.0).on_hover_text(hover);
        ui.label(text.into()).on_hover_text(hover);
    });
}

fn paint(p: &egui::Painter, rect: egui::Rect, icon: Icon) {
    let c = rect.center();
    let s = rect.width();
    let u = s / 12.0; // icon grid unit
    match icon {
        Icon::Brimstone => {
            let gold = egui::Color32::from_rgb(220, 170, 60);
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x, rect.min.y + u),
                    egui::pos2(rect.max.x - u, rect.max.y - 2.0 * u),
                    egui::pos2(rect.min.x + u, rect.max.y - 2.0 * u),
                ],
                gold,
                egui::Stroke::NONE,
            ));
            p.circle_filled(egui::pos2(c.x, rect.max.y - 3.5 * u), 1.4 * u, egui::Color32::from_rgb(120, 60, 20));
        }
        Icon::Hellsteel => {
            let steel = egui::Color32::from_rgb(150, 155, 165);
            let st = egui::Stroke::new(1.6 * u, steel);
            p.circle_stroke(egui::pos2(c.x - 2.0 * u, c.y), 3.0 * u, st);
            p.circle_stroke(egui::pos2(c.x + 2.0 * u, c.y), 3.0 * u, st);
        }
        Icon::Charge => {
            p.circle_filled(egui::pos2(c.x, c.y + u), 3.6 * u, egui::Color32::from_rgb(60, 52, 48));
            p.line_segment(
                [egui::pos2(c.x + 2.0 * u, c.y - 2.0 * u), egui::pos2(c.x + 4.0 * u, c.y - 4.5 * u)],
                egui::Stroke::new(1.2 * u, egui::Color32::from_rgb(160, 130, 90)),
            );
            p.circle_filled(egui::pos2(c.x + 4.4 * u, c.y - 4.8 * u), 1.2 * u, egui::Color32::from_rgb(255, 150, 40));
        }
        Icon::Dressing => {
            let red = egui::Color32::from_rgb(200, 70, 60);
            p.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(8.0 * u, 2.6 * u)),
                0.0,
                red,
            );
            p.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(2.6 * u, 8.0 * u)),
                0.0,
                red,
            );
        }
        Icon::Cells => {
            let iron = egui::Color32::from_rgb(140, 130, 120);
            for k in -1..=1 {
                p.line_segment(
                    [
                        egui::pos2(c.x + k as f32 * 3.0 * u, rect.min.y + 2.0 * u),
                        egui::pos2(c.x + k as f32 * 3.0 * u, rect.max.y - 2.0 * u),
                    ],
                    egui::Stroke::new(1.2 * u, iron),
                );
            }
            p.line_segment(
                [egui::pos2(rect.min.x + u, rect.min.y + 2.0 * u), egui::pos2(rect.max.x - u, rect.min.y + 2.0 * u)],
                egui::Stroke::new(1.2 * u, iron),
            );
        }
        Icon::Blade => {
            let steel = egui::Color32::from_rgb(200, 200, 195);
            p.line_segment(
                [egui::pos2(c.x - 3.0 * u, c.y + 3.0 * u), egui::pos2(c.x + 3.5 * u, c.y - 3.5 * u)],
                egui::Stroke::new(1.6 * u, steel),
            );
            p.line_segment(
                [egui::pos2(c.x - 1.4 * u, c.y + 0.6 * u), egui::pos2(c.x + 0.6 * u, c.y + 2.6 * u)],
                egui::Stroke::new(1.4 * u, egui::Color32::from_rgb(160, 130, 70)),
            );
        }
        Icon::Kneel => {
            let tan = egui::Color32::from_rgb(190, 170, 130);
            p.circle_filled(egui::pos2(c.x - u, c.y - 3.0 * u), 1.6 * u, tan);
            p.line_segment(
                [egui::pos2(c.x - u, c.y - 1.6 * u), egui::pos2(c.x - u, c.y + 1.4 * u)],
                egui::Stroke::new(1.6 * u, tan),
            );
            p.line_segment(
                [egui::pos2(c.x - u, c.y + 1.4 * u), egui::pos2(c.x + 3.0 * u, c.y + 4.0 * u)],
                egui::Stroke::new(1.6 * u, tan),
            );
        }
        Icon::Blood => {
            let red = egui::Color32::from_rgb(210, 50, 40);
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x, rect.min.y + 1.5 * u),
                    egui::pos2(c.x + 2.6 * u, c.y + u),
                    egui::pos2(c.x - 2.6 * u, c.y + u),
                ],
                red,
                egui::Stroke::NONE,
            ));
            p.circle_filled(egui::pos2(c.x, c.y + 1.6 * u), 2.7 * u, red);
        }
        Icon::Eye => {
            let violet = egui::Color32::from_rgb(170, 90, 220);
            p.circle_stroke(c, 3.6 * u, egui::Stroke::new(1.3 * u, violet));
            p.circle_filled(c, 1.5 * u, violet);
        }
        Icon::Down => {
            let grey = egui::Color32::from_gray(150);
            let st = egui::Stroke::new(1.6 * u, grey);
            p.line_segment(
                [rect.min + egui::vec2(2.0 * u, 2.0 * u), rect.max - egui::vec2(2.0 * u, 2.0 * u)],
                st,
            );
            p.line_segment(
                [
                    egui::pos2(rect.max.x - 2.0 * u, rect.min.y + 2.0 * u),
                    egui::pos2(rect.min.x + 2.0 * u, rect.max.y - 2.0 * u),
                ],
                st,
            );
        }
        Icon::Flare => {
            let teal = egui::Color32::from_rgb(60, 230, 200);
            p.circle_filled(egui::pos2(c.x, c.y + 2.0 * u), 2.0 * u, teal);
            for k in 0..4 {
                let a = k as f32 * std::f32::consts::FRAC_PI_2 + std::f32::consts::FRAC_PI_4;
                p.line_segment(
                    [
                        egui::pos2(c.x + a.cos() * 3.0 * u, c.y + 2.0 * u + a.sin() * 3.0 * u),
                        egui::pos2(c.x + a.cos() * 5.0 * u, c.y + 2.0 * u + a.sin() * 5.0 * u),
                    ],
                    egui::Stroke::new(u, teal),
                );
            }
        }
    }
}

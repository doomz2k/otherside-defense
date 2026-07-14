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
    /// Snap shot: one slug.
    Snap,
    /// Aimed shot: a slug in a ring.
    Aimed,
    /// Auto fire: three slugs.
    Auto,
    /// Reload: the circling arrow over a magazine.
    Reload,
    /// Swap weapons: two opposing arrows.
    Swap,
    /// Smoke: the grey billow.
    Smoke,
    /// A door, ajar.
    Door,
    /// The binding rod: a shackle.
    Bind,
    /// A chalked ward circle.
    Ward,
    /// The rally banner.
    Rally,
    /// The bone saw.
    Amputate,
    /// Execution: the red cross-out.
    Execute,
    /// The confessor's candle.
    Steady,
    /// Dread: the violet jag.
    Dread,
    /// Scavenge: the reaching hook.
    Scavenge,
    /// Carry a body: one figure over another.
    Carry,
    /// Next soldier: the forward chevron over a head.
    Next,
    /// End turn: the hourglass.
    EndTurn,
    /// Threat overlay: the skull.
    Threat,
    /// The tactical map.
    Map,
    /// Floor cutaway: offset layers.
    Cutaway,
    /// Watch cones: the spread of an eye.
    Cones,
    /// No reserve: the open dash.
    NoWatch,
}

/// Paint an icon into an allocated square and return its response.
pub fn draw(ui: &mut egui::Ui, icon: Icon, size: f32) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    paint(ui.painter(), rect, icon);
    resp
}

/// A console button: a framed cell with the icon in it. Greyed cells
/// don't answer; active cells burn gold at the rim.
pub fn button(
    ui: &mut egui::Ui,
    icon: Icon,
    size: f32,
    enabled: bool,
    active: bool,
    hover: &str,
) -> bool {
    let sense = if enabled { egui::Sense::click() } else { egui::Sense::hover() };
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), sense);
    let hovered = enabled && resp.hovered();
    let fill = if active {
        egui::Color32::from_rgb(56, 44, 22)
    } else if hovered {
        egui::Color32::from_rgb(38, 32, 28)
    } else {
        egui::Color32::from_rgb(26, 22, 20)
    };
    let rim = if active {
        egui::Color32::from_rgb(230, 190, 90)
    } else if enabled {
        egui::Color32::from_gray(90)
    } else {
        egui::Color32::from_gray(45)
    };
    let p = ui.painter();
    p.rect_filled(rect, 2.0, fill);
    p.rect_stroke(rect, 2.0, egui::Stroke::new(1.0, rim), egui::StrokeKind::Inside);
    paint(p, rect.shrink(size * 0.14), icon);
    if !enabled {
        // The cell sleeps: a wash of dark over the glyph.
        p.rect_filled(rect, 2.0, egui::Color32::from_rgba_unmultiplied(12, 10, 10, 170));
    }
    let resp = resp.on_hover_text(hover);
    enabled && resp.clicked()
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
        Icon::Snap => {
            slug(p, c, u);
        }
        Icon::Aimed => {
            slug(p, c, u);
            let gold = egui::Color32::from_rgb(220, 190, 110);
            p.circle_stroke(c, 4.6 * u, egui::Stroke::new(0.9 * u, gold));
            for (dx, dy) in [(0.0, -1.0), (0.0, 1.0), (-1.0, 0.0), (1.0, 0.0)] {
                p.line_segment(
                    [
                        egui::pos2(c.x + dx * 4.6 * u, c.y + dy * 4.6 * u),
                        egui::pos2(c.x + dx * 5.8 * u, c.y + dy * 5.8 * u),
                    ],
                    egui::Stroke::new(0.9 * u, gold),
                );
            }
        }
        Icon::Auto => {
            slug(p, egui::pos2(c.x - 3.2 * u, c.y + 1.0 * u), u * 0.8);
            slug(p, egui::pos2(c.x, c.y - 0.5 * u), u * 0.8);
            slug(p, egui::pos2(c.x + 3.2 * u, c.y - 2.0 * u), u * 0.8);
        }
        Icon::Reload => {
            let brass = egui::Color32::from_rgb(200, 170, 90);
            p.rect_filled(
                egui::Rect::from_center_size(
                    egui::pos2(c.x, c.y + 2.6 * u),
                    egui::vec2(4.6 * u, 3.4 * u),
                ),
                0.5 * u,
                brass,
            );
            let steel = egui::Color32::from_rgb(170, 175, 185);
            let st = egui::Stroke::new(1.2 * u, steel);
            // Three strokes of a circling arrow.
            p.line_segment([egui::pos2(c.x - 3.4 * u, c.y - 1.4 * u), egui::pos2(c.x - 2.0 * u, c.y - 3.6 * u)], st);
            p.line_segment([egui::pos2(c.x - 2.0 * u, c.y - 3.6 * u), egui::pos2(c.x + 2.4 * u, c.y - 3.6 * u)], st);
            p.line_segment([egui::pos2(c.x + 2.4 * u, c.y - 3.6 * u), egui::pos2(c.x + 3.4 * u, c.y - 1.6 * u)], st);
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x + 4.6 * u, c.y - 2.4 * u),
                    egui::pos2(c.x + 2.2 * u, c.y - 2.4 * u),
                    egui::pos2(c.x + 3.4 * u, c.y - 0.2 * u),
                ],
                steel,
                egui::Stroke::NONE,
            ));
        }
        Icon::Swap => {
            let a = egui::Color32::from_rgb(200, 200, 195);
            let b = egui::Color32::from_rgb(160, 130, 70);
            arrow(p, egui::pos2(c.x - 4.0 * u, c.y - 2.0 * u), egui::pos2(c.x + 4.0 * u, c.y - 2.0 * u), u, a);
            arrow(p, egui::pos2(c.x + 4.0 * u, c.y + 2.0 * u), egui::pos2(c.x - 4.0 * u, c.y + 2.0 * u), u, b);
        }
        Icon::Smoke => {
            for (dx, dy, r, g) in
                [(-2.2, 1.6, 2.4, 130u8), (1.8, 1.0, 2.8, 150), (-0.2, -2.0, 3.0, 170)]
            {
                p.circle_filled(
                    egui::pos2(c.x + dx * u, c.y + dy * u),
                    r * u,
                    egui::Color32::from_gray(g),
                );
            }
        }
        Icon::Door => {
            let wood = egui::Color32::from_rgb(150, 110, 60);
            p.rect_stroke(
                egui::Rect::from_center_size(c, egui::vec2(7.0 * u, 9.0 * u)),
                0.0,
                egui::Stroke::new(1.1 * u, wood),
                egui::StrokeKind::Middle,
            );
            // The leaf, swung open.
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - 3.5 * u, c.y - 4.5 * u),
                    egui::pos2(c.x + 1.5 * u, c.y - 2.5 * u),
                    egui::pos2(c.x + 1.5 * u, c.y + 4.0 * u),
                    egui::pos2(c.x - 3.5 * u, c.y + 4.5 * u),
                ],
                egui::Color32::from_rgb(110, 80, 45),
                egui::Stroke::NONE,
            ));
            p.circle_filled(egui::pos2(c.x + 0.4 * u, c.y + 0.6 * u), 0.8 * u, egui::Color32::from_rgb(220, 190, 110));
        }
        Icon::Bind => {
            let iron = egui::Color32::from_rgb(150, 150, 160);
            p.circle_stroke(egui::pos2(c.x, c.y + 1.4 * u), 3.0 * u, egui::Stroke::new(1.4 * u, iron));
            p.line_segment(
                [egui::pos2(c.x - 4.6 * u, c.y - 3.6 * u), egui::pos2(c.x + 4.6 * u, c.y - 3.6 * u)],
                egui::Stroke::new(1.4 * u, iron),
            );
            p.line_segment(
                [egui::pos2(c.x, c.y - 3.6 * u), egui::pos2(c.x, c.y - 1.6 * u)],
                egui::Stroke::new(1.2 * u, iron),
            );
        }
        Icon::Ward => {
            let chalk = egui::Color32::from_rgb(240, 220, 160);
            // A dashed circle: chalk on stone.
            for k in 0..8 {
                let a0 = k as f32 * std::f32::consts::TAU / 8.0;
                let a1 = a0 + 0.55;
                p.line_segment(
                    [
                        egui::pos2(c.x + a0.cos() * 4.2 * u, c.y + a0.sin() * 4.2 * u),
                        egui::pos2(c.x + a1.cos() * 4.2 * u, c.y + a1.sin() * 4.2 * u),
                    ],
                    egui::Stroke::new(1.1 * u, chalk),
                );
            }
            p.circle_filled(c, 1.2 * u, chalk);
        }
        Icon::Rally => {
            let pole = egui::Color32::from_rgb(160, 140, 110);
            p.line_segment(
                [egui::pos2(c.x - 3.0 * u, rect.max.y - 1.5 * u), egui::pos2(c.x - 3.0 * u, rect.min.y + 1.5 * u)],
                egui::Stroke::new(1.1 * u, pole),
            );
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - 3.0 * u, rect.min.y + 1.5 * u),
                    egui::pos2(c.x + 4.5 * u, rect.min.y + 3.0 * u),
                    egui::pos2(c.x - 3.0 * u, rect.min.y + 5.0 * u),
                ],
                egui::Color32::from_rgb(200, 60, 50),
                egui::Stroke::NONE,
            ));
        }
        Icon::Amputate => {
            let steel = egui::Color32::from_rgb(190, 195, 200);
            p.line_segment(
                [egui::pos2(c.x - 4.5 * u, c.y - 1.0 * u), egui::pos2(c.x + 4.5 * u, c.y - 1.0 * u)],
                egui::Stroke::new(1.6 * u, steel),
            );
            // Teeth.
            for k in 0..5 {
                let x = c.x - 3.6 * u + k as f32 * 1.8 * u;
                p.add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(x - 0.7 * u, c.y - 0.4 * u),
                        egui::pos2(x + 0.7 * u, c.y - 0.4 * u),
                        egui::pos2(x, c.y + 1.4 * u),
                    ],
                    steel,
                    egui::Stroke::NONE,
                ));
            }
            p.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x + 3.4 * u, c.y - 2.6 * u), egui::vec2(3.0 * u, 1.6 * u)),
                0.5 * u,
                egui::Color32::from_rgb(150, 110, 60),
            );
        }
        Icon::Execute => {
            let red = egui::Color32::from_rgb(210, 60, 50);
            let st = egui::Stroke::new(1.9 * u, red);
            p.line_segment([rect.min + egui::vec2(2.5 * u, 2.5 * u), rect.max - egui::vec2(2.5 * u, 2.5 * u)], st);
            p.line_segment(
                [
                    egui::pos2(rect.max.x - 2.5 * u, rect.min.y + 2.5 * u),
                    egui::pos2(rect.min.x + 2.5 * u, rect.max.y - 2.5 * u),
                ],
                st,
            );
        }
        Icon::Steady => {
            let wax = egui::Color32::from_rgb(230, 220, 190);
            p.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 1.6 * u), egui::vec2(2.6 * u, 5.6 * u)),
                0.5 * u,
                wax,
            );
            p.circle_filled(egui::pos2(c.x, c.y - 2.8 * u), 1.5 * u, egui::Color32::from_rgb(255, 170, 60));
        }
        Icon::Dread => {
            let violet = egui::Color32::from_rgb(170, 90, 220);
            // A jagged bolt of wrongness.
            let pts = [
                egui::pos2(c.x + 1.5 * u, rect.min.y + 1.5 * u),
                egui::pos2(c.x - 1.5 * u, c.y - 0.5 * u),
                egui::pos2(c.x + 0.8 * u, c.y + 0.5 * u),
                egui::pos2(c.x - 1.5 * u, rect.max.y - 1.5 * u),
            ];
            for w in pts.windows(2) {
                p.line_segment([w[0], w[1]], egui::Stroke::new(1.4 * u, violet));
            }
        }
        Icon::Scavenge => {
            let tan = egui::Color32::from_rgb(190, 170, 130);
            arrow(p, egui::pos2(c.x, rect.min.y + 1.5 * u), egui::pos2(c.x, c.y + 1.0 * u), u, tan);
            p.rect_stroke(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 3.2 * u), egui::vec2(7.0 * u, 3.4 * u)),
                0.0,
                egui::Stroke::new(1.1 * u, egui::Color32::from_rgb(150, 110, 60)),
                egui::StrokeKind::Middle,
            );
        }
        Icon::Carry => {
            let tan = egui::Color32::from_rgb(190, 170, 130);
            p.circle_filled(egui::pos2(c.x - 1.0 * u, c.y + 0.2 * u), 1.6 * u, tan);
            p.line_segment(
                [egui::pos2(c.x - 1.0 * u, c.y + 1.6 * u), egui::pos2(c.x - 1.0 * u, rect.max.y - 1.5 * u)],
                egui::Stroke::new(1.5 * u, tan),
            );
            // The carried: slung level across the shoulders.
            let grey = egui::Color32::from_gray(150);
            p.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y - 2.6 * u), egui::vec2(8.0 * u, 1.8 * u)),
                0.8 * u,
                grey,
            );
        }
        Icon::Next => {
            let gold = egui::Color32::from_rgb(220, 190, 110);
            p.circle_filled(egui::pos2(c.x - 2.6 * u, c.y - 2.0 * u), 1.7 * u, gold);
            p.line_segment(
                [egui::pos2(c.x - 2.6 * u, c.y - 0.4 * u), egui::pos2(c.x - 2.6 * u, c.y + 3.6 * u)],
                egui::Stroke::new(1.5 * u, gold),
            );
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x + 0.6 * u, c.y - 2.6 * u),
                    egui::pos2(c.x + 4.6 * u, c.y),
                    egui::pos2(c.x + 0.6 * u, c.y + 2.6 * u),
                ],
                gold,
                egui::Stroke::NONE,
            ));
        }
        Icon::EndTurn => {
            let sand = egui::Color32::from_rgb(220, 190, 110);
            let st = egui::Stroke::new(1.0 * u, egui::Color32::from_gray(140));
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x - 3.2 * u, rect.min.y + 1.5 * u),
                    egui::pos2(c.x + 3.2 * u, rect.min.y + 1.5 * u),
                    egui::pos2(c.x, c.y),
                ],
                sand,
                st,
            ));
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(c.x, c.y),
                    egui::pos2(c.x + 3.2 * u, rect.max.y - 1.5 * u),
                    egui::pos2(c.x - 3.2 * u, rect.max.y - 1.5 * u),
                ],
                egui::Color32::from_rgb(90, 76, 45),
                st,
            ));
        }
        Icon::Threat => {
            let bone = egui::Color32::from_rgb(225, 220, 205);
            p.circle_filled(egui::pos2(c.x, c.y - 1.0 * u), 3.6 * u, bone);
            p.rect_filled(
                egui::Rect::from_center_size(egui::pos2(c.x, c.y + 2.6 * u), egui::vec2(4.4 * u, 2.4 * u)),
                0.0,
                bone,
            );
            let dark = egui::Color32::from_rgb(30, 24, 22);
            p.circle_filled(egui::pos2(c.x - 1.4 * u, c.y - 1.2 * u), 1.0 * u, dark);
            p.circle_filled(egui::pos2(c.x + 1.4 * u, c.y - 1.2 * u), 1.0 * u, dark);
            for k in -1..=1 {
                p.line_segment(
                    [
                        egui::pos2(c.x + k as f32 * 1.4 * u, c.y + 1.6 * u),
                        egui::pos2(c.x + k as f32 * 1.4 * u, c.y + 3.6 * u),
                    ],
                    egui::Stroke::new(0.7 * u, dark),
                );
            }
        }
        Icon::Map => {
            let paper = egui::Color32::from_rgb(210, 195, 150);
            p.rect_filled(
                egui::Rect::from_center_size(c, egui::vec2(9.0 * u, 7.0 * u)),
                0.0,
                paper,
            );
            let fold = egui::Color32::from_rgb(160, 145, 105);
            p.line_segment([egui::pos2(c.x - 1.5 * u, c.y - 3.5 * u), egui::pos2(c.x - 1.5 * u, c.y + 3.5 * u)], egui::Stroke::new(0.8 * u, fold));
            p.line_segment([egui::pos2(c.x + 1.5 * u, c.y - 3.5 * u), egui::pos2(c.x + 1.5 * u, c.y + 3.5 * u)], egui::Stroke::new(0.8 * u, fold));
            p.circle_filled(egui::pos2(c.x + 2.8 * u, c.y - 1.4 * u), 0.9 * u, egui::Color32::from_rgb(200, 60, 50));
        }
        Icon::Cutaway => {
            let st = egui::Stroke::new(1.0 * u, egui::Color32::from_rgb(170, 160, 140));
            for k in 0..3 {
                let y = c.y - 2.4 * u + k as f32 * 2.4 * u;
                let inset = k as f32 * 0.8 * u;
                p.line_segment(
                    [egui::pos2(rect.min.x + 2.0 * u + inset, y), egui::pos2(rect.max.x - 2.0 * u - inset, y)],
                    st,
                );
            }
        }
        Icon::Cones => {
            let gold = egui::Color32::from_rgba_unmultiplied(220, 190, 110, 120);
            p.add(egui::Shape::convex_polygon(
                vec![
                    egui::pos2(rect.min.x + 2.0 * u, rect.max.y - 2.0 * u),
                    egui::pos2(rect.max.x - 1.5 * u, c.y - 1.0 * u),
                    egui::pos2(c.x + 1.0 * u, rect.min.y + 1.5 * u),
                ],
                gold,
                egui::Stroke::NONE,
            ));
            p.circle_filled(egui::pos2(rect.min.x + 2.0 * u, rect.max.y - 2.0 * u), 1.3 * u, egui::Color32::from_rgb(220, 190, 110));
        }
        Icon::NoWatch => {
            p.line_segment(
                [egui::pos2(c.x - 3.0 * u, c.y), egui::pos2(c.x + 3.0 * u, c.y)],
                egui::Stroke::new(1.4 * u, egui::Color32::from_gray(130)),
            );
        }
    }
}

/// One rifle slug, angled like it means it.
fn slug(p: &egui::Painter, c: egui::Pos2, u: f32) {
    let brass = egui::Color32::from_rgb(210, 175, 90);
    p.rect_filled(
        egui::Rect::from_center_size(egui::pos2(c.x, c.y + 1.2 * u), egui::vec2(2.4 * u, 4.0 * u)),
        0.4 * u,
        brass,
    );
    p.add(egui::Shape::convex_polygon(
        vec![
            egui::pos2(c.x - 1.2 * u, c.y - 0.8 * u),
            egui::pos2(c.x + 1.2 * u, c.y - 0.8 * u),
            egui::pos2(c.x, c.y - 3.4 * u),
        ],
        egui::Color32::from_rgb(150, 110, 70),
        egui::Stroke::NONE,
    ));
}

/// A stroked arrow with a solid head.
fn arrow(p: &egui::Painter, from: egui::Pos2, to: egui::Pos2, u: f32, color: egui::Color32) {
    p.line_segment([from, to], egui::Stroke::new(1.2 * u, color));
    let dir = (to - from).normalized();
    let side = egui::vec2(-dir.y, dir.x);
    p.add(egui::Shape::convex_polygon(
        vec![to, to - dir * 2.4 * u + side * 1.6 * u, to - dir * 2.4 * u - side * 1.6 * u],
        color,
        egui::Stroke::NONE,
    ));
}

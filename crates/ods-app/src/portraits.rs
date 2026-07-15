//! Seeded pixel portraits: every soldier gets a face, drawn from their
//! name alone — skin, hair, eyes, jaw, and the scars the war added.

fn hash(seed: u64, k: u32) -> u32 {
    let mut h = (seed as u32)
        .wrapping_mul(747796405)
        .wrapping_add(k)
        .wrapping_mul(2654435761);
    h ^= h >> 15;
    h.wrapping_mul(2246822519) >> 8
}

pub fn seed_of(name: &str) -> u64 {
    name.bytes().fold(0xCBF2_9CE4u64, |acc, b| {
        (acc ^ b as u64).wrapping_mul(0x0100_0000_01B3)
    })
}

/// Draw a portrait plate into an allocated square. `scars` adds marks.
pub fn draw(ui: &mut egui::Ui, seed: u64, size: f32, scars: usize) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size), egui::Sense::hover());
    let p = ui.painter_at(rect);
    let u = size / 16.0; // portrait grid unit

    // The plate.
    p.rect_filled(rect, 1.0, egui::Color32::from_rgb(26, 20, 22));
    p.rect_stroke(
        rect,
        1.0,
        egui::Stroke::new(1.0, egui::Color32::from_rgb(90, 72, 40)),
        egui::StrokeKind::Inside,
    );

    let skin = [
        egui::Color32::from_rgb(224, 178, 140),
        egui::Color32::from_rgb(198, 150, 110),
        egui::Color32::from_rgb(150, 105, 72),
        egui::Color32::from_rgb(104, 72, 50),
    ][hash(seed, 1) as usize % 4];
    let hair = [
        egui::Color32::from_rgb(38, 30, 24),
        egui::Color32::from_rgb(96, 66, 34),
        egui::Color32::from_rgb(160, 140, 110),
        egui::Color32::from_rgb(70, 70, 74),
    ][hash(seed, 2) as usize % 4];

    let cx = rect.center().x;
    // Shoulders in the Order's blue.
    p.rect_filled(
        egui::Rect::from_min_max(
            egui::pos2(rect.min.x + 2.0 * u, rect.max.y - 4.0 * u),
            egui::pos2(rect.max.x - 2.0 * u, rect.max.y - u),
        ),
        0.0,
        egui::Color32::from_rgb(58, 86, 128),
    );
    // The face: jaw width varies.
    let jaw = 3.4 + (hash(seed, 3) % 3) as f32 * 0.7;
    let face = egui::Rect::from_min_max(
        egui::pos2(cx - jaw * u, rect.min.y + 4.0 * u),
        egui::pos2(cx + jaw * u, rect.max.y - 4.0 * u),
    );
    p.rect_filled(face, 1.0, skin);
    // Hair: bald / cap / side-part / full, by fate.
    match hash(seed, 4) % 4 {
        0 => {}
        1 => {
            p.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(face.min.x - 0.5 * u, face.min.y - 1.2 * u),
                    egui::pos2(face.max.x + 0.5 * u, face.min.y + 1.6 * u),
                ),
                0.0,
                hair,
            );
        }
        2 => {
            p.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(face.min.x - 0.5 * u, face.min.y - 1.0 * u),
                    egui::pos2(cx + 1.0 * u, face.min.y + 2.4 * u),
                ),
                0.0,
                hair,
            );
        }
        _ => {
            p.rect_filled(
                egui::Rect::from_min_max(
                    egui::pos2(face.min.x - 0.8 * u, face.min.y - 1.4 * u),
                    egui::pos2(face.max.x + 0.8 * u, face.min.y + 2.0 * u),
                ),
                0.0,
                hair,
            );
            // Sideburns.
            for x in [face.min.x - 0.8 * u, face.max.x] {
                p.rect_filled(
                    egui::Rect::from_min_size(
                        egui::pos2(x, face.min.y + 2.0 * u),
                        egui::vec2(0.8 * u, 3.0 * u),
                    ),
                    0.0,
                    hair,
                );
            }
        }
    }
    // Eyes and brows.
    let eye_y = face.min.y + 3.6 * u;
    for sx in [-1.9f32, 1.9] {
        p.rect_filled(
            egui::Rect::from_center_size(egui::pos2(cx + sx * u, eye_y), egui::vec2(1.2 * u, u)),
            0.0,
            egui::Color32::from_rgb(20, 18, 20),
        );
        p.rect_filled(
            egui::Rect::from_center_size(
                egui::pos2(cx + sx * u, eye_y - 1.2 * u),
                egui::vec2(1.8 * u, 0.6 * u),
            ),
            0.0,
            hair,
        );
    }
    // Mouth: a grim line, set by temperament.
    let mouth_y = face.max.y - 2.0 * u;
    let tilt = (hash(seed, 5) % 3) as f32 * 0.4 - 0.4;
    p.line_segment(
        [
            egui::pos2(cx - 1.4 * u, mouth_y + tilt * u),
            egui::pos2(cx + 1.4 * u, mouth_y - tilt * u),
        ],
        egui::Stroke::new(0.7 * u, egui::Color32::from_rgb(110, 60, 50)),
    );
    // Scars: the war writes on the face.
    for k in 0..scars.min(3) as u32 {
        let x = face.min.x + (1.0 + (hash(seed, 10 + k) % 5) as f32) * u;
        let y = face.min.y + (2.0 + (hash(seed, 20 + k) % 6) as f32) * u;
        p.line_segment(
            [egui::pos2(x, y), egui::pos2(x + 1.6 * u, y + 2.2 * u)],
            egui::Stroke::new(0.5 * u, egui::Color32::from_rgb(160, 70, 60)),
        );
    }
    resp
}

/// The soldier in the glass: an isometric painter's render of the actual
/// carved blueprint — every tagged box drawn as a little prism, deepest
/// first, so the figure reads as a miniature on a stand. `yaw` turns it.
pub fn draw_figure_iso(
    ui: &mut egui::Ui,
    species: ods_sim::units::Species,
    weapon_key: Option<&str>,
    size: f32,
    yaw: f32,
) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(size, size * 1.2), egui::Sense::hover());
    let p = ui.painter_at(rect);
    // The stand.
    p.rect_filled(
        egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.max.y - size * 0.05),
            egui::vec2(size * 0.65, size * 0.08),
        ),
        3.0,
        egui::Color32::from_rgb(40, 32, 26),
    );
    let scale = size / 22.0;
    let origin = egui::pos2(rect.center().x, rect.max.y - size * 0.10);
    let (ys, yc) = yaw.sin_cos();
    // Isometric axes after the turn: x' spreads right, y' spreads left,
    // z rises. Screen y is inverted.
    let project = |q: glam::Vec3| -> egui::Pos2 {
        let rx = q.x * yc - q.y * ys;
        let ry = q.x * ys + q.y * yc;
        egui::pos2(
            origin.x + (rx - ry) * 0.86 * scale,
            origin.y - ((rx + ry) * 0.5 + q.z * 1.35) * scale,
        )
    };
    let mut boxes: Vec<crate::figures::PartBox> =
        crate::figures::blueprint(species).to_vec();
    if let Some(key) = weapon_key {
        boxes.extend_from_slice(crate::figures::weapon_model(key));
    }
    // Painter's order: back-to-front along the turned depth axis.
    boxes.sort_by(|a, b| {
        let da = (a.min + a.max) / 2.0;
        let db = (b.min + b.max) / 2.0;
        let ka = (da.x * yc - da.y * ys) + (da.x * ys + da.y * yc) + da.z * 0.05;
        let kb = (db.x * yc - db.y * ys) + (db.x * ys + db.y * yc) + db.z * 0.05;
        ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
    });
    for b in &boxes {
        let c = |f: f32, arr: [f32; 4]| {
            egui::Color32::from_rgb(
                (arr[0] * f * 255.0) as u8,
                (arr[1] * f * 255.0) as u8,
                (arr[2] * f * 255.0) as u8,
            )
        };
        let (lo, hi) = (b.min, b.max);
        let v = |x: f32, y: f32, z: f32| project(glam::Vec3::new(x, y, z));
        // Top face, then the two visible flanks.
        p.add(egui::Shape::convex_polygon(
            vec![v(lo.x, lo.y, hi.z), v(hi.x, lo.y, hi.z), v(hi.x, hi.y, hi.z), v(lo.x, hi.y, hi.z)],
            c(1.0, b.color),
            egui::Stroke::NONE,
        ));
        p.add(egui::Shape::convex_polygon(
            vec![v(hi.x, lo.y, lo.z), v(hi.x, hi.y, lo.z), v(hi.x, hi.y, hi.z), v(hi.x, lo.y, hi.z)],
            c(0.72, b.color),
            egui::Stroke::NONE,
        ));
        p.add(egui::Shape::convex_polygon(
            vec![v(lo.x, lo.y, lo.z), v(hi.x, lo.y, lo.z), v(hi.x, lo.y, hi.z), v(lo.x, lo.y, hi.z)],
            c(0.52, b.color),
            egui::Stroke::NONE,
        ));
    }
    resp
}

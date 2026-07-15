//! The Basescape: the chapterhouse as a voxel diorama — the classic base
//! cross-section, one distinctive building per facility cell, rendered as
//! a lit scene behind the management panels.

use glam::Vec3;
use ods_geo::{Chapterhouse, Facility, GRID};
use ods_render::LitVertex;

/// Cell spacing and building footprint, in scene units.
const PITCH: f32 = 44.0;
const CELL: f32 = 17.0; // half-extent of a building footprint

/// Where the camera should look.
pub fn scene_center() -> Vec3 {
    let extent = GRID as f32 * PITCH;
    Vec3::new(extent / 2.0, extent / 2.0, 6.0)
}

/// Build the whole diorama for one chapterhouse — its people included:
/// soldiers drill in rows before the gate, occultists pace the library
/// walks, artificers stand at the workshop, and lanterns burn at every
/// finished door. `time` moves them.
pub fn build_base_scene(
    base: &Chapterhouse,
    soldiers_here: usize,
    prisoners: bool,
    time: f32,
) -> (Vec<LitVertex>, Vec<u32>) {
    let mut v = Vec::new();
    let mut i = Vec::new();
    let extent = GRID as f32 * PITCH;
    let mid = extent / 2.0;

    // The founding slab, and a ring of wild ground beyond it.
    box_at(
        &mut v,
        &mut i,
        Vec3::new(mid, mid, -4.0),
        Vec3::new(mid + 20.0, mid + 20.0, 4.0),
        [0.24, 0.23, 0.20, 1.0],
    );
    box_at(
        &mut v,
        &mut i,
        Vec3::new(mid, mid, -9.0),
        Vec3::new(mid + 46.0, mid + 46.0, 4.0),
        [0.20, 0.24, 0.16, 1.0],
    );

    for y in 0..GRID {
        for x in 0..GRID {
            let c = Vec3::new(
                x as f32 * PITCH + PITCH / 2.0,
                y as f32 * PITCH + PITCH / 2.0,
                0.0,
            );
            match base.facility_at(x, y) {
                None => {
                    // An empty pad, swept and waiting.
                    box_at(
                        &mut v,
                        &mut i,
                        c + Vec3::new(0.0, 0.0, 0.5),
                        Vec3::new(CELL, CELL, 0.5),
                        [0.17, 0.16, 0.15, 1.0],
                    );
                }
                Some((f, true)) => {
                    push_facility(&mut v, &mut i, c, f);
                    // A lantern at every finished door: the Order keeps
                    // its lights against the dark.
                    box_at(
                        &mut v,
                        &mut i,
                        c + Vec3::new(CELL - 2.0, -CELL + 2.0, 10.0),
                        Vec3::new(1.0, 1.0, 1.4),
                        [1.0, 0.8, 0.35, 1.0],
                    );
                    // The living house: staff at their stations.
                    match f {
                        Facility::Library => {
                            for k in 0..base.occultists.min(3) {
                                let a = time * 0.35 + k as f32 * 2.1;
                                let at = c + Vec3::new(a.cos() * 12.0, a.sin() * 12.0, 0.0);
                                mini_figure(&mut v, &mut i, at, a + 1.57, Kind::Occultist, time + k as f32);
                            }
                        }
                        Facility::Workshop => {
                            for k in 0..base.artificers.min(3) {
                                let at = c + Vec3::new(-10.0 + k as f32 * 10.0, -CELL + 6.0, 0.0);
                                mini_figure(&mut v, &mut i, at, 1.57, Kind::Artificer, time + k as f32);
                            }
                        }
                        Facility::Vault if prisoners => {
                            // Something paces behind the wards.
                            let a = time * 0.8;
                            mini_figure(
                                &mut v,
                                &mut i,
                                c + Vec3::new(a.cos() * 5.0, a.sin() * 5.0, 0.0),
                                a + 1.57,
                                Kind::Captive,
                                time,
                            );
                        }
                        Facility::Kennel => {
                            // A hound circles its pen.
                            let a = time * 1.3;
                            let at = c + Vec3::new(a.cos() * 9.0, a.sin() * 9.0, 0.0);
                            box_at(&mut v, &mut i, at + Vec3::new(0.0, 0.0, 2.5), Vec3::new(2.6, 1.4, 1.6), [0.35, 0.10, 0.08, 1.0]);
                            box_at(&mut v, &mut i, at + Vec3::new(a.cos() * 2.4, a.sin() * 2.4, 3.4), Vec3::new(1.1, 1.1, 1.1), [0.22, 0.07, 0.06, 1.0]);
                        }
                        _ => {}
                    }
                }
                Some((_, false)) => {
                    // Scaffolding: four corner posts and a half-raised frame.
                    let wood = [0.42, 0.32, 0.18, 1.0];
                    for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        box_at(
                            &mut v,
                            &mut i,
                            c + Vec3::new(sx * (CELL - 2.0), sy * (CELL - 2.0), 9.0),
                            Vec3::new(1.4, 1.4, 9.0),
                            wood,
                        );
                    }
                    box_at(
                        &mut v,
                        &mut i,
                        c + Vec3::new(0.0, 0.0, 18.0),
                        Vec3::new(CELL - 1.0, CELL - 1.0, 1.2),
                        wood,
                    );
                    box_at(
                        &mut v,
                        &mut i,
                        c + Vec3::new(0.0, 0.0, 3.0),
                        Vec3::new(CELL - 4.0, CELL - 4.0, 3.0),
                        [0.30, 0.29, 0.27, 1.0],
                    );
                }
            }
        }
    }
    // The muster: soldiers drill in ranks on the open ground south of
    // the founding slab, marking time.
    for k in 0..soldiers_here.min(12) {
        let (col, row) = (k % 4, k / 4);
        let at = Vec3::new(
            mid - 33.0 + col as f32 * 22.0,
            -30.0 - row as f32 * 20.0,
            0.0,
        );
        mini_figure(&mut v, &mut i, at, 1.57, Kind::Soldier, time * 1.4 + k as f32 * 0.7);
    }
    (v, i)
}

/// Who a miniature is, which decides its colors.
#[derive(Clone, Copy)]
enum Kind {
    Soldier,
    Occultist,
    Artificer,
    Captive,
}

/// A tiny figure on the diorama: boots, coat, head, marking time with a
/// slight bob. Facing turns the shoulders; phase staggers the bob.
fn mini_figure(
    v: &mut Vec<LitVertex>,
    i: &mut Vec<u32>,
    at: Vec3,
    facing: f32,
    kind: Kind,
    phase: f32,
) {
    let (coat, trim) = match kind {
        Kind::Soldier => ([0.25, 0.42, 0.68, 1.0], [0.15, 0.15, 0.18, 1.0]),
        Kind::Occultist => ([0.30, 0.12, 0.40, 1.0], [0.20, 0.08, 0.28, 1.0]),
        Kind::Artificer => ([0.45, 0.32, 0.18, 1.0], [0.25, 0.18, 0.10, 1.0]),
        Kind::Captive => ([0.55, 0.14, 0.10, 1.0], [0.90, 0.85, 0.70, 1.0]),
    };
    let bob = (phase * 2.0).sin() * 0.5;
    let base = at + Vec3::new(0.0, 0.0, bob);
    let (fs, fc) = facing.sin_cos();
    let side = Vec3::new(fc, fs, 0.0) * 1.4;
    // Legs, coat, head, and a shoulder line to carry the facing.
    box_at(v, i, base + Vec3::new(0.0, 0.0, 2.0) - side * 0.8, Vec3::new(1.0, 1.0, 2.0), trim);
    box_at(v, i, base + Vec3::new(0.0, 0.0, 2.0) + side * 0.8, Vec3::new(1.0, 1.0, 2.0), trim);
    box_at(v, i, base + Vec3::new(0.0, 0.0, 6.5), Vec3::new(2.4, 2.0, 2.6), coat);
    box_at(v, i, base + Vec3::new(0.0, 0.0, 8.6) + side, Vec3::new(1.4, 1.4, 0.8), coat);
    box_at(v, i, base + Vec3::new(0.0, 0.0, 8.6) - side, Vec3::new(1.4, 1.4, 0.8), coat);
    box_at(v, i, base + Vec3::new(0.0, 0.0, 10.6), Vec3::new(1.3, 1.3, 1.3), [0.85, 0.72, 0.6, 1.0]);
}

/// One finished building, styled by what it is.
fn push_facility(v: &mut Vec<LitVertex>, i: &mut Vec<u32>, c: Vec3, f: Facility) {
    let stone = [0.36, 0.33, 0.29, 1.0];
    let dark = [0.22, 0.20, 0.18, 1.0];
    match f {
        Facility::Gatehouse => {
            // Two towers and the lintel between them: the way in and out.
            for side in [-1.0, 1.0] {
                box_at(
                    v,
                    i,
                    c + Vec3::new(side * (CELL - 5.0), 0.0, 16.0),
                    Vec3::new(5.0, 8.0, 16.0),
                    stone,
                );
                box_at(
                    v,
                    i,
                    c + Vec3::new(side * (CELL - 5.0), 0.0, 34.0),
                    Vec3::new(6.0, 9.0, 2.0),
                    dark,
                );
            }
            box_at(v, i, c + Vec3::new(0.0, 0.0, 26.0), Vec3::new(CELL - 2.0, 7.0, 4.0), stone);
        }
        Facility::Quarters => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 8.0), Vec3::new(CELL - 1.0, 12.0, 8.0), [0.40, 0.28, 0.18, 1.0]);
            box_at(v, i, c + Vec3::new(0.0, 0.0, 18.5), Vec3::new(CELL - 3.0, 10.0, 2.5), dark);
            box_at(v, i, c + Vec3::new(CELL - 7.0, 4.0, 24.0), Vec3::new(1.8, 1.8, 4.0), stone);
        }
        Facility::AugurArray => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 5.0), Vec3::new(9.0, 9.0, 5.0), stone);
            box_at(v, i, c + Vec3::new(0.0, 0.0, 16.0), Vec3::new(1.6, 1.6, 8.0), dark);
            // The listening slab, tipped toward the sky.
            box_at(v, i, c + Vec3::new(0.0, -3.0, 26.0), Vec3::new(10.0, 7.0, 1.4), [0.15, 0.55, 0.50, 1.0]);
        }
        Facility::Library => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 11.0), Vec3::new(CELL - 2.0, CELL - 4.0, 11.0), [0.30, 0.30, 0.38, 1.0]);
            for k in -1..=1 {
                box_at(
                    v,
                    i,
                    c + Vec3::new(k as f32 * 9.0, -(CELL - 3.0), 9.0),
                    Vec3::new(1.6, 1.6, 9.0),
                    [0.55, 0.53, 0.48, 1.0],
                );
            }
            box_at(v, i, c + Vec3::new(0.0, 0.0, 24.0), Vec3::new(CELL, CELL - 2.0, 2.0), dark);
        }
        Facility::Infirmary => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 9.0), Vec3::new(CELL - 2.0, CELL - 2.0, 9.0), [0.62, 0.62, 0.58, 1.0]);
            // The crossed sign of mending, laid on the roof.
            box_at(v, i, c + Vec3::new(0.0, 0.0, 19.5), Vec3::new(8.0, 2.4, 1.4), [0.70, 0.12, 0.10, 1.0]);
            box_at(v, i, c + Vec3::new(0.0, 0.0, 19.5), Vec3::new(2.4, 8.0, 1.4), [0.70, 0.12, 0.10, 1.0]);
        }
        Facility::Workshop => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 9.0), Vec3::new(CELL - 1.0, CELL - 3.0, 9.0), [0.42, 0.36, 0.26, 1.0]);
            box_at(v, i, c + Vec3::new(-(CELL - 8.0), -(CELL - 10.0), 24.0), Vec3::new(2.6, 2.6, 7.0), dark);
            box_at(v, i, c + Vec3::new(-(CELL - 8.0), -(CELL - 10.0), 31.5), Vec3::new(3.0, 3.0, 1.0), [0.95, 0.45, 0.10, 1.0]);
        }
        Facility::Chapel => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 10.0), Vec3::new(11.0, CELL - 2.0, 10.0), stone);
            // The pitched roof, stacked toward heaven, and the spire.
            box_at(v, i, c + Vec3::new(0.0, 0.0, 22.0), Vec3::new(8.0, CELL - 3.0, 2.5), dark);
            box_at(v, i, c + Vec3::new(0.0, 0.0, 26.0), Vec3::new(4.5, CELL - 5.0, 2.0), dark);
            box_at(v, i, c + Vec3::new(0.0, CELL - 8.0, 34.0), Vec3::new(1.6, 1.6, 8.0), stone);
        }
        Facility::Sanctum => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 8.0), Vec3::new(CELL - 4.0, CELL - 4.0, 8.0), [0.16, 0.10, 0.20, 1.0]);
            box_at(v, i, c + Vec3::new(0.0, 0.0, 17.5), Vec3::new(CELL - 8.0, CELL - 8.0, 1.5), [0.45, 0.20, 0.60, 1.0]);
        }
        Facility::TrainingGround => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 1.0), Vec3::new(CELL - 1.0, CELL - 1.0, 1.0), [0.35, 0.30, 0.22, 1.0]);
            for (sx, sy) in [(-1.0, -1.0), (1.0, 1.0), (-1.0, 1.0), (1.0, -1.0)] {
                box_at(
                    v,
                    i,
                    c + Vec3::new(sx * 9.0, sy * 9.0, 6.0),
                    Vec3::new(1.4, 1.4, 5.0),
                    [0.45, 0.35, 0.20, 1.0],
                );
            }
        }
        Facility::WardTower => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 18.0), Vec3::new(6.0, 6.0, 18.0), stone);
            box_at(v, i, c + Vec3::new(0.0, 0.0, 30.0), Vec3::new(7.0, 7.0, 1.6), [0.15, 0.85, 0.75, 1.0]);
            box_at(v, i, c + Vec3::new(0.0, 0.0, 38.0), Vec3::new(4.5, 4.5, 2.0), dark);
        }
        Facility::Kennel => {
            box_at(v, i, c + Vec3::new(-5.0, 0.0, 6.0), Vec3::new(8.0, 9.0, 6.0), [0.38, 0.30, 0.20, 1.0]);
            // The run: a low fence around the yard.
            for k in -2..=2 {
                box_at(
                    v,
                    i,
                    c + Vec3::new(CELL - 4.0, k as f32 * 6.5, 3.0),
                    Vec3::new(0.9, 0.9, 3.0),
                    [0.45, 0.38, 0.26, 1.0],
                );
            }
        }
        Facility::Vault => {
            box_at(v, i, c + Vec3::new(0.0, 0.0, 7.0), Vec3::new(CELL - 3.0, CELL - 3.0, 7.0), [0.10, 0.09, 0.11, 1.0]);
            box_at(v, i, c + Vec3::new(0.0, 0.0, 14.8), Vec3::new(CELL - 5.0, CELL - 5.0, 0.8), [0.75, 0.60, 0.20, 1.0]);
        }
    }
}

/// An axis-aligned box with correct outward normals.
fn box_at(
    vertices: &mut Vec<LitVertex>,
    indices: &mut Vec<u32>,
    center: Vec3,
    half: Vec3,
    color: [f32; 4],
) {
    let h = [half.x, half.y, half.z];
    for d in 0..3usize {
        let u = (d + 1) % 3;
        let w = (d + 2) % 3;
        for front in [true, false] {
            let mut normal = [0.0f32; 3];
            normal[d] = if front { 1.0 } else { -1.0 };
            let corner = |cu: f32, cw: f32| -> [f32; 3] {
                let mut p = [0.0f32; 3];
                p[d] = if front { h[d] } else { -h[d] };
                p[u] = cu;
                p[w] = cw;
                [center.x + p[0], center.y + p[1], center.z + p[2]]
            };
            let (p00, p10, p11, p01) = (
                corner(-h[u], -h[w]),
                corner(h[u], -h[w]),
                corner(h[u], h[w]),
                corner(-h[u], h[w]),
            );
            let first = vertices.len() as u32;
            let quad = if front { [p00, p10, p11, p01] } else { [p00, p01, p11, p10] };
            for p in quad {
                vertices.push(LitVertex { position: p, normal, color });
            }
            indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ods_geo::Region;

    #[test]
    fn a_founding_base_builds_a_well_formed_diorama() {
        let base = Chapterhouse::founding(Region::Europe);
        let (verts, indices) = build_base_scene(&base, 6, true, 1.0);
        assert!(!verts.is_empty());
        assert_eq!(indices.len() % 3, 0);
        let max = *indices.iter().max().unwrap() as usize;
        assert!(max < verts.len());
        // Four founding facilities plus 32 empty pads plus the ground: the
        // scene has real volume, not a stray quad.
        assert!(verts.len() > 500, "{} vertices", verts.len());
    }
}

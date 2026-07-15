//! The Geoscape globe: a lit UV-sphere with continents and biomes from an
//! embedded equirectangular class map, surface markers for rifts/nests/the
//! chapterhouse, and ray-sphere picking to select regions.
//!
//! Coordinates: Z is up (north pole +Z), latitude in degrees [-90, 90],
//! longitude in degrees [-180, 180] with 0 at the +X meridian.

use glam::Vec3;
use ods_geo::{Campaign, Region};
use ods_render::LitVertex;

pub const GLOBE_RADIUS: f32 = 200.0;

// Fine enough that the half-degree earth plates resolve: 0.625° per quad.
const STACKS: usize = 288;
const SLICES: usize = 576;

// The 1994 palette: a deep saturated ocean, land colored by biome — the
// dedicated globe shader keeps them flat, so the colors carry the look.
const OCEAN: [f32; 3] = [0.03, 0.10, 0.32];
const ICE: [f32; 3] = [0.84, 0.87, 0.92];
const HIGHLIGHT: [f32; 3] = [0.45, 0.38, 0.12];

/// The cartographer's plates: a 720x360 equirectangular class map, half a
/// degree per texel, row 0 = north. Each byte is a biome class; 0 is ocean.
/// Traced from real continent outlines and climate zones by
/// `tools/gen_earth.py`, which regenerates this file.
static EARTH: &[u8] = include_bytes!("../assets/earth.bin");
const EARTH_W: usize = 720;
const EARTH_H: usize = 360;

const CLASS_OCEAN: u8 = 0;
const CLASS_ICE: u8 = 1;

/// Biome class at a point: 0 ocean, 1 ice, 2 tundra, 3 boreal, 4 temperate,
/// 5 steppe, 6 savanna, 7 desert, 8 rainforest.
fn earth_class(lat: f32, lon: f32) -> u8 {
    let row = (((90.0 - lat) / 180.0) * EARTH_H as f32) as isize;
    let col = (((lon + 180.0) / 360.0) * EARTH_W as f32) as isize;
    let row = row.clamp(0, EARTH_H as isize - 1) as usize;
    let col = col.rem_euclid(EARTH_W as isize) as usize;
    EARTH[row * EARTH_W + col]
}

fn is_land(lat: f32, lon: f32) -> bool {
    earth_class(lat, lon) != CLASS_OCEAN
}

/// Each biome's base ink, chosen for the flat globe shader: sand yellow for
/// desert, straw for steppe, sun-cured gold for savanna, honest green for
/// temperate country, near-black green for spruce and jungle, washed moss
/// for tundra, cap white for the ice.
fn class_color(class: u8) -> [f32; 3] {
    match class {
        CLASS_ICE => ICE,
        2 => [0.42, 0.44, 0.32], // tundra
        3 => [0.08, 0.27, 0.11], // boreal
        4 => [0.16, 0.44, 0.13], // temperate
        5 => [0.31, 0.36, 0.11], // steppe
        6 => [0.42, 0.40, 0.12], // savanna
        7 => [0.74, 0.62, 0.29], // desert
        8 => [0.05, 0.23, 0.07], // rainforest
        _ => OCEAN,
    }
}

// The great mountain chains, as ridgelines (lat, lon) the relief follows —
// so the ranges rise where they really are, not as scattered confetti peaks.
const RANGES: [&[(f32, f32)]; 15] = [
    &[(35.0, 71.0), (34.0, 76.0), (30.0, 82.0), (28.0, 88.0), (27.0, 92.0), (25.0, 96.0)], // Himalaya
    &[(10.0, -72.0), (0.0, -78.0), (-15.0, -72.0), (-30.0, -70.0), (-45.0, -73.0), (-52.0, -72.0)], // Andes
    &[(63.0, -142.0), (54.0, -125.0), (46.0, -116.0), (39.0, -106.0), (33.0, -108.0)], // Rockies
    &[(44.0, 7.0), (46.0, 10.0), (47.0, 13.0)],                                          // Alps
    &[(68.0, 66.0), (60.0, 59.0), (54.0, 59.0), (50.0, 58.0)],                           // Urals
    &[(31.0, -8.0), (33.0, -2.0), (35.0, 4.0), (36.0, 9.0)],                             // Atlas
    &[(43.0, 42.0), (42.0, 45.0), (41.0, 47.0)],                                          // Caucasus
    &[(47.0, -70.0), (41.0, -78.0), (35.0, -83.0)],                                       // Appalachians
    &[(-18.0, 146.0), (-28.0, 152.0), (-36.0, 148.0)],                                    // Gt Dividing
    &[(69.0, 20.0), (63.0, 12.0), (60.0, 8.0)],                                            // Scandes
    &[(14.0, 38.0), (9.0, 39.0), (6.0, 38.0)],                                             // Ethiopian
    &[(-28.0, 29.0), (-31.0, 28.0)],                                                       // Drakensberg
    &[(37.0, 45.0), (33.0, 50.0), (29.0, 54.0)],                                           // Zagros
    &[(43.0, 75.0), (42.0, 80.0), (41.0, 85.0)],                                           // Tien Shan
    &[(46.0, 100.0), (48.0, 92.0), (50.0, 88.0)],                                          // Altai
];

// The great rivers, as courses (lat, lon) inked across the land.
const RIVERS: [&[(f32, f32)]; 10] = [
    &[(31.0, 30.0), (27.0, 31.0), (22.0, 32.0), (16.0, 33.0), (10.0, 31.0), (3.0, 32.0)], // Nile
    &[(-1.0, -49.0), (-3.0, -58.0), (-4.0, -66.0), (-5.0, -73.0), (-11.0, -74.0)],        // Amazon
    &[(29.0, -89.0), (35.0, -90.0), (42.0, -91.0), (47.0, -95.0)],                        // Mississippi
    &[(46.0, 48.0), (51.0, 46.0), (55.0, 48.0), (57.0, 44.0)],                            // Volga
    &[(-6.0, 12.0), (-2.0, 18.0), (1.0, 24.0), (2.0, 25.0)],                              // Congo
    &[(31.0, 121.0), (30.0, 112.0), (29.0, 104.0), (33.0, 98.0)],                         // Yangtze
    &[(23.0, 90.0), (25.0, 85.0), (27.0, 80.0), (30.0, 78.0)],                            // Ganges
    &[(66.0, 69.0), (60.0, 73.0), (55.0, 76.0)],                                          // Ob
    &[(45.0, 29.0), (45.0, 20.0), (48.0, 17.0), (48.0, 12.0)],                            // Danube
    &[(-34.0, -58.0), (-27.0, -55.0), (-20.0, -50.0)],                                    // Paraná
];

/// Planar (equirectangular, longitude scaled by latitude) distance in degrees
/// from a point to a lat/lon segment — cheap and good enough for continental
/// features away from the poles. `cl` is cos(lat), passed in so a whole set
/// of segments at one latitude share the single trig call.
fn seg_dist_deg(lat: f32, lon: f32, a: (f32, f32), b: (f32, f32), cl: f32) -> f32 {
    let (px, py) = ((lon - a.1) * cl, lat - a.0);
    let (bx, by) = ((b.1 - a.1) * cl, b.0 - a.0);
    let t = ((px * bx + py * by) / (bx * bx + by * by + 1e-6)).clamp(0.0, 1.0);
    let (dx, dy) = (px - bx * t, py - by * t);
    (dx * dx + dy * dy).sqrt()
}

fn dist_to_lines_deg(lat: f32, lon: f32, lines: &[&[(f32, f32)]]) -> f32 {
    let cl = lat.to_radians().cos().max(0.15);
    let mut best = f32::INFINITY;
    for line in lines {
        for w in line.windows(2) {
            best = best.min(seg_dist_deg(lat, lon, w[0], w[1], cl));
        }
    }
    best
}

/// Extra relief from the nearest mountain chain: a ridged crest that falls
/// off over a few degrees to either side of the ridgeline.
fn mountain_rise(lat: f32, lon: f32) -> f32 {
    let d = dist_to_lines_deg(lat, lon, &RANGES);
    let w = 4.5;
    if d >= w {
        return 0.0;
    }
    let t = 1.0 - d / w;
    t * t * 10.0
}

/// Distance-to-coast field over the earth grid, in degrees, built once by a
/// weighted two-pass chamfer transform (land seeds at 0, longitude wraps).
/// Horizontal steps carry each row's true angular width, so the distance is
/// angular, not raw cells — good enough for the shelf/deep-water bands.
fn coast_field() -> &'static [f32] {
    static FIELD: std::sync::OnceLock<Vec<f32>> = std::sync::OnceLock::new();
    FIELD.get_or_init(|| {
        let (w, h) = (EARTH_W, EARTH_H);
        let vstep = 180.0 / h as f32;
        let hstep: Vec<f32> = (0..h)
            .map(|r| {
                let lat = 90.0 - (r as f32 + 0.5) * 180.0 / h as f32;
                (360.0 / w as f32) * lat.to_radians().cos().max(0.15)
            })
            .collect();
        let mut d = vec![1.0e9f32; w * h];
        for i in 0..w * h {
            if EARTH[i] != CLASS_OCEAN {
                d[i] = 0.0;
            }
        }
        // Forward: relax from the west and north (already-settled) neighbours.
        for r in 0..h {
            let hs = hstep[r];
            let diag = (hs * hs + vstep * vstep).sqrt();
            for c in 0..w {
                let mut best = d[r * w + c];
                best = best.min(d[r * w + (c + w - 1) % w] + hs);
                if r > 0 {
                    let up = (r - 1) * w;
                    best = best.min(d[up + c] + vstep);
                    best = best.min(d[up + (c + w - 1) % w] + diag);
                    best = best.min(d[up + (c + 1) % w] + diag);
                }
                d[r * w + c] = best;
            }
        }
        // Backward: relax from the east and south.
        for r in (0..h).rev() {
            let hs = hstep[r];
            let diag = (hs * hs + vstep * vstep).sqrt();
            for c in (0..w).rev() {
                let mut best = d[r * w + c];
                best = best.min(d[r * w + (c + 1) % w] + hs);
                if r + 1 < h {
                    let dn = (r + 1) * w;
                    best = best.min(d[dn + c] + vstep);
                    best = best.min(d[dn + (c + w - 1) % w] + diag);
                    best = best.min(d[dn + (c + 1) % w] + diag);
                }
                d[r * w + c] = best;
            }
        }
        d
    })
}

/// Distance (in degrees) from a point to the nearest coast — an O(1) sample
/// of the cached field above, capped at the far band.
fn coast_dist_deg(lat: f32, lon: f32) -> f32 {
    let row = (((90.0 - lat) / 180.0) * EARTH_H as f32) as isize;
    let col = (((lon + 180.0) / 360.0) * EARTH_W as f32) as isize;
    let row = row.clamp(0, EARTH_H as isize - 1) as usize;
    let col = col.rem_euclid(EARTH_W as isize) as usize;
    coast_field()[row * EARTH_W + col].min(18.0)
}

/// Rough box-partition of the world into our eight council regions.
pub fn region_at(lat: f32, lon: f32) -> Region {
    if !(-60.0..=66.0).contains(&lat) {
        return Region::Arctic; // both polar wastes report to the same desk
    }
    if lat > 12.0 && (26.0..63.0).contains(&lon) && lat < 42.0 {
        return Region::MiddleEast;
    }
    if lon < -30.0 {
        return if lat > 13.0 { Region::NorthAmerica } else { Region::SouthAmerica };
    }
    if lon < 40.0 {
        return if lat > 36.0 { Region::Europe } else { Region::Africa };
    }
    if lat < 5.0 && lon > 95.0 {
        return Region::Oceania;
    }
    Region::Asia
}

/// Marker anchor per region (lat, lon).
/// Where a region's name hangs on the map, and the surface normal there
/// (for fading names over the horizon).
pub fn region_label_pos(region: Region) -> (Vec3, Vec3) {
    let (lat, lon) = centroid(region);
    let p = latlon_to_pos(lat, lon, GLOBE_RADIUS + 12.0);
    let n = p.normalize();
    (p, n)
}

fn centroid(region: Region) -> (f32, f32) {
    match region {
        Region::NorthAmerica => (45.0, -100.0),
        Region::SouthAmerica => (-15.0, -60.0),
        Region::Europe => (50.0, 15.0),
        Region::Africa => (5.0, 20.0),
        Region::MiddleEast => (28.0, 45.0),
        Region::Asia => (45.0, 90.0),
        Region::Oceania => (-25.0, 135.0),
        Region::Arctic => (75.0, -40.0),
    }
}

pub fn latlon_to_pos(lat: f32, lon: f32, radius: f32) -> Vec3 {
    let (lat, lon) = (lat.to_radians(), lon.to_radians());
    Vec3::new(
        radius * lat.cos() * lon.cos(),
        radius * lat.cos() * lon.sin(),
        radius * lat.sin(),
    )
}

/// Terrain relief above the sphere: land rises on the same noise that
/// mottles it, and the ice caps sit proud of the sea.
fn surface_rise(lat: f32, lon: f32) -> f32 {
    surface_rise_with(lat, lon, mountain_rise(lat, lon))
}

/// The relief with the mountain term supplied — so the hot per-vertex path
/// can compute the (expensive) `mountain_rise` once and share it with the
/// rock/snow coloring instead of paying for it twice.
fn surface_rise_with(lat: f32, lon: f32, mr: f32) -> f32 {
    let relief_h = {
        let mut x = (lat * 53.7) as i32 as u32;
        x = x
            .wrapping_mul(2654435761)
            .wrapping_add((lon * 39.1) as i32 as u32)
            .wrapping_mul(1274126177);
        x ^= x >> 15;
        ((x.wrapping_mul(2246822519) >> 9) & 255) as f32 / 255.0
    };
    let mut rise = 0.0;
    let class = earth_class(lat, lon);
    if class != CLASS_OCEAN {
        rise += relief_h * relief_h * 4.5;
        rise += mr; // the great chains stand proud
        if class == CLASS_ICE {
            rise += 2.0; // the sheets sit proud of the sea
        }
    } else if lat > 72.0 {
        // Arctic pack ice creeps over the polar sea.
        rise += ((lat - 72.0) / 8.0).min(1.0) * 1.8;
    }
    rise
}

/// Build the globe sphere, optionally tinting one region's land.
pub fn build_globe(selected: Option<Region>) -> (Vec<LitVertex>, Vec<u32>) {
    let mut vertices = Vec::with_capacity((STACKS + 1) * (SLICES + 1));
    let mut indices = Vec::new();

    for i in 0..=STACKS {
        let lat = 90.0 - 180.0 * i as f32 / STACKS as f32;
        for j in 0..=SLICES {
            let lon = -180.0 + 360.0 * j as f32 / SLICES as f32;
            let class = earth_class(lat, lon);
            let land = class != CLASS_OCEAN;
            // One mountain lookup per land vertex, shared by relief and
            // coloring; the sea never needs it.
            let mr = if land { mountain_rise(lat, lon) } else { 0.0 };
            let pos = latlon_to_pos(lat, lon, GLOBE_RADIUS + surface_rise_with(lat, lon, mr));
            let normal = pos.normalize();

            let mut color = if land { class_color(class) } else { OCEAN };
            // The sea, read as bathymetry: a dark ink line right at the
            // shore, a pale shelf just off it, honest ocean beyond, and the
            // deep abyssal blue where no coast is near.
            if !land {
                let cd = coast_dist_deg(lat, lon);
                const ABYSS: [f32; 3] = [0.015, 0.05, 0.20];
                const SHELF: [f32; 3] = [OCEAN[0] + 0.06, OCEAN[1] + 0.11, OCEAN[2] + 0.10];
                const CONTOUR: [f32; 3] = [0.02, 0.06, 0.15];
                color = if cd <= 0.8 {
                    CONTOUR
                } else if cd <= 2.0 {
                    SHELF
                } else if cd <= 4.0 {
                    OCEAN
                } else {
                    let t = ((cd - 4.0) / 14.0).clamp(0.0, 1.0);
                    [
                        OCEAN[0] + (ABYSS[0] - OCEAN[0]) * t,
                        OCEAN[1] + (ABYSS[1] - OCEAN[1]) * t,
                        OCEAN[2] + (ABYSS[2] - OCEAN[2]) * t,
                    ]
                };
            }
            if land && class != CLASS_ICE {
                // Mottle within the biome like the original's hand-placed
                // terrain pixels — texture, never a different climate.
                let h = {
                    let mut x = (lat * 53.7) as i32 as u32;
                    x = x
                        .wrapping_mul(2654435761)
                        .wrapping_add((lon * 39.1) as i32 as u32)
                        .wrapping_mul(1274126177);
                    x ^= x >> 15;
                    ((x.wrapping_mul(2246822519) >> 9) & 255) as f32 / 255.0
                };
                let m = 0.88 + 0.26 * h;
                for c in color.iter_mut() {
                    *c = (*c * m).min(1.0);
                }
                // The great chains carry their own rock, and snow only on
                // the highest crests — high near the poles, near-bare at the
                // equator, so tropical ranges show stone, not toothpaste.
                if mr > 2.5 {
                    let rock = [0.44, 0.42, 0.40];
                    let t = ((mr - 2.5) / 4.5).clamp(0.0, 1.0);
                    for (c, r) in color.iter_mut().zip(rock) {
                        *c = *c + (r - *c) * t;
                    }
                    let snowline = 9.6 - (lat.abs() / 90.0) * 4.6;
                    if mr > snowline {
                        let s = ((mr - snowline) / 1.6).clamp(0.0, 1.0);
                        for c in color.iter_mut() {
                            *c = *c + (0.78 - *c) * s;
                        }
                    }
                }
            }
            // Arctic pack ice whitens the polar sea.
            if !land && lat > 72.0 {
                let ice = ((lat - 72.0) / 6.0).clamp(0.0, 1.0);
                for (c, i) in color.iter_mut().zip(ICE) {
                    *c = *c + (i - *c) * ice;
                }
            }
            if land
                && let Some(sel) = selected
                && region_at(lat, lon) == sel
            {
                for (c, h) in color.iter_mut().zip(HIGHLIGHT) {
                    *c = (*c + h).min(1.0);
                }
            }

            vertices.push(LitVertex {
                position: pos.to_array(),
                normal: normal.to_array(),
                color: [color[0], color[1], color[2], 1.0],
            });
        }
    }

    // The mapmaker's ink: territory borders sampled where the answer to
    // "whose land is this" changes, laid as small ink marks hugging the
    // terrain — the 1994 political map, hand-ruled.
    {
        let ink = [0.72, 0.60, 0.34, 1.0f32];
        let step = 1.0f32;
        let mut lat = -78.0f32;
        while lat < 78.0 {
            let mut lon = -180.0f32;
            while lon < 180.0 {
                let here = region_at(lat, lon);
                for (dlat, dlon) in [(0.0f32, step), (step, 0.0f32)] {
                    if region_at(lat + dlat, lon + dlon) != here {
                        let (mlat, mlon) = (lat + dlat / 2.0, lon + dlon / 2.0);
                        let center = latlon_to_pos(
                            mlat,
                            mlon,
                            GLOBE_RADIUS + surface_rise(mlat, mlon) + 0.9,
                        );
                        let normal = center.normalize();
                        let east = Vec3::Z.cross(normal).normalize_or(Vec3::X);
                        let north = normal.cross(east);
                        let first = vertices.len() as u32;
                        for (du, dv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                            let p = center + (east * du + north * dv) * 0.85;
                            vertices.push(LitVertex {
                                position: p.to_array(),
                                normal: normal.to_array(),
                                color: ink,
                            });
                        }
                        indices.extend([0u32, 1, 2, 0, 2, 3].map(|k| first + k));
                    }
                }
                lon += step;
            }
            lat += step;
        }
    }

    // The great rivers, inked as dark-water threads that hug the land they
    // cross — sampled densely along each authored course.
    {
        let water = [0.13, 0.26, 0.40, 1.0f32];
        for course in &RIVERS {
            for seg in course.windows(2) {
                let (a, b) = (seg[0], seg[1]);
                let span = ((b.0 - a.0).powi(2) + (b.1 - a.1).powi(2)).sqrt();
                let steps = (span / 0.6).ceil().max(1.0) as usize;
                for s in 0..=steps {
                    let t = s as f32 / steps as f32;
                    let lat = a.0 + (b.0 - a.0) * t;
                    let lon = a.1 + (b.1 - a.1) * t;
                    if !is_land(lat, lon) {
                        continue;
                    }
                    // Ride above the local relief and clear of the border
                    // ink (at +0.9) so the two thin quad layers never fight.
                    let center =
                        latlon_to_pos(lat, lon, GLOBE_RADIUS + surface_rise(lat, lon) + 1.3);
                    let normal = center.normalize();
                    let east = Vec3::Z.cross(normal).normalize_or(Vec3::X);
                    let north = normal.cross(east);
                    let first = vertices.len() as u32;
                    for (du, dv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
                        let p = center + (east * du + north * dv) * 0.7;
                        vertices.push(LitVertex {
                            position: p.to_array(),
                            normal: normal.to_array(),
                            color: water,
                        });
                    }
                    indices.extend([0u32, 1, 2, 0, 2, 3].map(|k| first + k));
                }
            }
        }
    }

    let cols = (SLICES + 1) as u32;
    for i in 0..STACKS as u32 {
        for j in 0..SLICES as u32 {
            let v00 = i * cols + j;
            let v01 = v00 + 1;
            let v10 = v00 + cols;
            let v11 = v10 + 1;
            indices.extend([v00, v10, v11, v00, v11, v01]);
        }
    }
    (vertices, indices)
}

/// Markers hovering just off the surface: detected rifts (red, brighter when
/// still unstable), nests (dark violet), and the chapterhouse (gold). `time`
/// drives the pulse of anything alive down there.
pub fn build_markers(campaign: &Campaign, time: f32) -> (Vec<LitVertex>, Vec<u32>) {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();

    // One placement helper: altitude and size separated, so keeps can
    // stack and gashes can lie flat.
    let mut push_at = |lat: f32, lon: f32, alt: f32, size: f32, color: [f32; 4]| {
        let center = latlon_to_pos(lat, lon, GLOBE_RADIUS + alt);
        push_cube(&mut vertices, &mut indices, center, size, color);
    };

    for base in &campaign.bases {
        // A keep, not a dot: bailey, tower, and a gold banner.
        let (lat, lon) = centroid(base.region);
        push_at(lat, lon, 6.5, 6.5, [0.45, 0.42, 0.40, 1.0]);
        push_at(lat, lon, 8.0, 3.8, [0.55, 0.52, 0.50, 1.0]);
        push_at(lat, lon, 13.0, 1.6, [1.0, 0.85, 0.25, 1.0]);
    }
    // Nests breathe, slow and swollen.
    let nest_pulse = 6.0 + 0.7 * (time * 1.7).sin();
    for nest in &campaign.nests {
        push_at(nest.lat, nest.lon, nest_pulse, nest_pulse, [0.45, 0.1, 0.55, 1.0]);
    }
    for rift in campaign.rifts.iter().filter(|r| r.detected) {
        // Unstable rifts flicker urgently; dug-in ones burn steady and dark.
        let (size, color) = if rift.is_stabilized() {
            (5.0 + 0.4 * (time * 2.3).sin(), [0.75, 0.12, 0.08, 1.0])
        } else {
            let throb = 0.5 + 0.5 * (time * 5.0).sin();
            (
                4.4 + 1.6 * throb,
                [0.8 + 0.2 * throb, 0.25 + 0.35 * throb, 0.12, 1.0],
            )
        };
        push_at(rift.lat, rift.lon, size, size, color);
        // The gash itself: three dark shards splayed across the ground.
        for (k, (dlat, dlon)) in [(0.0f32, 0.0f32), (2.0, 3.0), (-2.5, 2.0)].iter().enumerate() {
            push_at(
                rift.lat + dlat,
                rift.lon + dlon,
                1.0,
                2.0 + (k as f32) * 0.6,
                [0.12, 0.02, 0.03, 1.0],
            );
        }
    }

    // Sorties crawl their great-circle routes: the zeppelin as a small
    // gold mote, with the road ahead dotted out to the rift.
    for sortie in &campaign.sorties {
        let Some(rift) = campaign.rifts.iter().find(|r| r.id == sortie.rift_id) else {
            continue;
        };
        let total = sortie.days_total.max(1) as f32;
        let progress = (1.0 - sortie.days_left as f32 / total).clamp(0.0, 1.0);
        let lerp = |t: f32| -> (f32, f32) {
            // Straight lat/lon interpolation, shortest way around.
            let mut dlon = rift.lon - sortie.from.1;
            if dlon > 180.0 {
                dlon -= 360.0;
            }
            if dlon < -180.0 {
                dlon += 360.0;
            }
            (
                sortie.from.0 + (rift.lat - sortie.from.0) * t,
                sortie.from.1 + dlon * t,
            )
        };
        // The road ahead, dotted.
        let mut t = progress;
        while t < 1.0 {
            let (lat, lon) = lerp(t);
            push_at(lat, lon, 1.4, 1.4, [0.9, 0.8, 0.5, 1.0]);
            t += 0.08;
        }
        // The ship itself: envelope fore and aft, gondola slung below,
        // bobbing on the wind, trailing a fading wake.
        let (lat, lon) = lerp(progress);
        let (lat2, lon2) = lerp((progress + 0.04).min(1.0));
        let bob = 8.0 + 0.5 * (time * 3.0).sin();
        let (dlat, dlon) = (lat2 - lat, lon2 - lon);
        let n = (dlat * dlat + dlon * dlon).sqrt().max(0.001);
        let (ulat, ulon) = (dlat / n, dlon / n);
        for (f, size) in [(0.0f32, 3.2f32), (1.6, 2.6), (-1.6, 2.6)] {
            push_at(lat + ulat * f, lon + ulon * f, bob, size, [0.82, 0.78, 0.66, 1.0]);
        }
        push_at(lat, lon, bob - 3.0, 1.6, [0.55, 0.45, 0.3, 1.0]);
        for w in 1..4 {
            let f = -2.5 - w as f32 * 1.6;
            push_at(
                lat + ulat * f,
                lon + ulon * f,
                bob + 0.4,
                1.2 - w as f32 * 0.25,
                [0.75, 0.75, 0.7, 1.0],
            );
        }
    }

    // The great cities, as small pale pinpricks riding the land (above its
    // relief, so highland cities aren't buried) — the day-side counterpart
    // to the night-side lights, and the anchors the map labels hang from.
    for &(lat, lon) in &CITIES {
        push_at(lat, lon, surface_rise(lat, lon) + 1.2, 0.7, [0.86, 0.80, 0.58, 1.0]);
    }

    (vertices, indices)
}

/// The great cities of the world (lat, lon) — visible as lights after dark.
const CITIES: [(f32, f32); 24] = [
    (40.7, -74.0),   // New York
    (34.0, -118.2),  // Los Angeles
    (41.9, -87.6),   // Chicago
    (19.4, -99.1),   // Mexico City
    (-23.5, -46.6),  // São Paulo
    (-34.6, -58.4),  // Buenos Aires
    (51.5, -0.1),    // London
    (48.9, 2.3),     // Paris
    (52.5, 13.4),    // Berlin
    (41.9, 12.5),    // Rome
    (55.8, 37.6),    // Moscow
    (30.0, 31.2),    // Cairo
    (6.5, 3.4),      // Lagos
    (-26.2, 28.0),   // Johannesburg
    (25.2, 55.3),    // Dubai
    (28.6, 77.2),    // Delhi
    (19.1, 72.9),    // Mumbai
    (39.9, 116.4),   // Beijing
    (31.2, 121.5),   // Shanghai
    (35.7, 139.7),   // Tokyo
    (37.6, 127.0),   // Seoul
    (1.4, 103.8),    // Singapore
    (-33.9, 151.2),  // Sydney
    (14.6, 121.0),   // Manila
];

/// Names for the cities above, in the same order — inked on the map when
/// the camera comes in close, and used to name where a rift really struck.
pub const CITY_NAMES: [&str; 24] = [
    "New York",
    "Los Angeles",
    "Chicago",
    "Mexico City",
    "São Paulo",
    "Buenos Aires",
    "London",
    "Paris",
    "Berlin",
    "Rome",
    "Moscow",
    "Cairo",
    "Lagos",
    "Johannesburg",
    "Dubai",
    "Delhi",
    "Mumbai",
    "Beijing",
    "Shanghai",
    "Tokyo",
    "Seoul",
    "Singapore",
    "Sydney",
    "Manila",
];

/// Each city's screen anchor: its surface position and outward normal, for
/// hanging a label (only drawn when it faces the camera and the map is
/// zoomed in close enough to read).
pub fn city_anchors() -> impl Iterator<Item = (&'static str, Vec3, Vec3)> {
    CITIES.iter().zip(CITY_NAMES.iter()).map(|(&(lat, lon), &name)| {
        let p = latlon_to_pos(lat, lon, GLOBE_RADIUS + 2.0);
        (name, p, p.normalize())
    })
}

/// The nearest great city to a point, for naming a rift or terror strike by
/// the place it fell on rather than the whole continent.
pub fn nearest_city(lat: f32, lon: f32) -> &'static str {
    let cl = lat.to_radians().cos().max(0.15);
    let mut best = ("", f32::INFINITY);
    for (&(clat, clon), &name) in CITIES.iter().zip(CITY_NAMES.iter()) {
        // Wrap the longitude gap the short way so the antimeridian doesn't
        // pair a point at +179° with a city at -179° across the whole globe.
        let dlon = (((lon - clon) + 540.0) % 360.0) - 180.0;
        let d = ((lat - clat).powi(2) + (dlon * cl).powi(2)).sqrt();
        if d < best.1 {
            best = (name, d);
        }
    }
    best.0
}

/// City lights on the night side of the terminator, for the fx overlay slot.
/// Each is a small warm quad flush with the surface; they twinkle faintly.
pub fn build_city_lights(
    campaign: &Campaign,
    sun_lon: f32,
    time: f32,
) -> (Vec<ods_render::OverlayVertex>, Vec<u32>) {
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    for (i, &(lat, lon)) in CITIES.iter().enumerate() {
        let region = region_at(lat, lon);
        // Panic puts the lights out street by street; an infiltrated
        // patron's cities burn the wrong color entirely.
        let panic = campaign.region_panic.get(&region).copied().unwrap_or(0) as f32;
        let corrupted = campaign.corrupted_patrons.contains(&region);
        let dimmed = (1.0 - (panic / 100.0).clamp(0.0, 1.0) * 0.85).max(0.1);
        let mut d = (lon - sun_lon).abs() % 360.0;
        if d > 180.0 {
            d = 360.0 - d;
        }
        if d < 90.0 {
            continue; // daylight: the lights drown in the sun
        }
        // Brighter the deeper into night, with a slow per-city shimmer.
        let depth = ((d - 90.0) / 90.0).clamp(0.0, 1.0);
        let shimmer = 0.85 + 0.15 * (time * 2.0 + i as f32 * 1.7).sin();
        let alpha = (0.35 + 0.55 * depth) * shimmer * dimmed;

        let center = latlon_to_pos(lat, lon, GLOBE_RADIUS + 1.5);
        let normal = center.normalize();
        // Tangent frame on the sphere surface.
        let east = Vec3::Z.cross(normal).normalize_or(Vec3::X);
        let north = normal.cross(east);
        let r = 2.4;
        let corners = [
            center - east * r - north * r,
            center + east * r - north * r,
            center + east * r + north * r,
            center - east * r + north * r,
        ];
        let first = verts.len() as u32;
        for p in corners {
            verts.push(ods_render::OverlayVertex {
                position: p.to_array(),
                color: if corrupted {
                    [0.75, 0.3, 0.95, alpha]
                } else {
                    [1.0, 0.85, 0.5, alpha]
                },
            });
        }
        indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
    }
    (verts, indices)
}

/// The wounds and the fear, in the fx overlay: violet plumes rising off
/// detected rifts (taller and faster the closer the eruption), and ember
/// specks drifting up from regions deep in panic.
pub fn build_geo_omens(
    campaign: &Campaign,
    time: f32,
) -> (Vec<ods_render::OverlayVertex>, Vec<u32>) {
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    let mut quad = |center: Vec3, r: f32, color: [f32; 4]| {
        let normal = center.normalize_or(Vec3::Z);
        let east = Vec3::Z.cross(normal).normalize_or(Vec3::X);
        let north = normal.cross(east);
        let first = verts.len() as u32;
        for (du, dv) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            verts.push(ods_render::OverlayVertex {
                position: (center + east * (r * du) + north * (r * dv)).to_array(),
                color,
            });
        }
        indices.extend([0u32, 1, 2, 0, 2, 3].map(|k| first + k));
    };
    for rift in campaign.rifts.iter().filter(|r| r.detected) {
        // The plume: urgency is height and heat.
        let urgency = (1.0 - rift.days_left as f32 / 10.0).clamp(0.2, 1.0);
        let segs = 4 + (urgency * 4.0) as i32;
        for k in 0..segs {
            let f = k as f32 / segs as f32;
            let wobble = ((time * (1.5 + urgency) + k as f32 * 1.3).sin()) * 1.5;
            let center = latlon_to_pos(
                rift.lat + wobble * 0.2,
                rift.lon + wobble * 0.3,
                GLOBE_RADIUS + 4.0 + f * (10.0 + 14.0 * urgency),
            );
            quad(
                center,
                2.6 * (1.0 - f * 0.6),
                [0.65, 0.2, 0.9, (0.5 - f * 0.4) * (0.6 + 0.4 * urgency)],
            );
        }
    }
    // Weather: a broken shell of drifting cloud banks, each a cluster of
    // soft quads riding its own latitude at its own pace.
    for k in 0..26u32 {
        let h = k.wrapping_mul(2654435761) >> 8;
        let lat = ((h % 120) as f32) - 60.0;
        let drift = 2.0 + (h % 5) as f32 * 0.8;
        let lon = ((h % 360) as f32 + time * drift) % 360.0 - 180.0;
        for (j, (dlat, dlon, r)) in
            [(0.0f32, 0.0f32, 7.0f32), (2.5, 4.0, 5.0), (-2.0, 5.5, 4.2)].iter().enumerate()
        {
            let center = latlon_to_pos(lat + dlat, lon + dlon, GLOBE_RADIUS + 14.0);
            quad(
                center,
                *r,
                [0.9, 0.92, 0.95, 0.07 + 0.02 * ((k + j as u32) % 3) as f32],
            );
        }
    }
    // The sky-fight: gargoyles wheel around the held zeppelin.
    if let Some(i) = &campaign.interception {
        let (lat, lon) = centroid(i.region);
        for g in 0..i.gargoyles.min(5) {
            let a = time * 2.2 + g as f32 * 2.1;
            let r = 5.0 + i.range as f32 * 0.6;
            let center = latlon_to_pos(
                lat + a.sin() * r * 0.4,
                lon + a.cos() * r * 0.6,
                GLOBE_RADIUS + 9.0 + (time * 3.0 + g as f32).sin() * 1.5,
            );
            quad(center, 1.6, [0.5, 0.15, 0.6, 0.85]);
        }
    }
    for (&region, &panic) in &campaign.region_panic {
        if panic < 40 {
            continue;
        }
        let (lat, lon) = centroid(region);
        let n = ((panic - 40) / 15).clamp(1, 5);
        for k in 0..n {
            let ph = (time * 0.7 + k as f32 * 1.61).fract();
            let center = latlon_to_pos(
                lat + ((k * 17) % 7) as f32 - 3.0,
                lon + ((k * 29) % 11) as f32 - 5.0,
                GLOBE_RADIUS + 2.0 + ph * 9.0,
            );
            quad(center, 1.1, [1.0, 0.45, 0.1, 0.5 * (1.0 - ph)]);
        }
    }
    (verts, indices)
}

fn push_cube(
    vertices: &mut Vec<LitVertex>,
    indices: &mut Vec<u32>,
    center: Vec3,
    half: f32,
    color: [f32; 4],
) {
    for d in 0..3usize {
        let u = (d + 1) % 3;
        let v = (d + 2) % 3;
        for front in [true, false] {
            let mut normal = [0.0f32; 3];
            normal[d] = if front { 1.0 } else { -1.0 };
            let corner = |cu: f32, cv: f32| -> [f32; 3] {
                let mut p = [0.0f32; 3];
                p[d] = if front { half } else { -half };
                p[u] = cu;
                p[v] = cv;
                [center.x + p[0], center.y + p[1], center.z + p[2]]
            };
            let (p00, p10, p11, p01) = (
                corner(-half, -half),
                corner(half, -half),
                corner(half, half),
                corner(-half, half),
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

/// Ray-sphere pick: which region did the cursor hit, if any?
pub fn pick_region(origin: Vec3, dir: Vec3) -> Option<Region> {
    let b = origin.dot(dir);
    let c = origin.dot(origin) - GLOBE_RADIUS * GLOBE_RADIUS;
    let disc = b * b - c;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    if t <= 0.0 {
        return None;
    }
    let p = origin + dir * t;
    let lat = (p.z / GLOBE_RADIUS).clamp(-1.0, 1.0).asin().to_degrees();
    let lon = p.y.atan2(p.x).to_degrees();
    Some(region_at(lat, lon))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn earth_plates_are_full_size() {
        assert_eq!(EARTH.len(), EARTH_W * EARTH_H);
        assert!(EARTH.iter().all(|&c| c <= 8), "unknown biome class");
        // The plates actually hold both land and sea.
        assert!(EARTH.contains(&CLASS_OCEAN));
        assert!(EARTH.iter().any(|&c| c != CLASS_OCEAN));
    }

    #[test]
    fn known_places_resolve() {
        assert!(is_land(48.0, 2.0), "Paris is on land");
        assert!(is_land(-15.0, -55.0), "Brazil is on land");
        assert!(is_land(51.5, -0.1), "London is on land");
        // Interior Europe and European Russia are filled, not sea.
        assert!(is_land(52.5, 13.4), "Berlin is on land");
        assert!(is_land(55.8, 37.6), "Moscow is on land");
        assert!(is_land(48.0, 16.0), "Vienna is on land");
        // Other inland cities the lake carve must not have swallowed.
        assert!(is_land(28.6, 77.2), "Delhi is on land");
        assert!(is_land(39.9, 116.4), "Beijing is on land");
        assert!(is_land(41.9, -87.6), "Chicago is on land");
        assert!(!is_land(0.0, -140.0), "the mid-Pacific is ocean");
        assert!(!is_land(0.0, -30.0), "the mid-Atlantic is ocean");
        assert!(!is_land(43.0, 34.0), "the Black Sea is water");
        assert_eq!(earth_class(23.0, 10.0), 7, "the Sahara is desert");
        assert_eq!(earth_class(-5.0, -60.0), 8, "the Amazon is rainforest");
        assert_eq!(earth_class(60.0, 100.0), 3, "Siberia is boreal");
        assert_eq!(earth_class(75.0, -40.0), CLASS_ICE, "Greenland is ice");
        assert_eq!(earth_class(-25.0, 133.0), 7, "the Outback is desert");
        // The great inland waters were carved back to sea.
        assert!(!is_land(43.0, 50.0), "the Caspian is water");
        assert!(!is_land(44.0, -84.0), "the Great Lakes are water");
        assert_eq!(region_at(48.0, 2.0), Region::Europe);
        assert_eq!(region_at(40.0, -100.0), Region::NorthAmerica);
        assert_eq!(region_at(-25.0, 135.0), Region::Oceania);
        assert_eq!(region_at(28.0, 45.0), Region::MiddleEast);
        assert_eq!(region_at(80.0, 0.0), Region::Arctic);
    }

    #[test]
    fn globe_mesh_is_well_formed() {
        let (vertices, indices) = build_globe(None);
        // The sphere grid, plus the mapmaker's border ink on top.
        let grid = (STACKS + 1) * (SLICES + 1);
        assert!(vertices.len() >= grid, "{} < {grid}", vertices.len());
        assert!(indices.len() >= STACKS * SLICES * 6);
        assert_eq!(indices.len() % 3, 0);
        let max = *indices.iter().max().unwrap() as usize;
        assert!(max < vertices.len());
        // Normals point outward.
        for v in vertices.iter().step_by(997) {
            let p = Vec3::from(v.position);
            let n = Vec3::from(v.normal);
            assert!(p.normalize().dot(n) > 0.99);
        }
    }

    #[test]
    fn mountains_rise_on_their_ranges() {
        // The Himalaya crest stands well above the sea; the open steppe
        // north of it does not; and the rise falls off monotonically as you
        // step off the ridgeline.
        assert!(mountain_rise(30.0, 84.0) > 4.0, "the Himalaya rises");
        assert_eq!(mountain_rise(55.0, 84.0), 0.0, "the open steppe is flat");
        assert!(
            mountain_rise(30.0, 84.0) > mountain_rise(31.8, 84.0),
            "the crest is higher than its flank",
        );
    }

    #[test]
    fn cities_name_and_anchor() {
        // Names line up with the marked cities, and the nearest resolves.
        assert_eq!(CITIES.len(), CITY_NAMES.len());
        assert_eq!(nearest_city(48.9, 2.4), "Paris");
        assert_eq!(nearest_city(35.6, 139.8), "Tokyo");
        // A point just west of the dateline: Sydney (151°E) is nearest only
        // if the longitude gap wraps — without the wrap it mis-picks a
        // western-hemisphere city, so this discriminates the bug.
        assert_eq!(nearest_city(-35.0, -179.0), "Sydney");
        // Anchors carry real content: a name, a point out past the surface,
        // and a unit normal pointing up from it.
        let anchors: Vec<_> = city_anchors().collect();
        assert_eq!(anchors.len(), CITIES.len());
        for (name, pos, normal) in anchors {
            assert!(!name.is_empty());
            assert!(pos.length() > GLOBE_RADIUS);
            assert!((normal.length() - 1.0).abs() < 1e-3);
        }
    }

    #[test]
    fn rivers_actually_touch_land() {
        // Every authored river must cross land the renderer will draw on —
        // a course entirely over sea (as the Volga and Danube once were,
        // before Europe was filled) renders nothing.
        for (i, course) in RIVERS.iter().enumerate() {
            let on_land = course.iter().filter(|&&(la, lo)| is_land(la, lo)).count();
            assert!(on_land >= 2, "river {i} barely touches land ({on_land} pts)");
        }
    }

    #[test]
    fn coast_shades_from_shelf_to_deep() {
        // A point hard against a coast finds land immediately; the deep
        // mid-ocean does not, out to the far ring.
        assert!(coast_dist_deg(30.0, 34.0) <= 4.0, "the Red Sea hugs a coast");
        assert!(coast_dist_deg(0.0, -140.0) > 12.0, "the mid-Pacific is deep");
    }

    #[test]
    fn picking_hits_the_facing_hemisphere() {
        // Camera out on +X looking at the origin: the pick lands near lon 0.
        let origin = Vec3::new(600.0, 0.0, 0.0);
        let region = pick_region(origin, Vec3::new(-1.0, 0.0, 0.0)).expect("hit");
        assert_eq!(region, region_at(0.0, 0.0));
        // A ray that misses the sphere entirely.
        assert_eq!(pick_region(origin, Vec3::new(0.0, 1.0, 0.0)), None);
    }
}

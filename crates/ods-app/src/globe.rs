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
            let pos = latlon_to_pos(lat, lon, GLOBE_RADIUS + surface_rise(lat, lon));
            let normal = pos.normalize();

            let class = earth_class(lat, lon);
            let land = class != CLASS_OCEAN;
            let mut color = if land { class_color(class) } else { OCEAN };
            // Shallows: ocean within reach of a coast shelves paler.
            if !land
                && (is_land(lat + 1.0, lon)
                    || is_land(lat - 1.0, lon)
                    || is_land(lat, lon + 1.0)
                    || is_land(lat, lon - 1.0))
            {
                color = [OCEAN[0] + 0.05, OCEAN[1] + 0.09, OCEAN[2] + 0.10];
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
                if h > 0.975 {
                    // The high ranges: rock, rarely snow — ranges, not
                    // dandruff.
                    color = [0.46, 0.44, 0.42];
                }
                if h > 0.995 {
                    color = [0.72, 0.74, 0.78];
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
        assert!(!is_land(0.0, -140.0), "the mid-Pacific is ocean");
        assert!(!is_land(0.0, -30.0), "the mid-Atlantic is ocean");
        assert_eq!(earth_class(23.0, 10.0), 7, "the Sahara is desert");
        assert_eq!(earth_class(-5.0, -60.0), 8, "the Amazon is rainforest");
        assert_eq!(earth_class(60.0, 100.0), 3, "Siberia is boreal");
        assert_eq!(earth_class(75.0, -40.0), CLASS_ICE, "Greenland is ice");
        assert_eq!(earth_class(-25.0, 133.0), 7, "the Outback is desert");
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
    fn picking_hits_the_facing_hemisphere() {
        // Camera out on +X looking at the origin: the pick lands near lon 0.
        let origin = Vec3::new(600.0, 0.0, 0.0);
        let region = pick_region(origin, Vec3::new(-1.0, 0.0, 0.0)).expect("hit");
        assert_eq!(region, region_at(0.0, 0.0));
        // A ray that misses the sphere entirely.
        assert_eq!(pick_region(origin, Vec3::new(0.0, 1.0, 0.0)), None);
    }
}

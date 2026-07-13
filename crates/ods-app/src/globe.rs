//! The Geoscape globe: a lit UV-sphere with continents from an embedded
//! equirectangular landmask, surface markers for rifts/nests/the
//! chapterhouse, and ray-sphere picking to select regions.
//!
//! Coordinates: Z is up (north pole +Z), latitude in degrees [-90, 90],
//! longitude in degrees [-180, 180] with 0 at the +X meridian.

use glam::Vec3;
use ods_geo::{Campaign, Region};
use ods_render::LitVertex;

pub const GLOBE_RADIUS: f32 = 200.0;

const STACKS: usize = 192;
const SLICES: usize = 384;

// The 1994 palette: a deep saturated ocean and honest green land — the
// dedicated globe shader keeps them flat, so the colors carry the look.
const OCEAN: [f32; 3] = [0.03, 0.10, 0.32];
const LAND: [f32; 3] = [0.16, 0.44, 0.13];
const ICE: [f32; 3] = [0.80, 0.84, 0.90];
const HIGHLIGHT: [f32; 3] = [0.45, 0.38, 0.12];

/// 64x32 equirectangular landmask, row 0 = north. '#' is land.
const LANDMASK: [&str; 32] = [
    "................................................................",
    "............########.#######.......#.........######.............",
    "..........##################.......#....#######################.",
    ".....################.######.......#############################",
    "...###################......##....##############################",
    "..####################............##############################",
    ".........##############......###################################",
    "..........#############.......#############################..#..",
    "..........############.......###############################....",
    "..........###########........###....#######################.....",
    "...........#########........############################........",
    "...........#####...........################.############........",
    "............######.........###############..####..#####.........",
    ".............####.........#############.....###...#######.......",
    "..............####..........############......#..#####.##.......",
    "...............#######......###########..........########.####..",
    "..............##########.....#########............#############.",
    "..............###########....########..............#####..####..",
    "..............###########....########...............#######.....",
    "...............#########.....#########.............#########....",
    "................#######.......#####.#..............#########....",
    "................######........####..................#######.....",
    "................#####..........#........................##......",
    "................####....................................##...##.",
    "................###.........................................##..",
    "................###.............................................",
    "................................................................",
    "...................###..........................................",
    "......#########...#############...#############...###########...",
    "....#########################################################...",
    "..#############################################################.",
    "################################################################",
];

fn is_land(lat: f32, lon: f32) -> bool {
    let row = (((90.0 - lat) / 180.0) * LANDMASK.len() as f32) as usize;
    let col = (((lon + 180.0) / 360.0) * 64.0) as usize;
    let row = row.min(LANDMASK.len() - 1);
    let col = col.min(63);
    LANDMASK[row].as_bytes()[col] == b'#'
}

/// The landmask is coarse; the mesh no longer is. Perturb the sample point
/// with position-hashed jitter so coastlines break into ragged, detailed
/// pixel-coast instead of mask-cell staircases.
fn is_land_detailed(lat: f32, lon: f32) -> bool {
    let h = |a: f32, b: f32, k: u32| -> f32 {
        let mut x = (a * 91.7) as i32 as u32;
        x = x
            .wrapping_mul(2654435761)
            .wrapping_add((b * 73.3) as i32 as u32)
            .wrapping_mul(1274126177)
            .wrapping_add(k);
        x ^= x >> 15;
        ((x.wrapping_mul(2246822519) >> 9) & 1023) as f32 / 1023.0 - 0.5
    };
    let jlat = h(lat, lon, 1) * 2.4;
    let jlon = h(lat, lon, 2) * 2.4;
    is_land((lat + jlat).clamp(-90.0, 90.0), lon + jlon)
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

/// Build the globe sphere, optionally tinting one region's land.
pub fn build_globe(selected: Option<Region>) -> (Vec<LitVertex>, Vec<u32>) {
    let mut vertices = Vec::with_capacity((STACKS + 1) * (SLICES + 1));
    let mut indices = Vec::new();

    for i in 0..=STACKS {
        let lat = 90.0 - 180.0 * i as f32 / STACKS as f32;
        for j in 0..=SLICES {
            let lon = -180.0 + 360.0 * j as f32 / SLICES as f32;
            let pos = latlon_to_pos(lat, lon, GLOBE_RADIUS);
            let normal = pos.normalize();

            let land = is_land_detailed(lat, lon);
            let mut color = if land { LAND } else { OCEAN };
            if land {
                // Mottle the land like the original's hand-placed terrain
                // pixels: forests, plains, and badlands in one green.
                let h = {
                    let mut x = (lat * 53.7) as i32 as u32;
                    x = x
                        .wrapping_mul(2654435761)
                        .wrapping_add((lon * 39.1) as i32 as u32)
                        .wrapping_mul(1274126177);
                    x ^= x >> 15;
                    ((x.wrapping_mul(2246822519) >> 9) & 255) as f32 / 255.0
                };
                let m = 0.80 + 0.45 * h;
                for c in color.iter_mut() {
                    *c = (*c * m).min(1.0);
                }
                if h > 0.86 {
                    // The occasional dun badland breaks the green.
                    color[0] = (color[0] + 0.14).min(1.0);
                    color[2] *= 0.6;
                }
            }
            // Polar ice creeps over everything near the caps.
            let ice = ((lat.abs() - 62.0) / 12.0).clamp(0.0, 1.0);
            if ice > 0.0 && (land || lat.abs() > 74.0) {
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

    let mut push = |lat: f32, lon: f32, size: f32, color: [f32; 4]| {
        let center = latlon_to_pos(lat, lon, GLOBE_RADIUS + size);
        push_cube(&mut vertices, &mut indices, center, size, color);
    };

    for base in &campaign.bases {
        let (lat, lon) = centroid(base.region);
        push(lat, lon, 7.0, [1.0, 0.85, 0.25, 1.0]);
    }
    // Nests breathe, slow and swollen.
    let nest_pulse = 6.0 + 0.7 * (time * 1.7).sin();
    for nest in &campaign.nests {
        push(nest.lat, nest.lon, nest_pulse, [0.45, 0.1, 0.55, 1.0]);
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
        push(rift.lat, rift.lon, size, color);
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
            push(lat, lon, 1.4, [0.9, 0.8, 0.5, 1.0]);
            t += 0.08;
        }
        // The ship itself, bobbing on the wind.
        let (lat, lon) = lerp(progress);
        push(lat, lon, 4.0 + 0.5 * (time * 3.0).sin(), [1.0, 0.85, 0.35, 1.0]);
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
    sun_lon: f32,
    time: f32,
) -> (Vec<ods_render::OverlayVertex>, Vec<u32>) {
    let mut verts = Vec::new();
    let mut indices = Vec::new();
    for (i, &(lat, lon)) in CITIES.iter().enumerate() {
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
        let alpha = (0.35 + 0.55 * depth) * shimmer;

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
                color: [1.0, 0.85, 0.5, alpha],
            });
        }
        indices.extend([0, 1, 2, 0, 2, 3].map(|k| first + k));
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
    fn landmask_rows_are_64_wide() {
        for (i, row) in LANDMASK.iter().enumerate() {
            assert_eq!(row.len(), 64, "row {i}");
        }
    }

    #[test]
    fn known_places_resolve() {
        assert!(is_land(48.0, 2.0), "Paris is on land");
        assert!(is_land(-15.0, -55.0), "Brazil is on land");
        assert!(!is_land(0.0, -140.0), "the mid-Pacific is ocean");
        assert_eq!(region_at(48.0, 2.0), Region::Europe);
        assert_eq!(region_at(40.0, -100.0), Region::NorthAmerica);
        assert_eq!(region_at(-25.0, 135.0), Region::Oceania);
        assert_eq!(region_at(28.0, 45.0), Region::MiddleEast);
        assert_eq!(region_at(80.0, 0.0), Region::Arctic);
    }

    #[test]
    fn globe_mesh_is_well_formed() {
        let (vertices, indices) = build_globe(None);
        assert_eq!(vertices.len(), (STACKS + 1) * (SLICES + 1));
        assert_eq!(indices.len(), STACKS * SLICES * 6);
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

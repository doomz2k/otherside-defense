//! Voxel figure assets, assembled from named body parts.
//!
//! Every species has a blueprint: a list of colored boxes, each box tagged
//! with the [`BodyPart`] it belongs to. That tagging is the point — a future
//! hit-location system can tint a crippled leg, drop a severed weapon, or
//! hide a lost part, and customisation (armour colours, insignia) can restyle
//! parts individually without touching geometry.
//!
//! Figure space: X is width (left/right), Y is depth (facing +Y), Z is up,
//! feet at z = 0, in voxel units (a tile is 16 across; figures stand ~12).

use glam::{IVec3, Vec3};
use ods_render::LitVertex;
use ods_sim::TILE_VOXELS;
use ods_sim::battle::Battle;
use ods_sim::body::BodyPart;
use ods_sim::scenario::GROUND_TOP;
use ods_sim::units::{Species, Unit};

/// One tagged box of a species blueprint.
#[derive(Clone, Copy, Debug)]
pub struct PartBox {
    pub part: BodyPart,
    pub min: Vec3,
    pub max: Vec3,
    pub color: [f32; 4],
}

const fn pb(part: BodyPart, min: (f32, f32, f32), max: (f32, f32, f32), c: [f32; 4]) -> PartBox {
    PartBox {
        part,
        min: Vec3::new(min.0, min.1, min.2),
        max: Vec3::new(max.0, max.1, max.2),
        color: c,
    }
}

// Palette
const ARMOR: [f32; 4] = [0.25, 0.42, 0.68, 1.0];
const ARMOR_DARK: [f32; 4] = [0.18, 0.30, 0.50, 1.0];
const VISOR: [f32; 4] = [0.85, 0.90, 0.95, 1.0];
const GUNMETAL: [f32; 4] = [0.15, 0.15, 0.18, 1.0];
const IMP_SKIN: [f32; 4] = [0.78, 0.22, 0.16, 1.0];
const IMP_DARK: [f32; 4] = [0.55, 0.14, 0.10, 1.0];
const HORN: [f32; 4] = [0.90, 0.85, 0.70, 1.0];
const ROBE: [f32; 4] = [0.30, 0.12, 0.40, 1.0];
const ROBE_DARK: [f32; 4] = [0.20, 0.08, 0.28, 1.0];
const PALE: [f32; 4] = [0.85, 0.80, 0.85, 1.0];
const HOUND: [f32; 4] = [0.35, 0.10, 0.08, 1.0];
const HOUND_DARK: [f32; 4] = [0.22, 0.07, 0.06, 1.0];
const FANG: [f32; 4] = [0.92, 0.90, 0.82, 1.0];
const BILE: [f32; 4] = [0.45, 0.65, 0.20, 1.0];
const BILE_DARK: [f32; 4] = [0.28, 0.42, 0.12, 1.0];
const BONE: [f32; 4] = [0.88, 0.86, 0.78, 1.0];
const BONE_DARK: [f32; 4] = [0.65, 0.62, 0.55, 1.0];
const HUSK_SKIN: [f32; 4] = [0.45, 0.52, 0.42, 1.0];
const HUSK_DARK: [f32; 4] = [0.33, 0.38, 0.31, 1.0];
const PRINCE_ROBE: [f32; 4] = [0.12, 0.05, 0.15, 1.0];
const PRINCE_DARK: [f32; 4] = [0.08, 0.03, 0.10, 1.0];
const GOLD: [f32; 4] = [0.95, 0.78, 0.25, 1.0];
const ASH: [f32; 4] = [0.70, 0.65, 0.72, 1.0];
const CIVVY: [f32; 4] = [0.55, 0.42, 0.28, 1.0];

/// The anatomy-tagged geometry of each species.
pub fn blueprint(species: Species) -> &'static [PartBox] {
    use BodyPart::*;
    match species {
        Species::Soldier => {
            const P: &[PartBox] = &[
            pb(LeftLeg, (-2.5, -1.0, 0.0), (-0.5, 1.0, 5.0), ARMOR_DARK),
            pb(RightLeg, (0.5, -1.0, 0.0), (2.5, 1.0, 5.0), ARMOR_DARK),
            pb(Torso, (-3.0, -1.5, 5.0), (3.0, 1.5, 9.0), ARMOR),
            pb(LeftArm, (-4.2, -1.0, 4.5), (-3.0, 1.0, 8.5), ARMOR_DARK),
            pb(RightArm, (3.0, -1.0, 4.5), (4.2, 1.0, 8.5), ARMOR_DARK),
            pb(Head, (-1.5, -1.5, 9.0), (1.5, 1.5, 11.5), ARMOR),
            pb(Head, (-1.2, 1.2, 9.6), (1.2, 1.6, 10.8), VISOR),
            pb(Weapon, (3.0, -0.5, 6.0), (4.2, 5.5, 7.4), GUNMETAL),
        ];
            P
        }
        Species::Imp => {
            const P: &[PartBox] = &[
            pb(LeftLeg, (-2.0, -1.0, 0.0), (-0.5, 1.0, 2.8), IMP_DARK),
            pb(RightLeg, (0.5, -1.0, 0.0), (2.0, 1.0, 2.8), IMP_DARK),
            pb(Torso, (-2.5, -1.5, 2.8), (2.5, 2.0, 5.8), IMP_SKIN),
            pb(LeftArm, (-3.6, -1.0, 1.8), (-2.5, 1.0, 5.4), IMP_SKIN),
            pb(RightArm, (2.5, -1.0, 1.8), (3.6, 1.0, 5.4), IMP_SKIN),
            pb(Head, (-2.0, -1.5, 5.8), (2.0, 2.0, 8.2), IMP_SKIN),
            pb(Horns, (-1.9, -0.5, 8.2), (-0.9, 0.5, 9.6), HORN),
            pb(Horns, (0.9, -0.5, 8.2), (1.9, 0.5, 9.6), HORN),
        ];
            P
        }
        Species::Overseer => {
            const P: &[PartBox] = &[
            pb(Torso, (-2.8, -1.8, 0.0), (2.8, 1.8, 9.5), ROBE),
            pb(Torso, (-2.2, -1.4, 3.0), (2.2, 1.4, 5.0), ROBE_DARK),
            pb(LeftArm, (-4.0, -1.0, 5.0), (-2.8, 1.0, 9.0), ROBE_DARK),
            pb(RightArm, (2.8, -1.0, 5.0), (4.0, 1.0, 9.0), ROBE_DARK),
            pb(Head, (-1.4, -1.2, 9.5), (1.4, 1.4, 12.0), PALE),
            pb(Horns, (-2.4, -0.4, 11.0), (-1.4, 0.4, 13.4), HORN),
            pb(Horns, (1.4, -0.4, 11.0), (2.4, 0.4, 13.4), HORN),
        ];
            P
        }
        Species::Hellhound => {
            const P: &[PartBox] = &[
            // Quadruped: each Leg part carries its side's pair.
            pb(LeftLeg, (-2.2, -3.5, 0.0), (-1.0, -2.0, 2.2), HOUND_DARK),
            pb(LeftLeg, (-2.2, 2.0, 0.0), (-1.0, 3.5, 2.2), HOUND_DARK),
            pb(RightLeg, (1.0, -3.5, 0.0), (2.2, -2.0, 2.2), HOUND_DARK),
            pb(RightLeg, (1.0, 2.0, 0.0), (2.2, 3.5, 2.2), HOUND_DARK),
            pb(Torso, (-2.2, -4.0, 2.2), (2.2, 4.0, 5.2), HOUND),
            pb(Head, (-1.6, 4.0, 3.0), (1.6, 6.5, 6.0), HOUND),
            pb(Maw, (-1.0, 6.5, 3.2), (1.0, 8.0, 4.4), FANG),
            pb(Tail, (-0.5, -6.0, 4.0), (0.5, -4.0, 5.0), HOUND_DARK),
        ];
            P
        }
        Species::BileWisp => {
            const P: &[PartBox] = &[
            // Floating: nothing touches the ground.
            pb(Sac, (-2.8, -2.8, 4.0), (2.8, 2.8, 9.0), BILE),
            pb(Sac, (-1.8, -1.8, 3.0), (1.8, 1.8, 4.0), BILE_DARK),
            pb(Maw, (-1.0, 2.8, 5.5), (1.0, 4.0, 7.0), BILE_DARK),
        ];
            P
        }
        Species::Taker => {
            const P: &[PartBox] = &[
            pb(LeftLeg, (-2.0, -0.8, 0.0), (-1.0, 0.8, 7.0), BONE_DARK),
            pb(RightLeg, (1.0, -0.8, 0.0), (2.0, 0.8, 7.0), BONE_DARK),
            pb(Torso, (-2.2, -1.2, 7.0), (2.2, 1.2, 11.0), BONE),
            pb(LeftArm, (-3.4, -0.8, 3.0), (-2.2, 0.8, 10.5), BONE),
            pb(LeftArm, (-3.6, 0.8, 3.0), (-2.0, 3.0, 4.2), BONE_DARK), // reaching claw
            pb(RightArm, (2.2, -0.8, 3.0), (3.4, 0.8, 10.5), BONE),
            pb(RightArm, (2.0, 0.8, 3.0), (3.6, 3.0, 4.2), BONE_DARK),
            pb(Head, (-1.2, -1.0, 11.0), (1.2, 1.2, 13.2), BONE),
        ];
            P
        }
        Species::Prince => {
            const P: &[PartBox] = &[
            pb(Torso, (-3.2, -2.0, 0.0), (3.2, 2.0, 10.5), PRINCE_ROBE),
            pb(Torso, (-2.4, -1.6, 4.0), (2.4, 1.6, 6.0), GOLD),
            pb(LeftArm, (-4.6, -1.2, 5.5), (-3.2, 1.2, 10.0), PRINCE_DARK),
            pb(RightArm, (3.2, -1.2, 5.5), (4.6, 1.2, 10.0), PRINCE_DARK),
            pb(Head, (-1.5, -1.3, 10.5), (1.5, 1.5, 13.2), ASH),
            pb(Horns, (-2.8, -0.5, 12.0), (-1.5, 0.5, 14.8), GOLD),
            pb(Horns, (1.5, -0.5, 12.0), (2.8, 0.5, 14.8), GOLD),
            pb(Wings, (-6.5, -2.6, 6.0), (-3.2, -1.8, 12.5), PRINCE_DARK),
            pb(Wings, (3.2, -2.6, 6.0), (6.5, -1.8, 12.5), PRINCE_DARK),
        ];
            P
        }
        Species::Husk => {
            const P: &[PartBox] = &[
            pb(LeftLeg, (-2.5, -1.0, 0.0), (-0.5, 1.0, 4.5), HUSK_DARK),
            pb(RightLeg, (0.5, -1.0, 0.0), (2.5, 1.0, 4.5), HUSK_DARK),
            // The slump: torso and head lean forward.
            pb(Torso, (-3.0, -0.5, 4.5), (3.0, 2.5, 8.0), HUSK_SKIN),
            pb(LeftArm, (-4.2, 0.0, 3.0), (-3.0, 2.0, 7.5), HUSK_DARK),
            pb(RightArm, (3.0, 0.0, 3.0), (4.2, 2.0, 7.5), HUSK_DARK),
            pb(Head, (-1.5, 0.5, 8.0), (1.5, 3.0, 10.2), HUSK_SKIN),
        ];
            P
        }
    }
}

/// Build the mesh for every visible unit on the field.
pub fn build_figures(
    battle: &Battle,
    visible: &std::collections::HashSet<IVec3>,
) -> (Vec<LitVertex>, Vec<u32>) {
    use ods_sim::units::Side;

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for u in &battle.units {
        if !u.alive {
            continue;
        }
        if u.side == Side::Demons && !visible.contains(&u.tile) {
            continue; // hidden in the fog
        }
        push_unit(&mut vertices, &mut indices, u);
    }
    (vertices, indices)
}

fn push_unit(vertices: &mut Vec<LitVertex>, indices: &mut Vec<u32>, unit: &Unit) {
    let feet = (unit.tile * TILE_VOXELS).as_vec3()
        + Vec3::new(8.0, 8.0, GROUND_TOP as f32);

    // Pose: kneeling compresses; the subdued lie in a heap.
    let z_scale = if !unit.conscious {
        0.22
    } else if unit.kneeling {
        0.72
    } else {
        1.0
    };

    for part in blueprint(unit.species) {
        // Weapons fall from unconscious hands.
        if !unit.conscious && part.part == BodyPart::Weapon {
            continue;
        }
        let mut color = part.color;
        // Townsfolk wear homespun, not armor.
        if unit.civilian && part.part != BodyPart::Head {
            color = CIVVY;
        }
        // Crippled parts darken to bruised red — hit location made visible.
        if unit.injuries.contains(&part.part) {
            color = [
                (color[0] * 0.4 + 0.35).min(1.0),
                color[1] * 0.25,
                color[2] * 0.25,
                1.0,
            ];
        }
        if !unit.conscious {
            // Drained of struggle.
            color = [color[0] * 0.6, color[1] * 0.6, color[2] * 0.6, 1.0];
        }
        let min = feet + Vec3::new(part.min.x, part.min.y, part.min.z * z_scale);
        let max = feet + Vec3::new(part.max.x, part.max.y, (part.max.z * z_scale).max(part.min.z * z_scale + 0.4));
        push_box(vertices, indices, min, max, color);
    }
}

fn push_box(
    vertices: &mut Vec<LitVertex>,
    indices: &mut Vec<u32>,
    min: Vec3,
    max: Vec3,
    color: [f32; 4],
) {
    for d in 0..3usize {
        let u = (d + 1) % 3;
        let v = (d + 2) % 3;
        for front in [true, false] {
            let mut normal = [0.0f32; 3];
            normal[d] = if front { 1.0 } else { -1.0 };
            let plane = if front { max[d] } else { min[d] };
            let corner = |cu: f32, cv: f32| -> [f32; 3] {
                let mut p = [0.0f32; 3];
                p[d] = plane;
                p[u] = cu;
                p[v] = cv;
                p
            };
            let (p00, p10, p11, p01) = (
                corner(min[u], min[v]),
                corner(max[u], min[v]),
                corner(max[u], max[v]),
                corner(min[u], max[v]),
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

    const ALL: [Species; 7] = [
        Species::Soldier,
        Species::Imp,
        Species::Overseer,
        Species::Hellhound,
        Species::BileWisp,
        Species::Taker,
        Species::Husk,
    ];

    #[test]
    fn blueprints_match_the_declared_anatomy() {
        for species in ALL {
            let declared = species.body_parts();
            let bp = blueprint(species);
            assert!(!bp.is_empty());
            for part_box in bp {
                assert!(
                    declared.contains(&part_box.part),
                    "{species:?} draws a {:?} it doesn't declare",
                    part_box.part
                );
            }
            // Every declared part gets at least one box: nothing invisible.
            for part in declared {
                assert!(
                    bp.iter().any(|b| b.part == *part),
                    "{species:?} declares {part:?} but never draws it"
                );
            }
        }
    }

    #[test]
    fn boxes_are_well_formed_and_grounded() {
        for species in ALL {
            for b in blueprint(species) {
                assert!(b.min.x < b.max.x && b.min.y < b.max.y && b.min.z < b.max.z);
                assert!(b.min.z >= 0.0, "{species:?} {:?} digs into the floor", b.part);
                assert!(b.max.z <= 15.0, "{species:?} {:?} pokes out of the tile", b.part);
            }
            // Wisps float; everyone else touches the ground.
            let lowest = blueprint(species)
                .iter()
                .map(|b| b.min.z)
                .fold(f32::INFINITY, f32::min);
            if species == Species::BileWisp {
                assert!(lowest > 1.0, "the wisp must hover");
            } else {
                assert_eq!(lowest, 0.0, "{species:?} should stand on the floor");
            }
        }
    }

    #[test]
    fn silhouettes_differ() {
        // Height is the crudest silhouette test: the roster must not be
        // uniform. (Art direction by assertion.)
        let mut heights: Vec<i32> = ALL
            .iter()
            .map(|&s| {
                (blueprint(s).iter().map(|b| b.max.z).fold(0.0, f32::max) * 10.0) as i32
            })
            .collect();
        heights.dedup();
        assert!(heights.len() >= 5, "too many species share a height: {heights:?}");
    }
}

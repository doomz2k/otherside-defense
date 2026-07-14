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
const GARG: [f32; 4] = [0.42, 0.42, 0.48, 1.0];
const GARG_DARK: [f32; 4] = [0.28, 0.28, 0.34, 1.0];
const BEHE: [f32; 4] = [0.42, 0.20, 0.12, 1.0];
const BEHE_DARK: [f32; 4] = [0.28, 0.13, 0.08, 1.0];

/// The anatomy-tagged geometry of each species — the high-density set:
/// each figure is a few dozen tagged boxes, so silhouettes read at a
/// glance and severed parts leave honest gaps. Coordinates are in legacy
/// 16-per-tile units; `push_unit` scales them to the live voxel grid.
pub fn blueprint(species: Species) -> &'static [PartBox] {
    use BodyPart::*;
    match species {
        Species::Soldier => {
            const P: &[PartBox] = &[
                // Boots, shins, thighs.
                pb(LeftLeg, (-2.6, -1.4, 0.0), (-0.6, 1.6, 1.2), GUNMETAL),
                pb(RightLeg, (0.6, -1.4, 0.0), (2.6, 1.6, 1.2), GUNMETAL),
                pb(LeftLeg, (-2.4, -1.0, 1.2), (-0.7, 1.0, 3.0), ARMOR_DARK),
                pb(RightLeg, (0.7, -1.0, 1.2), (2.4, 1.0, 3.0), ARMOR_DARK),
                pb(LeftLeg, (-2.6, -1.2, 3.0), (-0.5, 1.2, 5.0), ARMOR),
                pb(RightLeg, (0.5, -1.2, 3.0), (2.6, 1.2, 5.0), ARMOR),
                // Torso: belt, cuirass, chest plate, collar.
                pb(Torso, (-2.8, -1.4, 5.0), (2.8, 1.4, 5.8), GUNMETAL),
                pb(Torso, (-3.0, -1.5, 5.8), (3.0, 1.5, 8.4), ARMOR),
                pb(Torso, (-2.2, 1.5, 6.2), (2.2, 1.9, 8.0), ARMOR_DARK),
                pb(Torso, (-1.6, -1.7, 8.4), (1.6, 1.7, 9.0), ARMOR_DARK),
                // Pauldrons + arms + gloves.
                pb(LeftArm, (-4.6, -1.4, 7.6), (-2.8, 1.4, 9.0), ARMOR),
                pb(RightArm, (2.8, -1.4, 7.6), (4.6, 1.4, 9.0), ARMOR),
                pb(LeftArm, (-4.2, -1.0, 4.8), (-3.0, 1.0, 7.6), ARMOR_DARK),
                pb(RightArm, (3.0, -1.0, 4.8), (4.2, 1.0, 7.6), ARMOR_DARK),
                pb(LeftArm, (-4.2, -1.0, 4.0), (-3.0, 1.2, 4.8), GUNMETAL),
                pb(RightArm, (3.0, -1.0, 4.0), (4.2, 1.2, 4.8), GUNMETAL),
                // Helmet: skull, brim, visor slit, crest.
                pb(Head, (-1.6, -1.6, 9.0), (1.6, 1.6, 11.4), ARMOR),
                pb(Head, (-1.9, -1.9, 9.0), (1.9, 1.9, 9.6), ARMOR_DARK),
                pb(Head, (-1.3, 1.3, 10.0), (1.3, 1.8, 10.7), VISOR),
                pb(Head, (-0.3, -1.8, 11.4), (0.3, 1.2, 12.2), ARMOR_DARK),
                // Rifle: stock, body, barrel, muzzle.
                pb(Weapon, (3.2, -2.2, 6.0), (4.4, -0.2, 7.2), CIVVY),
                pb(Weapon, (3.2, -0.2, 6.2), (4.4, 3.4, 7.4), GUNMETAL),
                pb(Weapon, (3.5, 3.4, 6.5), (4.1, 6.2, 7.1), GUNMETAL),
                pb(Weapon, (3.4, 6.2, 6.4), (4.2, 6.8, 7.2), ARMOR_DARK),
            ];
            P
        }
        Species::Imp => {
            const P: &[PartBox] = &[
                // Digitigrade legs with hoofed feet.
                pb(LeftLeg, (-2.2, -1.6, 0.0), (-0.7, 0.2, 0.9), HORN),
                pb(RightLeg, (0.7, -1.6, 0.0), (2.2, 0.2, 0.9), HORN),
                pb(LeftLeg, (-2.0, -1.0, 0.9), (-0.6, 1.0, 2.8), IMP_DARK),
                pb(RightLeg, (0.6, -1.0, 0.9), (2.0, 1.0, 2.8), IMP_DARK),
                // Pot-bellied torso with ridged spine.
                pb(Torso, (-2.5, -1.5, 2.8), (2.5, 2.2, 5.8), IMP_SKIN),
                pb(Torso, (-0.5, -2.0, 3.4), (0.5, -1.5, 5.4), IMP_DARK),
                // Long arms with claw hands.
                pb(LeftArm, (-3.6, -1.0, 2.4), (-2.5, 1.0, 5.4), IMP_SKIN),
                pb(RightArm, (2.5, -1.0, 2.4), (3.6, 1.0, 5.4), IMP_SKIN),
                pb(LeftArm, (-3.9, -0.6, 1.5), (-2.8, 1.4, 2.4), IMP_DARK),
                pb(RightArm, (2.8, -0.6, 1.5), (3.9, 1.4, 2.4), IMP_DARK),
                // Head: jaw, snout, ears, two-segment horns.
                pb(Head, (-1.8, -1.4, 5.8), (1.8, 1.8, 8.0), IMP_SKIN),
                pb(Head, (-1.2, 1.8, 6.0), (1.2, 2.6, 7.0), IMP_DARK),
                pb(Head, (-2.4, -0.6, 6.6), (-1.8, 0.4, 7.6), IMP_DARK),
                pb(Head, (1.8, -0.6, 6.6), (2.4, 0.4, 7.6), IMP_DARK),
                pb(Horns, (-1.8, -0.5, 8.0), (-0.9, 0.5, 9.2), HORN),
                pb(Horns, (-2.2, -0.5, 9.2), (-1.3, 0.5, 10.0), HORN),
                pb(Horns, (0.9, -0.5, 8.0), (1.8, 0.5, 9.2), HORN),
                pb(Horns, (1.3, -0.5, 9.2), (2.2, 0.5, 10.0), HORN),
                pb(Tail, (-0.4, -3.4, 2.4), (0.4, -1.5, 3.2), IMP_DARK),
                pb(Tail, (-0.3, -4.4, 3.0), (0.3, -3.4, 3.8), HORN),
            ];
            P
        }
        Species::Overseer => {
            const P: &[PartBox] = &[
                // Layered robes: hem, skirt, girdle, mantle.
                pb(Torso, (-3.2, -2.2, 0.0), (3.2, 2.2, 1.0), ROBE_DARK),
                pb(Torso, (-2.8, -1.8, 1.0), (2.8, 1.8, 6.0), ROBE),
                pb(Torso, (-2.3, -1.5, 4.4), (2.3, 1.5, 5.2), GOLD),
                pb(Torso, (-3.0, -2.0, 6.0), (3.0, 2.0, 9.5), ROBE_DARK),
                // Sleeved arms with pale grasping hands.
                pb(LeftArm, (-4.2, -1.2, 5.0), (-2.9, 1.2, 9.0), ROBE_DARK),
                pb(RightArm, (2.9, -1.2, 5.0), (4.2, 1.2, 9.0), ROBE_DARK),
                pb(LeftArm, (-4.4, -0.6, 4.0), (-3.3, 0.8, 5.0), PALE),
                pb(RightArm, (3.3, -0.6, 4.0), (4.4, 0.8, 5.0), PALE),
                // Cowled head, sunken face, backswept horn crown.
                pb(Head, (-1.7, -1.6, 9.5), (1.7, 1.4, 12.2), ROBE),
                pb(Head, (-1.2, 1.2, 10.0), (1.2, 1.8, 11.6), PALE),
                pb(Horns, (-2.4, -0.6, 11.2), (-1.5, 0.4, 13.0), HORN),
                pb(Horns, (-2.9, -1.2, 12.4), (-2.0, -0.2, 13.8), HORN),
                pb(Horns, (1.5, -0.6, 11.2), (2.4, 0.4, 13.0), HORN),
                pb(Horns, (2.0, -1.2, 12.4), (2.9, -0.2, 13.8), HORN),
            ];
            P
        }
        Species::Hellhound => {
            const P: &[PartBox] = &[
                // Four legs with paws; heavier haunches at the rear.
                pb(LeftLeg, (-2.3, -3.6, 0.0), (-1.0, -1.8, 1.0), HOUND_DARK),
                pb(LeftLeg, (-2.2, -3.4, 1.0), (-1.1, -2.1, 2.4), HOUND),
                pb(LeftLeg, (-2.3, 2.0, 0.0), (-1.0, 3.6, 1.0), HOUND_DARK),
                pb(LeftLeg, (-2.2, 2.1, 1.0), (-1.1, 3.4, 2.4), HOUND),
                pb(RightLeg, (1.0, -3.6, 0.0), (2.3, -1.8, 1.0), HOUND_DARK),
                pb(RightLeg, (1.1, -3.4, 1.0), (2.2, -2.1, 2.4), HOUND),
                pb(RightLeg, (1.0, 2.0, 0.0), (2.3, 3.6, 1.0), HOUND_DARK),
                pb(RightLeg, (1.1, 2.1, 1.0), (2.2, 3.4, 2.4), HOUND),
                // Body: haunches, barrel chest, shoulder hump, spine ridge.
                pb(Torso, (-2.4, -4.2, 2.2), (2.4, -1.2, 5.4), HOUND),
                pb(Torso, (-2.2, -1.2, 2.4), (2.2, 4.0, 5.0), HOUND),
                pb(Torso, (-1.8, 1.6, 5.0), (1.8, 3.8, 6.0), HOUND_DARK),
                pb(Torso, (-0.4, -3.8, 5.2), (0.4, 1.6, 5.8), HOUND_DARK),
                // Head: skull, ears, brow; the maw with two teeth rows.
                pb(Head, (-1.6, 4.0, 3.2), (1.6, 6.4, 5.8), HOUND),
                pb(Head, (-1.5, 3.9, 5.8), (-0.7, 4.9, 6.6), HOUND_DARK),
                pb(Head, (0.7, 3.9, 5.8), (1.5, 4.9, 6.6), HOUND_DARK),
                pb(Maw, (-1.1, 6.4, 3.8), (1.1, 8.0, 4.6), HOUND_DARK),
                pb(Maw, (-0.9, 6.6, 4.6), (0.9, 7.8, 5.0), FANG),
                pb(Maw, (-0.9, 6.6, 3.4), (0.9, 7.8, 3.8), FANG),
                // Tail, raised.
                pb(Tail, (-0.5, -5.6, 4.2), (0.5, -4.0, 5.0), HOUND_DARK),
                pb(Tail, (-0.4, -6.6, 5.0), (0.4, -5.4, 5.8), HOUND_DARK),
            ];
            P
        }
        Species::BileWisp => {
            const P: &[PartBox] = &[
                // A lobed, sagging float-sac with drips and tendrils.
                pb(Sac, (-2.8, -2.8, 4.6), (2.8, 2.8, 8.6), BILE),
                pb(Sac, (-2.2, -2.2, 8.6), (2.2, 2.2, 9.6), BILE_DARK),
                pb(Sac, (-3.4, -1.4, 5.4), (-2.8, 1.4, 7.6), BILE_DARK),
                pb(Sac, (2.8, -1.4, 5.4), (3.4, 1.4, 7.6), BILE_DARK),
                pb(Sac, (-1.8, -1.8, 3.6), (1.8, 1.8, 4.6), BILE_DARK),
                pb(Sac, (-0.6, -0.6, 2.6), (0.6, 0.6, 3.6), BILE),
                pb(Sac, (-0.3, -0.3, 1.8), (0.3, 0.3, 2.6), BILE_DARK),
                // The puckered maw, dripping.
                pb(Maw, (-1.1, 2.8, 5.6), (1.1, 4.0, 7.0), BILE_DARK),
                pb(Maw, (-0.5, 3.2, 4.8), (0.5, 3.8, 5.6), BILE),
            ];
            P
        }
        Species::Taker => {
            const P: &[PartBox] = &[
                // Stilted shin-bone legs.
                pb(LeftLeg, (-2.0, -0.8, 0.0), (-1.1, 0.8, 3.6), BONE_DARK),
                pb(LeftLeg, (-2.2, -1.0, 3.6), (-0.9, 1.0, 7.0), BONE),
                pb(RightLeg, (1.1, -0.8, 0.0), (2.0, 0.8, 3.6), BONE_DARK),
                pb(RightLeg, (0.9, -1.0, 3.6), (2.2, 1.0, 7.0), BONE),
                // Gaunt trunk with visible rib bars.
                pb(Torso, (-2.2, -1.2, 7.0), (2.2, 1.2, 11.0), BONE),
                pb(Torso, (-2.3, 1.2, 7.6), (2.3, 1.4, 8.0), BONE_DARK),
                pb(Torso, (-2.3, 1.2, 8.6), (2.3, 1.4, 9.0), BONE_DARK),
                pb(Torso, (-2.3, 1.2, 9.6), (2.3, 1.4, 10.0), BONE_DARK),
                // The arms: too long, ending in finger-blades.
                pb(LeftArm, (-3.3, -0.8, 4.0), (-2.2, 0.8, 10.5), BONE),
                pb(RightArm, (2.2, -0.8, 4.0), (3.3, 0.8, 10.5), BONE),
                pb(LeftArm, (-3.6, 0.8, 3.0), (-2.6, 3.2, 3.6), BONE_DARK),
                pb(LeftArm, (-3.2, 0.8, 2.2), (-2.2, 3.6, 2.8), FANG),
                pb(RightArm, (2.6, 0.8, 3.0), (3.6, 3.2, 3.6), BONE_DARK),
                pb(RightArm, (2.2, 0.8, 2.2), (3.2, 3.6, 2.8), FANG),
                // A skull, not a face; the jaw hangs.
                pb(Head, (-1.2, -1.0, 11.0), (1.2, 1.2, 13.0), BONE),
                pb(Head, (-0.9, 1.0, 11.2), (0.9, 1.5, 12.6), BONE_DARK),
                pb(Head, (-0.8, 1.0, 10.4), (0.8, 1.6, 11.0), BONE),
            ];
            P
        }
        Species::Prince => {
            const P: &[PartBox] = &[
                // Robes in tiers, belted in gold, trailing to the ground.
                pb(Torso, (-3.6, -2.4, 0.0), (3.6, 2.4, 1.2), PRINCE_DARK),
                pb(Torso, (-3.2, -2.0, 1.2), (3.2, 2.0, 7.0), PRINCE_ROBE),
                pb(Torso, (-2.5, -1.7, 4.6), (2.5, 1.7, 5.6), GOLD),
                pb(Torso, (-3.0, -1.9, 7.0), (3.0, 1.9, 10.5), PRINCE_DARK),
                pb(Torso, (-1.4, 1.9, 7.4), (1.4, 2.3, 9.8), GOLD),
                // Clawed sleeves.
                pb(LeftArm, (-4.7, -1.2, 5.5), (-3.2, 1.2, 10.0), PRINCE_DARK),
                pb(RightArm, (3.2, -1.2, 5.5), (4.7, 1.2, 10.0), PRINCE_DARK),
                pb(LeftArm, (-4.9, -0.6, 4.4), (-3.7, 1.0, 5.5), ASH),
                pb(RightArm, (3.7, -0.6, 4.4), (4.9, 1.0, 5.5), ASH),
                // The ash-grey head under a golden horn-crown.
                pb(Head, (-1.5, -1.3, 10.5), (1.5, 1.5, 13.0), ASH),
                pb(Head, (-1.0, 1.4, 11.0), (1.0, 1.8, 12.4), PRINCE_DARK),
                pb(Horns, (-2.6, -0.5, 12.0), (-1.5, 0.5, 13.6), GOLD),
                pb(Horns, (-3.2, -0.5, 13.0), (-2.2, 0.5, 14.6), GOLD),
                pb(Horns, (1.5, -0.5, 12.0), (2.6, 0.5, 13.6), GOLD),
                pb(Horns, (2.2, -0.5, 13.0), (3.2, 0.5, 14.6), GOLD),
                pb(Horns, (-0.5, -0.5, 13.0), (0.5, 0.5, 14.2), GOLD),
                // Wings: bone fingers with stretched membrane.
                pb(Wings, (-6.8, -2.4, 5.6), (-3.2, -2.0, 12.8), PRINCE_DARK),
                pb(Wings, (-6.2, -2.2, 6.4), (-3.2, -1.8, 11.6), ROBE),
                pb(Wings, (3.2, -2.4, 5.6), (6.8, -2.0, 12.8), PRINCE_DARK),
                pb(Wings, (3.2, -2.2, 6.4), (6.2, -1.8, 11.6), ROBE),
            ];
            P
        }
        Species::Gargoyle => {
            const P: &[PartBox] = &[
                // Perched haunches and gripping stone claws.
                pb(Torso, (-2.2, -1.8, 1.2), (2.2, 1.2, 3.0), GARG_DARK),
                pb(LeftArm, (-2.6, -0.6, 0.0), (-1.4, 1.2, 1.2), GARG),
                pb(RightArm, (1.4, -0.6, 0.0), (2.6, 1.2, 1.2), GARG),
                // Hunched trunk with a spined back.
                pb(Torso, (-2.0, -1.5, 3.0), (2.0, 1.5, 6.5), GARG),
                pb(Torso, (-0.4, -2.0, 4.0), (0.4, -1.5, 6.2), GARG_DARK),
                // Snouted head with ear-horns.
                pb(Head, (-1.4, 1.0, 6.0), (1.4, 2.6, 8.2), GARG),
                pb(Head, (-0.9, 2.6, 6.3), (0.9, 3.4, 7.2), GARG_DARK),
                pb(Head, (-1.5, 0.8, 8.2), (-0.8, 1.6, 9.2), GARG_DARK),
                pb(Head, (0.8, 0.8, 8.2), (1.5, 1.6, 9.2), GARG_DARK),
                // Wings: leading-edge bone, membrane, wingtip claw.
                pb(Wings, (-5.6, -1.6, 8.4), (-2.0, -1.0, 9.6), GARG),
                pb(Wings, (-5.4, -1.7, 4.2), (-2.0, -1.1, 8.4), GARG_DARK),
                pb(Wings, (-6.2, -1.5, 8.8), (-5.4, -0.9, 9.8), GARG),
                pb(Wings, (2.0, -1.6, 8.4), (5.6, -1.0, 9.6), GARG),
                pb(Wings, (2.0, -1.7, 4.2), (5.4, -1.1, 8.4), GARG_DARK),
                pb(Wings, (5.4, -1.5, 8.8), (6.2, -0.9, 9.8), GARG),
                pb(Tail, (-0.4, -4.4, 3.0), (0.4, -1.6, 3.8), GARG_DARK),
                pb(Tail, (-0.6, -5.2, 3.4), (0.6, -4.4, 4.2), GARG),
            ];
            P
        }
        Species::Behemoth => {
            const P: &[PartBox] = &[
                // Pillar legs on splayed feet.
                pb(LeftLeg, (-5.4, -2.6, 0.0), (-1.8, 2.4, 1.4), BEHE_DARK),
                pb(RightLeg, (1.8, -2.6, 0.0), (5.4, 2.4, 1.4), BEHE_DARK),
                pb(LeftLeg, (-5.0, -2.0, 1.4), (-2.0, 2.0, 5.0), BEHE),
                pb(RightLeg, (2.0, -2.0, 1.4), (5.0, 2.0, 5.0), BEHE),
                // The mountain: gut, chest, shoulder plates, back spines.
                pb(Torso, (-6.0, -3.0, 5.0), (6.0, 3.0, 9.0), BEHE),
                pb(Torso, (-5.4, -2.6, 9.0), (5.4, 2.6, 12.0), BEHE_DARK),
                pb(Torso, (-7.0, -2.4, 10.4), (-4.6, 2.4, 12.6), BEHE_DARK),
                pb(Torso, (4.6, -2.4, 10.4), (7.0, 2.4, 12.6), BEHE_DARK),
                pb(Torso, (-1.0, -3.8, 8.4), (1.0, -3.0, 11.0), HORN),
                pb(Torso, (-0.8, -4.4, 6.4), (0.8, -3.6, 9.0), HORN),
                // Arms that end in knuckled fists dragging low.
                pb(LeftArm, (-7.8, -2.0, 4.2), (-6.0, 2.0, 10.8), BEHE_DARK),
                pb(RightArm, (6.0, -2.0, 4.2), (7.8, 2.0, 10.8), BEHE_DARK),
                pb(LeftArm, (-8.4, -1.6, 2.2), (-6.2, 2.4, 4.2), BEHE),
                pb(RightArm, (6.2, -1.6, 2.2), (8.4, 2.4, 4.2), BEHE),
                // Head low between the shoulders, tusked and horned.
                pb(Head, (-2.2, 3.0, 9.0), (2.2, 5.4, 12.6), BEHE),
                pb(Head, (-1.8, 5.4, 9.4), (1.8, 6.2, 10.8), BEHE_DARK),
                pb(Head, (-2.0, 5.2, 8.2), (-1.2, 6.4, 9.4), FANG),
                pb(Head, (1.2, 5.2, 8.2), (2.0, 6.4, 9.4), FANG),
                pb(Horns, (-3.6, 3.2, 11.6), (-2.2, 4.2, 13.4), HORN),
                pb(Horns, (-4.4, 3.2, 12.8), (-3.2, 4.2, 14.6), HORN),
                pb(Horns, (2.2, 3.2, 11.6), (3.6, 4.2, 13.4), HORN),
                pb(Horns, (3.2, 3.2, 12.8), (4.4, 4.2, 14.6), HORN),
            ];
            P
        }
        Species::Husk => {
            const P: &[PartBox] = &[
                // The shamble: uneven legs, one dragging.
                pb(LeftLeg, (-2.5, -1.0, 0.0), (-0.6, 1.0, 4.5), HUSK_DARK),
                pb(RightLeg, (0.6, -0.6, 0.0), (2.5, 1.6, 4.0), HUSK_DARK),
                // Caved torso showing rib shadows, slumped forward.
                pb(Torso, (-3.0, -0.5, 4.5), (3.0, 2.5, 8.0), HUSK_SKIN),
                pb(Torso, (-2.6, 2.5, 5.2), (2.6, 2.7, 5.6), HUSK_DARK),
                pb(Torso, (-2.6, 2.5, 6.2), (2.6, 2.7, 6.6), HUSK_DARK),
                // Arms hang wrong; one reaches.
                pb(LeftArm, (-4.2, 0.0, 3.0), (-3.0, 2.0, 7.5), HUSK_DARK),
                pb(RightArm, (3.0, 0.6, 4.4), (4.2, 3.2, 5.6), HUSK_DARK),
                pb(RightArm, (3.2, 3.2, 4.2), (4.0, 4.4, 5.0), HUSK_SKIN),
                // The head lolls; the jaw no longer closes.
                pb(Head, (-1.5, 0.5, 8.0), (1.5, 3.0, 10.2), HUSK_SKIN),
                pb(Head, (-1.0, 2.6, 7.2), (1.0, 3.4, 8.0), HUSK_DARK),
            ];
            P
        }
    }
}

/// Per-unit animation state the battle screen tracks between frames.
#[derive(Clone, Copy, Default)]
pub struct AnimState {
    /// Walk-cycle phase in radians; 0 means standing still.
    pub walk: f32,
    /// Seconds of fire recoil remaining.
    pub recoil: f32,
    /// The shared clock, for idle breathing.
    pub breath: f32,
    /// Eased vertical pose (kneels sink, deaths crumple); 0 = unset.
    pub pose: f32,
}

/// The z-scale a unit's state calls for — the tween's destination.
pub fn pose_target(unit: &Unit) -> f32 {
    if !unit.alive {
        0.12
    } else if !unit.conscious {
        0.22
    } else if unit.kneeling {
        0.72
    } else if unit.morale < 35 {
        0.9
    } else {
        1.0
    }
}

/// Build the mesh for every visible unit on the field. `visual` overrides
/// feet positions for gliding movement (missing entries snap to the tile);
/// `anim` carries walk phases and recoil so the figures move like figures.
pub fn build_figures(
    battle: &Battle,
    visible: &std::collections::HashSet<IVec3>,
    visual: &std::collections::HashMap<u32, Vec3>,
    anim: &std::collections::HashMap<u32, AnimState>,
) -> (Vec<LitVertex>, Vec<u32>) {
    use ods_sim::units::{Side, Species};

    let night = battle.vision_tiles < 14;
    let soldier_tiles: Vec<IVec3> = battle
        .units
        .iter()
        .filter(|u| u.is_active() && u.side == Side::Order)
        .map(|u| u.tile)
        .collect();
    let near_soldier = |tile: IVec3, range: i32| {
        soldier_tiles
            .iter()
            .any(|s| (s.x - tile.x).abs().max((s.y - tile.y).abs()) <= range)
    };

    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    for u in &battle.units {
        // The dead stay on the field — collapsed, darkened, recoverable —
        // unless they were eaten, defiled, or blown apart.
        if !u.alive && !u.is_corpse() {
            continue;
        }
        if u.side == Side::Demons && !visible.contains(&u.tile) {
            // At night, the pack beyond your lamplight is a pair of eyes.
            if night && u.is_active() && near_soldier(u.tile, battle.vision_tiles + 4) {
                push_eyes(&mut vertices, &mut indices, u);
            }
            continue; // hidden in the fog
        }
        // The Taker is never truly seen: at arm's length, or lit by open
        // flame — otherwise only its footprints and the noises it makes.
        if u.species == Species::Taker
            && u.is_active()
            && !near_soldier(u.tile, 2)
            && !battle.clouds.iter().any(|(t, k, _)| {
                *k == ods_sim::battle::CloudKind::Fire
                    && (t.x - u.tile.x).abs().max((t.y - u.tile.y).abs()) <= 1
            })
        {
            if night {
                push_eyes(&mut vertices, &mut indices, u);
            }
            continue;
        }
        let feet = visual.get(&u.id.0).copied();
        let state = anim.get(&u.id.0).copied().unwrap_or_default();
        push_unit(&mut vertices, &mut indices, u, feet, state);
    }
    (vertices, indices)
}

/// Two burning points at head height: the shape of a demon you can't see.
fn push_eyes(vertices: &mut Vec<LitVertex>, indices: &mut Vec<u32>, unit: &ods_sim::units::Unit) {
    let half = ods_sim::TILE_VOXELS as f32 / 2.0;
    let feet = (unit.tile * ods_sim::TILE_VOXELS).as_vec3()
        + Vec3::new(half, half, GROUND_TOP as f32);
    // Over-unit color: survives the diffuse term and reads as glow.
    let glow = [3.0, 0.35, 0.2, 1.0];
    let vs = ods_sim::VS as f32;
    for dx in [-1.4f32, 1.4] {
        let c = feet + Vec3::new(dx, 0.0, 10.5) * vs;
        push_box(
            vertices,
            indices,
            c - Vec3::splat(0.45 * vs),
            c + Vec3::splat(0.45 * vs),
            glow,
        );
    }
}

fn push_unit(
    vertices: &mut Vec<LitVertex>,
    indices: &mut Vec<u32>,
    unit: &Unit,
    feet_override: Option<Vec3>,
    anim: AnimState,
) {
    let mut feet = feet_override.unwrap_or_else(|| {
        (unit.tile * TILE_VOXELS).as_vec3()
            + Vec3::new(
                TILE_VOXELS as f32 / 2.0,
                TILE_VOXELS as f32 / 2.0,
                GROUND_TOP as f32,
            )
    });
    let vs = ods_sim::VS as f32;
    // The walk: the whole body bobs on the beat of the stride.
    let moving = unit.is_active() && anim.walk != 0.0;
    if moving {
        feet.z += (anim.walk * 2.0).sin().abs() * 0.35 * vs;
    } else if unit.is_active() {
        // Idle breathing: barely there, phase-shifted per figure.
        feet.z += (anim.breath * 1.4 + unit.id.0 as f32 * 1.7).sin() * 0.10 * vs;
    }
    let swing = if moving { anim.walk.sin() } else { 0.0 };
    let face = Vec3::new(unit.facing.x as f32, unit.facing.y as f32, 0.0).normalize_or(Vec3::X);
    // Recoil: the shot kicks the shooter back off the line for a blink.
    let kick = if unit.is_active() { anim.recoil.max(0.0) } else { 0.0 };
    let body_offset = face * (-kick * 9.0 * vs);

    // Pose: kneeling compresses, the subdued heap, the dead crumple —
    // eased by the anim state where the battle screen provides one.
    let z_scale = if anim.pose > 0.0 { anim.pose } else { pose_target(unit) };

    for part in blueprint(unit.species) {
        // Weapons fall from unconscious hands.
        if !unit.conscious && part.part == BodyPart::Weapon {
            continue;
        }
        // Severed parts are simply not there.
        if unit.severed.contains(&part.part) {
            continue;
        }
        let mut color = part.color;
        // Corpses drain toward grave-grey.
        if !unit.alive {
            color = [color[0] * 0.3, color[1] * 0.3, color[2] * 0.3, 1.0];
        } else {
            // The hurt wear it: blood-darkened toward the end.
            let vitality =
                (unit.health.max(0) as f32 / unit.health_max.max(1) as f32).clamp(0.0, 1.0);
            let dim = 0.55 + 0.45 * vitality;
            color = [color[0] * dim, color[1] * dim, color[2] * dim, color[3]];
        }
        // Rot glows from inside the part it holds.
        if unit.infected.map(|(p, _)| p) == Some(part.part) {
            color = [0.35, 0.9, 0.25, 1.0];
        }
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
        // The stride: legs scissor along the facing, arms answer opposite,
        // and the swinging leg lifts. The weapon rides the recoil.
        let mut offset = body_offset;
        if z_scale >= 0.7 {
            match part.part {
                BodyPart::LeftLeg => {
                    offset += face * (swing * 1.4 * vs);
                    offset.z += swing.max(0.0) * 0.8 * vs;
                }
                BodyPart::RightLeg => {
                    offset += face * (-swing * 1.4 * vs);
                    offset.z += (-swing).max(0.0) * 0.8 * vs;
                }
                BodyPart::LeftArm => offset += face * (-swing * 0.9 * vs),
                BodyPart::RightArm | BodyPart::Weapon => {
                    offset += face * (swing * 0.9 * vs);
                    if part.part == BodyPart::Weapon {
                        offset.z += kick * 12.0 * vs;
                    }
                }
                _ => {}
            }
        }
        let min = feet + offset + Vec3::new(part.min.x, part.min.y, part.min.z * z_scale) * vs;
        let max = feet
            + offset
            + Vec3::new(
                part.max.x,
                part.max.y,
                (part.max.z * z_scale).max(part.min.z * z_scale + 0.4),
            ) * vs;
        push_box(vertices, indices, min, max, color);
    }
}

/// The bedrock skirt: rough dark rock hanging below the battlefield's rim,
/// so the map reads as a place instead of a slab floating in the void.
pub fn build_skirt(min_tile: IVec3, max_tile: IVec3, seed: u64) -> (Vec<LitVertex>, Vec<u32>) {
    let t = ods_sim::TILE_VOXELS;
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let hash = |x: i32, y: i32| -> f32 {
        let mut h = (seed as u32)
            .wrapping_mul(747796405)
            .wrapping_add(x as u32)
            .wrapping_mul(2654435761)
            .wrapping_add(y as u32)
            .wrapping_mul(1274126177);
        h ^= h >> 15;
        ((h >> 8) & 255) as f32 / 255.0
    };
    let mut slab = |x0: f32, y0: f32, x1: f32, y1: f32, h: f32| {
        let depth = (14.0 + 22.0 * h) * ods_sim::VS as f32;
        let shade = 0.75 + 0.35 * h;
        let color = [0.14 * shade, 0.12 * shade, 0.10 * shade, 1.0];
        push_box(
            &mut vertices,
            &mut indices,
            Vec3::new(x0, y0, -depth),
            Vec3::new(x1, y1, 2.0),
            color,
        );
    };
    let (x0, y0) = ((min_tile.x * t) as f32, (min_tile.y * t) as f32);
    let (x1, y1) = ((max_tile.x * t) as f32, (max_tile.y * t) as f32);
    let hang = 6.0 * ods_sim::VS as f32;
    for tx in min_tile.x..max_tile.x {
        let (a, b) = ((tx * t) as f32, ((tx + 1) * t) as f32);
        slab(a, y0 - hang, b, y0 + 1.0, hash(tx, -1));
        slab(a, y1 - 1.0, b, y1 + hang, hash(tx, -2));
    }
    for ty in min_tile.y..max_tile.y {
        let (a, b) = ((ty * t) as f32, ((ty + 1) * t) as f32);
        slab(x0 - hang, a, x0 + 1.0, b, hash(-1, ty));
        slab(x1 - 1.0, a, x1 + hang, b, hash(-2, ty));
    }
    (vertices, indices)
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

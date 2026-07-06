//! Body-part taxonomy. Every species is built from named parts — the visual
//! figures attach geometry to these, and a future hit-location system will
//! attach per-part damage/injury to them (crippled legs slow movement,
//! wounded arms spoil aim, headshots stun). Keep gameplay identifiers here,
//! in the sim, so both layers agree on the anatomy.

use crate::units::Species;

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, serde::Serialize, serde::Deserialize)]
pub enum BodyPart {
    Head,
    Torso,
    LeftArm,
    RightArm,
    /// For quadofrupeds this covers the whole left pair.
    LeftLeg,
    RightLeg,
    /// Carried weapon (soldiers' rifles); severing it is disarmament.
    Weapon,
    Horns,
    Tail,
    Maw,
    /// The Bile-wisp's floating body is one distended organ.
    Sac,
    Wings,
}

impl BodyPart {
    pub fn name(self) -> &'static str {
        match self {
            BodyPart::Head => "head",
            BodyPart::Torso => "torso",
            BodyPart::LeftArm => "left arm",
            BodyPart::RightArm => "right arm",
            BodyPart::LeftLeg => "left leg",
            BodyPart::RightLeg => "right leg",
            BodyPart::Weapon => "weapon",
            BodyPart::Horns => "horns",
            BodyPart::Tail => "tail",
            BodyPart::Maw => "maw",
            BodyPart::Sac => "sac",
            BodyPart::Wings => "wings",
        }
    }
}

impl Species {
    /// The anatomy of each breed: which parts exist to be drawn — and, in
    /// time, to be hit.
    pub fn body_parts(self) -> &'static [BodyPart] {
        use BodyPart::*;
        match self {
            Species::Soldier => &[Head, Torso, LeftArm, RightArm, LeftLeg, RightLeg, Weapon],
            Species::Imp => &[Head, Torso, LeftArm, RightArm, LeftLeg, RightLeg, Horns],
            Species::Overseer => &[Head, Torso, LeftArm, RightArm, Horns],
            Species::Hellhound => &[Head, Maw, Torso, LeftLeg, RightLeg, Tail],
            Species::BileWisp => &[Sac, Maw],
            Species::Taker => &[Head, Torso, LeftArm, RightArm, LeftLeg, RightLeg],
            Species::Husk => &[Head, Torso, LeftArm, RightArm, LeftLeg, RightLeg],
            Species::Prince => &[Head, Torso, LeftArm, RightArm, Horns, Wings],
            Species::Gargoyle => &[Head, Torso, LeftArm, RightArm, Wings, Tail],
            Species::Behemoth => &[Head, Torso, LeftArm, RightArm, LeftLeg, RightLeg, Horns],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_species_has_an_anatomy() {
        for species in [
            Species::Soldier,
            Species::Imp,
            Species::Overseer,
            Species::Hellhound,
            Species::BileWisp,
            Species::Taker,
            Species::Husk,
        ] {
            let parts = species.body_parts();
            assert!(!parts.is_empty(), "{species:?} has no parts");
            // Anything that walks has legs; anything that grips has arms.
            if species != Species::BileWisp {
                assert!(
                    parts.contains(&BodyPart::Torso),
                    "{species:?} needs a torso"
                );
            }
        }
        assert!(Species::Soldier.body_parts().contains(&BodyPart::Weapon));
        assert!(Species::Hellhound.body_parts().contains(&BodyPart::Maw));
    }
}

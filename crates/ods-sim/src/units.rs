//! Units and weapons. Stats follow the original game's ranges; there are no
//! classes — soldiers differentiate by what happens to them (progression
//! lives in the campaign layer). Demons differentiate by species: each breed
//! changes the tactical rules rather than just the numbers.

use glam::IVec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Serialize, serde::Deserialize)]
pub enum Side {
    Order,
    Demons,
}

impl Side {
    pub fn enemy(self) -> Side {
        match self {
            Side::Order => Side::Demons,
            Side::Demons => Side::Order,
        }
    }
}

/// What kind of creature a unit is — drives AI, visuals, and special rules.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, serde::Serialize, serde::Deserialize)]
pub enum Species {
    Soldier,
    /// Weak ranged swarm grunt.
    Imp,
    /// Pack leader: hellspit plus the Terrify psi attack.
    Overseer,
    /// Fast melee pouncer with a thick hide.
    Hellhound,
    /// Floating acid-sac; lobs arcing globs over cover.
    BileWisp,
    /// The horror: a melee one-hit killer whose victims rise as Husks.
    Taker,
    /// A Taken body. Slow, but every one is a Taker in waiting.
    Husk,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub struct UnitId(pub u32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FireMode {
    Snap,
    Aimed,
    /// Burst fire: several rounds for one action, each rolled separately.
    Auto,
}

/// Burst-fire behaviour, for weapons that support it.
#[derive(Clone, Copy, Debug)]
pub struct AutoFire {
    pub cost_pct: i32,
    pub acc: i32,
    pub rounds: u32,
}

#[derive(Clone, Debug)]
pub struct Weapon {
    pub name: &'static str,
    /// Base damage; actual damage rolls 0–200% of this (X-COM style).
    pub power: i32,
    /// TU costs as a percentage of the shooter's max TUs.
    pub snap_cost_pct: i32,
    pub aimed_cost_pct: i32,
    /// Accuracy multipliers per fire mode, in percent.
    pub snap_acc: i32,
    pub aimed_acc: i32,
    pub auto: Option<AutoFire>,
    /// Radius of terrain destroyed where a stray shot lands.
    pub breach_radius: f32,
    /// Melee weapons strike adjacent tiles only and never fly wide.
    pub melee: bool,
    /// Arcing weapons lob over cover: no line of sight needed in range.
    pub arcing: bool,
}

impl Weapon {
    fn base(name: &'static str, power: i32) -> Self {
        Self {
            name,
            power,
            snap_cost_pct: 30,
            aimed_cost_pct: 60,
            snap_acc: 60,
            aimed_acc: 100,
            auto: None,
            breach_radius: 1.0,
            melee: false,
            arcing: false,
        }
    }
}

pub fn rifle() -> Weapon {
    Weapon {
        snap_cost_pct: 25,
        aimed_cost_pct: 50,
        aimed_acc: 110,
        auto: Some(AutoFire { cost_pct: 35, acc: 35, rounds: 3 }),
        breach_radius: 1.6,
        ..Weapon::base("consecrated rifle", 30)
    }
}

pub fn hellspit() -> Weapon {
    Weapon {
        snap_acc: 55,
        breach_radius: 1.2,
        ..Weapon::base("hellspit", 18)
    }
}

pub fn claw(name: &'static str, power: i32) -> Weapon {
    Weapon {
        snap_cost_pct: 25,
        snap_acc: 85,
        aimed_acc: 110,
        melee: true,
        breach_radius: 0.0,
        ..Weapon::base(name, power)
    }
}

pub fn bile_lob() -> Weapon {
    Weapon {
        snap_cost_pct: 40,
        snap_acc: 65,
        arcing: true,
        breach_radius: 2.5,
        ..Weapon::base("bile glob", 22)
    }
}

/// Maximum range of arcing weapons, in tiles (Chebyshev).
pub const ARC_RANGE_TILES: i32 = 8;

#[derive(Clone, Debug)]
pub struct Unit {
    pub id: UnitId,
    pub side: Side,
    pub species: Species,
    pub name: String,
    pub tile: IVec3,
    pub tu_max: i32,
    pub tu: i32,
    pub health_max: i32,
    pub health: i32,
    /// Non-lethal trauma; at `health` or above the unit falls unconscious.
    pub stun: i32,
    /// False while unconscious. Dead units are `!alive` regardless.
    pub conscious: bool,
    /// 0..=100; low morale risks panic or berserk at turn start.
    pub morale: i32,
    pub reactions: i32,
    pub accuracy: i32,
    pub bravery: i32,
    /// Kneeling: +15% accuracy until the unit moves.
    pub kneeling: bool,
    /// Reserve enough TUs for a snap shot when moving.
    pub reserve_snap: bool,
    /// Can use the Terrify psi attack.
    pub psi: bool,
    pub weapon: Weapon,
    /// Open fatal wounds; each bleeds 1 health at the unit's turn start.
    pub wounds: i32,
    /// Hellfire charges carried (thrown explosives).
    pub grenades: u32,
    /// Field-dressing uses left (staunches wounds, restores some health).
    pub heal_charges: u32,
    pub alive: bool,
}

impl Unit {
    fn base(id: u32, side: Side, species: Species, name: &str, tile: IVec3) -> Self {
        Self {
            id: UnitId(id),
            side,
            species,
            name: name.to_string(),
            tile,
            tu_max: 45,
            tu: 45,
            health_max: 18,
            health: 18,
            stun: 0,
            conscious: true,
            morale: 100,
            reactions: 40,
            accuracy: 45,
            bravery: 50,
            kneeling: false,
            reserve_snap: false,
            psi: false,
            weapon: hellspit(),
            wounds: 0,
            grenades: 0,
            heal_charges: 0,
            alive: true,
        }
    }

    /// Alive and conscious: able to act, react, and hold the field.
    pub fn is_active(&self) -> bool {
        self.alive && self.conscious
    }

    pub fn soldier(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            tu_max: 55,
            tu: 55,
            health_max: 32,
            health: 32,
            reactions: 50,
            accuracy: 60,
            bravery: 30,
            weapon: rifle(),
            grenades: 2,
            heal_charges: 3,
            ..Self::base(id, Side::Order, Species::Soldier, name, tile)
        }
    }

    pub fn imp(id: u32, name: &str, tile: IVec3) -> Self {
        Self::base(id, Side::Demons, Species::Imp, name, tile)
    }

    pub fn overseer(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            tu_max: 50,
            tu: 50,
            health_max: 24,
            health: 24,
            accuracy: 55,
            bravery: 70,
            psi: true,
            ..Self::base(id, Side::Demons, Species::Overseer, name, tile)
        }
    }

    pub fn hellhound(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            tu_max: 70,
            tu: 70,
            health_max: 30,
            health: 30,
            reactions: 55,
            accuracy: 60,
            bravery: 60,
            weapon: claw("fangs", 25),
            ..Self::base(id, Side::Demons, Species::Hellhound, name, tile)
        }
    }

    pub fn bile_wisp(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            tu_max: 40,
            tu: 40,
            health_max: 12,
            health: 12,
            weapon: bile_lob(),
            ..Self::base(id, Side::Demons, Species::BileWisp, name, tile)
        }
    }

    pub fn taker(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            tu_max: 70,
            tu: 70,
            health_max: 35,
            health: 35,
            reactions: 60,
            accuracy: 70,
            bravery: 90,
            weapon: claw("taking claws", 90),
            ..Self::base(id, Side::Demons, Species::Taker, name, tile)
        }
    }

    pub fn husk(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            tu_max: 30,
            tu: 30,
            health_max: 25,
            health: 25,
            reactions: 20,
            accuracy: 40,
            bravery: 100,
            weapon: claw("dead hands", 15),
            ..Self::base(id, Side::Demons, Species::Husk, name, tile)
        }
    }

    /// TU cost for a fire mode; None when the weapon lacks the mode.
    pub fn fire_cost(&self, mode: FireMode) -> Option<i32> {
        let pct = match mode {
            FireMode::Snap => self.weapon.snap_cost_pct,
            FireMode::Aimed => self.weapon.aimed_cost_pct,
            FireMode::Auto => self.weapon.auto.as_ref()?.cost_pct,
        };
        Some(self.tu_max * pct / 100)
    }

    /// Hit chance in percent, clamped to 5..=95 so nothing is ever certain.
    /// None when the weapon lacks the mode. Kneeling grants +15%.
    pub fn hit_chance(&self, mode: FireMode) -> Option<i32> {
        let mode_acc = match mode {
            FireMode::Snap => self.weapon.snap_acc,
            FireMode::Aimed => self.weapon.aimed_acc,
            FireMode::Auto => self.weapon.auto.as_ref()?.acc,
        };
        let mut chance = self.accuracy * mode_acc / 100;
        if self.kneeling {
            chance = chance * 115 / 100;
        }
        Some(chance.clamp(5, 95))
    }

    pub fn rounds_per_action(&self, mode: FireMode) -> u32 {
        match mode {
            FireMode::Auto => self.weapon.auto.as_ref().map_or(1, |a| a.rounds),
            _ => 1,
        }
    }
}

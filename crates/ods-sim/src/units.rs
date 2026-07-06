//! Units and weapons. Stats follow the original game's ranges; there are no
//! classes — soldiers differentiate by what happens to them (progression
//! arrives with the campaign layer).

use glam::IVec3;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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
}

pub fn rifle() -> Weapon {
    Weapon {
        name: "consecrated rifle",
        power: 30,
        snap_cost_pct: 25,
        aimed_cost_pct: 50,
        snap_acc: 60,
        aimed_acc: 110,
        auto: Some(AutoFire { cost_pct: 35, acc: 35, rounds: 3 }),
        breach_radius: 1.6,
    }
}

pub fn hellspit() -> Weapon {
    Weapon {
        name: "hellspit",
        power: 18,
        snap_cost_pct: 30,
        aimed_cost_pct: 60,
        snap_acc: 55,
        aimed_acc: 100,
        auto: None,
        breach_radius: 1.2,
    }
}

#[derive(Clone, Debug)]
pub struct Unit {
    pub id: UnitId,
    pub side: Side,
    pub name: String,
    pub tile: IVec3,
    pub tu_max: i32,
    pub tu: i32,
    pub health_max: i32,
    pub health: i32,
    /// 0..=100; low morale risks panic or berserk at turn start.
    pub morale: i32,
    pub reactions: i32,
    pub accuracy: i32,
    pub bravery: i32,
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
    pub fn soldier(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            id: UnitId(id),
            side: Side::Order,
            name: name.to_string(),
            tile,
            tu_max: 55,
            tu: 55,
            health_max: 32,
            health: 32,
            morale: 100,
            reactions: 50,
            accuracy: 60,
            bravery: 30,
            weapon: rifle(),
            wounds: 0,
            grenades: 2,
            heal_charges: 3,
            alive: true,
        }
    }

    pub fn imp(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            id: UnitId(id),
            side: Side::Demons,
            name: name.to_string(),
            tile,
            tu_max: 45,
            tu: 45,
            health_max: 18,
            health: 18,
            morale: 100,
            reactions: 40,
            accuracy: 45,
            bravery: 50,
            weapon: hellspit(),
            wounds: 0,
            grenades: 0,
            heal_charges: 0,
            alive: true,
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
    /// None when the weapon lacks the mode.
    pub fn hit_chance(&self, mode: FireMode) -> Option<i32> {
        let mode_acc = match mode {
            FireMode::Snap => self.weapon.snap_acc,
            FireMode::Aimed => self.weapon.aimed_acc,
            FireMode::Auto => self.weapon.auto.as_ref()?.acc,
        };
        Some((self.accuracy * mode_acc / 100).clamp(5, 95))
    }

    pub fn rounds_per_action(&self, mode: FireMode) -> u32 {
        match mode {
            FireMode::Auto => self.weapon.auto.as_ref().map_or(1, |a| a.rounds),
            _ => 1,
        }
    }
}

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
    /// 0..=100; low morale risks panic at turn start.
    pub morale: i32,
    pub reactions: i32,
    pub accuracy: i32,
    pub bravery: i32,
    pub weapon: Weapon,
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
            alive: true,
        }
    }

    pub fn fire_cost(&self, mode: FireMode) -> i32 {
        let pct = match mode {
            FireMode::Snap => self.weapon.snap_cost_pct,
            FireMode::Aimed => self.weapon.aimed_cost_pct,
        };
        self.tu_max * pct / 100
    }

    /// Hit chance in percent, clamped to 5..=95 so nothing is ever certain.
    pub fn hit_chance(&self, mode: FireMode) -> i32 {
        let mode_acc = match mode {
            FireMode::Snap => self.weapon.snap_acc,
            FireMode::Aimed => self.weapon.aimed_acc,
        };
        (self.accuracy * mode_acc / 100).clamp(5, 95)
    }
}

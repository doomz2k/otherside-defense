//! Data-driven balance: weapon and species tables parsed from RON.
//!
//! The tables ship embedded in the binary, but a file at `./data/<name>.ron`
//! beside the executable overrides them at startup — the modding hook.

use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct AutoDef {
    pub cost_pct: i32,
    pub acc: i32,
    pub rounds: u32,
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct WeaponDef {
    pub power: i32,
    pub snap_cost_pct: i32,
    pub aimed_cost_pct: i32,
    pub snap_acc: i32,
    pub aimed_acc: i32,
    pub auto: Option<AutoDef>,
    pub breach_radius: f32,
    pub melee: bool,
    pub arcing: bool,
    #[serde(default)]
    pub silent: bool,
    #[serde(default)]
    pub fire_cone: bool,
    #[serde(default)]
    pub stun_power: i32,
}

#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct SpeciesDef {
    pub tu: i32,
    pub health: i32,
    pub reactions: i32,
    pub accuracy: i32,
    pub bravery: i32,
    pub armor: (i32, i32, i32),
}

fn load<T: serde::de::DeserializeOwned>(file: &str, embedded: &str) -> T {
    let text = std::fs::read_to_string(format!("data/{file}")).unwrap_or_else(|_| embedded.to_string());
    ron::from_str(&text).unwrap_or_else(|e| panic!("bad {file}: {e}"))
}

pub fn weapons() -> &'static HashMap<String, WeaponDef> {
    static TABLE: OnceLock<HashMap<String, WeaponDef>> = OnceLock::new();
    TABLE.get_or_init(|| load("weapons.ron", include_str!("../data/weapons.ron")))
}

pub fn species() -> &'static HashMap<String, SpeciesDef> {
    static TABLE: OnceLock<HashMap<String, SpeciesDef>> = OnceLock::new();
    TABLE.get_or_init(|| load("species.ron", include_str!("../data/species.ron")))
}

#[cfg(test)]
mod tests {
    #[test]
    fn embedded_tables_parse_and_cover_the_roster() {
        let w = super::weapons();
        for name in [
            "rifle", "hellspit", "bile_lob", "fangs", "taking_claws", "dead_hands",
            "stone_talons", "crushing_fists", "bare_hands", "hellfire_lance",
        ] {
            assert!(w.contains_key(name), "missing weapon {name}");
        }
        assert_eq!(w["rifle"].power, 30);
        let s = super::species();
        for name in [
            "soldier", "imp", "overseer", "hellhound", "bile_wisp", "taker",
            "husk", "prince", "gargoyle", "behemoth", "civilian",
        ] {
            assert!(s.contains_key(name), "missing species {name}");
        }
        assert_eq!(s["behemoth"].armor.0, 8);
    }
}

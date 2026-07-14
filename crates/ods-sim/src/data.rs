//! Data-driven balance: weapon and species tables parsed from RON.
//!
//! The tables ship embedded in the binary. Two modding hooks stack on top
//! at startup:
//!   1. `./data/<name>.ron` beside the executable REPLACES a whole table.
//!   2. every `./mods/<mod>/<name>.ron` OVERLAYS entries onto the result,
//!      in alphabetical mod order — a mod ships only the keys it changes.

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
    /// Rounds per magazine; 0 = self-powered (claws, hellspit, blades).
    #[serde(default)]
    pub clip: u32,
    /// Part of the creature that wields it: never dropped, never salvaged.
    #[serde(default)]
    pub natural: bool,
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

fn load<T: serde::de::DeserializeOwned>(file: &str, embedded: &str) -> HashMap<String, T> {
    let text = std::fs::read_to_string(format!("data/{file}")).unwrap_or_else(|_| embedded.to_string());
    let mut table: HashMap<String, T> =
        ron::from_str(&text).unwrap_or_else(|e| panic!("bad {file}: {e}"));
    // Mods overlay, alphabetically, later mods winning contested keys.
    let mut mods: Vec<std::path::PathBuf> = std::fs::read_dir("mods")
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .collect();
    mods.sort();
    for dir in mods {
        let path = dir.join(file);
        if let Ok(text) = std::fs::read_to_string(&path) {
            match apply_overlay(&mut table, &text) {
                Ok(n) => eprintln!("mods: {} overlays {n} entr(ies) of {file}", dir.display()),
                Err(e) => eprintln!("mods: BAD {} — ignored: {e}", path.display()),
            }
        }
    }
    table
}

/// Merge a RON table fragment onto an existing table. Returns how many
/// entries the fragment carried.
fn apply_overlay<T: serde::de::DeserializeOwned>(
    table: &mut HashMap<String, T>,
    text: &str,
) -> Result<usize, ron::error::SpannedError> {
    let overlay: HashMap<String, T> = ron::from_str(text)?;
    let n = overlay.len();
    table.extend(overlay);
    Ok(n)
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
    fn mod_overlays_replace_only_what_they_name() {
        let mut table = super::load::<super::WeaponDef>(
            "weapons.ron",
            include_str!("../data/weapons.ron"),
        );
        let stock_hellspit = table["hellspit"].power;
        let n = super::apply_overlay(
            &mut table,
            r#"{ "rifle": (power: 99, snap_cost_pct: 25, aimed_cost_pct: 50,
                 snap_acc: 60, aimed_acc: 110, auto: None,
                 breach_radius: 0.0, melee: false, arcing: false) }"#,
        )
        .unwrap();
        assert_eq!(n, 1);
        assert_eq!(table["rifle"].power, 99, "the named key is overridden");
        assert_eq!(table["hellspit"].power, stock_hellspit, "the rest survive");
        // A broken fragment reports instead of corrupting the table.
        assert!(super::apply_overlay::<super::WeaponDef>(&mut table, "not ron").is_err());
        assert_eq!(table["rifle"].power, 99);
    }

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

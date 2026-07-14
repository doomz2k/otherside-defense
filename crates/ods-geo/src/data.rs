//! Campaign balance tunables, moddable the same way as the sim's tables:
//! drop an edited `data/economy.ron` beside the executable (or in a mod
//! folder under `mods/<name>/economy.ron`) to override any subset. Every
//! field defaults to the shipped balance, so overlays stay tiny.

use std::sync::OnceLock;

#[derive(Clone, Debug, serde::Deserialize)]
#[serde(default)]
pub struct Economy {
    pub soldier_hire: i64,
    pub soldier_salary: i64,
    pub occultist_hire: i64,
    pub occultist_salary: i64,
    pub artificer_hire: i64,
    pub artificer_salary: i64,
    pub chapterhouse: i64,
    pub zeppelin: i64,
    pub store_base_cap: u32,
    pub store_vault_cap: u32,
}

impl Default for Economy {
    fn default() -> Self {
        Self {
            soldier_hire: crate::campaign::SOLDIER_HIRE_COST,
            soldier_salary: crate::campaign::SOLDIER_SALARY,
            occultist_hire: crate::campaign::OCCULTIST_HIRE_COST,
            occultist_salary: crate::campaign::OCCULTIST_SALARY,
            artificer_hire: crate::campaign::ARTIFICER_HIRE_COST,
            artificer_salary: crate::campaign::ARTIFICER_SALARY,
            chapterhouse: crate::campaign::CHAPTERHOUSE_COST,
            zeppelin: crate::campaign::ZEPPELIN_COST,
            store_base_cap: crate::campaign::STORE_BASE_CAP,
            store_vault_cap: crate::campaign::STORE_VAULT_CAP,
        }
    }
}

/// The active table: shipped defaults, then `data/economy.ron`, then each
/// mod folder alphabetically (later mods winning).
pub fn economy() -> &'static Economy {
    static TABLE: OnceLock<Economy> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut eco = Economy::default();
        let mut sources: Vec<std::path::PathBuf> =
            vec![std::path::PathBuf::from("data/economy.ron")];
        let mut mods: Vec<std::path::PathBuf> = std::fs::read_dir("mods")
            .into_iter()
            .flatten()
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        mods.sort();
        sources.extend(mods.into_iter().map(|d| d.join("economy.ron")));
        for path in sources {
            if let Ok(text) = std::fs::read_to_string(&path) {
                match ron::from_str::<Economy>(&text) {
                    Ok(over) => {
                        eprintln!("mods: {} overrides the economy", path.display());
                        eco = over;
                    }
                    Err(e) => eprintln!("mods: BAD {} — ignored: {e}", path.display()),
                }
            }
        }
        eco
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_defaults_match_the_shipped_balance() {
        let eco = economy();
        assert_eq!(eco.soldier_hire, crate::campaign::SOLDIER_HIRE_COST);
        assert_eq!(eco.zeppelin, crate::campaign::ZEPPELIN_COST);
        // An overlay fragment can override a single field.
        let over: Economy = ron::from_str("(soldier_hire: 99)").unwrap();
        assert_eq!(over.soldier_hire, 99);
        assert_eq!(over.soldier_salary, crate::campaign::SOLDIER_SALARY);
    }
}

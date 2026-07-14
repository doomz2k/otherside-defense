//! Player preferences, persisted beside the executable — volume, camera,
//! pacing, pixel scale, CRT dressing, and rebound keys.

use winit::keyboard::KeyCode;

pub const CONFIG_PATH: &str = "otherside-config.json";

fn one() -> f32 {
    1.0
}

fn yes() -> bool {
    true
}

#[derive(serde::Serialize, serde::Deserialize)]
pub struct Config {
    pub volume: f32,
    #[serde(default = "one")]
    pub music_volume: f32,
    #[serde(default = "one")]
    pub sfx_volume: f32,
    #[serde(default = "one")]
    pub ambient_volume: f32,
    /// UI zoom factor, 0.8..=1.4.
    #[serde(default = "one")]
    pub ui_scale: f32,
    pub cam_sense: f32,
    pub anim_speed: f32,
    pub pixel_scale: u32,
    pub crt: bool,
    /// Pan the battle camera to visible demon action during their turn.
    #[serde(default)]
    pub event_cam: bool,
    /// First-encounter hints in the log (the guided first month).
    #[serde(default = "yes")]
    pub hints: bool,
    /// Colorblind-safe overlays: orange/blue instead of red/green.
    #[serde(default)]
    pub colorblind: bool,
    /// Damp screen flashes and pulses.
    #[serde(default)]
    pub reduce_flash: bool,
    /// Only the bindings that differ from default: (action label, key name).
    #[serde(default)]
    pub binds: Vec<(String, String)>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            volume: 1.0,
            music_volume: 1.0,
            sfx_volume: 1.0,
            ambient_volume: 1.0,
            ui_scale: 1.0,
            cam_sense: 1.0,
            anim_speed: 1.0,
            pixel_scale: 3,
            crt: false,
            event_cam: true,
            hints: true,
            colorblind: false,
            reduce_flash: false,
            binds: Vec::new(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        std::fs::read_to_string(CONFIG_PATH)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(CONFIG_PATH, s);
        }
    }
}

/// One rebindable action: what it's called, what it ships as, what the
/// player moved it to.
#[derive(Clone, Copy)]
pub struct Bind {
    pub label: &'static str,
    pub default: KeyCode,
    pub current: KeyCode,
}

/// The battle actions a player may move (camera keys stay put).
pub fn default_binds() -> Vec<Bind> {
    use KeyCode as K;
    [
        ("End turn", K::Space),
        ("Next soldier", K::Tab),
        ("Kneel", K::KeyK),
        ("Arm charge", K::KeyG),
        ("Dress wounds", K::KeyH),
        ("Pop smoke", K::KeyV),
        ("Open door", K::KeyO),
        ("Bind demon", K::KeyB),
        ("Carry body", K::KeyU),
        ("Scavenge", K::KeyJ),
        ("Amputate", K::KeyX),
        ("Inscribe ward", K::KeyR),
        ("Rally", K::KeyY),
        ("Threat overlay", K::KeyT),
        ("Tactical map", K::KeyM),
        ("Floor cutaway", K::KeyF),
    ]
    .into_iter()
    .map(|(label, default)| Bind { label, default, current: default })
    .collect()
}

/// Route a physical key through the player's bindings: the battle screen
/// always thinks in defaults. None means the key is bound away entirely.
pub fn translate(binds: &[Bind], code: KeyCode) -> Option<KeyCode> {
    if let Some(b) = binds.iter().find(|b| b.current == code) {
        return Some(b.default);
    }
    if binds.iter().any(|b| b.default == code && b.current != code) {
        return None; // its old meaning moved elsewhere
    }
    Some(code)
}

/// The keys a binding may be moved to (and the name<->code dictionary).
pub const REBINDABLE: &[KeyCode] = &[
    KeyCode::KeyA, KeyCode::KeyB, KeyCode::KeyC, KeyCode::KeyD, KeyCode::KeyE,
    KeyCode::KeyF, KeyCode::KeyG, KeyCode::KeyH, KeyCode::KeyI, KeyCode::KeyJ,
    KeyCode::KeyK, KeyCode::KeyL, KeyCode::KeyM, KeyCode::KeyN, KeyCode::KeyO,
    KeyCode::KeyP, KeyCode::KeyQ, KeyCode::KeyR, KeyCode::KeyS, KeyCode::KeyT,
    KeyCode::KeyU, KeyCode::KeyV, KeyCode::KeyW, KeyCode::KeyX, KeyCode::KeyY,
    KeyCode::KeyZ, KeyCode::Digit0, KeyCode::Digit1, KeyCode::Digit2,
    KeyCode::Digit3, KeyCode::Digit4, KeyCode::Digit5, KeyCode::Digit6,
    KeyCode::Digit7, KeyCode::Digit8, KeyCode::Digit9, KeyCode::Space,
    KeyCode::Tab, KeyCode::Enter, KeyCode::Semicolon, KeyCode::Comma,
    KeyCode::Period, KeyCode::Slash, KeyCode::Quote, KeyCode::BracketLeft,
    KeyCode::BracketRight, KeyCode::Minus, KeyCode::Equal, KeyCode::F1,
    KeyCode::F2, KeyCode::F3, KeyCode::F4, KeyCode::F5, KeyCode::F6,
    KeyCode::F7, KeyCode::F8, KeyCode::F9, KeyCode::F10, KeyCode::F11,
    KeyCode::F12,
];

pub fn code_name(code: KeyCode) -> String {
    format!("{code:?}")
}

pub fn name_code(name: &str) -> Option<KeyCode> {
    REBINDABLE.iter().copied().find(|c| code_name(*c) == name)
}

/// Fold saved (label, key) pairs back onto the default table, ignoring
/// anything that no longer parses.
pub fn apply_saved(binds: &mut [Bind], saved: &[(String, String)]) {
    for (label, key) in saved {
        if let (Some(b), Some(code)) = (
            binds.iter_mut().find(|b| b.label == label),
            name_code(key),
        ) {
            b.current = code;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rebinding_routes_and_swallows() {
        let mut binds = default_binds();
        // Kneel moves from K to C.
        binds.iter_mut().find(|b| b.label == "Kneel").unwrap().current = KeyCode::KeyC;
        assert_eq!(translate(&binds, KeyCode::KeyC), Some(KeyCode::KeyK), "C now kneels");
        assert_eq!(translate(&binds, KeyCode::KeyK), None, "K means nothing anymore");
        assert_eq!(translate(&binds, KeyCode::KeyG), Some(KeyCode::KeyG), "others untouched");
    }

    #[test]
    fn key_names_round_trip_and_saved_binds_apply() {
        for &code in REBINDABLE {
            assert_eq!(name_code(&code_name(code)), Some(code));
        }
        let mut binds = default_binds();
        apply_saved(
            &mut binds,
            &[
                ("Kneel".to_string(), "KeyC".to_string()),
                ("Ghost action".to_string(), "KeyZ".to_string()), // unknown: ignored
                ("Rally".to_string(), "NotAKey".to_string()),     // unparsable: ignored
            ],
        );
        assert_eq!(binds.iter().find(|b| b.label == "Kneel").unwrap().current, KeyCode::KeyC);
        assert_eq!(binds.iter().find(|b| b.label == "Rally").unwrap().current, KeyCode::KeyY);
    }
}

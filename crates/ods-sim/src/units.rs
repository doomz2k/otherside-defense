//! Units and weapons. Stats follow the original game's ranges; there are no
//! classes — soldiers differentiate by what happens to them (progression
//! lives in the campaign layer). Demons differentiate by species: each breed
//! changes the tactical rules rather than just the numbers.

use glam::IVec3;

use crate::body::BodyPart;

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
    /// A lord of the Otherside: psi mastery up to full possession.
    Prince,
    /// Winged skirmisher: true flight, perches where it pleases.
    Gargoyle,
    /// A siege-beast that walks through walls and leaves rubble.
    Behemoth,
}

impl Species {
    /// Every demonic breed, in codex order.
    pub const DEMONS: [Species; 9] = [
        Species::Imp,
        Species::Hellhound,
        Species::BileWisp,
        Species::Overseer,
        Species::Gargoyle,
        Species::Taker,
        Species::Husk,
        Species::Behemoth,
        Species::Prince,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Species::Soldier => "Soldier",
            Species::Imp => "Imp",
            Species::Overseer => "Overseer",
            Species::Hellhound => "Hellhound",
            Species::BileWisp => "Bile Wisp",
            Species::Taker => "Taker",
            Species::Husk => "Husk",
            Species::Prince => "Prince",
            Species::Gargoyle => "Gargoyle",
            Species::Behemoth => "Behemoth",
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
    /// The balance-table key it was built from (for salvage bookkeeping).
    pub key: String,
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
    /// Silent weapons betray no position (no noise cue when fired).
    pub silent: bool,
    /// Fires a cone of burning ground instead of a projectile (the censer).
    pub fire_cone: bool,
    /// Deals stun instead of blood (the salt-shot mortar); 0 = normal.
    pub stun_power: i32,
    /// Rounds per magazine; 0 = self-powered (claws, hellspit, blades).
    pub clip: u32,
    /// Part of the creature that wields it: never dropped, never salvaged.
    pub natural: bool,
}

impl Weapon {
    /// Build a weapon from the balance tables (`crate::data`), keyed by its
    /// table name. `name` stays a display string; numerics come from data.
    pub fn from_data(name: &'static str, key: &str) -> Self {
        let d = crate::data::weapons()
            .get(key)
            .unwrap_or_else(|| panic!("weapons.ron is missing \"{key}\""));
        Self {
            name,
            key: key.to_string(),
            power: d.power,
            snap_cost_pct: d.snap_cost_pct,
            aimed_cost_pct: d.aimed_cost_pct,
            snap_acc: d.snap_acc,
            aimed_acc: d.aimed_acc,
            auto: d.auto.map(|a| AutoFire { cost_pct: a.cost_pct, acc: a.acc, rounds: a.rounds }),
            breach_radius: d.breach_radius,
            melee: d.melee,
            arcing: d.arcing,
            silent: d.silent,
            fire_cone: d.fire_cone,
            stun_power: d.stun_power,
            clip: d.clip,
            natural: d.natural,
        }
    }
}

pub fn rifle() -> Weapon {
    Weapon::from_data("consecrated rifle", "rifle")
}

pub fn hellspit() -> Weapon {
    Weapon::from_data("hellspit", "hellspit")
}

pub fn bile_lob() -> Weapon {
    Weapon::from_data("bile glob", "bile_lob")
}

pub fn hellfire_lance() -> Weapon {
    Weapon::from_data("hellfire lance", "hellfire_lance")
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
    /// Firing accuracy: the base of every ranged to-hit roll.
    pub accuracy: i32,
    pub bravery: i32,
    /// The body's battery: moving spends it, resting turns refill a third.
    /// Empty lungs make every step cost more.
    pub stamina_max: i32,
    pub stamina: i32,
    /// Carry capacity, throw range, and melee weight.
    pub strength: i32,
    /// Throwing accuracy: how honestly a lobbed charge lands.
    pub throwing: i32,
    /// Melee accuracy: blade-work, fangs, ripostes.
    pub melee: i32,
    /// Kneeling: +15% accuracy until the unit moves.
    pub kneeling: bool,
    /// Overwatch stance: TUs banked for this fire mode while moving, and
    /// reaction fire answers with it. None = spend freely, react snap.
    pub reserve: Option<FireMode>,
    /// Broken as a pack: running for where it came in, done fighting.
    pub routed: bool,
    /// It reached the way out and is gone — alive, uncounted, reporting.
    pub escaped: bool,
    /// Can use the Terrify psi attack.
    pub psi: bool,
    /// Can use full Possession (Princes).
    pub psi_master: bool,
    /// Facing (unit step vector, z ignored). Reactions only fire into the
    /// forward arc.
    pub facing: IVec3,
    /// Directional armor: flat damage soak by attack sector.
    pub armor_front: i32,
    pub armor_side: i32,
    pub armor_rear: i32,
    /// Incoming fire this enemy turn; degrades aim and reactions.
    pub suppression: i32,
    /// Crippled body parts (battle-local; the campaign turns them into
    /// longer convalescence).
    pub injuries: Vec<BodyPart>,
    /// Parts severed outright — gone for good, on the ground somewhere.
    pub severed: Vec<BodyPart>,
    /// Demonic rot in a crippled part: (part, turns festering). Amputate
    /// before it finishes, or the soldier stops being a soldier.
    pub infected: Option<(BodyPart, u32)>,
    /// This corpse has been eaten or defiled — no longer recoverable.
    pub consumed: bool,
    /// Death by overkill left nothing whole to bury (or to hatch from).
    pub gibbed: bool,
    /// Horrors witnessed this battle (gibs, Takings, atrocities...). The
    /// campaign turns these into lasting sanity damage.
    pub horror: u32,
    /// Turns remaining under a Prince's control (acts for the enemy).
    pub possessed: u32,
    /// Smoke grenades carried.
    pub smoke_grenades: u32,
    /// Ward kits: chalk-and-salt for one burning ward each.
    pub ward_kits: u32,
    /// A consecrated blade at the hip: free riposte against melee attacks.
    pub blade: bool,
    /// A warded circlet: shatters to block one psi assault.
    pub circlet: bool,
    /// Officers rally once per battle (set by the campaign from rank).
    pub can_rally: bool,
    /// The rally has been spent.
    pub rally_spent: bool,
    /// Non-combatant caught in the massacre (soldier-shaped, unarmed).
    pub civilian: bool,
    /// True flight: ignores floors, ramps, and drops.
    pub flies: bool,
    /// Walks through walls, demolishing them (Behemoths).
    pub smasher: bool,
    /// Unconscious ally being hauled (their tile follows the carrier).
    pub carrying: Option<UnitId>,
    pub weapon: Weapon,
    /// Rounds loaded in the weapon (meaningful only when its clip > 0).
    pub ammo: i32,
    /// Spare magazines on the belt — they feed whatever is in hand.
    pub mags: u32,
    /// A backup at the hip, drawn with [`crate::battle::Action::SwapWeapon`].
    pub sidearm: Option<Weapon>,
    pub sidearm_ammo: i32,
    /// Open fatal wounds; each bleeds 1 health at the unit's turn start.
    pub wounds: i32,
    /// Hellfire charges carried (thrown explosives).
    pub grenades: u32,
    /// Field-dressing uses left (staunches wounds, restores some health).
    pub heal_charges: u32,
    /// Witchfire flares: thrown light for the long nights.
    pub flares: u32,
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
            stamina_max: 50,
            stamina: 50,
            strength: 30,
            throwing: 45,
            melee: 40,
            kneeling: false,
            reserve: None,
            routed: false,
            escaped: false,
            psi: false,
            psi_master: false,
            facing: IVec3::new(-1, 0, 0),
            armor_front: 0,
            armor_side: 0,
            armor_rear: 0,
            suppression: 0,
            injuries: Vec::new(),
            severed: Vec::new(),
            infected: None,
            consumed: false,
            gibbed: false,
            horror: 0,
            possessed: 0,
            smoke_grenades: 0,
            ward_kits: 0,
            blade: false,
            circlet: false,
            can_rally: false,
            rally_spent: false,
            civilian: false,
            flies: false,
            smasher: false,
            carrying: None,
            weapon: hellspit(),
            ammo: 0,
            mags: 0,
            sidearm: None,
            sidearm_ammo: 0,
            wounds: 0,
            grenades: 0,
            heal_charges: 0,
            flares: 0,
            alive: true,
        }
    }

    /// Apply base stats from the species balance table (`crate::data`).
    fn stats(mut self, key: &str) -> Self {
        let d = crate::data::species()
            .get(key)
            .unwrap_or_else(|| panic!("species.ron is missing \"{key}\""));
        self.tu_max = d.tu;
        self.tu = d.tu;
        self.health_max = d.health;
        self.health = d.health;
        self.reactions = d.reactions;
        self.accuracy = d.accuracy;
        self.bravery = d.bravery;
        self.armor_front = d.armor.0;
        self.armor_side = d.armor.1;
        self.armor_rear = d.armor.2;
        // Demons fight with what they are: their table accuracy serves as
        // throwing and melee skill alike, so balance is unchanged.
        self.throwing = d.accuracy;
        self.melee = d.accuracy;
        self
    }

    /// Alive and conscious: able to act, react, and hold the field.
    pub fn is_active(&self) -> bool {
        self.alive && self.conscious
    }

    pub fn soldier(id: u32, name: &str, tile: IVec3) -> Self {
        let mut u = Self {
            weapon: rifle(),
            mags: 2,
            grenades: 2,
            heal_charges: 3,
            smoke_grenades: 1,
            ward_kits: 1,
            flares: 2,
            facing: IVec3::new(1, 0, 0),
            ..Self::base(id, Side::Order, Species::Soldier, name, tile)
        }
        .stats("soldier");
        u.ammo = u.weapon.clip as i32;
        u
    }

    /// An unarmed townsperson caught in the massacre.
    pub fn civilian(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            weapon: Weapon::from_data("bare hands", "bare_hands"),
            civilian: true,
            ..Self::base(id, Side::Order, Species::Soldier, name, tile)
        }
        .stats("civilian")
    }

    pub fn imp(id: u32, name: &str, tile: IVec3) -> Self {
        Self::base(id, Side::Demons, Species::Imp, name, tile).stats("imp")
    }

    pub fn overseer(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            psi: true,
            ..Self::base(id, Side::Demons, Species::Overseer, name, tile)
        }
        .stats("overseer")
    }

    pub fn gargoyle(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            weapon: Weapon::from_data("stone talons", "stone_talons"),
            flies: true,
            ..Self::base(id, Side::Demons, Species::Gargoyle, name, tile)
        }
        .stats("gargoyle")
    }

    pub fn behemoth(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            weapon: Weapon::from_data("crushing fists", "crushing_fists"),
            smasher: true,
            ..Self::base(id, Side::Demons, Species::Behemoth, name, tile)
        }
        .stats("behemoth")
    }

    /// A lord of the Otherside. Every Prince is a psi master.
    pub fn prince(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            psi: true,
            psi_master: true,
            ..Self::base(id, Side::Demons, Species::Prince, name, tile)
        }
        .stats("prince")
    }

    pub fn hellhound(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            weapon: Weapon::from_data("fangs", "fangs"),
            ..Self::base(id, Side::Demons, Species::Hellhound, name, tile)
        }
        .stats("hellhound")
    }

    pub fn bile_wisp(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            weapon: bile_lob(),
            ..Self::base(id, Side::Demons, Species::BileWisp, name, tile)
        }
        .stats("bile_wisp")
    }

    pub fn taker(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            weapon: Weapon::from_data("taking claws", "taking_claws"),
            ..Self::base(id, Side::Demons, Species::Taker, name, tile)
        }
        .stats("taker")
    }

    pub fn husk(id: u32, name: &str, tile: IVec3) -> Self {
        Self {
            weapon: Weapon::from_data("dead hands", "dead_hands"),
            ..Self::base(id, Side::Demons, Species::Husk, name, tile)
        }
        .stats("husk")
    }

    /// TU cost for a fire mode; None when the weapon lacks the mode.
    /// A clip-fed weapon with an empty chamber has nothing to say.
    pub fn has_shot(&self) -> bool {
        self.weapon.clip == 0 || self.ammo > 0
    }

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
        // Blades and fangs are melee-skill work; triggers are firing work.
        let skill = if self.weapon.melee { self.melee } else { self.accuracy };
        let mut chance = skill * mode_acc / 100;
        if self.kneeling {
            chance = chance * 115 / 100;
        }
        for injury in &self.injuries {
            chance = match injury {
                BodyPart::LeftArm | BodyPart::RightArm => chance * 70 / 100,
                BodyPart::Weapon => chance * 60 / 100,
                _ => chance,
            };
        }
        // A missing arm is worse than a mangled one.
        for lost in &self.severed {
            if matches!(lost, BodyPart::LeftArm | BodyPart::RightArm) {
                chance = chance * 60 / 100;
            }
        }
        chance = chance * (100 - (self.suppression * 5).min(30)) / 100;
        Some(chance.clamp(5, 95))
    }

    /// Flat damage soaked, judged by where the blow came from relative to
    /// the unit's facing.
    pub fn armor_against(&self, from: IVec3) -> i32 {
        let d = from - self.tile;
        let dir = glam::Vec2::new(d.x as f32, d.y as f32).normalize_or_zero();
        let face = glam::Vec2::new(self.facing.x as f32, self.facing.y as f32).normalize_or_zero();
        let dot = dir.dot(face);
        if dot >= 0.38 {
            self.armor_front
        } else if dot <= -0.38 {
            self.armor_rear
        } else {
            self.armor_side
        }
    }

    /// Movement cost multiplier from leg damage: crippled legs hobble,
    /// severed legs reduce a soldier to a crawl.
    pub fn move_cost_mult(&self) -> i32 {
        // Spent lungs: an exhausted walker pays double for every step.
        let winded = if self.stamina <= 0 && !self.flies { 1 } else { 0 };
        winded + if self
            .severed
            .iter()
            .any(|p| matches!(p, BodyPart::LeftLeg | BodyPart::RightLeg))
        {
            3
        } else if self
            .injuries
            .iter()
            .any(|p| matches!(p, BodyPart::LeftLeg | BodyPart::RightLeg))
        {
            2
        } else {
            1
        }
    }

    /// A body the field teams could still bring home (and demons could
    /// still put to their own uses).
    pub fn is_corpse(&self) -> bool {
        !self.alive && !self.gibbed && !self.consumed
    }

    pub fn rounds_per_action(&self, mode: FireMode) -> u32 {
        match mode {
            FireMode::Auto => self.weapon.auto.as_ref().map_or(1, |a| a.rounds),
            _ => 1,
        }
    }
}

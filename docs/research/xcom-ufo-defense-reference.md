# X-COM: UFO Defense (UFO: Enemy Unknown, 1994) — Design Reference

This is the source-material bible for **Otherside Defense**. It documents how the
original game works, system by system, so we can decide what to keep, adapt, or
drop. See `docs/design/homage-translation.md` for how each system maps onto our
demonic setting.

---

## 1. Overview & History

- Developed by **Mythos Games** (Julian & Nick Gollop) and published by
  **MicroProse** in March 1994. Known as *UFO: Enemy Unknown* in Europe and
  *X-COM: UFO Defense* in North America.
- Originally conceived as a sequel to *Laser Squad* (1988). MicroProse pushed
  Gollop to add a strategic layer: a modern-day Earth setting, a world map, and
  the ability to capture and reverse-engineer alien technology.
- Development took ~30 months and cost about £115,000. The game was at one
  point officially cancelled by parent company Spectrum HoloByte; MicroProse UK
  management quietly ignored the order and let the Gollops finish it.
- The core loop that made it a classic: **two interlocking games** — a
  real-time strategic layer (Geoscape) that generates and gives stakes to a
  turn-based tactical layer (Battlescape), with research/economy feeding back
  into both.

## 2. Game Structure

The game begins on **January 1, 1999**. The player commands X-COM, a secret
international paramilitary organisation funded by 16 nations.

Two views:

1. **Geoscape** — a rotating 3D globe with time compression (5 sec to 1 day per
   tick). Shows X-COM bases, craft, detected UFOs, alien bases, terror sites.
   Everything strategic happens here: interception, base building, research,
   manufacturing, hiring, purchasing, monthly reports.
2. **Battlescape** — isometric, tile-based, turn-based tactical combat, entered
   whenever soldiers deploy to a mission site. Fully destructible terrain,
   line-of-sight fog of war, individual soldiers with persistent stats.

The player loses if: they run a debt over $1M for two consecutive months, score
"badly losing" ratings two months in a row, lose all bases, or fail the final
mission. Victory requires researching the alien command chain (captured live
Leader/Commander), locating the alien HQ at **Cydonia on Mars**, and winning the
two-stage final assault.

## 3. Geoscape (Strategic Layer)

### 3.1 Funding & Score
- **16 funding nations** provide roughly **$6M/month** combined at game start.
- Each month ends with a report: nations individually raise or lower funding
  based on the score (alien activity vs. X-COM results) in their region.
- Score accrues from missions won, UFOs shot down, aliens killed/captured, and
  research; it drains from UFO activity, ignored terror sites, dead civilians,
  and standing alien bases (an alien base bleeds ~5 points/day from a region).
- Nations can be **infiltrated**: the aliens sign a secret pact, the nation's
  funding drops to zero permanently, and an alien base spawns. This is the
  slow-strangulation loss vector.

### 3.2 Detection & Interception
- Bases carry radar (Small/Large/Hyper-Wave Decoder). Radar gives a per-tick
  detection chance inside its radius; the Hyper-Wave Decoder (researched)
  detects everything in range **and** reveals the UFO's race, mission, and
  destination.
- Detected UFOs are intercepted by craft armed with purchasable/manufactured
  weapons. Interception is a semi-interactive minigame: choose standoff /
  cautious / standard / aggressive postures; weapons trade range, damage, and
  reload rate.
- Shot-down UFOs create **crash sites** (ground mission, damaged loot); UFOs
  that land intact create **landing sites** (harder fight, pristine loot,
  intact power sources full of Elerium).

### 3.3 Alien Missions (the invisible director)
Each month the alien strategy layer generates missions per region:
- **Research** — scouts survey a region (small UFOs, easy pickings).
- **Harvest / Abduction** — cattle/human abduction runs; landed UFOs are
  lootable cash cows.
- **Infiltration** — a large fleet visits a country; on success the country
  defects permanently and a base is built.
- **Base Construction** — ships land quietly; ~50% chance a new alien base
  spawns if uninterrupted.
- **Terror** — a Terror Ship unloads aliens + terror units into a city.
- **Retaliation** — triggered by shooting down UFOs: the same race hunts for
  the offending X-COM base and eventually assaults it (Base Defense mission).
- **Supply** — standing alien bases get nightly supply runs (~6%/night per
  base), which are farmable by the player.

This gives the strategic layer a readable rhythm: escalating UFO waves with
purpose behind them, which the player learns to interpret and counter.

## 4. Battlescape (Tactical Layer)

### 4.1 Turn & Action System
- Square-tile isometric maps with multiple elevation levels; player turn then
  alien turn, plus hidden civilian movement on terror missions.
- Every unit has **Time Units (TUs)** refreshed each turn. Everything costs
  TUs: moving one tile, turning, kneeling (kneel improves accuracy ~15%),
  opening doors, picking items up, firing, reloading, priming grenades,
  throwing them.
- Action costs are mostly **percentages of max TUs** for firing (so fast
  soldiers shoot more often), flat values for movement/inventory.
- **Energy/stamina** limits sustained running; **health + fatal wounds**
  (per-body-part bleeding that needs a medikit) model injury; heavily wounded
  soldiers spend weeks recovering back at base.

### 4.2 Shooting
- Three fire modes: **Aimed** (high TU, high accuracy), **Snap** (cheap,
  medium), **Auto** (three-round burst, low accuracy per shot, best hits/TU at
  close range). Not all weapons have all modes.
- Hit chance = soldier Firing Accuracy × weapon mode accuracy × modifiers
  (kneeling bonus, one-handed penalty, wound penalty). Misses travel along a
  deviated ballistic line and hit whatever is in the way — walls, cover,
  civilians, teammates. There is no to-hit dice-roll abstraction: every shot is
  a physical projectile in the world.
- **Reaction fire**: units keep unspent TUs; an "initiative" score
  (Reactions × %TUs-remaining) is compared against the moving enemy — win and
  you take snap shots during their turn. Overwatch emerges from the TU economy
  rather than being a button.

### 4.3 Damage, Armor & Terrain
- Damage types: Armor-Piercing, Laser, Plasma, High-Explosive, Incendiary,
  Stun, Melee, Acid. Armor has **separate values per facing** (front, sides,
  rear, under) — flanking matters mechanically.
- Damage rolls are wildly swingy (0–200% of listed power), which keeps even
  outclassed weapons and outclassed enemies scary.
- Terrain is **destructible**: walls breach, buildings collapse, fires spread,
  smoke blocks sight. Explosives are terrain-editing tools as much as weapons.
- **Stun** is a parallel non-lethal track (stun rod, small launcher): unconscious
  aliens can be captured, and live captures gate most of the important research.

### 4.4 Morale, Panic & Psionics
- Every unit has Morale (starts 100). Taking wounds and seeing friendlies die
  drains it (officers dying hurts more; higher Bravery drains slower).
- Low morale rolls: **panic** (drop weapon, flee, freeze) or **berserk** (fire
  wildly). Applies to aliens too — breaking enemy morale is a real tactic.
- **Psionics** is the late-game mind war. Psi attacks (Panic Unit / Mind
  Control) work through walls at any distance with no line of sight. Attack
  strength = (Psi Strength × Psi Skill / 50) − distance/2 + random(0–55) vs.
  defense = Psi Strength + Psi Skill/5. Mind Control is ~3× harder than Panic.
  Soldiers train Psi Skill in Psi Labs (a month reveals psi stats, 16–24 skill);
  low-Psi-Strength soldiers are permanent liabilities the enemy will puppet.

### 4.5 Information & Atmosphere
- Fog of war with true line-of-sight; aliens have wider vision at night unless
  the player uses electro-flares or waits for daylight. Night terror missions
  are the game's signature horror set pieces.
- Motion scanner shows blips through walls; Mind Probe reads alien stats.
- Mission ends when all enemies are dead/unconscious, the player aborts (units
  in the exit zone extract; the rest are lost), or all soldiers die. Loot from
  the field is recovered afterwards, including everything aliens dropped.

## 5. Soldiers

- Recruits arrive with **randomized stats**: Time Units, Stamina, Health,
  Bravery, Reactions, Firing Accuracy, Throwing Accuracy, Strength (carry
  weight), plus hidden Psi Strength / Psi Skill until tested.
- **Stats improve by doing**: firing accuracy by landing hits, reactions by
  taking reaction shots, TUs/stamina by spending them, bravery by surviving
  panic. No classes, no skill trees — soldiers differentiate organically.
- Ranks (Rookie → Commander) come from squad-size quotas and affect morale:
  losing officers tanks squad morale.
- Death is **permanent**. Named soldiers with 30-mission careers die to one
  plasma bolt from the dark. This permadeath + organic growth is the emotional
  engine of the whole game.
- Heavily wounded survivors spend days–weeks in sick bay, forcing roster depth.
- **HWPs** (Heavy Weapons Platforms — tanks) are expendable robotic units that
  take 4 soldier slots on the transport: scouts, door-openers, morale-immune
  fire support that costs money instead of grief.

## 6. Research, Manufacturing & Economy

- **Research** consumes scientists (hired, salaried, housed in Living Quarters,
  working in Laboratories, 50 per lab). Topics come from recovered artifacts,
  corpses, and **live captures** — interrogations of Navigators/Engineers/
  Medics/Leaders unlock UFO data; a live **Leader or Commander** is required to
  unlock the final mission chain (Martian Solution → Cydonia or Bust).
- Rough tech arc: ballistics → **lasers** (no ammo!) → **plasma** (alien tech
  turned back on them) → **fusion**; armor: none → Personal Armor → Power Suit
  → Flying Suit; craft: Interceptor/Skyranger → Firestorm → Lightning →
  **Avenger** (the ship that flies to Mars).
- **Elerium-115** is the unobtainium: fuel for advanced craft, ammo for plasma
  and fusion, cannot be manufactured — only looted from intact UFO power
  sources. It's the scarcity that makes shooting UFOs down carefully (and
  raiding landed ones) matter forever.
- **Manufacturing** consumes engineers + workshop space + materials + time.
  Famously, some products (Laser Cannons, Fusion Ball Launchers) sell above
  cost, letting a workshop-heavy base run as a factory — a beloved exploit
  worth taming, not deleting, in a homage.
- Everything is sellable; alien corpses and surplus plasma rifles are a major
  income stream. Monthly costs: salaries, base maintenance, craft upkeep.

## 7. Bases

- Up to 8 bases placed anywhere on the globe. Built on a **6×6 grid** of
  facilities connected by corridors: Access Lift (mandatory entry), Hangars
  (2×2, one per craft), Living Quarters, Labs, Workshops, General Stores,
  Alien Containment, Radar systems, missile/laser/plasma/fusion **defenses**,
  Grav Shield, Mind Shield (hides base from retaliation scans), Psi Labs.
- **The base layout is a tactical map.** When aliens assault, you fight a Base
  Defense battle inside your own floor plan; aliens enter via the Access Lift
  and Hangars. Players learn to design chokepoints — base building doubles as
  fortress design.
- Typical mature setup: one main combat base plus radar/interception satellite
  bases and dedicated factory/lab bases.

## 8. The Bestiary

Races (each with Soldier/Navigator/Medic/Engineer/Leader/Commander ranks):

| Race | Role | Signature traits |
|---|---|---|
| **Sectoid** | early grunt | weak, grey-alien archetype; Leaders/Commanders have psionics |
| **Floater** | early grunt | cybernetic torso, flies; no psi; weak but mobile |
| **Snakeman** | mid-game | tough, slow reptilians; brutal on terror missions |
| **Muton** | late grunt | elite shock troops, very tough, immune to interrogation about strategy |
| **Ethereal** | late leader | frail robed psi-masters; every one has psionics; the true commanders |
| **Celatid / Silacoid** | Muton terror pair | floating acid-spitter / molten crawler that ignites terrain |
| **Reaper** | Floater terror unit | huge bite melee beast, bullet-spongy fur |
| **Cyberdisc** | Sectoid terror unit | flying disc, heavy plasma, explodes on death |
| **Sectopod** | Ethereal terror unit | walking tank, laser-resistant armor |
| **Chryssalid** | Snakeman terror unit | THE horror icon: fast melee one-hit kill that implants victims — the zombie stands up and births a new Chryssalid |
| **Zombie** | Chryssalid byproduct | slow, but every one is a Chryssalid in waiting |

Design lesson: each race pairs a rank ladder (same body, scaling stats +
psionics at the top) with one **terror unit** that changes the tactical rules —
the bestiary is small but every entry forces a different answer.

## 9. UFO Roster

Small Scout → Medium Scout → Large Scout → Harvester → Abductor → Supply Ship →
Terror Ship → **Battleship**. Bigger hulls: more aliens, better loot, more
intact power sources (Elerium), and multi-floor interior fights. Battleships
run retaliation/infiltration and can outgun early interceptors.

## 10. Difficulty & Pacing

- Five levels (Beginner → Superhuman): higher levels add 20–100% more aliens
  per mission, faster/more accurate UFO weapons in interception, smarter psi
  use, and harsher scoring.
- Alien tech and race mix escalates by calendar month regardless of player
  progress — time pressure is real, and turtling loses via score/infiltration.
- The famous difficulty bug in the original DOS release reset every game to
  Beginner; generations of players who found it brutally hard were playing the
  easiest setting.

## 11. Why It Worked — Design Pillars for the Homage

1. **Two layers, one fate.** Strategic choices (where to base, what to
   research, which UFO to chase) become tactical situations, and tactical
   outcomes (who died, what was captured) rewrite the strategic game.
2. **Permadeath + organic growth** makes soldiers into stories. No classes;
   biography is the build.
3. **Simulation over abstraction**: physical bullets, destructible walls,
   per-facing armor, items dropped where units died. The world is consistent,
   so creative plans work.
4. **Scarcity with teeth**: Elerium and live captures force risky play styles
   (capture squads, raiding landed UFOs) that money can't replace.
5. **Readable escalation**: the invisible alien mission director creates waves
   the player learns to read, predict, and disrupt.
6. **Fear as a mechanic**: night missions, morale, Chryssalids, and psionics
   attack the player's composure, not just their hit points.
7. **Losing slowly is possible**: funding, infiltration, and score create a
   long defeat spiral the player can feel and fight against.

---

## Sources

- [UFO: Enemy Unknown — Wikipedia](https://en.wikipedia.org/wiki/UFO:_Enemy_Unknown)
- [UFOpaedia.org](https://www.ufopaedia.org/) — Geoscape, Battlescape, Stats,
  Psionics/Psionic Equations, Research Trees, Base Facilities (EU), UFOs,
  Alien Missions, Shot Types, Manufacturing Profitability, Difficulty Levels,
  Council of Funding Nations, Terror Mission, Alien Base Assault pages
- [XCOM: UFO Defense — XCOM Wiki (Fandom)](https://xcom.fandom.com/wiki/XCOM:_UFO_Defense)
- [X-COM: UFO Defense — TV Tropes](https://tvtropes.org/pmwiki/pmwiki.php/VideoGame/XComUfoDefense)
- [MCV: History Lesson — the story of XCOM](https://mcvuk.com/business-news/media-pr/history-lesson-the-story-of-xcom/)
- [StrategyCore — Alien Races & Weapons databank](https://www.strategycore.co.uk/databank/games/ufo-enemy-unknown/)
- [GameFAQs strategy guides (kschang77, Csabi_B)](https://gamefaqs.gamespot.com/pc/199362-x-com-ufo-defense/faqs)
- [Official game manual (PDF, Steam)](http://cdn.akamai.steamstatic.com/steam/apps/7760/manuals/x-com%20ufo%20defense%20manual.pdf)

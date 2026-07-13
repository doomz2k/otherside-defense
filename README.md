# otherside-defense

A turn-based demon-fighting strategy game based roughly on the old UFO defense.

Demonic forces pour through rifts from the Otherside; you command the secret
organisation that detects incursions, fights them on the ground, and turns
hell's own weapons back against it.

## Docs

- `docs/research/xcom-ufo-defense-reference.md` — how the 1994 original works,
  system by system.
- `docs/design/homage-translation.md` — how each system maps onto our setting.
- `docs/design/tech-stack.md` — stack decisions, architecture, milestones.

## Building

Rust stable (1.85+ for edition 2024). Custom voxel engine on wgpu.

```sh
cargo test --workspace                # headless: voxel core + sim + campaign tests
cargo run -p ods-app                  # the game (needs a display + GPU)
cargo run -p ods-app -- --headless    # sim-only smoke test (CI / cloud)
cargo run -p ods-app -- --campaign 6  # 6-month Geoscape chronicle, fully headless
```

The app opens on a main menu: **New campaign** (full Geoscape: manage the
chapterhouse, advance days, and answer rifts — lead any assault yourself in
the 3D Battlescape or hand it to the auto-resolver), **Load campaign**
(campaigns save to `otherside-save.json`, RNG state included, so a loaded
game continues the same timeline), or **Quick skirmish**.

## The first skirmish

Four Order soldiers vs four imps in a ruined chapel yard. Fully destructible
voxel terrain: misses chip walls, and a breached wall changes line of sight
and pathing.

| Input | Action |
|---|---|
| Hover | move cost preview, reachable tiles, hit odds vs demons |
| Left click soldier / ground / demon | select / move / fire |
| `1` / `2` / `3` | snap / aimed / auto fire mode |
| `G` then click | throw a hellfire charge (arcs over walls) |
| `B` | bind: stun an adjacent demon with the rod (captures!) |
| `K` | kneel (+15% accuracy until you move) |
| `H` | field-dress the selected soldier (staunch bleeding) |
| `V` / `O` | pop smoke ahead / open an adjacent door |
| `U` / `J` | carry a fallen comrade / scavenge a weapon |
| `F` | floor cutaway (see inside ground-floor interiors) |
| Tab | next soldier |
| Space or Enter | end turn (demons play) |
| Right-drag / scroll / WASD | orbit / zoom / pan camera |
| Esc | disarm charge / deselect |

Damage numbers, misses, terror, and crippled parts float up from the field;
night fights sink into cold blue lit only by muzzle flash and open flame.

## The bestiary

Imps swarm; **Overseers** whisper Terrify through walls; **Hellhounds** charge
on 70 TU; **Bile-wisps** lob acid over cover; **Gargoyles** fly true and perch
where they please; **Behemoths** walk straight through walls; **Princes**
possess minds outright; and the **Taker** kills soldiers into Husks that stand
up on the wrong side — and hatch a fresh Taker when destroyed. Packs escalate
by campaign month. Every creature is drawn as a voxel figure assembled from
named body parts (head, torso, each limb, horns, maws, tails — declared in
`ods-sim`'s anatomy, built in `ods-app/figures.rs`); heavy hits cripple those
same parts (arms spoil aim, legs slow movement, headshots stun), and wounds
that never heal right become permanent scars on the roster. The in-game
**Bestiary** fills in lore for every breed met — take one alive and the
occultists open it up, anatomy and all.

All weapon and species numbers live in RON tables
(`crates/ods-sim/data/*.ron`), embedded at build time and overridable by
dropping edited copies in `./data/` next to the executable — the modding hook.

## The horror of it

This is not a clean war. A crippled limb struck again is **severed** — gone
from the figure, gone from the roster, permanent until the workshop casts a
hellsteel limb or cuts a **flesh graft** from captured demon flesh (better
than what it replaces, and it costs the wearer sleep). Overkill **gibs**;
blood and viscera stain the voxel ground for the whole battle; corpses
persist — carry your dead home for burial honors, or watch demons *eat*
them and Takers raise them back up. Demon claws seed **rot** in the wounds
they leave: amputate in the field [X] or watch the name on the roster stand
up on the wrong side.

Hell glows. Summoning pentagrams scribe themselves in burning sigil-light
and deliver reinforcements unless you foul the circle; the obelisk wears
rune script and veins the ground with creeping corruption that whispers at
whoever stands on it; your soldiers answer with chalked **ward sigils** [R]
that burn whatever crosses them. At night the unseen pack is a field of
glowing eyes, the Taker is only footprints and noises until it's at arm's
length, and possession is a turning halo, a violet vignette, and a whisper.

Morale resets every battle. **Sanity doesn't.** Gibs, Takings, and the
gibbet-post atrocities on terror maps erode it mission after mission;
broken soldiers are unfit until the **Chapel** talks them back, and minds
pushed too far crack into permanent phobias. Once a month, sometimes, the
augurs go quiet three days ahead of a **blood moon** — stronger packs,
double salvage, and a sky like a wound. The worn-thin wake screaming; some
of those dreams are maps.

## Where you fight, and how you get there

Rift battlemaps are procedurally generated from the rift's **biome** — the
world region decides the country, the seed decides the map. Temperate ground
(Europe, the Americas' north, Asia) gets chapels, hedgerows, and rubble;
deserts (Africa, Middle East) get climbable dunes and dry-stone ruins;
jungles (South America, Oceania) grow trees soldiers slip beneath and
gargoyles perch on; the Arctic is snowdrifts and ice boulders with long,
cold sightlines. Structural layouts and feature scatter are seeded, so no
two sites fight the same — while the deployment strips, the obelisk, and
the approach always stay open.

Squads travel by the Order's consecrated **zeppelin**. A rift in a region
with a chapterhouse is struck the same day; anywhere else the squad must be
**dispatched** — one to three days of flight by great-circle distance, the
soldiers locked aboard, while the rift digs in and its garrison hardens.
Fly-and-lead holds on arrival for your order; fly-and-auto fights the day
it lands. If the rift closes mid-flight the sortie turns for home. Founding
chapterhouses abroad is how you shorten the war.

## The deep war

The armoury runs deeper than the rifle: forge the silent **arbalest**, the
fire-hosing **censer**, the wall-cracking **ram hammer**, the stunning
**salt-shot mortar**; issue **consecrated blades** (free ripostes), **warded
circlets** (shatter to stop one psi assault), plate and **abyssal aegis**
armor tiers, and named **relics** rolled from the rubble. Missions have
shapes now — terror rifts are *evacuations* on a clock, harvests are
*ritual interruptions*, infiltrations are *snatch* jobs where the mark
must live. Chapels grow lofts; floors collapse when you shoot out what
holds them; sandstorms, snowfall, and rain each change the fight — and
demons hunt by sight, scent, and *sound*, so a squad that moves low and
kills quiet gets close before the pack knows.

Strategically: standing squads with banners answer their own calls,
gargoyle **sky hunts** maul unescorted zeppelins, corrupted council
patrons siphon the tithe until you **purge** their manors (half the
garrison is human), Ward Towers / Kennels / Vaults fortify Reckonings,
and the drill yard trains the B-team. Soldiers **bond** in the field and
grieve when it breaks; the first Prince to escape becomes a named
**nemesis** that grows with every slip; Commanders **rally** the line
once a battle. When it ends, the game writes the whole war to a markdown
**chronicle** — and if you won, the **Second Dawn** reopens the veil for
an endless, harder war with the Ledger as your scoreboard.

## The 1994 ethos, brought forward

The presentation is the original's, rebuilt honest: a fixed dimetric
battlefield in chunky virtual pixels (1×–4× scale, optional **CRT**
scanlines), a flat saturated **globe** with a knife-edge dithered
terminator, a 30° graticule, city lights after dark, and a purple nebula
behind the world. Time on the Geoscape is **real**: pause and three
compressions, auto-pausing dead the moment anything happens. The right
rail is the command sidebar; the **Basescape** renders your chapterhouse
as an orbitable voxel diorama, one building per facility. Gargoyle sky
hunts on led sorties are fought from the gondola guns, round by round.

In battle: the squad deploys down the ramp of the **grounded zeppelin**
(evacuations end at its lantern-lit mouth), a bottom **squad console**
carries every soldier's vitals, the hover line quotes the resolver's true
**shot forecast**, lost demons leave **ghost intel** at their last seen
tile, and [T] tints every tile a known demon watches. The demon turn hides
behind the **HIDDEN MOVEMENT** curtain. Overhead markers, a click-to-jump
tactical map [M], muzzle flashes, material debris, positional panned audio
(boots tell the ground: earth, planking, snow), and per-biome furniture —
fences, carts, gravestones, a scarecrow with the wrong head — finish the
field. Fire now eats what feeds it (hedges char to nothing, fueled ground
burns hot and long), witchfire **flares** [L] buy light in the deep night,
gargoyles start perched on rooftops, and terror sites stage three distinct
atrocity vignettes. After action: a debrief with commendations. Preferences,
**rebindable keys**, rolling autosaves, and a `mods/` overlay folder for
the RON balance tables round out the shell.

## The campaign arc

Bind demons in battle (`B`), drag them home, and the interrogation chain —
Interrogation → the Herald's Confession → **The Name of the Enemy** — unlocks
the final assault: burn 50 brimstone, breach the Otherside, and end it.
Difficulty (Novice/Veteran/Legend) scales hell's monthly plan and garrisons.
Workshops manufacture charges, dressings, and trade arms; loadouts draw from
real stock; the fallen go on a memorial wall with rank and cause. The globe
carries a day/night terminator — assaults on the night side fight at 9 tiles
of vision instead of 14. Tracers, blasts, camera shake, and synthesized sound
round out the battle.

## Workspace

| Crate | Purpose |
|---|---|
| `crates/ods-voxel` | voxel storage, greedy meshing, raycasts, destruction |
| `crates/ods-sim` | headless deterministic Battlescape rules |
| `crates/ods-geo` | headless deterministic Geoscape: campaign, bases, rift director |
| `crates/ods-render` | wgpu renderer |
| `crates/ods-app` | binary: window, input, UI shell |

## The Geoscape (v1, headless)

Eight world regions fund the Order monthly. Hell's director schedules an
escalating plan of rifts each month — scouting, soul harvests, massacres,
cult infiltrations (permanent funding damage), and nest foundings (a standing
nest bleeds score daily until razed). Augur arrays detect rifts; assaulting
one runs a **real Battlescape battle** (AI on both sides, deterministic) —
deaths are permanent, the wounded convalesce, and survivors log missions.
Chapterhouse facilities build on a 6×6 grid; occultists grind through the
Forbidden Codex (blessed arms, hellsteel plate, rift augury). Two badly-losing
months or deep debt ends the campaign — the classic slow defeat is fully in.

Beyond v1: rifts are **soft for their first two days** and dig in afterwards,
so striking fast matters; victories salvage **brimstone and hellsteel** (sold
at reliquary prices that reroll monthly, or spent to unlock the Hellfire
Lance); soldiers **grow by doing** — accuracy from hits, reactions from
overwatch, bravery from surviving dread — with kills, missions, quirks they
were born with, and lasting scars all on their permanent record. Regions have
a **panic** level: expiries frighten them, banishments soothe them, and past
the breakpoint their patrons flee and hell schedules extra terror to feed on
the fear. Every banishment heats hell's attention: at 5 heat a **Reckoning**
strikes one of your chapterhouses — a base-defense battle on a map generated
from that house's actual floor plan, defended by the soldiers stationed
there. Losing an outpost costs the outpost; losing the founding house ends
the campaign. The Order's whole record — missions, kills, captures, shots
fired, civilians saved — accrues in the **Ledger**.

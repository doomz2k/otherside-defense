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
| `F` | floor cutaway (see inside ground-floor interiors) |
| Tab | next soldier |
| Space or Enter | end turn (demons play) |
| Right-drag / scroll / WASD | orbit / zoom / pan camera |
| Esc | disarm charge / deselect |

## The bestiary

Imps swarm; **Overseers** whisper Terrify through walls; **Hellhounds** charge
on 70 TU; **Bile-wisps** lob acid over cover; and the **Taker** kills soldiers
into Husks that stand up on the wrong side — and hatch a fresh Taker when
destroyed. Packs escalate by campaign month. Every creature is drawn as a
voxel figure assembled from named body parts (head, torso, each limb, horns,
maws, tails — declared in `ods-sim`'s anatomy, built in `ods-app/figures.rs`)
so location-based damage and customisation can hook in per-part later.

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
so striking fast matters; victories salvage **brimstone and hellsteel** (sell
it, or spend it to unlock the Hellfire Lance); soldiers **grow by doing** —
accuracy from hits, reactions from overwatch, bravery from surviving dread —
with kills and missions on their permanent record. And every banishment heats
hell's attention: at 5 heat a **Reckoning** is scheduled, a base-defense
battle fought on a map generated from your actual chapterhouse floor plan.
Lose it (or have no fit defenders) and the campaign ends with the
chapterhouse in ruins.

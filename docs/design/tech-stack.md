# Tech Stack — Decisions & Architecture

Status: **decided** (2026-07-06). Revisit only if a milestone proves one of
these wrong in practice.

## Decisions

| Decision | Choice | Rationale |
|---|---|---|
| Language / GPU API | **Rust + wgpu** | Modern GPU API with a healthy gamedev ecosystem; memory safety pays off in a long-lived custom engine; keeps a future wasm/WebGPU port plausible. |
| Engine | **Custom voxel engine** | Destructible terrain is a design pillar; turn-based tactics is the friendliest genre for a first custom engine (no frame-time pressure on simulation, discrete destruction events, small maps). |
| Art direction | **Fine voxels** (~8–16 voxels per meter, MagicaVoxel-style assets) | "Miniatures diorama" look; chunky, satisfying destruction; supports a grim tone better than meter-scale blocks. |
| Platform | **Desktop native first** (Windows/Linux/macOS) | Simplest debugging and best perf headroom while the engine is young. Browser build is a possible later bonus via wgpu's WebGPU backend — don't spend on it now. |

## Core architectural rule: grid over voxels

Two representations of the battlefield, with a strict relationship:

1. **Voxel world** (fine): raw material occupancy in chunks. Owns destruction,
   line of sight, projectile ray-casts, explosions, fire/smoke volumes.
2. **Gameplay tile grid** (coarse): one tile = one **16³ voxel block**
   (~1 m³). Owns movement costs, TU budgeting, pathfinding, cover values, AI
   reasoning, and everything the player must be able to *read at a glance*.

The tile layer is **derived data**: walkability, cover, and occlusion summaries
are computed from voxel occupancy and invalidated when voxels change (dirty
per-chunk flags → re-derive affected tiles). Gameplay rules never touch raw
voxels directly except through ray/volume queries.

This mirrors the original game, which ray-traced shots through 16×16×24 voxel
templates hidden under its isometric tiles — we're building the real version of
the same idea.

## Simulation / presentation split

- `sim` is a **headless, deterministic** crate: fixed-seed RNG, no wall-clock,
  no rendering types. The whole Battlescape must be runnable in a unit test
  (and later, for AI tournaments and replay files).
- The renderer consumes sim state + an event stream (unit moved, voxels
  destroyed, projectile fired) and is free to be as pretty and stateful as it
  likes without ever feeding back into rules.
- Determinism is a feature we protect from day one: replays, desync-free
  save/load, and reproducible bug reports all fall out of it.

## Cargo workspace layout

```
otherside-defense/
  crates/
    ods-voxel     # voxel storage, chunking, meshing (greedy), raycasts, CSG damage
    ods-sim       # battlescape rules: units, TUs, actions, LOS, morale — headless
    ods-render    # wgpu renderer: chunk meshes, camera, picking, effects
    ods-app       # binary: winit window, input, UI shell, wires the above
  assets/         # voxel models (.vox), palettes, data tables (RON)
  docs/
```

Geoscape comes much later and gets its own crate when it exists; it will be
ordinary 2D/UI rendering, not voxels.

## Key crates

| Concern | Crate | Notes |
|---|---|---|
| Windowing/input | `winit` | the standard pairing with wgpu |
| GPU | `wgpu` | Vulkan/Metal/DX12 backends; WebGPU later if ever |
| Math | `glam` | fast, simple, the ecosystem default |
| Debug/tools UI | `egui` (via `egui-wgpu`/`egui-winit`) | in-game inspector, TU/LOS debug overlays; NOT the shipping game UI |
| Data files | `serde` + `ron` | weapon tables, unit stats, map recipes as human-diffable text |
| Voxel assets | `dot_vox` | load MagicaVoxel `.vox` models |
| Errors | `anyhow` / `thiserror` | app / library respectively |
| Logging | `tracing` + `tracing-subscriber` | structured, filterable |
| RNG | `rand` + `rand_pcg` | seedable PCG for the deterministic sim |
| Parallelism | `rayon` | chunk meshing; keep OUT of `ods-sim` (determinism) |

Deliberately avoided for now: ECS frameworks (`bevy_ecs`, `hecs`) — a
turn-based sim with dozens of units doesn't need one, and plain structs keep
the rules legible; physics engines — destruction is CSG on voxels plus simple
ballistic rays, not rigid-body dynamics.

## Performance envelope (why this is comfortably feasible)

- Map budget: X-COM-scale 50×50×4 tiles → 800×800×64 voxels at 16³/tile
  ≈ 41M voxel cells, overwhelmingly empty/uniform → chunked storage
  (32³ chunks) with palette compression makes this small in practice.
- Meshing: greedy meshing per chunk, re-mesh only dirty chunks on destruction
  events; `rayon` across chunks. Turn-based = destruction is bursty, not
  per-frame.
- Rendering: static chunk meshes + a handful of animated units and particles.
  Any 2015-era GPU will be bored.

## Milestones

- **M0 — triangle to terrain**: winit window, wgpu device, camera orbit, render
  one procedurally-filled chunk with greedy meshing. Engine heartbeat.
- **M1 — the diorama**: load `.vox` tiles into a small map, voxel raycast
  picking, blow a hole in a wall with a debug explosion, re-mesh dirty chunks.
  *Destructibility proven.*
- **M2 — the grid**: derive walkability/cover tiles from voxels, click-to-path
  a unit with TU costs, fog of war from voxel LOS. *The X-COM feel begins.*
- **M3 — first skirmish**: 4 soldiers vs 4 imps, snap/aimed shots as voxel
  raycasts, reaction fire, morale, win/lose. *A playable slice of Battlescape.*

Each milestone should be demoable (screenshot or capture) before the next
starts.

## Dev environment notes

- This cloud workspace has Rust 1.94 and crates.io access; it can build and
  run headless tests (`ods-voxel`, `ods-sim`) but cannot open a window —
  rendering work is verified locally, sim/logic work is verified here.
- CI (later): `cargo test` + `cargo clippy` on Linux; a headless
  golden-image render test is a stretch goal once `ods-render` stabilizes.

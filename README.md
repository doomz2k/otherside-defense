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
cargo test --workspace              # headless: voxel core + sim tests
cargo run -p ods-app                # the skirmish (needs a display + GPU)
cargo run -p ods-app -- --headless  # sim-only smoke test (CI / cloud)
```

## The first skirmish

Four Order soldiers vs four imps in a ruined chapel yard. Fully destructible
voxel terrain: misses chip walls, and a breached wall changes line of sight
and pathing.

| Input | Action |
|---|---|
| Left click soldier / ground / imp | select / move / fire |
| `1` / `2` / `3` | snap / aimed / auto fire mode |
| `G` then click | throw a hellfire charge (arcs over walls) |
| `H` | field-dress the selected soldier (staunch bleeding) |
| Tab | next soldier |
| Space or Enter | end turn (demons play) |
| Right-drag / scroll / WASD | orbit / zoom / pan camera |
| Esc | disarm charge / deselect |

## Workspace

| Crate | Purpose |
|---|---|
| `crates/ods-voxel` | voxel storage, greedy meshing, raycasts, destruction |
| `crates/ods-sim` | headless deterministic Battlescape rules |
| `crates/ods-render` | wgpu renderer |
| `crates/ods-app` | binary: window, input, UI shell |

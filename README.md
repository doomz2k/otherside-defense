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
cargo test --workspace   # headless: voxel core + sim tests
cargo run -p ods-app     # currently a headless voxel-engine smoke test
```

## Workspace

| Crate | Purpose |
|---|---|
| `crates/ods-voxel` | voxel storage, greedy meshing, raycasts, destruction |
| `crates/ods-sim` | headless deterministic Battlescape rules |
| `crates/ods-render` | wgpu renderer |
| `crates/ods-app` | binary: window, input, UI shell |

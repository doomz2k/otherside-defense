//! Headless screenshots: boot a scene, render one frame with the real
//! renderer (lavapipe/llvmpipe will do), write a PNG. This is how the
//! game gets LOOKED AT on machines with no window — CI, agents, and
//! anyone who wants a quick still without launching.
//!
//!   ods-app --screenshot battle  shot.png [seed]
//!   ods-app --screenshot globe   shot.png
//!   ods-app --screenshot base    shot.png

use anyhow::{Context, Result};
use glam::{Vec3, Vec4};
use ods_render::{OrbitCamera, Renderer};

const W: u32 = 1600;
const H: u32 = 900;

pub fn capture(scene: &str, out: &str, seed: u64) -> Result<()> {
    let mut renderer =
        Renderer::headless(W, H).context("no GPU adapter — install mesa-vulkan-drivers")?;
    match scene {
        "globe" => globe(&mut renderer)?,
        "base" => base(&mut renderer)?,
        _ => battle(&mut renderer, seed)?,
    }
    let (rgba, w, h) = renderer.read_rgba()?;
    let file = std::fs::File::create(out)?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()?.write_image_data(&rgba)?;
    eprintln!("wrote {out} ({w}x{h}, scene: {scene})");
    Ok(())
}

/// A real campaign mission two seconds in — sculpted ground, dressed
/// walls, the squad deployed — framed close on the deployment strip.
fn battle(renderer: &mut Renderer, seed: u64) -> Result<()> {
    let squad: Vec<ods_sim::units::Unit> = (0..6)
        .map(|i| {
            ods_sim::units::Unit::soldier(i, &format!("Vera {i}"), glam::IVec3::new(2, 8 + i as i32, 0))
        })
        .collect();
    let battle = ods_sim::scenario::incursion_in_biome(
        seed,
        squad,
        8,
        2,
        3,
        ods_sim::scenario::Biome::Temperate,
    );
    let mut screen = crate::battle_screen::BattleScreen::new(renderer, battle, None);
    for _ in 0..44 {
        screen.update_frame(0.05, renderer, None, W as f32, H as f32);
    }
    // Frame the squad, close enough to see the carve.
    let mid = screen
        .battle
        .units
        .iter()
        .filter(|u| u.side == ods_sim::units::Side::Order)
        .map(|u| (u.tile * ods_sim::TILE_VOXELS).as_vec3())
        .fold(Vec3::ZERO, |a, b| a + b)
        / 6.0;
    // Halfway between the squad and the heart of the map: deployment on
    // one side, the hamlet on the other.
    let (lo, hi) = screen.battle.tiles.bounds();
    let center = ((lo + hi) / 2 * ods_sim::TILE_VOXELS).as_vec3();
    screen.camera.target = (mid + center) / 2.0;
    screen.camera.distance *= 0.42;
    let vp = screen.camera_vp(W as f32 / H as f32);
    renderer.set_camera_flash(vp, Vec3::new(0.45, 0.55, 0.5), 1.6, Vec4::ZERO);
    renderer.render(None)?;
    renderer.render(None)?; // second pass: the headless target now exists
    Ok(())
}

/// The world from orbit, with a young campaign's markers on it.
fn globe(renderer: &mut Renderer) -> Result<()> {
    let c = ods_geo::Campaign::new(7);
    let (verts, idx) = crate::globe::build_globe(None);
    renderer.set_globe(&verts, &idx);
    let (mv, mi) = crate::globe::build_markers(&c, 1.0);
    renderer.set_markers(&mv, &mi);
    let (mut fx, mut fxi) = crate::globe::build_city_lights(&c, 40.0, 1.0);
    let (ov, oi) = crate::globe::build_geo_omens(&c, 1.0);
    let base = fx.len() as u32;
    fx.extend(ov);
    fxi.extend(oi.iter().map(|i| i + base));
    renderer.set_fx(&fx, &fxi);
    let mut cam = OrbitCamera::new(Vec3::ZERO);
    cam.distance = 640.0;
    cam.pitch = 0.35;
    let sun = crate::globe::latlon_to_pos(12.0, 30.0, 1.0);
    let eye = cam.eye();
    renderer.set_camera_flash(
        cam.view_proj(W as f32 / H as f32),
        sun,
        1.0,
        Vec4::new(eye.x, eye.y, eye.z, 0.0),
    );
    renderer.render(None)?;
    renderer.render(None)?;
    Ok(())
}

/// A founding chapterhouse with its garrison on the yard.
fn base(renderer: &mut Renderer) -> Result<()> {
    let house = ods_geo::Chapterhouse::founding(ods_geo::Region::Europe);
    let (verts, idx) = crate::basescape::build_base_scene(&house, 8, true, 1.0);
    renderer.set_figures(&verts, &idx);
    let extent = ods_geo::GRID as f32 * 44.0;
    renderer.shadow_bounds = Some((
        Vec3::new(-40.0, -80.0, -10.0),
        Vec3::new(extent + 40.0, extent + 40.0, 60.0),
    ));
    let mut cam = OrbitCamera::isometric(crate::basescape::scene_center());
    cam.distance = 420.0;
    renderer.set_camera(cam.view_proj(W as f32 / H as f32), Vec3::new(0.5, 0.4, 0.65), 1.0);
    renderer.render(None)?;
    renderer.render(None)?;
    Ok(())
}

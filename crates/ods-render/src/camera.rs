//! Orbit camera for the Battlescape diorama, plus screen-to-world rays for
//! mouse picking.

use glam::{Mat4, Vec3, Vec4};

#[derive(Clone)]
pub struct OrbitCamera {
    /// Point the camera orbits around and looks at.
    pub target: Vec3,
    /// Radians around +Z, 0 looking along +X.
    pub yaw: f32,
    /// Radians above the horizon.
    pub pitch: f32,
    pub distance: f32,
    pub fov_y: f32,
    /// Orthographic projection: the classic isometric tabletop. No
    /// perspective convergence — X-COM's eye.
    pub ortho: bool,
}

/// The classic dimetric battle angle: down the diagonal, ~35° above.
pub const ISO_YAW: f32 = -std::f32::consts::FRAC_PI_4;
pub const ISO_PITCH: f32 = 0.615;

impl OrbitCamera {
    pub fn new(target: Vec3) -> Self {
        Self {
            target,
            yaw: ISO_YAW,
            pitch: 0.9,
            distance: 420.0,
            fov_y: 45f32.to_radians(),
            ortho: false,
        }
    }

    /// The Battlescape's eye: orthographic, down the diagonal, X-COM angle.
    pub fn isometric(target: Vec3) -> Self {
        Self {
            target,
            yaw: ISO_YAW,
            pitch: ISO_PITCH,
            distance: 420.0,
            fov_y: 45f32.to_radians(),
            ortho: true,
        }
    }

    pub fn eye(&self) -> Vec3 {
        let (sy, cy) = self.yaw.sin_cos();
        let (sp, cp) = self.pitch.sin_cos();
        self.target + self.distance * Vec3::new(cp * cy, cp * sy, sp)
    }

    pub fn view_proj(&self, aspect: f32) -> Mat4 {
        let view = Mat4::look_at_rh(self.eye(), self.target, Vec3::Z);
        let aspect = aspect.max(0.01);
        let proj = if self.ortho {
            let half_h = self.distance * 0.42;
            Mat4::orthographic_rh(
                -half_h * aspect,
                half_h * aspect,
                -half_h,
                half_h,
                -2000.0,
                4000.0,
            )
        } else {
            Mat4::perspective_rh(self.fov_y, aspect, 1.0, 4000.0)
        };
        proj * view
    }

    /// Rotate the view a quarter-turn around the field, staying on the
    /// diagonal facings (X-COM's four-way battlescape rotation).
    pub fn snap_turn(&mut self, steps: i32) {
        let quarter = std::f32::consts::FRAC_PI_2;
        let k = ((self.yaw - ISO_YAW) / quarter).round() + steps as f32;
        self.yaw = ISO_YAW + k * quarter;
    }

    /// World-space ray through a screen pixel.
    pub fn screen_ray(&self, px: f32, py: f32, width: f32, height: f32) -> (Vec3, Vec3) {
        let ndc_x = 2.0 * px / width.max(1.0) - 1.0;
        let ndc_y = 1.0 - 2.0 * py / height.max(1.0);
        let inv = self.view_proj(width / height.max(1.0)).inverse();

        let unproject = |z: f32| -> Vec3 {
            let p = inv * Vec4::new(ndc_x, ndc_y, z, 1.0);
            p.truncate() / p.w
        };
        let near = unproject(0.0);
        let far = unproject(1.0);
        (near, (far - near).normalize())
    }

    pub fn orbit(&mut self, dyaw: f32, dpitch: f32) {
        self.yaw += dyaw;
        self.pitch = (self.pitch + dpitch).clamp(0.15, 1.45);
    }

    pub fn zoom(&mut self, factor: f32) {
        self.distance = (self.distance * factor).clamp(60.0, 1500.0);
    }

    /// Pan the target in the ground plane, relative to the view direction.
    pub fn pan(&mut self, right: f32, forward: f32) {
        let (sy, cy) = self.yaw.sin_cos();
        let fwd = Vec3::new(-cy, -sy, 0.0);
        let rgt = Vec3::new(-sy, cy, 0.0);
        self.target += rgt * right + fwd * forward;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn screen_center_ray_points_at_target() {
        let cam = OrbitCamera::new(Vec3::new(100.0, 100.0, 0.0));
        let (origin, dir) = cam.screen_ray(400.0, 300.0, 800.0, 600.0);
        // The center ray must pass very close to the orbit target.
        let to_target = cam.target - origin;
        let closest = origin + dir * to_target.dot(dir);
        assert!(
            (closest - cam.target).length() < 1.0,
            "center ray misses target by {}",
            (closest - cam.target).length()
        );
    }

    #[test]
    fn eye_is_above_and_away() {
        let cam = OrbitCamera::new(Vec3::ZERO);
        let eye = cam.eye();
        assert!(eye.z > 0.0);
        assert!((eye - cam.target).length() > 100.0);
    }
}

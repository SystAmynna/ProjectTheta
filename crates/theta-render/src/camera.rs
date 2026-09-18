//! Caméra de jeu : zone de monde visible fixe, indépendante de l'écran.
//!
//! La projection montre toujours [`VIEW_SIZE`] unités de monde, que la fenêtre
//! fasse 1280 × 720, soit en plein écran 1080p ou en 4K, quel que soit le
//! facteur d'échelle DPI : l'écran ne change que la taille d'affichage, jamais
//! la distance de vue. Si le ratio de la fenêtre diffère de celui de
//! [`VIEW_SIZE`], le rendu est réduit à un viewport centré au bon ratio, entouré
//! de bandes noires.

use avian2d::physics_transform::PhysicsTransformSystems;
use bevy::camera::{CameraOutputMode, ScalingMode, Viewport};
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

/// Zone de monde visible, en unités monde (1 tile = 32 unités).
pub const VIEW_SIZE: Vec2 = Vec2::new(1920.0, 1080.0);

/// Entité que la caméra suit : le joueur local, marqué par `theta-client`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CameraTarget;

/// La caméra qui affiche le monde.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct GameCamera;

pub(crate) struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_camera)
            // Le viewport est lu par `CameraUpdateSystems` pour calculer la
            // projection : il doit être à jour avant.
            .add_systems(
                PostUpdate,
                fit_viewport.before(bevy::camera::CameraUpdateSystems),
            )
            // Après l'écriture de `Transform` depuis `Position` (interpolation
            // et correction comprises), et avant sa propagation : la caméra
            // colle ainsi à l'image du joueur telle qu'elle sera affichée.
            .add_systems(
                PostUpdate,
                follow_camera_target
                    .after(PhysicsTransformSystems::PositionToTransform)
                    .before(TransformSystems::Propagate),
            );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        GameCamera,
        Camera2d,
        Camera {
            // La fenêtre est entièrement effacée avant que l'image de la caméra
            // n'y soit copiée, limitée au viewport : ce qui reste autour forme
            // les bandes noires.
            output_mode: CameraOutputMode::Write {
                blend_state: None,
                clear_color: ClearColorConfig::Custom(Color::BLACK),
            },
            ..default()
        },
        // `AutoMin` plutôt que `Fixed` : le viewport a déjà le ratio de
        // `VIEW_SIZE`, mais son arrondi au pixel ne doit pas étirer l'image.
        Projection::Orthographic(OrthographicProjection {
            scaling_mode: ScalingMode::AutoMin {
                min_width: VIEW_SIZE.x,
                min_height: VIEW_SIZE.y,
            },
            ..OrthographicProjection::default_2d()
        }),
    ));
}

/// Restreint la caméra au plus grand rectangle centré, au ratio de
/// [`VIEW_SIZE`], que contient la fenêtre.
///
/// Le calcul se fait en pixels physiques : le facteur d'échelle DPI n'a ainsi
/// aucune influence sur ce qui est montré.
fn fit_viewport(
    windows: Query<&Window, (With<PrimaryWindow>, Changed<Window>)>,
    mut cameras: Query<&mut Camera, With<GameCamera>>,
) {
    let Ok(window) = windows.single() else {
        return;
    };
    let Some(viewport) = letterbox(window.physical_size()) else {
        // Fenêtre minimisée : on garde le viewport précédent.
        return;
    };
    for mut camera in &mut cameras {
        let current = camera.viewport.as_ref();
        if current.map(|v| (v.physical_position, v.physical_size))
            != Some((viewport.physical_position, viewport.physical_size))
        {
            camera.viewport = Some(viewport.clone());
        }
    }
}

/// Plus grand viewport centré au ratio de [`VIEW_SIZE`] dans une fenêtre de
/// `window` pixels, ou `None` si la fenêtre est vide.
fn letterbox(window: UVec2) -> Option<Viewport> {
    if window.x == 0 || window.y == 0 {
        return None;
    }
    let scale = (window.x as f32 / VIEW_SIZE.x).min(window.y as f32 / VIEW_SIZE.y);
    let size = (VIEW_SIZE * scale)
        .round()
        .as_uvec2()
        .clamp(UVec2::ONE, window);
    Some(Viewport {
        physical_position: (window - size) / 2,
        physical_size: size,
        ..default()
    })
}

/// Centre la caméra sur sa cible.
fn follow_camera_target(
    target: Query<&Transform, (With<CameraTarget>, Without<GameCamera>)>,
    mut cameras: Query<&mut Transform, With<GameCamera>>,
) {
    let Ok(target) = target.single() else {
        return;
    };
    for mut camera in &mut cameras {
        camera.translation.x = target.translation.x;
        camera.translation.y = target.translation.y;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_fills_matching_ratio() {
        let v = letterbox(UVec2::new(2560, 1440)).unwrap();
        assert_eq!(v.physical_position, UVec2::ZERO);
        assert_eq!(v.physical_size, UVec2::new(2560, 1440));
    }

    #[test]
    fn letterbox_pillarboxes_wide_window() {
        let v = letterbox(UVec2::new(3440, 1440)).unwrap();
        assert_eq!(v.physical_size, UVec2::new(2560, 1440));
        assert_eq!(v.physical_position, UVec2::new(440, 0));
    }

    #[test]
    fn letterbox_letterboxes_tall_window() {
        let v = letterbox(UVec2::new(1920, 1200)).unwrap();
        assert_eq!(v.physical_size, UVec2::new(1920, 1080));
        assert_eq!(v.physical_position, UVec2::new(0, 60));
    }

    #[test]
    fn letterbox_ignores_empty_window() {
        assert!(letterbox(UVec2::new(0, 720)).is_none());
    }
}

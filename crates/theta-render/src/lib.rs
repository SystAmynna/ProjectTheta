use avian2d::physics_transform::PhysicsTransformSystems;
use avian2d::prelude::{Position, Rotation};
use bevy::prelude::*;
use bevy::window::{PresentMode, WindowResolution};
use theta_core::{Player, PlayerColor};

pub mod assets;
pub mod terrain;

pub use assets::{ASSET_ROOT_ENV, GameAssets, asset_plugin, asset_root};

/// [`ImagePlugin`] du client : filtrage au plus proche, pour que les tiles en
/// pixel art restent nettes au lieu d'être lissées.
///
/// À passer à `DefaultPlugins`, comme [`asset_plugin`] et [`window_plugin`].
pub fn image_plugin() -> ImagePlugin {
    ImagePlugin::default_nearest()
}

/// [`WindowPlugin`] du client : la fenêtre de jeu.
///
/// `PresentMode::AutoNoVsync` découple le rendu de la dalle : la boucle
/// `Update` — donc les sprites, la caméra et l'interpolation — tourne aussi vite
/// que la machine le permet, au-delà du taux de rafraîchissement. La simulation,
/// elle, reste à `theta_core::TICK_HZ` : rien de ce qui est fait ici ne peut la
/// faire varier. Repasser à `PresentMode::AutoVsync` suffit à réactiver la
/// synchronisation verticale (moins de déchirement, FPS bornés par l'écran).
///
/// À passer à `DefaultPlugins`, comme [`asset_plugin`] :
///
/// ```no_run
/// # use bevy::prelude::*;
/// App::new().add_plugins(
///     DefaultPlugins
///         .set(theta_render::asset_plugin())
///         .set(theta_render::window_plugin()),
/// );
/// ```
pub fn window_plugin() -> WindowPlugin {
    WindowPlugin {
        primary_window: Some(Window {
            title: "ProjectTheta".to_string(),
            resolution: WindowResolution::new(1280, 720),
            present_mode: PresentMode::AutoNoVsync,
            ..default()
        }),
        ..default()
    }
}

/// Rendu : caméra, chargement des assets et représentation visuelle des
/// entités de jeu.
///
/// Ce plugin n'introduit aucune entité de gameplay : il se contente
/// d'habiller celles créées par `theta-core` / `theta-client`.
///
/// Le répertoire des assets se configure via [`asset_plugin`], à passer à
/// `DefaultPlugins` — voir [`assets`].
pub struct RenderPlugin;

impl Plugin for RenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((assets::AssetsPlugin, terrain::TerrainRenderPlugin))
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, attach_player_sprite)
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

/// Entité que la caméra suit : le joueur local, marqué par `theta-client`.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CameraTarget;

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Centre la caméra sur sa cible.
fn follow_camera_target(
    target: Query<&Transform, (With<CameraTarget>, Without<Camera2d>)>,
    mut cameras: Query<&mut Transform, With<Camera2d>>,
) {
    let Ok(target) = target.single() else {
        return;
    };
    for mut camera in &mut cameras {
        camera.translation.x = target.translation.x;
        camera.translation.y = target.translation.y;
    }
}

/// Un joueur prêt à être affiché mais qui n'a pas encore de sprite.
type UndressedPlayer = (With<Player>, With<Position>, With<Rotation>, Without<Sprite>);

/// Donne un sprite à tout joueur qui vient d'apparaître et n'en a pas encore.
///
/// Le filtre exige `Position` **et** `Rotation` : sur un joueur interpolé, ces
/// deux composants n'arrivent pas forcément au même instant, et la
/// synchronisation vers `Transform` n'a lieu que si les deux sont présents. Sans
/// cette condition, le joueur s'afficherait quelques images à l'origine.
fn attach_player_sprite(
    mut commands: Commands,
    assets: Res<GameAssets>,
    players: Query<(Entity, Option<&PlayerColor>), UndressedPlayer>,
) {
    for (entity, tint) in &players {
        let color = tint.copied().unwrap_or_default().0;

        commands.entity(entity).insert(Sprite {
            custom_size: Some(Vec2::splat(64.0)),
            color: Color::linear_rgb(color[0], color[1], color[2]),
            ..Sprite::from_image(assets.player.clone())
        });
    }
}

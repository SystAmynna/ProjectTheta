use bevy::prelude::*;
use theta_core::Player;

pub mod assets;

pub use assets::{ASSET_ROOT_ENV, GameAssets, asset_plugin, asset_root};

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
        app.add_plugins(assets::AssetsPlugin)
            .add_systems(Startup, spawn_camera)
            .add_systems(Update, attach_player_sprite);
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn(Camera2d);
}

/// Donne un sprite à tout joueur qui vient d'apparaître et n'en a pas encore.
fn attach_player_sprite(
    mut commands: Commands,
    assets: Res<GameAssets>,
    players: Query<Entity, (With<Player>, Without<Sprite>)>,
) {
    for entity in &players {
        commands.entity(entity).insert(Sprite {
            custom_size: Some(Vec2::splat(64.0)),
            ..Sprite::from_image(assets.player.clone())
        });
    }
}

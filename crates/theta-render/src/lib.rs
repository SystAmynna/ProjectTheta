use avian2d::prelude::{Position, Rotation};
use bevy::prelude::*;
use theta_core::{Player, PlayerColor};

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

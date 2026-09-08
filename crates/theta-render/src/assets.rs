use std::path::{Path, PathBuf};

use bevy::asset::AssetPlugin;
use bevy::prelude::*;

/// Variable d'environnement permettant de forcer le répertoire des assets.
pub const ASSET_ROOT_ENV: &str = "THETA_ASSET_ROOT";

/// Répertoire des assets tel qu'il existe dans le dépôt, résolu à la
/// compilation : `<racine du projet>/assets`, hors de `crates/`.
const REPO_ASSET_ROOT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../assets");

/// Détermine le répertoire `assets/` à utiliser.
///
/// Dans l'ordre :
/// 1. la variable d'environnement [`ASSET_ROOT_ENV`] ;
/// 2. le `assets/` à la racine du dépôt, s'il existe (build de développement) ;
/// 3. le `assets/` posé à côté de l'exécutable (build distribué).
pub fn asset_root() -> PathBuf {
    if let Some(path) = std::env::var_os(ASSET_ROOT_ENV) {
        return PathBuf::from(path);
    }

    let repo_root = Path::new(REPO_ASSET_ROOT);
    if repo_root.is_dir() {
        // `canonicalize` retire les `..` : les chemins affichés dans les
        // messages d'erreur de Bevy restent lisibles.
        return repo_root
            .canonicalize()
            .unwrap_or_else(|_| repo_root.to_path_buf());
    }

    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("assets")))
        .unwrap_or_else(|| PathBuf::from("assets"))
}

/// [`AssetPlugin`] configuré sur le répertoire renvoyé par [`asset_root`].
///
/// À passer à `DefaultPlugins` :
///
/// ```no_run
/// # use bevy::prelude::*;
/// App::new().add_plugins(DefaultPlugins.set(theta_render::asset_plugin()));
/// ```
pub fn asset_plugin() -> AssetPlugin {
    // Le chemin est absolu : Bevy le `join` sur sa base path, ce qui le laisse
    // intact et neutralise donc `CARGO_MANIFEST_DIR` (qui pointerait sur la
    // crate du binaire, pas sur la racine du projet).
    AssetPlugin {
        file_path: asset_root().to_string_lossy().into_owned(),
        ..default()
    }
}

/// Handles des assets chargés une fois pour toutes au démarrage.
#[derive(Resource, Debug, Clone)]
pub struct GameAssets {
    /// Sprite du joueur.
    pub player: Handle<Image>,
}

pub(crate) struct AssetsPlugin;

impl Plugin for AssetsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_assets);
    }
}

fn load_assets(mut commands: Commands, assets: Res<AssetServer>) {
    debug!("Répertoire des assets : {}", asset_root().display());

    commands.insert_resource(GameAssets {
        player: assets.load("a.png"),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repo_asset_root_exists() {
        assert!(
            Path::new(REPO_ASSET_ROOT).is_dir(),
            "`{REPO_ASSET_ROOT}` devrait exister : le dossier assets/ est à la racine du projet"
        );
    }

    #[test]
    fn asset_root_is_absolute() {
        assert!(asset_root().is_absolute());
    }

    #[test]
    fn asset_root_contains_player_sprite() {
        assert!(asset_root().join("a.png").is_file());
    }
}

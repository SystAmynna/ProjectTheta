use std::net::SocketAddr;

use bevy::prelude::*;
use theta_core::{PlayerBundle, Speed};

mod input;

pub use input::KeyBindings;

/// Paramètres d'exécution du client, fournis par le binaire.
#[derive(Resource, Debug, Clone)]
pub struct ClientConfig {
    /// Adresse du serveur à rejoindre.
    pub server: SocketAddr,
}

/// Marque le joueur contrôlé par cette instance du jeu.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct LocalPlayer;

/// Côté client : saisie locale, connexion au serveur, prédiction et interpolation.
///
/// `CorePlugin` (theta-core) et `ProtocolPlugin` (theta-protocole) doivent être
/// ajoutés séparément par le binaire, ainsi que la ressource [`ClientConfig`].
pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(input::InputPlugin)
            .add_systems(Startup, (log_startup, spawn_local_player));
        // TODO: configurer la connexion client lightyear.
    }
}

fn log_startup(config: Res<ClientConfig>) {
    info!("Client configuré pour le serveur {}", config.server);
}

fn spawn_local_player(mut commands: Commands) {
    commands.spawn((LocalPlayer, PlayerBundle::new(Vec2::ZERO, Speed::DEFAULT)));
}

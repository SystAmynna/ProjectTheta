use std::net::SocketAddr;

use bevy::prelude::*;

/// Paramètres d'exécution du serveur, fournis par le binaire.
#[derive(Resource, Debug, Clone)]
pub struct ServerConfig {
    /// Adresse d'écoute.
    pub bind: SocketAddr,
    /// Fréquence de simulation, en ticks par seconde.
    pub tick_rate: u32,
}

/// Côté serveur : autorité de simulation et réplication vers les clients.
///
/// `CorePlugin` (theta-core) et `ProtocolPlugin` (theta-protocole) doivent être
/// ajoutés séparément par le binaire, ainsi que la ressource [`ServerConfig`].
pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, log_startup);
        // TODO: configurer le serveur lightyear (écoute, réplication,
        // spawn d'un joueur par connexion entrante).
    }
}

fn log_startup(config: Res<ServerConfig>) {
    info!(
        "Serveur en écoute sur {} ({} ticks/s)",
        config.bind, config.tick_rate
    );
}

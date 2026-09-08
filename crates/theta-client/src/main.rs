use std::net::SocketAddr;

use bevy::prelude::*;
use clap::Parser;
use lightyear::prelude::client::ClientPlugins;
use theta_client::{ClientConfig, ClientPlugin};
use theta_core::{CorePlugin, tick_duration};
use theta_protocole::ProtocolPlugin;
use theta_render::{RenderPlugin, asset_plugin, window_plugin};

/// Client de ProjectTheta : fenêtre de jeu, rendu et saisie clavier.
#[derive(Parser, Debug)]
#[command(name = "theta-client", version, about = "ProjectTheta — client.")]
struct Cli {
    /// Adresse du serveur à rejoindre (`ip:port`).
    #[arg(long, default_value = "127.0.0.1:5000")]
    server: SocketAddr,
}

fn main() {
    let cli = Cli::parse();

    App::new()
        // Le répertoire des assets et la fenêtre sont décidés par `theta-render`.
        .add_plugins(DefaultPlugins.set(asset_plugin()).set(window_plugin()))
        // La cadence vient de `theta-core` et n'est pas configurable : elle ne
        // limite que `FixedUpdate`. Le rendu et la saisie tournent en `Update`,
        // à la fréquence d'images de la machine, sans effet sur les ticks.
        // Lightyear en premier : `ProtocolPlugin` a besoin de son registre de
        // composants pour enregistrer ceux d'Avian.
        .add_plugins(ClientPlugins {
            tick_duration: tick_duration(),
        })
        .add_plugins(CorePlugin)
        .add_plugins(ProtocolPlugin)
        .add_plugins(RenderPlugin)
        .add_plugins(ClientPlugin)
        .insert_resource(ClientConfig { server: cli.server })
        .run();
}

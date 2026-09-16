use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use clap::Parser;
use lightyear::prelude::server::ServerPlugins;
use theta_core::{CorePlugin, tick_duration};
use theta_protocole::ProtocolPlugin;
use theta_server::{ServerConfig, ServerPlugin};

/// Serveur de ProjectTheta : simulation autoritaire, sans fenêtre ni rendu.
#[derive(Parser, Debug)]
#[command(name = "theta-server", version, about = "ProjectTheta — serveur.")]
struct Cli {
    /// Adresse d'écoute (`ip:port`).
    #[arg(long, default_value = "0.0.0.0:5000")]
    bind: SocketAddr,
    /// Graine du monde. Tirée de l'heure si absente.
    #[arg(long)]
    seed: Option<u64>,
    // Pas d'option de cadence : `theta_core::TICK_HZ` fait loi, sans quoi un
    // serveur lancé autrement parlerait un autre protocole que ses clients.
}

fn main() {
    let cli = Cli::parse();
    let config = ServerConfig {
        bind: cli.bind,
        seed: cli.seed.unwrap_or_else(random_seed),
    };

    // Le serveur n'affiche rien : sa boucle principale peut battre au rythme de
    // la simulation, une image par tick.
    let tick_period = tick_duration();

    App::new()
        .add_plugins(MinimalPlugins.set(ScheduleRunnerPlugin::run_loop(tick_period)))
        .add_plugins(LogPlugin::default())
        .add_plugins(StatesPlugin)
        // Lightyear en premier : `ProtocolPlugin` a besoin de son registre de
        // composants pour enregistrer ceux d'Avian.
        .add_plugins(ServerPlugins {
            tick_duration: tick_period,
        })
        .add_plugins(CorePlugin)
        .add_plugins(ProtocolPlugin)
        .add_plugins(ServerPlugin)
        .insert_resource(config)
        .run();
}

/// Graine par défaut : l'heure courante, pour qu'un serveur lancé sans `--seed`
/// ne génère pas toujours le même monde.
fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or_default()
}

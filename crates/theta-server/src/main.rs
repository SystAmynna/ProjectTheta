use std::net::SocketAddr;
use std::time::Duration;

use bevy::app::ScheduleRunnerPlugin;
use bevy::log::LogPlugin;
use bevy::prelude::*;
use bevy::state::app::StatesPlugin;
use clap::Parser;
use lightyear::prelude::server::ServerPlugins;
use theta_core::CorePlugin;
use theta_protocole::{ProtocolPlugin, TICK_HZ};
use theta_server::{ServerConfig, ServerPlugin};

/// Serveur de ProjectTheta : simulation autoritaire, sans fenêtre ni rendu.
#[derive(Parser, Debug)]
#[command(name = "theta-server", version, about = "ProjectTheta — serveur.")]
struct Cli {
    /// Adresse d'écoute (`ip:port`).
    #[arg(long, default_value = "0.0.0.0:5000")]
    bind: SocketAddr,

    /// Fréquence de simulation, en ticks par seconde.
    #[arg(long, default_value_t = TICK_HZ as u32)]
    tick_rate: u32,
}

fn main() {
    let cli = Cli::parse();
    let config = ServerConfig {
        bind: cli.bind,
        tick_rate: cli.tick_rate.max(1),
    };

    let tick_period = Duration::from_secs_f64(1.0 / f64::from(config.tick_rate));

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

use avian2d::{PhysicsPlugins, dynamics::integrator::Gravity};
use clap::{Parser, Subcommand};
use bevy::prelude::*;

mod player;

#[derive(Parser, Debug)]
#[command(name="ProjectTheta", version, about="ProjectTheta game.")]
struct Cli {
    #[command(subcommand)]
    mode : Option<Mode>
}

#[derive(Subcommand, Debug, Default)]
enum Mode {
    #[default]
    #[cfg(all(feature = "server", feature = "client"))]
    HostClient,
    #[cfg(feature = "server")]
    Server,
    #[cfg(feature = "client")]
    Client
    
}

fn main() {
    // Argumentation de l'application
    let cli = Cli::parse();
    // Application bevy
    let mut app = App::new();

    app.add_plugins(DefaultPlugins)
        .add_plugins(PhysicsPlugins::default())
        .insert_resource(Gravity(Vec2::ZERO));


    // Gérer l'argumentation 
    let mode = cli.mode.unwrap_or_default();
    match mode {
        #[cfg(feature = "server")]
        Mode::Server => {
            println!("Server mode activé")
        },
        #[cfg(feature = "client")]
        Mode::Client => {
            println!("Client mode activé");
            client_app_setup(&mut app);
        },
        #[cfg(all(feature = "server", feature = "client"))]
        Mode::HostClient => {
            println!("Server & Client mode activé");
            client_app_setup(&mut app);
        }
    }

    app.run();

}

fn client_app_setup(app : &mut App) {
    app.add_systems(Startup, spawn_camera)
        .add_plugins(player::PlayerPlugin);
}

fn spawn_camera(mut commands : Commands) {
    commands.spawn(Camera2d);
}
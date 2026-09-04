use clap::{Parser, Subcommand};
use bevy::prelude::*;

#[derive(Parser, Debug)]
#[command(name="ProjectTheta", version, about="ProjectTheta game.")]
struct Cli {
    #[command(subcommand)]
    mode : Mode
}

#[derive(Subcommand, Debug)]
enum Mode {
    #[cfg(feature = "server")]
    Server,
    #[cfg(feature = "client")]
    Client,
    #[cfg(all(feature = "server", feature = "client"))]
    HostClient
}

fn main() {
    // Argumentation de l'application
    let cli = Cli::parse();
    // Application bevy
    let mut app = App::new();

    match cli.mode {
        #[cfg(feature = "server")]
        Mode::Server => {
            println!("Server mode activé")
        },
        #[cfg(feature = "client")]
        Mode::Client => {
            println!("Client mode activé")
        },
        #[cfg(all(feature = "server", feature = "client"))]
        Mode::HostClient => {
            println!("Server & Client mode activé")
        }
    }

}

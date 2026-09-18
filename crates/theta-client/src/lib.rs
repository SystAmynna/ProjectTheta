use std::net::SocketAddr;

use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolate;
use lightyear::prelude::*;
use lightyear::prelude::input::native::InputMarker;
use theta_core::{PlayerSimulationBundle, Speed};
use theta_protocole::{MoveInput, PlayerId};
use theta_render::CameraTarget;

mod connection;
mod input;
mod menu;
mod terrain;

pub use connection::{AppState, ConnectionError};
pub use input::KeyBindings;

/// Paramètres d'exécution du client, fournis par le binaire.
#[derive(Resource, Debug, Clone)]
pub struct ClientConfig {
    /// Adresse du serveur à rejoindre.
    pub server: SocketAddr,
}

/// Marque le joueur contrôlé par cette instance du jeu.
///
/// Il n'est pas créé localement : il arrive du serveur par réplication, et se
/// reconnaît à ce qu'il est à la fois prédit et contrôlé par nous.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct LocalPlayer;

/// Côté client : menu, connexion au serveur, saisie locale, prédiction et
/// interpolation.
///
/// `CorePlugin` (theta-core) et `ProtocolPlugin` (theta-protocole) doivent être
/// ajoutés séparément par le binaire, ainsi que la ressource [`ClientConfig`].
pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((
            connection::ConnectionPlugin,
            menu::MenuPlugin,
            input::InputPlugin,
            terrain::TerrainPlugin,
        ))
        // La prédiction est pilotée par une ressource globale.
        .init_resource::<PredictionManager>()
        .add_observer(on_predicted)
        .add_observer(on_interpolated);
    }
}

/// Le serveur nous a envoyé une entité prédite : si elle nous appartient, c'est
/// notre joueur.
fn on_predicted(
    predicted: On<Add, Predicted>,
    controlled: Query<(), With<Controlled>>,
    mut commands: Commands,
) {
    if !controlled.contains(predicted.entity) {
        return;
    }

    commands.entity(predicted.entity).insert((
        LocalPlayer,
        CameraTarget,
        // Le joueur prédit est simulé localement : il lui faut le corps
        // physique que le serveur simule de son côté. Sa pose, elle, vient de
        // la réplication : surtout ne pas la réinitialiser ici.
        PlayerSimulationBundle::new(Speed::DEFAULT),
        // Apporte, par composants requis, l'`InputBuffer` puis l'`ActionState`.
        InputMarker::<MoveInput>::default(),
        // Le joueur prédit n'avance qu'en `FixedUpdate`, soit 60 fois par
        // seconde : sans ce marqueur, il sauterait d'un tick à l'autre dès que
        // l'écran affiche plus d'images que ça. Lightyear l'affiche alors avec
        // un tick de retard, interpolé selon l'overstep de `Time<Fixed>`, en
        // `PostUpdate` et donc à chaque image. Le plugin qui l'exploite est
        // déjà installé par `LightyearAvianPlugin`, qui enregistre la
        // correction visuelle de `Position` et `Rotation`.
        FrameInterpolate,
    ));

    info!("Joueur local prêt");
}

/// Un joueur distant est apparu : il est interpolé, jamais simulé ici.
fn on_interpolated(interpolated: On<Add, Interpolated>, players: Query<&PlayerId>) {
    if let Ok(id) = players.get(interpolated.entity) {
        info!("Joueur distant visible : {:?}", id.0);
    }
}

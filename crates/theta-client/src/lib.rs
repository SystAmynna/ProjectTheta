use std::net::{Ipv4Addr, SocketAddr};

use bevy::prelude::*;
use lightyear::frame_interpolation::FrameInterpolate;
use lightyear::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::input::native::InputMarker;
use theta_core::{PlayerSimulationBundle, Speed};
use theta_protocole::token::request_token;
use theta_protocole::{MoveInput, PlayerId};
use theta_render::CameraTarget;

mod input;
mod terrain;

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

/// Côté client : saisie locale, connexion au serveur, prédiction et interpolation.
///
/// `CorePlugin` (theta-core) et `ProtocolPlugin` (theta-protocole) doivent être
/// ajoutés séparément par le binaire, ainsi que la ressource [`ClientConfig`].
pub struct ClientPlugin;

impl Plugin for ClientPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins((input::InputPlugin, terrain::TerrainPlugin)) // Ajout des plugins client
            // La prédiction est pilotée par une ressource globale.
            .init_resource::<PredictionManager>()
            .add_systems(Startup, connect) // todo : retirer la connexion du Startup quand on aura un menu principal
            .add_observer(on_predicted)
            .add_observer(on_interpolated)
            .add_observer(on_connected)
            .add_observer(on_disconnected);
    }
}

/// Connecte le client au serveur
fn connect(config: Res<ClientConfig>, mut commands: Commands) {
    // Le client cherche à s'identifier au prêt du serveur avec un token
    // Bloquant (peu durer plusieurs secondes)
    let token = match request_token(config.server) {
        Ok(token) => token,
        Err(error) => {
            error!(
                "Impossible d'obtenir un token de {} : {error}",
                config.server
            );
            return;
        }
    };

    let netcode = match NetcodeClient::new(Authentication::Token(token), NetcodeConfig::default()) {
        Ok(netcode) => netcode,
        Err(error) => {
            error!("Impossible de préparer la connexion : {error:?}");
            return;
        }
    };

    let client = commands
        .spawn((
            Name::new("Client"),
            netcode,
            // Port 0 : le système en attribue un libre.
            LocalAddr(SocketAddr::new(Ipv4Addr::UNSPECIFIED.into(), 0)),
            UdpIo::default(),
            ReplicationReceiver,
            // Non inséré automatiquement côté client, et sans lui les horloges
            // du client et du serveur ne se synchronisent pas.
            PingManager::default(),
        ))
        .id();

    commands.trigger(Connect { entity: client });

    info!("Connexion au serveur {}", config.server);
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

fn on_connected(_: On<Add, Connected>) {
    info!("Connecté au serveur");
}

fn on_disconnected(disconnected: On<Add, Disconnected>, clients: Query<&Disconnected>) {
    let Ok(state) = clients.get(disconnected.entity) else {
        return;
    };

    // `Unknown` est l'état initial d'un client qui n'a pas encore tenté de se
    // connecter : ce n'est pas une déconnexion.
    if !matches!(state.reason, DisconnectedReason::Unknown) {
        warn!("Déconnecté du serveur : {:?}", state.reason);
    }
}

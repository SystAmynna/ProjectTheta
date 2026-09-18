use std::net::SocketAddr;

use bevy::prelude::*;
use lightyear::netcode::generate_key;
use lightyear::prelude::*;
use lightyear::prelude::server::*;
use theta_core::{PlayerBundle, Speed, TICK_HZ, spawn_point};
use theta_protocole::{PROTOCOL_ID, PlayerId, player_color};
use theta_worldgen::WorldGenerator;

mod terrain;
mod token;

pub use terrain::{SUBSCRIBE_RADIUS, UNSUBSCRIBE_RADIUS};

/// Paramètres d'exécution du serveur, fournis par le binaire.
#[derive(Resource, Debug, Clone)]
pub struct ServerConfig {
    /// Adresse d'écoute.
    pub bind: SocketAddr,
    /// Graine du monde : la même graine redonne le même terrain.
    pub seed: u64,
}

// todo : refaire le système de spawn
// limiter les spawn du monde à un chunk (par défaut l'origine, si plein, le plus proche)
// prendre une tile libre alétoire en fonction de l'ID network unique du joueur dans le chunk

/// Nombre de joueurs déjà accueillis, pour répartir les points d'apparition.
///
/// Ce compteur ne décroît jamais : deux joueurs successifs n'ont pas à occuper
/// le même emplacement sous prétexte que le premier est parti.
#[derive(Resource, Debug, Default)]
struct SpawnCounter(u32);

/// Côté serveur : autorité de simulation et réplication vers les clients.
///
/// `CorePlugin` (theta-core) et `ProtocolPlugin` (theta-protocole) doivent être
/// ajoutés séparément par le binaire, ainsi que la ressource [`ServerConfig`].
pub struct ServerPlugin;

impl Plugin for ServerPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(terrain::TerrainPlugin)
            .init_resource::<SpawnCounter>()
            .add_systems(Startup, (create_world, start_listening))
            .add_observer(on_link)
            .add_observer(on_connected)
            .add_observer(on_disconnected);
    }
}

// todo : gérer les mondes, via une save, avoir plusieurs dimensions.

/// Prépare le générateur du monde. Aucun chunk n'est créé d'avance : chacun
/// l'est au moment où un joueur s'en approche.
fn create_world(config: Res<ServerConfig>, mut commands: Commands) {
    commands.insert_resource(WorldGenerator::new(config.seed));
    info!("Monde généré depuis la graine {}", config.seed);
}

/// Ouvre l'écoute UDP et démarre le serveur.
fn start_listening(config: Res<ServerConfig>, mut commands: Commands) {
    // Clé propre à cette instance : aucun client ne la connaît, ils demandent
    // leur token au service ci-dessous.
    let key = generate_key();

    if let Err(error) = token::spawn_token_service(config.bind, key) {
        error!(
            "Impossible d'ouvrir le service de tokens sur {} : {error}",
            config.bind
        );
        return;
    }

    let netcode = NetcodeServer::new(
        NetcodeConfig::default()
            .with_protocol_id(PROTOCOL_ID)
            .with_key(key),
    );

    // `ServerUdpIo` exige `Server`, qu'il ajoute lui-même, et `LocalAddr`, qui
    // doit être présent avant le démarrage.
    let server = commands
        .spawn((
            Name::new("Server"),
            netcode,
            LocalAddr(config.bind),
            ServerUdpIo::default(),
        ))
        .id();

    commands.trigger(Start { entity: server });

    info!("Serveur en écoute sur {} ({TICK_HZ} ticks/s)", config.bind);
}

/// Un client vient d'ouvrir un lien : on lui branche l'envoi de réplication.
///
/// C'est plus tôt que la connexion proprement dite ([`on_connected`]) : le
/// handshake netcode n'a pas encore eu lieu. Sans `ReplicationSender`, rien ne
/// serait jamais envoyé à ce client. Ses abonnements au terrain partent vides :
/// ils se remplissent dès que son joueur existe.
fn on_link(link: On<Add, LinkOf>, mut commands: Commands) {
    commands.entity(link.entity).insert((
        Name::new("ClientOf"),
        ReplicationSender,
        terrain::ChunkSubscriptions::default(),
    ));
}

/// Un client a terminé son handshake : on lui donne un joueur dans le monde.
fn on_connected(
    connected: On<Add, Connected>,
    clients: Query<&RemoteId, With<ClientOf>>,
    mut counter: ResMut<SpawnCounter>,
    mut commands: Commands,
) {
    // L'observer voit toutes les entités `Connected` ; seules celles marquées
    // `ClientOf` sont des clients de ce serveur.
    let Ok(remote) = clients.get(connected.entity) else {
        return;
    };
    let peer = remote.0;

    let index = counter.0;
    let spawn = spawn_point(index);
    counter.0 += 1;

    // todo : récuperer le pseudo plus tard
    commands.spawn((
        Name::new("Player"),
        PlayerId(peer),
        player_color(index),
        PlayerBundle::new(spawn, Speed::DEFAULT),
        // Tout le monde voit le joueur…
        Replicate::to_clients(NetworkTarget::All),
        // …mais seul son propriétaire le prédit ; les autres l'interpolent.
        PredictionTarget::to_clients(NetworkTarget::Single(peer)),
        InterpolationTarget::to_clients(NetworkTarget::AllExceptSingle(peer)),
        // `SessionBased` : le joueur disparaît de lui-même à la déconnexion.
        ControlledBy {
            owner: connected.entity,
            lifetime: Lifetime::SessionBased,
        },
    ));

    info!("Joueur {peer:?} connecté, apparu en {spawn:?}");
}

fn on_disconnected(
    disconnected: On<Add, Disconnected>,
    clients: Query<&RemoteId, With<ClientOf>>,
) {
    if let Ok(remote) = clients.get(disconnected.entity) {
        info!("Joueur {:?} déconnecté", remote.0);
    }
}

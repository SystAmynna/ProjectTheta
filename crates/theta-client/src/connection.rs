//! Cycle de vie de la connexion au serveur : menu, connexion, partie.
//!
//! La demande de token est bloquante (voir `request_token`) : elle tourne sur
//! l'`IoTaskPool`, et la fenêtre continue d'être rendue pendant ce temps. Le
//! client ne passe en [`AppState::InGame`] qu'une fois le handshake netcode
//! terminé, et revient au menu à la moindre déconnexion.

use std::io;
use std::net::{Ipv4Addr, SocketAddr};

use bevy::prelude::*;
use bevy::tasks::futures::check_ready;
use bevy::tasks::{IoTaskPool, Task};
use lightyear::netcode::ConnectToken;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use theta_protocole::token::request_token;

use crate::ClientConfig;

/// Étape du client.
#[derive(States, Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub enum AppState {
    /// Aucune connexion : le joueur choisit de rejoindre le serveur.
    #[default]
    Menu,
    /// Demande de token puis handshake netcode en cours.
    Connecting,
    /// Connecté : le monde est reçu et le joueur local simulé.
    InGame,
}

/// Raison du dernier échec de connexion, affichée par le menu.
#[derive(Resource, Debug, Default)]
pub struct ConnectionError(pub Option<String>);

/// Demande de token en cours, sur l'`IoTaskPool`.
#[derive(Resource)]
struct TokenRequest(Task<io::Result<ConnectToken>>);

pub(crate) struct ConnectionPlugin;

impl Plugin for ConnectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_state::<AppState>()
            .init_resource::<ConnectionError>()
            .add_systems(OnEnter(AppState::Connecting), start_token_request)
            .add_systems(
                Update,
                poll_token_request.run_if(resource_exists::<TokenRequest>),
            )
            // Quitter l'étape abandonne une demande restée sans réponse.
            .add_systems(OnExit(AppState::Connecting), |mut commands: Commands| {
                commands.remove_resource::<TokenRequest>();
            })
            .add_observer(on_connected)
            .add_observer(on_disconnected);
    }
}

/// Lance la demande de token, sans bloquer la boucle de rendu.
fn start_token_request(
    config: Res<ClientConfig>,
    mut error: ResMut<ConnectionError>,
    mut commands: Commands,
) {
    error.0 = None;
    let server = config.server;
    let task = IoTaskPool::get().spawn(async move { request_token(server) });
    commands.insert_resource(TokenRequest(task));
    info!("Demande de token à {server}");
}

/// Relève la réponse du serveur dès qu'elle est arrivée, puis ouvre la
/// connexion UDP ou revient au menu.
fn poll_token_request(
    mut request: ResMut<TokenRequest>,
    config: Res<ClientConfig>,
    mut error: ResMut<ConnectionError>,
    mut next: ResMut<NextState<AppState>>,
    mut commands: Commands,
) {
    let Some(result) = check_ready(&mut request.0) else {
        return;
    };
    commands.remove_resource::<TokenRequest>();

    let opened = result
        .map_err(|e| format!("impossible d'obtenir un token de {} : {e}", config.server))
        .and_then(|token| open_connection(token, &mut commands));
    if let Err(message) = opened {
        error!("{message}");
        error.0 = Some(message);
        next.set(AppState::Menu);
    }
}

/// Crée l'entité du client lightyear et démarre le handshake avec le token.
///
/// L'entité disparaît au retour au menu : chaque tentative repart d'une
/// entité neuve.
fn open_connection(token: ConnectToken, commands: &mut Commands) -> Result<(), String> {
    let netcode = NetcodeClient::new(Authentication::Token(token), NetcodeConfig::default())
        .map_err(|e| format!("impossible de préparer la connexion : {e:?}"))?;

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
            DespawnOnEnter(AppState::Menu),
        ))
        .id();
    commands.trigger(Connect { entity: client });
    Ok(())
}

fn on_connected(_: On<Add, Connected>, mut next: ResMut<NextState<AppState>>) {
    info!("Connecté au serveur");
    next.set(AppState::InGame);
}

/// Une déconnexion, ou un handshake qui échoue, ramène au menu.
fn on_disconnected(
    disconnected: On<Add, Disconnected>,
    clients: Query<&Disconnected>,
    mut error: ResMut<ConnectionError>,
    mut next: ResMut<NextState<AppState>>,
) {
    let Ok(state) = clients.get(disconnected.entity) else {
        return;
    };

    // `Unknown` est l'état initial d'un client qui n'a pas encore tenté de se
    // connecter : ce n'est pas une déconnexion.
    if matches!(state.reason, DisconnectedReason::Unknown) {
        return;
    }
    warn!("Déconnecté du serveur : {}", state.reason);
    error.0 = Some(format!("déconnecté : {}", state.reason));
    next.set(AppState::Menu);
}

#[cfg(test)]
mod tests {
    use std::net::TcpListener;
    use std::time::{Duration, Instant};

    use bevy::state::app::StatesPlugin;

    use super::*;

    #[test]
    fn unreachable_server_returns_to_the_menu_with_an_error() {
        // Un port qu'on vient de libérer : la connexion y est refusée.
        let server = {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            listener.local_addr().unwrap()
        };

        let mut app = App::new();
        app.add_plugins((MinimalPlugins, StatesPlugin, ConnectionPlugin))
            .insert_resource(ClientConfig { server });
        app.update();

        app.world_mut()
            .resource_mut::<NextState<AppState>>()
            .set(AppState::Connecting);
        app.update();
        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::Connecting
        );

        let deadline = Instant::now() + Duration::from_secs(10);
        while app.world().resource::<ConnectionError>().0.is_none() {
            assert!(
                Instant::now() < deadline,
                "la demande de token n'aboutit pas"
            );
            std::thread::sleep(Duration::from_millis(10));
            app.update();
        }
        app.update();

        assert_eq!(
            *app.world().resource::<State<AppState>>().get(),
            AppState::Menu
        );
        assert!(!app.world().contains_resource::<TokenRequest>());
    }
}

//! Terrain côté serveur : génération à la demande et abonnements des clients.
//!
//! Chaque client est **abonné** aux chunks proches de son joueur. Il reçoit un
//! chunk en entier quand celui-ci entre dans son rayon, puis seulement ses
//! modifications ; quand le chunk sort d'un rayon un peu plus large, le client
//! reçoit l'ordre de l'oublier et le serveur cesse de lui en parler. Tout passe
//! par [`TerrainUpdate`], sur le canal ordonné [`TerrainChannel`].
//!
//! Le serveur ne garde en mémoire que les chunks observés, plus ceux qu'une
//! modification a rendus uniques : un chunk jamais modifié qui n'a plus
//! d'abonné est oublié, et sera régénéré à l'identique depuis la graine.

use avian2d::prelude::Position;
use bevy::ecs::system::SystemParam;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;
use lightyear::prelude::server::*;
use lightyear::prelude::*;
use theta_core::{
    ChunkCoord, ChunkTiles, Player, PlayerSystems, Terrain, TerrainIndex, TileEdit, TileKind,
    spawn_chunk,
};
use theta_protocole::{TerrainChannel, TerrainUpdate};
use theta_worldgen::WorldGenerator;

/// Rayon d'abonnement, en chunks (distance de Chebyshev) : le client reçoit les
/// 3 × 3 chunks autour du sien.
///
/// Le bord du terrain connu est donc toujours à plus d'un chunk du joueur
/// (`CHUNK_WORLD_SIZE`, 2048 px), bien plus que la demi-largeur de la zone
/// visible (960 px, `VIEW_SIZE` dans `theta-render`) : aucun trou n'est jamais
/// visible. À vitesse normale, il faut près de 7 s pour traverser un chunk, ce
/// qui laisse au suivant tout le temps d'arriver.
pub const SUBSCRIBE_RADIUS: u32 = 1;

/// Au-delà de ce rayon, un chunk est oublié par le client.
///
/// Plus grand que [`SUBSCRIBE_RADIUS`] : un joueur qui longe la frontière d'un
/// chunk ne se le fait pas envoyer puis retirer en boucle.
pub const UNSUBSCRIBE_RADIUS: u32 = 2;

/// Nombre maximal de chunks envoyés à un même client en un tick.
///
/// À la connexion, les neuf chunks partent sur trois ticks plutôt que d'un bloc,
/// le plus proche d'abord : celui du joueur arrive en premier.
const MAX_SNAPSHOTS_PER_TICK: usize = 4;

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PendingChunkEdits>()
            .init_resource::<OutgoingTerrain>()
            .add_systems(
                FixedUpdate,
                // Les modifications partent avant les nouveaux abonnements : un
                // client qui découvre un chunk ce tick-ci reçoit un contenu qui
                // les inclut déjà, sans les recevoir en plus.
                (
                    apply_tile_edits,
                    queue_chunk_edits,
                    update_subscriptions,
                    unload_unobserved_chunks,
                    send_terrain,
                )
                    .chain()
                    .after(PlayerSystems::Collide),
            )
            .add_observer(on_link_disconnected);
    }
}

/// Chunks auxquels un client est abonné. Porté par son entité de lien.
#[derive(Component, Debug, Default)]
pub(crate) struct ChunkSubscriptions(HashSet<ChunkCoord>);

/// Nombre de clients abonnés à un chunk.
#[derive(Component, Debug, Default)]
struct ChunkSubscribers(u32);

/// Le chunk a été modifié : il diffère de ce que redonnerait la graine, et doit
/// donc rester en mémoire même sans abonné.
#[derive(Component, Debug, Default)]
struct ChunkModified;

/// Modifications du tick, par chunk, en attente d'envoi aux abonnés.
#[derive(Resource, Debug, Default)]
struct PendingChunkEdits(HashMap<ChunkCoord, Vec<(u16, TileKind)>>);

/// Messages de terrain du tick, dans l'ordre où ils doivent partir, avec leurs
/// destinataires.
///
/// La tenue des abonnements ne fait que remplir cette file ; seul
/// [`send_terrain`] parle au réseau. L'ordre de la file est celui du canal : pour
/// un client donné, un `Unload` puis un `Snapshot` du même chunk arrivent dans
/// cet ordre.
#[derive(Resource, Debug, Default)]
struct OutgoingTerrain(Vec<(Vec<PeerId>, TerrainUpdate)>);

/// Chunks du serveur, du point de vue des abonnements.
#[derive(SystemParam)]
struct SubscribedChunks<'w, 's> {
    index: Res<'w, TerrainIndex>,
    chunks: Query<'w, 's, (&'static ChunkTiles, &'static mut ChunkSubscribers)>,
    generator: Res<'w, WorldGenerator>,
}

impl SubscribedChunks<'_, '_> {
    /// Ajoute un abonné à un chunk en mémoire et renvoie son contenu, ou `None`
    /// s'il faut d'abord le générer.
    fn subscribe(&mut self, chunk: ChunkCoord) -> Option<Vec<TileKind>> {
        let entity = self.index.get(chunk)?;
        let (tiles, mut subscribers) = self.chunks.get_mut(entity).ok()?;
        subscribers.0 += 1;
        Some(tiles.as_slice().to_vec())
    }

    /// Contenu d'un chunk qui n'est pas en mémoire.
    fn generate(&self, chunk: ChunkCoord) -> ChunkTiles {
        self.generator.generate_chunk(chunk)
    }

    fn unsubscribe(&mut self, chunk: ChunkCoord) {
        if let Some(entity) = self.index.get(chunk)
            && let Ok((_, mut subscribers)) = self.chunks.get_mut(entity)
        {
            subscribers.0 = subscribers.0.saturating_sub(1);
        }
    }
}

/// Applique les [`TileEdit`] émis par le gameplay.
///
/// Une modification sur un chunk qui n'est pas en mémoire le génère d'abord :
/// personne n'y est abonné, il n'y a donc rien à envoyer, mais il faut le garder.
fn apply_tile_edits(
    mut edits: MessageReader<TileEdit>,
    mut terrain: Terrain,
    generator: Res<WorldGenerator>,
    mut pending: ResMut<PendingChunkEdits>,
    mut commands: Commands,
    mut unloaded: Local<HashMap<ChunkCoord, (ChunkTiles, bool)>>,
) {
    for edit in edits.read() {
        let chunk = edit.coord.chunk();
        let index = edit.coord.local_index();

        match terrain.set(edit.coord, edit.kind) {
            Some(true) => {
                if let Some(entity) = terrain.chunk_entity(chunk) {
                    commands.entity(entity).insert(ChunkModified);
                }
                pending
                    .0
                    .entry(chunk)
                    .or_default()
                    .push((index as u16, edit.kind));
            }
            Some(false) => {}
            None => {
                let (tiles, changed) = unloaded
                    .entry(chunk)
                    .or_insert_with(|| (generator.generate_chunk(chunk), false));
                *changed |= tiles.set(index, edit.kind);
            }
        }
    }

    for (chunk, (tiles, changed)) in unloaded.drain() {
        if changed {
            spawn_chunk(&mut commands, chunk, tiles).insert((ChunkSubscribers(0), ChunkModified));
        }
    }
}

/// Adresse les modifications du tick aux seuls clients abonnés à chaque chunk.
fn queue_chunk_edits(
    mut pending: ResMut<PendingChunkEdits>,
    links: Query<(&RemoteId, &ChunkSubscriptions)>,
    mut outgoing: ResMut<OutgoingTerrain>,
) {
    for (chunk, edits) in pending.0.drain() {
        let subscribers: Vec<PeerId> = links
            .iter()
            .filter(|(_, subscriptions)| subscriptions.0.contains(&chunk))
            .map(|(remote, _)| remote.0)
            .collect();
        if !subscribers.is_empty() {
            outgoing
                .0
                .push((subscribers, TerrainUpdate::Edits { chunk, edits }));
        }
    }
}

/// Met à jour les abonnements de chaque client selon la position de son joueur.
///
/// Recalculé à chaque tick : avec au plus 9 chunks voulus et 25 abonnés par
/// joueur, c'est moins cher que de suivre les changements de chunk — et un
/// abonnement que le plafond [`MAX_SNAPSHOTS_PER_TICK`] a retardé est
/// naturellement repris au tick suivant.
fn update_subscriptions(
    players: Query<(&Position, &ControlledBy), With<Player>>,
    mut links: Query<(&RemoteId, &mut ChunkSubscriptions)>,
    mut loaded: SubscribedChunks,
    mut outgoing: ResMut<OutgoingTerrain>,
    mut commands: Commands,
    mut generated: Local<HashMap<ChunkCoord, (ChunkTiles, u32)>>,
) {
    for (position, controlled) in &players {
        let Ok((remote, mut subscriptions)) = links.get_mut(controlled.owner) else {
            continue;
        };
        let peer = remote.0;
        let center = ChunkCoord::from_world(position.0);
        let mut send = |update: TerrainUpdate| outgoing.0.push((vec![peer], update));

        let (leaving, entering) = subscription_changes(&subscriptions.0, center);

        for chunk in leaving {
            subscriptions.0.remove(&chunk);
            loaded.unsubscribe(chunk);
            send(TerrainUpdate::Unload { chunk });
        }

        for chunk in entering {
            // Pas encore en mémoire : généré une seule fois, même si plusieurs
            // joueurs le découvrent pendant le même tick.
            let tiles = loaded.subscribe(chunk).unwrap_or_else(|| {
                let (tiles, subscribers) = generated
                    .entry(chunk)
                    .or_insert_with(|| (loaded.generate(chunk), 0));
                *subscribers += 1;
                tiles.as_slice().to_vec()
            });

            send(TerrainUpdate::Snapshot { chunk, tiles });
            subscriptions.0.insert(chunk);
        }
    }

    for (chunk, (tiles, subscribers)) in generated.drain() {
        spawn_chunk(&mut commands, chunk, tiles).insert(ChunkSubscribers(subscribers));
    }
}

/// Un chunk dont le nombre d'abonnés vient de changer, et que la graine saurait
/// refaire à l'identique.
type RegenerableChunk = (Changed<ChunkSubscribers>, Without<ChunkModified>);

/// Oublie les chunks que plus personne n'observe et que la graine sait refaire.
fn unload_unobserved_chunks(
    chunks: Query<(Entity, &ChunkSubscribers), RegenerableChunk>,
    mut commands: Commands,
) {
    for (entity, subscribers) in &chunks {
        if subscribers.0 == 0 {
            commands.entity(entity).despawn();
        }
    }
}

/// Envoie, dans l'ordre, les messages de terrain accumulés pendant le tick.
///
/// Sans serveur démarré, il n'y a personne à qui les envoyer : la file est
/// simplement vidée.
fn send_terrain(
    mut outgoing: ResMut<OutgoingTerrain>,
    servers: Query<&Server>,
    mut sender: ServerMultiMessageSender,
) {
    let Ok(server) = servers.single() else {
        outgoing.0.clear();
        return;
    };

    for (peers, update) in outgoing.0.drain(..) {
        let target = match peers.as_slice() {
            &[peer] => NetworkTarget::Single(peer),
            _ => NetworkTarget::Only(peers.into()),
        };
        if let Err(error) = sender.send::<_, TerrainChannel>(&update, server, &target) {
            error!("Envoi du terrain impossible : {error}");
        }
    }
}

/// Un client parti libère ses abonnements.
fn on_link_disconnected(
    disconnected: On<Add, Disconnected>,
    mut links: Query<&mut ChunkSubscriptions>,
    mut loaded: SubscribedChunks,
) {
    let Ok(mut subscriptions) = links.get_mut(disconnected.entity) else {
        return;
    };

    for chunk in std::mem::take(&mut subscriptions.0) {
        loaded.unsubscribe(chunk);
    }
}

/// Chunks à oublier et chunks à envoyer pour un client centré sur `center`.
///
/// Les nouveaux abonnements sont triés du plus proche au plus lointain et
/// plafonnés à [`MAX_SNAPSHOTS_PER_TICK`].
fn subscription_changes(
    subscribed: &HashSet<ChunkCoord>,
    center: ChunkCoord,
) -> (Vec<ChunkCoord>, Vec<ChunkCoord>) {
    let leaving = subscribed
        .iter()
        .filter(|chunk| chunk.distance(center) > UNSUBSCRIBE_RADIUS)
        .copied()
        .collect();
    let entering = chunks_around(center, SUBSCRIBE_RADIUS)
        .filter(|chunk| !subscribed.contains(chunk))
        .take(MAX_SNAPSHOTS_PER_TICK)
        .collect();

    (leaving, entering)
}

/// Chunks à distance au plus `radius` de `center`, du plus proche au plus
/// lointain : celui du joueur d'abord, puis ses voisins directs.
fn chunks_around(center: ChunkCoord, radius: u32) -> impl Iterator<Item = ChunkCoord> {
    let radius = radius as i32;
    let mut chunks: Vec<ChunkCoord> = (-radius..=radius)
        .flat_map(|x| (-radius..=radius).map(move |y| ChunkCoord(center.0 + IVec2::new(x, y))))
        .collect();
    chunks.sort_by_key(|chunk| (chunk.0 - center.0).length_squared());
    chunks.into_iter()
}

#[cfg(test)]
mod tests {
    use bevy::time::TimeUpdateStrategy;
    use theta_core::{CorePlugin, TileCoord, tick_duration};

    use super::*;

    #[test]
    fn chunks_around_starts_with_the_center() {
        let center = ChunkCoord::new(4, -2);
        let chunks: Vec<_> = chunks_around(center, SUBSCRIBE_RADIUS).collect();

        assert_eq!(chunks.len(), 9);
        assert_eq!(chunks[0], center);
        assert!(
            chunks
                .iter()
                .all(|chunk| chunk.distance(center) <= SUBSCRIBE_RADIUS)
        );
        // Les voisins directs avant les diagonales.
        assert!(
            chunks[1..5]
                .iter()
                .all(|chunk| (chunk.0 - center.0).length_squared() == 1)
        );
    }

    /// Applique les changements d'abonnement jusqu'à stabilité, comme le
    /// ferait une suite de ticks, et renvoie ce qui a été envoyé puis oublié.
    fn settle(
        subscribed: &mut HashSet<ChunkCoord>,
        center: ChunkCoord,
    ) -> (Vec<ChunkCoord>, Vec<ChunkCoord>) {
        let (mut sent, mut forgotten) = (Vec::new(), Vec::new());
        loop {
            let (leaving, entering) = subscription_changes(subscribed, center);
            if leaving.is_empty() && entering.is_empty() {
                return (sent, forgotten);
            }
            for chunk in &leaving {
                subscribed.remove(chunk);
            }
            subscribed.extend(entering.iter().copied());
            sent.extend(entering);
            forgotten.extend(leaving);
        }
    }

    #[test]
    fn walking_subscribes_ahead_and_forgets_behind() {
        let mut subscribed = HashSet::new();

        // Arrivée : les 9 chunks autour du joueur, en plusieurs ticks.
        let (sent, _) = settle(&mut subscribed, ChunkCoord::new(0, 0));
        assert_eq!(sent.len(), 9);
        assert_eq!(sent[0], ChunkCoord::new(0, 0), "le chunk du joueur d'abord");
        assert_eq!(
            subscription_changes(&HashSet::new(), ChunkCoord::new(0, 0))
                .1
                .len(),
            MAX_SNAPSHOTS_PER_TICK
        );

        // Un chunk vers l'est : 3 nouveaux, rien d'oublié (hystérésis).
        let (sent, forgotten) = settle(&mut subscribed, ChunkCoord::new(1, 0));
        assert_eq!(sent.len(), 3);
        assert!(sent.iter().all(|chunk| chunk.0.x == 2));
        assert!(forgotten.is_empty());

        // Encore un : la colonne x = -1 est maintenant à 3 chunks, oubliée.
        let (sent, forgotten) = settle(&mut subscribed, ChunkCoord::new(2, 0));
        assert_eq!(sent.len(), 3);
        assert_eq!(forgotten.len(), 3);
        assert!(forgotten.iter().all(|chunk| chunk.0.x == -1));

        // Demi-tour d'un chunk : tout est encore là, rien ne circule.
        let (sent, forgotten) = settle(&mut subscribed, ChunkCoord::new(1, 0));
        assert!(sent.is_empty() && forgotten.is_empty());
    }

    /// Serveur sans réseau : le cœur, et un temps qui avance d'un tick à chaque
    /// `update`. Reprend les systèmes de [`TerrainPlugin`] sauf
    /// [`send_terrain`], qui exige lightyear : les messages restent dans
    /// [`OutgoingTerrain`], où [`sent`] les relève.
    fn server() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CorePlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(tick_duration()))
            .insert_resource(WorldGenerator::new(42))
            // Exigée par lightyear à l'ajout de `Disconnected`.
            .init_resource::<PeerMetadata>()
            .init_resource::<PendingChunkEdits>()
            .init_resource::<OutgoingTerrain>()
            .add_systems(
                FixedUpdate,
                (
                    apply_tile_edits,
                    queue_chunk_edits,
                    update_subscriptions,
                    unload_unobserved_chunks,
                )
                    .chain()
                    .after(PlayerSystems::Collide),
            )
            .add_observer(on_link_disconnected);
        app.finish();
        app.cleanup();
        app
    }

    /// Un client relié, dont le joueur se tient en `position`.
    fn join(app: &mut App, id: u64, position: Vec2) -> (Entity, Entity) {
        let link = app
            .world_mut()
            .spawn((RemoteId(PeerId::Netcode(id)), ChunkSubscriptions::default()))
            .id();
        let player = app
            .world_mut()
            .spawn((
                Player,
                Position(position),
                ControlledBy {
                    owner: link,
                    lifetime: Lifetime::SessionBased,
                },
            ))
            .id();
        (link, player)
    }

    /// Déconnecte un client comme le fait lightyear : `Disconnected` sur le
    /// lien, et despawn de son joueur, dont la durée de vie est liée à la
    /// session.
    fn leave(app: &mut App, (link, player): (Entity, Entity)) {
        app.world_mut().entity_mut(link).insert(Disconnected {
            reason: DisconnectedReason::Unknown,
        });
        app.world_mut().despawn(player);
    }

    /// Fait tourner assez de ticks pour que les abonnements se stabilisent.
    fn settle_ticks(app: &mut App) {
        for _ in 0..4 {
            app.update();
        }
    }

    /// Vide la file des messages et la renvoie.
    fn sent(app: &mut App) -> Vec<(Vec<PeerId>, TerrainUpdate)> {
        std::mem::take(&mut app.world_mut().resource_mut::<OutgoingTerrain>().0)
    }

    fn subscribers(app: &mut App, chunk: ChunkCoord) -> Option<u32> {
        let entity = app.world().resource::<TerrainIndex>().get(chunk)?;
        app.world().get::<ChunkSubscribers>(entity).map(|s| s.0)
    }

    fn edit(app: &mut App, coord: TileCoord, kind: TileKind) {
        app.world_mut().write_message(TileEdit { coord, kind });
    }

    #[test]
    fn a_player_receives_the_nine_chunks_around_it() {
        let mut app = server();
        join(&mut app, 1, Vec2::ZERO);
        settle_ticks(&mut app);

        let snapshots = sent(&mut app)
            .into_iter()
            .filter(|(_, update)| matches!(update, TerrainUpdate::Snapshot { .. }))
            .count();
        assert_eq!(snapshots, 9);
        assert_eq!(app.world().resource::<TerrainIndex>().len(), 9);
    }

    #[test]
    fn shared_chunks_count_each_subscriber_and_survive_a_departure() {
        let mut app = server();
        let first = join(&mut app, 1, Vec2::ZERO);
        join(&mut app, 2, Vec2::ZERO);
        settle_ticks(&mut app);
        assert_eq!(subscribers(&mut app, ChunkCoord::new(0, 0)), Some(2));

        leave(&mut app, first);
        app.update();
        assert_eq!(subscribers(&mut app, ChunkCoord::new(0, 0)), Some(1));
    }

    #[test]
    fn unobserved_chunks_are_forgotten_unless_modified() {
        let mut app = server();
        let client = join(&mut app, 1, Vec2::ZERO);
        settle_ticks(&mut app);

        let modified = ChunkCoord::new(1, 1).tile(UVec2::ZERO);
        edit(&mut app, modified, TileKind::Wall);
        edit(&mut app, modified, TileKind::Floor);
        edit(&mut app, modified, TileKind::Wall);
        app.update();

        leave(&mut app, client);
        app.update();

        let index = app.world().resource::<TerrainIndex>();
        assert!(
            index.contains(modified.chunk()),
            "le chunk modifié est gardé"
        );
        assert_eq!(index.len(), 1, "les autres sont oubliés");
    }

    #[test]
    fn edits_reach_subscribers_only_when_something_changed() {
        let mut app = server();
        join(&mut app, 1, Vec2::ZERO);
        join(&mut app, 2, Vec2::splat(CHUNK_FAR));
        settle_ticks(&mut app);
        sent(&mut app);

        // La tile vaut déjà ce sol, garanti autour du point d'apparition.
        let tile = TileCoord::new(0, 0);
        edit(&mut app, tile, TileKind::Floor);
        app.update();
        assert!(
            sent(&mut app).is_empty(),
            "une édition sans effet ne part pas"
        );

        edit(&mut app, tile, TileKind::Wall);
        app.update();
        assert_eq!(
            sent(&mut app),
            vec![(
                vec![PeerId::Netcode(1)],
                TerrainUpdate::Edits {
                    chunk: tile.chunk(),
                    edits: vec![(tile.local_index() as u16, TileKind::Wall)],
                }
            )]
        );
    }

    #[test]
    fn editing_an_unloaded_chunk_keeps_it_in_memory() {
        let mut app = server();
        let tile = TileCoord::new(10_000, 10_000);
        let kind = match WorldGenerator::new(42).tile(tile) {
            TileKind::Wall => TileKind::Floor,
            TileKind::Floor => TileKind::Wall,
        };

        edit(&mut app, tile, kind);
        app.update();
        app.update();

        let entity = app
            .world()
            .resource::<TerrainIndex>()
            .get(tile.chunk())
            .expect("le chunk modifié est créé");
        assert!(app.world().get::<ChunkModified>(entity).is_some());
        assert_eq!(
            app.world()
                .get::<ChunkTiles>(entity)
                .unwrap()
                .get(tile.local_index()),
            kind
        );
    }

    /// Assez loin de l'origine pour ne partager aucun chunk avec elle.
    const CHUNK_FAR: f32 = 10.0 * theta_core::terrain::CHUNK_WORLD_SIZE;
}

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
/// Avec des chunks de 1024 px, le bord du terrain connu est toujours à plus de
/// 1024 px du joueur — plus que la demi-largeur de la fenêtre (640 px) : aucun
/// trou n'est jamais visible. Il faut plus de 3 s pour l'atteindre, ce qui laisse
/// au chunk suivant tout le temps d'arriver.
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
            .add_systems(
                FixedUpdate,
                // Les modifications partent avant les nouveaux abonnements : un
                // client qui découvre un chunk ce tick-ci reçoit un contenu qui
                // les inclut déjà, sans les recevoir en plus.
                (
                    apply_tile_edits,
                    send_chunk_edits,
                    update_subscriptions,
                    unload_unobserved_chunks,
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

/// Envoie les modifications du tick aux seuls clients abonnés à chaque chunk.
fn send_chunk_edits(
    mut pending: ResMut<PendingChunkEdits>,
    links: Query<(&RemoteId, &ChunkSubscriptions)>,
    servers: Query<&Server>,
    mut sender: ServerMultiMessageSender,
) {
    let Ok(server) = servers.single() else {
        pending.0.clear();
        return;
    };

    for (chunk, edits) in pending.0.drain() {
        let subscribers: Vec<PeerId> = links
            .iter()
            .filter(|(_, subscriptions)| subscriptions.0.contains(&chunk))
            .map(|(remote, _)| remote.0)
            .collect();
        if subscribers.is_empty() {
            continue;
        }

        let update = TerrainUpdate::Edits { chunk, edits };
        if let Err(error) = sender.send::<_, TerrainChannel>(
            &update,
            server,
            &NetworkTarget::Only(subscribers.into()),
        ) {
            error!(
                "Envoi des modifications du chunk {:?} impossible : {error}",
                chunk.0
            );
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
    servers: Query<&Server>,
    mut sender: ServerMultiMessageSender,
    mut commands: Commands,
    mut generated: Local<HashMap<ChunkCoord, (ChunkTiles, u32)>>,
) {
    let Ok(server) = servers.single() else {
        return;
    };

    for (position, controlled) in &players {
        let Ok((remote, mut subscriptions)) = links.get_mut(controlled.owner) else {
            continue;
        };
        let target = NetworkTarget::Single(remote.0);
        let center = ChunkCoord::from_world(position.0);
        let mut send = |update: TerrainUpdate| {
            if let Err(error) = sender.send::<_, TerrainChannel>(&update, server, &target) {
                error!("Envoi du terrain à {:?} impossible : {error}", remote.0);
            }
        };

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
}

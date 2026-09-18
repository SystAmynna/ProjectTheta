//! Terrain côté client : il n'est jamais généré ici, seulement reçu.
//!
//! Le serveur envoie les chunks proches du joueur ([`TerrainUpdate::Snapshot`]),
//! puis leurs modifications ([`TerrainUpdate::Edits`]), puis l'ordre de les
//! oublier ([`TerrainUpdate::Unload`]). Le client applique tout, dans l'ordre, et
//! ne modifie jamais une tile de lui-même.

use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use lightyear::prelude::client::*;
use lightyear::prelude::*;
use theta_core::terrain::CHUNK_AREA;
use theta_core::{ChunkCoord, ChunkTiles, TerrainChunk, TerrainIndex, spawn_chunk};
use theta_protocole::{TerrainUpdate, TileChange};

pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        // En `PreUpdate`, dès la réception, et pas en `FixedPreUpdate` :
        // lightyear vide les messages non lus à la fin de chaque image, et une
        // image sans tick de simulation les perdrait. Le collider suit au
        // prochain tick, sur `Changed<ChunkTiles>`.
        app.add_systems(PreUpdate, receive_terrain.after(MessageSystems::Receive))
            .add_observer(forget_terrain);
    }
}

/// État d'un chunk au fil d'un lot de messages, avant de toucher au monde.
#[derive(Debug, PartialEq)]
enum Staged {
    /// Contenu complet reçu dans le lot, modifications ultérieures comprises.
    Loaded(ChunkTiles),
    /// Modifications d'un chunk déjà présent dans le monde, à lui appliquer.
    Edited(Vec<TileChange>),
    /// Le chunk doit être oublié.
    Unloaded,
}

/// Applique les mises à jour du terrain reçues du serveur.
///
/// Un même lot peut enchaîner, pour un chunk, son contenu puis ses
/// modifications, voire son oubli : la création d'une entité étant différée, le
/// lot est d'abord replié par [`stage`], chunk par chunk, puis appliqué au monde.
fn receive_terrain(
    mut receivers: Query<&mut MessageReceiver<TerrainUpdate>, With<Client>>,
    index: Res<TerrainIndex>,
    mut chunks: Query<&mut ChunkTiles>,
    mut commands: Commands,
    mut staged: Local<HashMap<ChunkCoord, Staged>>,
) {
    for mut receiver in &mut receivers {
        for update in receiver.receive() {
            stage(&mut staged, update);
        }
    }

    for (chunk, state) in staged.drain() {
        let loaded = index.get(chunk);
        match state {
            Staged::Loaded(tiles) => match loaded.and_then(|e| chunks.get_mut(e).ok()) {
                Some(mut current) => *current = tiles,
                None => {
                    spawn_chunk(&mut commands, chunk, tiles);
                }
            },
            Staged::Edited(edits) => match loaded.and_then(|e| chunks.get_mut(e).ok()) {
                Some(mut tiles) => apply(&mut tiles, &edits),
                // Impossible sur un canal ordonné : le contenu précède
                // toujours ses modifications.
                None => warn!("Modifications reçues pour le chunk inconnu {:?}", chunk.0),
            },
            Staged::Unloaded => {
                if let Some(entity) = loaded {
                    commands.entity(entity).despawn();
                }
            }
        }
    }
}

/// Applique des modifications déjà validées par [`stage`].
fn apply(tiles: &mut ChunkTiles, edits: &[TileChange]) {
    for edit in edits {
        tiles.set(edit.layer, usize::from(edit.index), edit.kind);
    }
}

/// Replie un message dans l'état du lot en cours.
///
/// Un contenu complet ou un oubli remplace tout ce qui précède pour ce chunk ;
/// des modifications s'ajoutent au contenu ou aux modifications déjà reçus. Les
/// index hors du chunk sont écartés : un message malformé ne doit pas faire
/// paniquer le client. Un contenu complet, lui, a déjà été validé à la
/// désérialisation.
fn stage(staged: &mut HashMap<ChunkCoord, Staged>, update: TerrainUpdate) {
    match update {
        TerrainUpdate::Snapshot { chunk, tiles } => {
            staged.insert(chunk, Staged::Loaded(tiles));
        }
        TerrainUpdate::Edits { chunk, edits } => {
            let edits: Vec<TileChange> = edits
                .into_iter()
                .filter(|edit| {
                    let valid = usize::from(edit.index) < CHUNK_AREA;
                    if !valid {
                        warn!("Tile {} hors du chunk {:?} ignorée", edit.index, chunk.0);
                    }
                    valid
                })
                .collect();
            match staged
                .entry(chunk)
                .or_insert_with(|| Staged::Edited(Vec::new()))
            {
                Staged::Loaded(tiles) => apply(tiles, &edits),
                Staged::Edited(pending) => pending.extend(edits),
                Staged::Unloaded => {
                    warn!("Modifications reçues pour le chunk oublié {:?}", chunk.0);
                }
            }
        }
        TerrainUpdate::Unload { chunk } => {
            staged.insert(chunk, Staged::Unloaded);
        }
    }
}

/// Une déconnexion rend le terrain connu caduc : il sera renvoyé en entier à la
/// prochaine connexion.
fn forget_terrain(
    _: On<Add, Disconnected>,
    chunks: Query<Entity, With<TerrainChunk>>,
    mut commands: Commands,
) {
    for entity in &chunks {
        commands.entity(entity).despawn();
    }
}

#[cfg(test)]
mod tests {
    use theta_core::{TileKind, TileLayer};

    use super::*;

    const CHUNK: ChunkCoord = ChunkCoord::new(2, -1);

    fn snapshot(tiles: ChunkTiles) -> TerrainUpdate {
        TerrainUpdate::Snapshot {
            chunk: CHUNK,
            tiles,
        }
    }

    fn wall(index: u16) -> TileChange {
        TileChange {
            layer: TileLayer::Object,
            index,
            kind: TileKind::Wall,
        }
    }

    fn edits(edits: Vec<TileChange>) -> TerrainUpdate {
        TerrainUpdate::Edits {
            chunk: CHUNK,
            edits,
        }
    }

    /// Replie un lot entier et renvoie l'état obtenu pour [`CHUNK`].
    fn fold(updates: Vec<TerrainUpdate>) -> Option<Staged> {
        let mut staged = HashMap::default();
        for update in updates {
            stage(&mut staged, update);
        }
        staged.remove(&CHUNK)
    }

    #[test]
    fn edits_in_the_same_batch_modify_the_snapshot() {
        let mut expected = ChunkTiles::default();
        expected.set(TileLayer::Object, 7, TileKind::Wall);

        let state = fold(vec![snapshot(ChunkTiles::default()), edits(vec![wall(7)])]);
        assert_eq!(state, Some(Staged::Loaded(expected)));
    }

    #[test]
    fn snapshot_after_unload_reloads_the_chunk() {
        let state = fold(vec![
            TerrainUpdate::Unload { chunk: CHUNK },
            snapshot(ChunkTiles::default()),
        ]);
        assert_eq!(state, Some(Staged::Loaded(ChunkTiles::default())));
    }

    #[test]
    fn unload_discards_what_came_before() {
        let state = fold(vec![
            snapshot(ChunkTiles::default()),
            TerrainUpdate::Unload { chunk: CHUNK },
            edits(vec![wall(0)]),
        ]);
        assert_eq!(state, Some(Staged::Unloaded));
    }

    #[test]
    fn edits_without_snapshot_accumulate_for_the_world() {
        let state = fold(vec![edits(vec![wall(1)]), edits(vec![wall(2)])]);
        assert_eq!(state, Some(Staged::Edited(vec![wall(1), wall(2)])));
    }

    #[test]
    fn out_of_range_edits_are_ignored() {
        let state = fold(vec![
            snapshot(ChunkTiles::default()),
            edits(vec![wall(CHUNK_AREA as u16), wall(u16::MAX)]),
        ]);
        assert_eq!(state, Some(Staged::Loaded(ChunkTiles::default())));
    }
}

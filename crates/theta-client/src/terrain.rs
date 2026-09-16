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
use theta_core::{ChunkCoord, ChunkTiles, TerrainChunk, TerrainIndex, spawn_chunk};
use theta_protocole::TerrainUpdate;

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
enum Staged {
    Loaded(ChunkTiles),
    Unloaded,
}

/// Applique les mises à jour du terrain reçues du serveur.
///
/// Un même lot peut enchaîner, pour un chunk, son contenu puis ses
/// modifications, voire son oubli : la création d'une entité étant différée, on
/// suit l'état de chaque chunk dans `staged` au lieu de relire le monde.
fn receive_terrain(
    mut receivers: Query<&mut MessageReceiver<TerrainUpdate>, With<Client>>,
    index: Res<TerrainIndex>,
    mut chunks: Query<&mut ChunkTiles>,
    mut commands: Commands,
    mut staged: Local<HashMap<ChunkCoord, Staged>>,
) {
    for mut receiver in &mut receivers {
        for update in receiver.receive() {
            match update {
                TerrainUpdate::Snapshot { chunk, tiles } => {
                    let Some(tiles) = ChunkTiles::from_vec(tiles) else {
                        warn!("Chunk {:?} reçu avec une taille invalide", chunk.0);
                        continue;
                    };
                    staged.insert(chunk, Staged::Loaded(tiles));
                }
                TerrainUpdate::Edits { chunk, edits } => {
                    let apply = |tiles: &mut ChunkTiles| {
                        for &(index, kind) in &edits {
                            tiles.set(index as usize, kind);
                        }
                    };
                    match staged.get_mut(&chunk) {
                        Some(Staged::Loaded(tiles)) => apply(tiles),
                        Some(Staged::Unloaded) => {
                            warn!("Modifications reçues pour le chunk oublié {:?}", chunk.0)
                        }
                        None => match index.get(chunk).and_then(|e| chunks.get_mut(e).ok()) {
                            Some(mut tiles) => apply(&mut tiles),
                            // Impossible sur un canal ordonné : le contenu
                            // précède toujours ses modifications.
                            None => {
                                warn!("Modifications reçues pour le chunk inconnu {:?}", chunk.0)
                            }
                        },
                    }
                }
                TerrainUpdate::Unload { chunk } => {
                    staged.insert(chunk, Staged::Unloaded);
                }
            }
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
            Staged::Unloaded => {
                if let Some(entity) = loaded {
                    commands.entity(entity).despawn();
                }
            }
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

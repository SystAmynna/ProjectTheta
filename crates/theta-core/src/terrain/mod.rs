//! Terrain en tiles, découpé en chunks.
//!
//! Ce module est **partagé** : il décrit ce qu'est un chunk, comment en faire un
//! collider et comment le retrouver, mais il ne génère rien. La génération
//! appartient au serveur (`theta-worldgen`) ; le client ne fait que recevoir des
//! chunks déjà remplis. Les deux camps les créent pourtant par le même
//! [`spawn_chunk`], si bien que la simulation voit exactement les mêmes murs des
//! deux côtés — condition pour que la prédiction du client tombe juste.

use avian2d::prelude::{Collider, CollisionLayers, Position, RigidBody, Rotation};
use bevy::ecs::lifecycle::HookContext;
use bevy::ecs::system::SystemParam;
use bevy::ecs::world::DeferredWorld;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;

use crate::layers::GameLayer;
use crate::player::PlayerSystems;

mod tile;

pub use tile::{
    CHUNK_AREA, CHUNK_SIZE, CHUNK_WORLD_SIZE, ChunkCoord, TILE_SIZE, TileCoord, TileKind,
    index_to_local, local_to_index,
};

/// Index des chunks chargés, et reconstruction de leur collider.
pub(crate) struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TerrainIndex>()
            .add_message::<TileEdit>()
            .configure_sets(
                FixedUpdate,
                TerrainSystems::Rebuild.before(PlayerSystems::Move),
            )
            .add_systems(
                FixedUpdate,
                rebuild_chunk_colliders.in_set(TerrainSystems::Rebuild),
            );
    }
}

/// Étapes du terrain, pour que les autres crates puissent s'ordonner autour.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TerrainSystems {
    /// Reconstruction des colliders des chunks modifiés, avant le mouvement.
    Rebuild,
}

/// Demande de modification d'une tile.
///
/// Le gameplay l'émet ; seul le serveur l'applique, puis diffuse le changement aux
/// clients abonnés au chunk. Un client qui en émet (pendant sa prédiction, par
/// exemple) n'obtient rien : le serveur fait autorité sur le terrain.
#[derive(Message, Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileEdit {
    pub coord: TileCoord,
    pub kind: TileKind,
}

/// Les tiles d'un chunk, en row-major, y vers le haut.
///
/// C'est la seule copie des données : le collider (ici) et le rendu
/// (`theta-render`) en sont dérivés, chacun sur `Changed<ChunkTiles>`.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
pub struct ChunkTiles(Box<[TileKind; CHUNK_AREA]>);

impl ChunkTiles {
    /// Chunk uniformément rempli.
    pub fn filled(kind: TileKind) -> Self {
        Self(Box::new([kind; CHUNK_AREA]))
    }

    /// Chunk dont chaque tile est calculée à partir de sa position locale.
    pub fn from_fn(mut tile: impl FnMut(UVec2) -> TileKind) -> Self {
        let mut tiles = Self::filled(TileKind::default());
        for (index, slot) in tiles.0.iter_mut().enumerate() {
            *slot = tile(index_to_local(index));
        }
        tiles
    }

    /// Reconstruit un chunk reçu du réseau. `None` si la taille ne correspond pas.
    pub fn from_vec(tiles: Vec<TileKind>) -> Option<Self> {
        tiles.into_boxed_slice().try_into().ok().map(Self)
    }

    pub fn get(&self, index: usize) -> TileKind {
        self.0[index]
    }

    /// Remplace une tile. Renvoie `false` si elle avait déjà cette valeur.
    pub fn set(&mut self, index: usize, kind: TileKind) -> bool {
        let changed = self.0[index] != kind;
        self.0[index] = kind;
        changed
    }

    pub fn as_slice(&self) -> &[TileKind] {
        self.0.as_slice()
    }

    /// Positions locales des tiles solides.
    pub fn solid_tiles(&self) -> impl Iterator<Item = UVec2> + '_ {
        self.0
            .iter()
            .enumerate()
            .filter(|(_, kind)| kind.is_solid())
            .map(|(index, _)| index_to_local(index))
    }
}

/// Marque l'entité d'un chunk et porte sa coordonnée.
///
/// Immuable : c'est ce qui permet à [`TerrainIndex`] de rester exact par de
/// simples hooks, sans système de synchronisation.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
#[component(immutable, on_insert = index_chunk, on_discard = unindex_chunk)]
pub struct TerrainChunk(pub ChunkCoord);

/// Chunks chargés, par coordonnée.
#[derive(Resource, Debug, Default)]
pub struct TerrainIndex(HashMap<ChunkCoord, Entity>);

impl TerrainIndex {
    pub fn get(&self, coord: ChunkCoord) -> Option<Entity> {
        self.0.get(&coord).copied()
    }

    pub fn contains(&self, coord: ChunkCoord) -> bool {
        self.0.contains_key(&coord)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

fn index_chunk(mut world: DeferredWorld, HookContext { entity, .. }: HookContext) {
    let Some(&TerrainChunk(coord)) = world.get::<TerrainChunk>(entity) else {
        return;
    };
    let Some(mut index) = world.get_resource_mut::<TerrainIndex>() else {
        return;
    };
    if let Some(previous) = index.0.insert(coord, entity)
        && previous != entity
    {
        warn!(
            "Deux entités pour le chunk {:?} : {previous} remplacée par {entity}",
            coord.0
        );
    }
}

fn unindex_chunk(mut world: DeferredWorld, HookContext { entity, .. }: HookContext) {
    let Some(&TerrainChunk(coord)) = world.get::<TerrainChunk>(entity) else {
        return;
    };
    let Some(mut index) = world.get_resource_mut::<TerrainIndex>() else {
        return;
    };
    // Un chunk remplacé entre-temps par une autre entité ne doit pas être
    // désindexé par la disparition de l'ancienne.
    if index.0.get(&coord) == Some(&entity) {
        index.0.remove(&coord);
    }
}

/// Accès au terrain par coordonnée de tile, sans manipuler les chunks.
#[derive(SystemParam)]
pub struct Terrain<'w, 's> {
    index: Res<'w, TerrainIndex>,
    chunks: Query<'w, 's, &'static mut ChunkTiles>,
}

impl Terrain<'_, '_> {
    /// Entité d'un chunk, s'il est chargé.
    pub fn chunk_entity(&self, chunk: ChunkCoord) -> Option<Entity> {
        self.index.get(chunk)
    }

    /// Tile à une coordonnée, ou `None` si son chunk n'est pas chargé.
    pub fn tile(&self, coord: TileCoord) -> Option<TileKind> {
        let entity = self.index.get(coord.chunk())?;
        let tiles = self.chunks.get(entity).ok()?;
        Some(tiles.get(coord.local_index()))
    }

    /// Remplace une tile.
    ///
    /// Renvoie `None` si son chunk n'est pas chargé, sinon si la tile a changé.
    /// Une tile inchangée ne déclenche pas `Changed<ChunkTiles>`.
    pub fn set(&mut self, coord: TileCoord, kind: TileKind) -> Option<bool> {
        let entity = self.index.get(coord.chunk())?;
        let mut tiles = self.chunks.get_mut(entity).ok()?;
        let index = coord.local_index();

        // Lecture d'abord, par `Deref` : `Mut` ne marque le chunk modifié qu'à
        // l'écriture.
        if tiles.get(index) == kind {
            return Some(false);
        }
        tiles.set(index, kind);
        Some(true)
    }
}

/// Fait exister un chunk dans la simulation.
///
/// Client et serveur passent tous deux par ici. `Position` et `Rotation` sont
/// posées explicitement : `CorePlugin` désactive `PhysicsTransformPlugin`, Avian
/// ne les déduirait donc pas du `Transform`. Le collider est construit tout de
/// suite, pour qu'un chunk soit solide dès le tick où il apparaît ; un chunk sans
/// tile solide n'en a pas.
pub fn spawn_chunk<'a>(
    commands: &'a mut Commands,
    coord: ChunkCoord,
    tiles: ChunkTiles,
) -> EntityCommands<'a> {
    let center = coord.center();
    let collider = chunk_collider(&tiles);

    let mut chunk = commands.spawn((
        Name::new(format!("Chunk {} {}", coord.0.x, coord.0.y)),
        TerrainChunk(coord),
        tiles,
        RigidBody::Static,
        Position(center),
        Rotation::default(),
        Transform::from_translation(center.extend(0.0)),
        CollisionLayers::new(GameLayer::Terrain, GameLayer::Player),
    ));
    if let Some(collider) = collider {
        chunk.insert(collider);
    }
    chunk
}

/// Collider d'un chunk : un voxel par tile solide, `None` s'il n'y en a aucune.
///
/// Un seul collider par chunk plutôt qu'un par tile : peu d'AABB dans le broad
/// phase, et la forme `Voxels` de parry sait qu'une rangée de tiles est une
/// surface continue — le joueur y glisse sans accrocher aux jointures.
///
/// L'entité du chunk est en son centre, et parry place le voxel `k` sur
/// `[k·s, (k+1)·s]` : la tile locale `(x, y)` est donc le voxel
/// `(x, y) - CHUNK_SIZE / 2`.
pub fn chunk_collider(tiles: &ChunkTiles) -> Option<Collider> {
    let half = IVec2::splat(CHUNK_SIZE as i32 / 2);
    let voxels: Vec<IVec2> = tiles
        .solid_tiles()
        .map(|local| local.as_ivec2() - half)
        .collect();

    (!voxels.is_empty()).then(|| Collider::voxels(Vec2::splat(TILE_SIZE), &voxels))
}

/// Un chunk dont les tiles ont changé depuis le dernier passage.
type ChangedChunk = (With<TerrainChunk>, Changed<ChunkTiles>);

/// Reconstruit le collider des chunks dont les tiles ont changé.
///
/// Les chunks tout juste créés passent aussi par ici, bien que [`spawn_chunk`]
/// leur ait déjà donné un collider : ils ont pu être modifiés entre leur création
/// et ce système, et reconstruire un chunk ne coûte que quelques microsecondes.
fn rebuild_chunk_colliders(
    chunks: Query<(Entity, &ChunkTiles), ChangedChunk>,
    mut commands: Commands,
) {
    for (entity, tiles) in &chunks {
        let mut chunk = commands.entity(entity);
        match chunk_collider(tiles) {
            Some(collider) => chunk.insert(collider),
            None => chunk.remove::<Collider>(),
        };
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use avian2d::prelude::LinearVelocity;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::time::TimeUpdateStrategy;

    use super::*;
    use crate::{CorePlugin, MoveIntent, PlayerBundle, tick_duration};

    /// Chunk dont seule la colonne locale `x` est pleine de murs.
    fn wall_column(x: u32) -> ChunkTiles {
        ChunkTiles::from_fn(|local| {
            if local.x == x {
                TileKind::Wall
            } else {
                TileKind::Floor
            }
        })
    }

    #[test]
    fn voxel_collider_covers_exactly_the_solid_tiles() {
        let tiles = ChunkTiles::from_fn(|local| {
            if (local.x * 7 + local.y * 3) % 5 == 0 {
                TileKind::Wall
            } else {
                TileKind::Floor
            }
        });
        let chunk = ChunkCoord::new(-1, 2);
        let collider = chunk_collider(&tiles).expect("le chunk a des murs");

        for index in 0..CHUNK_AREA {
            let tile = chunk.tile(index_to_local(index));
            let inside =
                collider.contains_point(chunk.center(), Rotation::default(), tile.center());
            assert_eq!(
                inside,
                tiles.get(index).is_solid(),
                "tile {:?} mal couverte par le collider",
                tile.0
            );
        }
    }

    #[test]
    fn empty_chunk_has_no_collider() {
        assert!(chunk_collider(&ChunkTiles::filled(TileKind::Floor)).is_none());
    }

    #[test]
    fn chunk_tiles_round_trip_through_a_vec() {
        let tiles = wall_column(4);
        let copy = ChunkTiles::from_vec(tiles.as_slice().to_vec()).unwrap();
        assert_eq!(copy, tiles);
        assert!(ChunkTiles::from_vec(vec![TileKind::Floor; 3]).is_none());
    }

    /// App minimale : `CorePlugin` sans réseau, et un temps qui avance d'un tick
    /// à chaque `update`.
    fn simulation() -> App {
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, CorePlugin))
            .insert_resource(TimeUpdateStrategy::ManualDuration(tick_duration()));
        // `run()` s'en charge d'ordinaire : Avian y crée ses ressources de
        // diagnostic, sans lesquelles ses systèmes refusent de tourner.
        app.finish();
        app.cleanup();
        app
    }

    fn run_for(app: &mut App, duration: Duration) {
        let ticks = duration.as_nanos() / tick_duration().as_nanos();
        for _ in 0..ticks {
            app.update();
        }
    }

    #[test]
    fn index_follows_spawn_and_despawn() {
        let mut app = simulation();
        let coord = ChunkCoord::new(3, -4);

        let entity = spawn_chunk(&mut app.world_mut().commands(), coord, wall_column(0)).id();
        app.world_mut().flush();
        assert_eq!(
            app.world().resource::<TerrainIndex>().get(coord),
            Some(entity)
        );

        app.world_mut().despawn(entity);
        assert!(!app.world().resource::<TerrainIndex>().contains(coord));
    }

    #[test]
    fn a_wall_stops_the_player_and_lets_it_slide() {
        // Assez proche du départ pour que le joueur l'atteigne bien avant la fin.
        const WALL_X: u32 = 5;

        let mut app = simulation();
        spawn_chunk(
            &mut app.world_mut().commands(),
            ChunkCoord::new(0, 0),
            wall_column(WALL_X),
        );

        let start = Vec2::new(100.0, 300.0);
        let player = app
            .world_mut()
            .spawn(PlayerBundle::new(start, 300.0))
            .insert(MoveIntent(Vec2::new(1.0, 1.0)))
            .id();

        run_for(&mut app, Duration::from_secs(2));

        let position = app.world().get::<Position>(player).unwrap().0;
        let velocity = app.world().get::<LinearVelocity>(player).unwrap().0;
        let wall_face = WALL_X as f32 * TILE_SIZE - crate::PlayerSimulationBundle::RADIUS;

        assert!(
            position.x <= wall_face + 0.5,
            "le joueur a traversé le mur : {position:?}"
        );
        assert!(
            position.x > wall_face - 2.0,
            "le joueur n'a pas atteint le mur : {position:?}"
        );
        assert!(
            position.y > start.y + 300.0,
            "le joueur ne glisse pas le long du mur : {position:?}"
        );
        assert!(
            velocity.x.abs() < 1e-3,
            "vitesse vers le mur non annulée : {velocity:?}"
        );
    }

    #[test]
    fn editing_a_tile_rebuilds_the_collider() {
        let mut app = simulation();
        let chunk = ChunkCoord::new(0, 0);
        let entity = spawn_chunk(&mut app.world_mut().commands(), chunk, wall_column(0)).id();
        app.update();

        let tile = chunk.tile(UVec2::new(5, 5));
        app.world_mut()
            .run_system_once(move |mut terrain: Terrain| {
                assert_eq!(terrain.set(tile, TileKind::Wall), Some(true));
                assert_eq!(terrain.set(tile, TileKind::Wall), Some(false));
            })
            .unwrap();
        run_for(&mut app, tick_duration());

        let collider = app.world().get::<Collider>(entity).unwrap();
        assert!(collider.contains_point(chunk.center(), Rotation::default(), tile.center()));
    }

    #[test]
    fn unloaded_chunks_are_neither_read_nor_written() {
        let mut app = simulation();
        app.world_mut()
            .run_system_once(|mut terrain: Terrain| {
                let tile = TileCoord::new(7, -3);
                assert_eq!(terrain.tile(tile), None);
                assert_eq!(terrain.set(tile, TileKind::Wall), None);
            })
            .unwrap();
    }

    #[test]
    fn clearing_every_wall_removes_the_collider() {
        let mut app = simulation();
        let chunk = ChunkCoord::new(0, 0);
        let entity = spawn_chunk(&mut app.world_mut().commands(), chunk, wall_column(0)).id();
        app.update();
        assert!(app.world().get::<Collider>(entity).is_some());

        *app.world_mut().get_mut::<ChunkTiles>(entity).unwrap() =
            ChunkTiles::filled(TileKind::Floor);
        app.update();
        assert!(app.world().get::<Collider>(entity).is_none());
    }

    #[test]
    fn replaced_chunk_stays_indexed_when_the_old_entity_goes() {
        let mut app = simulation();
        let coord = ChunkCoord::new(1, 1);

        let old = spawn_chunk(&mut app.world_mut().commands(), coord, wall_column(0)).id();
        let new = spawn_chunk(&mut app.world_mut().commands(), coord, wall_column(1)).id();
        app.world_mut().flush();
        assert_eq!(app.world().resource::<TerrainIndex>().get(coord), Some(new));

        app.world_mut().despawn(old);
        assert_eq!(app.world().resource::<TerrainIndex>().get(coord), Some(new));
    }
}

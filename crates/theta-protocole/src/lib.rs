use bevy::ecs::entity::{EntityMapper, MapEntities};
use bevy::prelude::*;
use lightyear::prelude::*;
use lightyear::prelude::input::native::{ActionState, InputPlugin};
use lightyear_avian2d::plugin::{AvianReplicationMode, LightyearAvianPlugin};
use serde::{Deserialize, Serialize};
use theta_core::{ChunkCoord, MoveIntent, Player, PlayerColor, PlayerSystems, TileKind};

// Récupère la cadence des ticks depuis theta-core
pub use theta_core::{TICK_HZ, tick_duration};

/// Identifiant unique du protocole, doit être changé à chaque modification du protocole
pub const PROTOCOL_ID: u64 = 0x7E7A_0004;

pub mod token;

/// Identité réseau unique d'un joueur, répliquée à tous.
#[derive(Component, Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PlayerId(pub PeerId);

impl PlayerId {
    /// Valeur numérique stable de l'identifiant, utilisable comme graine.
    pub fn seed(&self) -> u64 {
        match self.0 {
            PeerId::Netcode(id) | PeerId::Steam(id) | PeerId::Local(id) => id,
            PeerId::Entity(bits) => bits,
            PeerId::Server => 0,
            // Les autres variantes n'apparaissent pas avec le transport UDP,
            // mais l'exhaustivité évite une rupture au prochain ajout.
            _ => 0,
        }
    }
}

/// Couleur attribuée au n-ième joueur de la partie.
/// todo : changer le moyen de distinction
pub fn player_color(index: u32) -> PlayerColor {
    const GOLDEN_RATIO_CONJUGATE: f32 = 0.618_034;

    let hue = (index as f32 * GOLDEN_RATIO_CONJUGATE).fract();
    PlayerColor(hsv_to_rgb(hue, 0.65, 1.0))
}

/// Conversion TSV → RVB, teinte dans `[0, 1)`.
fn hsv_to_rgb(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let sector = hue * 6.0;
    let offset = sector - sector.floor();

    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * offset);
    let t = value * (1.0 - saturation * (1.0 - offset));

    match sector as u32 % 6 {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

/// Entrée de déplacement d'un joueur pour un tick.
///
/// C'est le seul message qui remonte du client vers le serveur : le serveur
/// reste maître de la position qui en découle.
#[derive(Component, Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, Reflect)]
pub struct MoveInput(pub Vec2);

impl MapEntities for MoveInput {
    // Aucune entité à remapper : l'input est purement scalaire.
    fn map_entities<M: EntityMapper>(&mut self, _mapper: &mut M) {}
}

/// Canal du terrain : fiable et **ordonné**, du serveur vers les clients.
///
/// L'ordre est ce qui rend le terrain cohérent : pour un chunk donné, le client
/// reçoit toujours son contenu, puis ses modifications, puis l'ordre de l'oublier
/// — jamais une modification avant le contenu qu'elle modifie.
pub struct TerrainChannel;

/// Mise à jour du terrain d'un client, décidée par le serveur.
///
/// Le serveur abonne chaque client aux chunks proches de son joueur : il envoie
/// un chunk en entier quand il entre dans le rayon, ne transmet ensuite que ses
/// modifications, et le fait oublier quand il en sort.
///
/// Un seul type de message pour les trois cas, et non trois : lightyear range
/// chaque type dans sa propre file de réception, et l'ordre entre, par exemple,
/// un `Unload` suivi d'un nouveau `Snapshot` du même chunk serait perdu.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TerrainUpdate {
    /// Contenu complet d'un chunk qui entre dans le rayon du client, en
    /// row-major, y vers le haut (`CHUNK_AREA` tiles).
    Snapshot {
        chunk: ChunkCoord,
        tiles: Vec<TileKind>,
    },
    /// Tiles modifiées pendant un tick, par index local dans le chunk.
    ///
    /// Chaque entrée signifie « la tile devient » : l'appliquer deux fois ne
    /// change rien.
    Edits {
        chunk: ChunkCoord,
        edits: Vec<(u16, TileKind)>,
    },
    /// Le chunk est sorti du rayon du client : il doit l'oublier. Le serveur ne
    /// lui en enverra plus rien, sauf un nouveau `Snapshot` s'il y revient.
    Unload { chunk: ChunkCoord },
}

/// Protocole réseau partagé : composants répliqués, inputs et messages
/// communs au client et au serveur (lightyear).
pub struct ProtocolPlugin;

impl Plugin for ProtocolPlugin {
    fn build(&self, app: &mut App) {
        // Réplication des composants physiques d'Avian. Le plugin enregistre
        // lui-même `Position`, `Rotation`, `LinearVelocity` et `AngularVelocity`
        // (réplication filtrée sur les `RigidBody`, prédiction, interpolation et
        // correction visuelle) : il ne faut surtout pas les réenregistrer ici.
        //
        // `sync_to_transform: false` : `Position`/`Rotation` sont la vérité, et
        // `Transform` n'en est qu'une projection d'affichage, écrite en
        // `PostUpdate` une fois l'interpolation et la correction appliquées.
        app.add_plugins(LightyearAvianPlugin {
            replication_mode: AvianReplicationMode::Position {
                sync_to_transform: false,
            },
            ..default()
        });

        app.add_plugins(InputPlugin::<MoveInput>::default());

        // Le terrain ne passe pas par la réplication d'entités : un chunk y
        // serait renvoyé en entier à chaque modification, et son `RigidBody`
        // attirerait la réplication d'Avian. Il voyage en messages, sur un canal
        // à lui.
        app.add_channel::<TerrainChannel>(ChannelSettings {
            mode: ChannelMode::OrderedReliable(ReliableSettings::default()),
            ..default()
        })
        .add_direction(NetworkDirection::ServerToClient);
        app.register_message::<TerrainUpdate>()
            .add_direction(NetworkDirection::ServerToClient);

        // `Player` est répliqué pour que le client sache qu'une entité interpolée
        // — qui n'a aucun composant de simulation — est bien un joueur.
        app.component::<Player>().replicate();
        app.component::<PlayerId>().replicate();
        // La couleur ne change jamais : inutile de la renvoyer à chaque tick.
        app.component::<PlayerColor>().replicate_once();

        // Le pont entre le réseau et le gameplay : `theta-core` ne connaît que
        // `MoveIntent`, et ignore d'où il vient. Ce système tourne à l'identique
        // des deux côtés — sur le client avec l'input qu'il vient de saisir, sur
        // le serveur avec celui qu'il vient de recevoir.
        // En `FixedUpdate` plutôt qu'en `FixedPreUpdate` : à ce moment-là
        // `ActionState` est à jour des deux côtés (le client l'a saisi puis
        // rejoué depuis son buffer en cas de rollback, le serveur l'a reçu), et
        // il reste juste avant le mouvement.
        app.add_systems(FixedUpdate, feed_move_intent.before(PlayerSystems::Move));
    }
}

/// Recopie l'entrée réseau du tick courant dans le `MoveIntent` de `theta-core`.
fn feed_move_intent(mut players: Query<(&ActionState<MoveInput>, &mut MoveIntent)>) {
    for (action, mut intent) in &mut players {
        intent.0 = action.0.0;
    }
}

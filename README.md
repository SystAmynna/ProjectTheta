# ProjectTheta

Jeu 2D multijoueur (Bevy + Avian2D + Lightyear), organisé en workspace Cargo.

## Structure

Il n'y a **pas de features** : chaque responsabilité est une crate, et chaque
binaire ne dépend que des crates dont il a besoin.

| Crate | Type | Rôle |
| --- | --- | --- |
| `theta-core` | lib | Gameplay commun client/serveur : physique, mouvement du joueur, **terrain en chunks** (sans génération). Ne lit aucune entrée. |
| `theta-protocole` | lib | Protocole réseau partagé (composants répliqués, inputs, messages). |
| `theta-render` | lib | Rendu : caméra, sprites, tilemap et **gestion des assets**. Uniquement côté client. |
| `theta-worldgen` | lib | **Génération du monde**. Uniquement côté serveur. |
| `theta-client` | lib + bin `theta-client` | Menu, connexion au serveur, saisie clavier, joueur local, réception du terrain. |
| `theta-server` | lib + bin `theta-server` | Simulation autoritaire, génération à la demande et diffusion du terrain, sans fenêtre ni rendu. |

Dépendances :

```
theta-client (bin) ──> theta-client, theta-render, theta-protocole, theta-core
theta-server (bin) ──> theta-server, theta-worldgen, theta-protocole, theta-core
```

Le serveur ne dépend jamais de `theta-render` : il ne compile ni ne lie
`bevy_winit`, `wgpu`, etc. Symétriquement, le client ne dépend jamais de
`theta-worldgen` : il ne sait pas générer le monde, il ne fait que le recevoir.

## Lancer

```sh
cargo run -p theta-client              # client (fenêtre de jeu)
cargo run -p theta-client -- --server 127.0.0.1:5000

cargo run -p theta-server              # serveur headless, graine tirée de l'heure
cargo run -p theta-server -- --bind 0.0.0.0:5000 --seed 42
```

Le client s'ouvre sur un menu provisoire : **Entrée** rejoint le serveur passé
en `--server`. Il passe par trois états (`AppState`) : `Menu`, `Connecting` —
demande de token sur l'`IoTaskPool`, sans figer la fenêtre, puis handshake — et
`InGame` une fois connecté. Un échec ou une déconnexion ramène au menu, qui en
affiche la raison.

## Vérifier

```sh
./check.sh    # cargo fmt --check, clippy sans avertissement, tests du workspace
```

## Assets

Les assets sont à la racine du projet, dans `assets/` — hors de `crates/`.
Toute la logique qui les concerne vit dans `theta-render` (`assets.rs`) :

- `asset_root()` résout le répertoire, dans cet ordre :
  1. la variable d'environnement `THETA_ASSET_ROOT` ;
  2. `<racine du projet>/assets`, résolu à la compilation depuis le
     `CARGO_MANIFEST_DIR` de `theta-render`, s'il existe (développement) ;
  3. `assets/` à côté de l'exécutable (build distribué).
- `asset_plugin()` renvoie un `AssetPlugin` pointant sur ce répertoire, avec un
  chemin **absolu** — ce qui neutralise le `CARGO_MANIFEST_DIR` par défaut de
  Bevy, qui pointerait sur la crate du binaire et non sur la racine du projet.
  Le binaire client l'applique via `DefaultPlugins.set(asset_plugin())`.
- `GameAssets` est une ressource chargée une seule fois en `PreStartup` ; les
  systèmes de rendu clonent ses handles au lieu d'appeler `AssetServer::load`.

`cargo run -p theta-client` et l'exécution directe de
`target/debug/theta-client` depuis n'importe quel répertoire fonctionnent donc
toutes les deux.

## Réseau

Le serveur fait autorité sur un monde partagé ; chaque client qui se connecte y
reçoit un joueur, prédit le sien et interpole ceux des autres.

| Brique | Où | Rôle |
| --- | --- | --- |
| Tokens (TCP) | `theta-server` / `theta-client` | Avant de se connecter, le client obtient un `ConnectToken` auprès du serveur. Voir [Connexion](#connexion). |
| Netcode (UDP) | `theta-server` / `theta-client` | Écoute, handshake, une entité de lien par client. |
| Réplication | serveur → clients | `Position`, `Rotation`, vélocités, `Player`, `PlayerId`, `PlayerColor`. |
| Prédiction | client, son joueur | Le clavier agit immédiatement ; le serveur corrige par rollback. |
| Interpolation | client, les autres joueurs | Mouvement lissé entre deux états reçus. |
| Inputs | client → serveur | `MoveInput`, un vecteur de direction par tick. |
| Terrain | serveur → client | `TerrainUpdate` sur `TerrainChannel` (fiable, ordonné) : chunk complet, modifications, oubli. Voir [Le monde](#le-monde). |

Les valeurs sur lesquelles les deux camps doivent s'accorder vivent dans le
code commun, jamais dans les binaires : `PROTOCOL_ID` dans `theta-protocole`, et
la cadence de simulation dans `theta-core`
(`TICK_HZ` = 60, `tick_duration()`, que `theta-protocole` réexporte).

Cette cadence n'est configurable nulle part — ni option de ligne de commande, ni
ressource : les deux binaires la lisent dans `theta-core` et la passent
telle quelle à lightyear, et `CorePlugin` en fait le pas de `Time<Fixed>`. Elle
ne borne que `FixedUpdate` (physique, inputs, réseau) ; le rendu tourne en
`Update`, aussi vite que la machine le permet, sans que le nombre d'images par
seconde ne change quoi que ce soit aux ticks — voir ci-dessous.

**L'ordre des plugins compte** : `ClientPlugins` / `ServerPlugins` (lightyear)
doivent être ajoutés **avant** `ProtocolPlugin`, qui installe
`LightyearAvianPlugin` et n'enregistre les composants physiques que si le
registre de lightyear existe déjà.

```
DefaultPlugins/MinimalPlugins -> ClientPlugins/ServerPlugins -> CorePlugin
    -> ProtocolPlugin -> RenderPlugin (client) -> ClientPlugin/ServerPlugin
```

### Connexion

La clé privée de netcode n'existe que sur le serveur : il en tire une nouvelle à
chaque lancement, et aucun client ne la connaît. Un client obtient donc son
`ConnectToken` auprès du serveur lui-même, par un court échange TCP sur le
**même numéro de port** que le jeu en UDP (`theta-protocole::token`) :

1. le client envoie `PROTOCOL_ID` et l'adresse du serveur telle qu'il la voit ;
2. le serveur refuse si le protocole diffère, sinon renvoie un token signé,
   valable 30 s, portant un `client_id` unique ;
3. le client ouvre la connexion UDP avec ce token.

Chaque échange est traité sur son propre thread, borné à quelques secondes : un
client muet ne retarde pas les autres. Le service n'authentifie personne ; une
mise en ligne réelle supposera un service de comptes en amont.

### FPS libres, simulation à 60 Hz

Le rendu n'est cadencé par rien : `window_plugin()` (theta-render) demande
`PresentMode::AutoNoVsync`, donc `Update` n'est même pas borné par le taux de
rafraîchissement de l'écran. Trois mécanismes séparent les images des ticks, et
chacun couvre un cas :

| Ce qui est affiché | Lissé par | Quand |
| --- | --- | --- |
| Les autres joueurs (`Interpolated`) | Interpolation entre deux états reçus du serveur (lightyear) | `Update`, à chaque image |
| Le joueur local (`Predicted`) | Frame interpolation : `FrameInterpolate`, posé sur le joueur prédit | `PostUpdate`, à chaque image |
| La correction après rollback | Correction visuelle de `Position` / `Rotation` (lightyear_avian) | `PostUpdate`, étalée sur plusieurs images |

Le joueur local est le cas piégeux : il est simulé en `FixedUpdate` et
n'avancerait donc que 60 fois par seconde, à saccades visibles au-delà. Le
marqueur `FrameInterpolate` (posé dans `on_predicted`) le fait afficher avec un
tick de retard, interpolé selon l'overstep de `Time<Fixed>`. Le plugin qui
l'exploite n'a pas à être ajouté : `LightyearAvianPlugin` l'installe en
enregistrant la correction visuelle de `Position` et `Rotation`.

C'est aussi pour ça que `CorePlugin` désactive `PhysicsInterpolationPlugin`
d'Avian : ce serait le même travail, fait deux fois, sur le même `Transform`.

Repasser à `PresentMode::AutoVsync` dans `window_plugin()` suffit à rétablir la
synchronisation verticale ; rien d'autre n'en dépend.

## Mouvement du joueur

La logique est coupée en trois, chacune ignorant l'étage du dessus :

- **`theta-client`** possède le clavier (`input::gather_move_input`, dans le set
  `WriteClientInputs` de `FixedPreUpdate`). Il traduit les touches
  (`KeyBindings`, WASD par défaut) en `ActionState<MoveInput>`, que lightyear
  bufferise, envoie au serveur et rejoue lors des rollbacks.
- **`theta-protocole`** fait le pont (`feed_move_intent`, en `FixedUpdate`) :
  il recopie l'`ActionState` du tick dans le `MoveIntent` de `theta-core`. Le
  même système tourne des deux côtés — avec l'entrée saisie sur le client, avec
  l'entrée reçue sur le serveur.
- **`theta-core`** possède le mouvement. `player::apply_move_intent` lit
  `MoveIntent` et en fait une `LinearVelocity` désirée, en pixels par seconde ;
  `player::slide_players` en fait un déplacement qui glisse le long du terrain.
  Il ignore complètement l'origine de cette intention.

```
clavier ──> ActionState<MoveInput> ──> MoveIntent ──> LinearVelocity ──> Position ──> Transform
(client)        (protocole)            (protocole)      (core)        (core, MoveAndSlide)  (rendu)
```

Le joueur est un corps **cinématique**, et Avian n'arrête pas un cinématique
contre un statique : il le ferait traverser les murs. `slide_players` le déplace
donc lui-même avec `MoveAndSlide` (le « collide and slide » d'Avian), qui avance
jusqu'au premier mur puis projette la vitesse restante le long de celui-ci. Le
joueur porte `CustomPositionIntegration`, pour qu'Avian n'intègre pas la
vélocité une seconde fois. La vélocité réécrite est la vitesse projetée : elle
ne pointe jamais dans un mur, sans quoi le joueur y serait repoussé à chaque
tick et, côté client, divergerait du serveur en déclenchant des rollbacks.

Seule la couche `GameLayer::Terrain` compte pour ce mouvement : les joueurs ne
se bloquent pas entre eux. Côté client, seul le joueur local a un collider (les
joueurs interpolés n'en ont pas) ; une collision entre joueurs sur le serveur
serait imprévisible pour le client.

`Position` et `Rotation` (Avian) sont la vérité de la simulation, et les
composants effectivement répliqués ; `Transform` n'en est qu'une projection
d'affichage, écrite en `PostUpdate` une fois l'interpolation et la correction
visuelle appliquées. **Rien ne doit écrire dans `Transform` pendant le jeu.**
C'est aussi pourquoi `CorePlugin` désactive les plugins `PhysicsTransformPlugin`
et `PhysicsInterpolationPlugin` d'Avian : `LightyearAvianPlugin` les remplace par
des versions compatibles avec la prédiction.

## Le monde

Le monde est **infini**, fait de tiles de 64 px (`TILE_SIZE` ; `TileKind` : sol
ou mur) regroupées en chunks de 32 × 32 tiles (2048 px). Les trois camps s'en partagent
la charge :

| Crate | Rôle |
| --- | --- |
| `theta-core::terrain` | Ce qu'est un chunk (`TerrainChunk`, `ChunkTiles`), son collider, l'index `TerrainIndex` et l'accès `Terrain`. **Ne génère rien.** |
| `theta-worldgen` | `WorldGenerator` : chaque tile est une fonction pure de `(graine, coordonnée)`, un bruit fBm. Serveur uniquement. |
| `theta-render::terrain` | Un `TilemapChunk` (Bevy) par chunk : un mesh et un draw call par chunk. |

Chaque chunk est une entité qui porte ses tiles (`ChunkTiles`), un
`RigidBody::Static` et **un seul** `Collider::voxels` : peu d'AABB pour le broad
phase, et une forme que parry sait continue — le joueur glisse le long d'une
rangée de tiles sans accrocher aux jointures. Client et serveur créent leurs
chunks par le même `spawn_chunk`, et `Changed<ChunkTiles>` suffit à reconstruire
le collider (`theta-core`) comme le rendu (`theta-render`).

- `spawn_point(index)` répartit les arrivants sur un cercle de rayon
  `SPAWN_RADIUS` ; le générateur garde ce cercle toujours praticable.
- Une modification de terrain s'exprime par un message Bevy `TileEdit`. Seul le
  serveur l'applique : le terrain est autoritaire, le client ne modifie jamais
  une tile de lui-même.

### Diffusion par abonnement

Le terrain ne passe **pas** par la réplication d'entités de lightyear : un chunk
y serait renvoyé en entier à chaque modification. Il voyage en messages
`TerrainUpdate`, sur le canal fiable et ordonné `TerrainChannel` :

| Message | Quand |
| --- | --- |
| `Snapshot { chunk, tiles }` | Le chunk entre dans le rayon d'abonnement du client : contenu complet. |
| `Edits { chunk, edits }` | Un chunk auquel le client est abonné change : seulement les tiles modifiées du tick. |
| `Unload { chunk }` | Le chunk sort du rayon de désabonnement : le client l'oublie. |

Le serveur (`theta-server::terrain`) recalcule à chaque tick les abonnements de
chaque client autour de son joueur : `SUBSCRIBE_RADIUS` = 1 (les 3 × 3 chunks
autour du sien, jamais de trou visible à l'écran) et `UNSUBSCRIBE_RADIUS` = 2,
plus large pour qu'un joueur longeant une frontière ne reçoive pas le même chunk
en boucle. Un chunk jamais modifié que plus personne n'observe est oublié par
le serveur aussi, puisque la graine le redonnera à l'identique ; un chunk
modifié reste en mémoire.

Un seul type de message pour les trois cas, et non trois : lightyear range
chaque type dans sa propre file, et l'ordre entre un `Unload` et un nouveau
`Snapshot` du même chunk serait perdu. Pour la même raison, le client lit ces
messages en `PreUpdate` et non en `FixedPreUpdate` : lightyear vide les
messages non lus à chaque image, et une image sans tick les perdrait.

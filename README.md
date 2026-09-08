# ProjectTheta

Jeu 2D multijoueur (Bevy + Avian2D + Lightyear), organisé en workspace Cargo.

## Structure

Il n'y a **pas de features** : chaque responsabilité est une crate, et chaque
binaire ne dépend que des crates dont il a besoin.

| Crate | Type | Rôle |
| --- | --- | --- |
| `theta-core` | lib | Gameplay commun client/serveur : physique, mouvement du joueur. Ne lit aucune entrée. |
| `theta-protocole` | lib | Protocole réseau partagé (composants répliqués, inputs, messages). |
| `theta-render` | lib | Rendu : caméra, sprites et **gestion des assets**. Uniquement côté client. |
| `theta-client` | lib + bin `theta-client` | Saisie clavier, connexion au serveur, joueur local. |
| `theta-server` | lib + bin `theta-server` | Simulation autoritaire, sans fenêtre ni rendu. |

Dépendances :

```
theta-client (bin) ──> theta-client, theta-render, theta-protocole, theta-core
theta-server (bin) ──> theta-server, theta-protocole, theta-core
```

Le serveur ne dépend jamais de `theta-render` : il ne compile ni ne lie
`bevy_winit`, `wgpu`, etc.

## Lancer

```sh
cargo run -p theta-client              # client (fenêtre de jeu)
cargo run -p theta-client -- --server 127.0.0.1:5000

cargo run -p theta-server              # serveur headless
cargo run -p theta-server -- --bind 0.0.0.0:5000
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

## Données statiques (`theta-core::data`)

Tout le contenu « data-oriented » du jeu — items, armes, équipements, entités,
ennemis — vit dans `crates/theta-core/src/data/`, en tables `const` Rust
intégrées au binaire à la compilation. Il n'y a donc **rien à charger au
démarrage** et aucun parsing : les champs manquants ou mal typés sont des
erreurs de compilation.

| Fichier | Table | Contenu |
| --- | --- | --- |
| `registry.rs` | — | `Id`, `Definition`, `Registry<T>` : le socle des tables. |
| `common.rs` | — | `Rarity`, `DamageKind`, `Faction`, `EquipmentSlot`, `Stats`. |
| `items.rs` | `ITEMS` | Tout ce qui entre en inventaire (`ItemKind`). |
| `weapons.rs` | `WEAPONS` | Cadence, dégâts, portée, `FireMode`. |
| `equipments.rs` | `EQUIPMENTS` | Emplacement, modificateurs de `Stats`, résistances. |
| `entities.rs` | `ENTITIES` | Vie, vitesse, hitbox, sprite — le corps. |
| `enemies.rs` | `ENEMIES` | `Behavior`, expérience, table de butin. |

Les tables se référencent **par `Id`** (une `&'static str` typée), et non par
pointeur : c'est ce qui permet de les déclarer en `const` et de sérialiser un
identifiant tel quel dans une sauvegarde ou sur le réseau.

```rust
use theta_core::data::{ENEMIES, ENTITIES, ITEMS, Id, WEAPONS};

let brute = ENEMIES.expect(Id("brute"));
let body = ENTITIES.expect(brute.entity);   // vie, vitesse, hitbox
for loot in brute.loot {
    let item = ITEMS.expect(loot.item);
}
```

- `get(id) -> Option<&'static T>` pour un identifiant venu d'une sauvegarde ou
  du réseau ;
- `expect(id)` pour un identifiant écrit en dur dans le code ;
- `all()` / `iter()` pour parcourir la table.

Les références croisées (une arme vers son item, un ennemi vers son entité, une
ligne de butin vers un item) ne sont pas vérifiables par le compilateur : elles
le sont par `data::validate()`, que `CorePlugin` appelle en `debug_assertions`
et que les tests couvrent. **Ajouter une donnée** = ajouter l'entrée dans la
table, créer l'éventuelle entrée cible, puis `cargo test -p theta-core`.

`theta-core` ne dépend pas du rendu : `Rarity::rgb()` renvoie un `[f32; 3]`,
c'est à `theta-render` d'en faire une `Color` Bevy.

## Réseau

Le serveur fait autorité sur un monde partagé ; chaque client qui se connecte y
reçoit un joueur, prédit le sien et interpole ceux des autres.

| Brique | Où | Rôle |
| --- | --- | --- |
| Netcode (UDP) | `theta-server` / `theta-client` | Écoute, handshake, une entité de lien par client. |
| Réplication | serveur → clients | `Position`, `Rotation`, vélocités, `Player`, `PlayerId`, `PlayerColor`. |
| Prédiction | client, son joueur | Le clavier agit immédiatement ; le serveur corrige par rollback. |
| Interpolation | client, les autres joueurs | Mouvement lissé entre deux états reçus. |
| Inputs | client → serveur | `MoveInput`, un vecteur de direction par tick. |

Les valeurs sur lesquelles les deux camps doivent s'accorder vivent dans le
code commun, jamais dans les binaires : `PROTOCOL_ID` et `PRIVATE_KEY` dans
`theta-protocole`, et la cadence de simulation dans `theta-core`
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

`PRIVATE_KEY` est une clé de développement, en dur et publique. Une mise en
ligne réelle suppose une clé secrète côté serveur et des `ConnectToken` délivrés
par un service d'authentification.

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
- **`theta-core`** possède le mouvement (`player::apply_move_intent`). Il lit
  `MoveIntent` et en fait une `LinearVelocity`, en pixels par seconde. Il ignore
  complètement l'origine de cette intention.

```
clavier ──> ActionState<MoveInput> ──> MoveIntent ──> LinearVelocity ──> Position ──> Transform
(client)        (protocole)            (protocole)      (core)          (Avian)      (rendu)
```

`Position` et `Rotation` (Avian) sont la vérité de la simulation, et les
composants effectivement répliqués ; `Transform` n'en est qu'une projection
d'affichage, écrite en `PostUpdate` une fois l'interpolation et la correction
visuelle appliquées. **Rien ne doit écrire dans `Transform` pendant le jeu.**
C'est aussi pourquoi `CorePlugin` désactive les plugins `PhysicsTransformPlugin`
et `PhysicsInterpolationPlugin` d'Avian : `LightyearAvianPlugin` les remplace par
des versions compatibles avec la prédiction.

## Le monde

`theta-core::world` définit le terrain : une aire rectangulaire centrée sur
l'origine (`GameWorld::half_extents`, 2000 × 2000 par défaut), sans décor ni
obstacle. Il n'y vit que des joueurs.

- `spawn_point(index)` répartit les arrivants sur un cercle, pour que deux
  joueurs ne se superposent jamais en apparaissant.
- `confine_players` les y maintient. Il ne se contente pas de replacer la
  position : il annule aussi la composante de vélocité qui pointe vers
  l'extérieur. Sans ça, Avian réintègre la vitesse juste après et le joueur
  ressort d'un tick à chaque frame — ce qui, côté client, diverge en permanence
  du serveur et déclenche des rollbacks en continu.

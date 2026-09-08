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
cargo run -p theta-server -- --bind 0.0.0.0:5000 --tick-rate 64
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

## Mouvement du joueur

La logique est volontairement coupée en deux :

- **`theta-core`** possède le mouvement (`player::apply_move_intent`, dans
  `FixedUpdate`). Il lit le composant `MoveIntent` (une direction) et déplace le
  `Transform` selon `Speed`, en pixels par seconde. Il ignore complètement
  l'origine de cette intention, ce qui permet au serveur de la remplir depuis
  le réseau.
- **`theta-client`** possède le clavier (`input::gather_move_intent`, dans
  `FixedPreUpdate`). Il traduit les touches (`KeyBindings`, WASD par défaut,
  modifiable à l'exécution) en `MoveIntent` sur l'entité marquée `LocalPlayer`.

```
clavier ──> MoveIntent ──> Transform
(client)     (core)         (core)
```

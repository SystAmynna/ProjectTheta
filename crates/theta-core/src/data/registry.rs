//! Briques communes à toutes les tables de données statiques.

use core::fmt;

/// Compare deux chaînes dans un contexte `const`.
///
/// `str: PartialEq` n'est pas utilisable en `const fn`, d'où cette version
/// octet par octet — elle sert aux `const` qui référencent une autre table.
pub const fn str_eq(a: &str, b: &str) -> bool {
    let (a, b) = (a.as_bytes(), b.as_bytes());

    if a.len() != b.len() {
        return false;
    }

    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }

    true
}

/// Identifiant textuel stable d'une définition statique.
///
/// Le texte est choisi une fois pour toutes et ne doit plus changer : il est
/// destiné à être écrit dans les sauvegardes et transmis sur le réseau.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Id(pub &'static str);

impl Id {
    pub const fn as_str(self) -> &'static str {
        self.0
    }

    /// Égalité utilisable en contexte `const`.
    pub const fn const_eq(self, other: Id) -> bool {
        str_eq(self.0, other.0)
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// Une définition statique, entrée d'un [`Registry`].
pub trait Definition: 'static {
    /// Nom de la famille de données, utilisé dans les messages d'erreur.
    const KIND: &'static str;

    fn id(&self) -> Id;
}

/// Table immuable de définitions, intégrée au binaire.
///
/// La recherche est linéaire : les tables comptent au plus quelques centaines
/// d'entrées et les identifiants sont surtout résolus au chargement, pas à
/// chaque tick. Si cela devient un point chaud, mémoriser la référence
/// `&'static T` plutôt que l'[`Id`].
pub struct Registry<T: Definition> {
    entries: &'static [T],
}

impl<T: Definition> Registry<T> {
    pub const fn new(entries: &'static [T]) -> Self {
        Self { entries }
    }

    /// Toutes les entrées, dans l'ordre de déclaration.
    pub const fn all(&self) -> &'static [T] {
        self.entries
    }

    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn iter(&self) -> core::slice::Iter<'static, T> {
        self.entries.iter()
    }

    /// Cherche une définition par identifiant.
    pub fn get(&self, id: Id) -> Option<&'static T> {
        self.entries.iter().find(|entry| entry.id() == id)
    }

    /// Comme [`Registry::get`], mais panique si l'identifiant est inconnu.
    ///
    /// À réserver aux identifiants écrits en dur dans le code : une donnée
    /// venant d'une sauvegarde ou du réseau doit passer par `get`.
    #[track_caller]
    pub fn expect(&self, id: Id) -> &'static T {
        self.get(id)
            .unwrap_or_else(|| panic!("{} inconnu : `{id}`", T::KIND))
    }

    pub fn contains(&self, id: Id) -> bool {
        self.get(id).is_some()
    }

    /// Vérifie qu'aucun identifiant n'est déclaré deux fois.
    ///
    /// Appelée par [`crate::data::validate`].
    pub(crate) fn assert_unique_ids(&self) {
        for (i, entry) in self.entries.iter().enumerate() {
            let duplicate = self.entries[..i].iter().any(|other| other.id() == entry.id());
            assert!(
                !duplicate,
                "{} déclaré deux fois : `{}`",
                T::KIND,
                entry.id()
            );
        }
    }
}

impl<T: Definition> IntoIterator for &Registry<T> {
    type Item = &'static T;
    type IntoIter = core::slice::Iter<'static, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

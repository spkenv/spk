// Copyright (c) Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::collections::{HashMap, HashSet};

use relative_path::RelativePathBuf;
use spk_exec::ResolvedLayer;
use spk_schema::name::PkgNameBuf;
use spk_schema::{BuildIdent, Package};
use spk_solve::Named;

pub struct OccupiedEntry<'a> {
    occupied: std::collections::hash_map::OccupiedEntry<'a, RelativePathBuf, ResolvedLayer>,
}

impl<'a> OccupiedEntry<'a> {
    pub fn get(&self) -> &ResolvedLayer {
        self.occupied.get()
    }
}

pub struct VacantEntry<'a> {
    pkg_name_to_files: &'a mut HashMap<PkgNameBuf, HashSet<RelativePathBuf>>,
    vacant: std::collections::hash_map::VacantEntry<'a, RelativePathBuf, ResolvedLayer>,
}

impl<'a> VacantEntry<'a> {
    pub fn insert(self, layer: ResolvedLayer) -> &'a mut ResolvedLayer {
        self.pkg_name_to_files
            .entry(layer.spec.ident().name().to_owned())
            .or_default()
            .insert(self.vacant.key().clone());
        self.vacant.insert(layer)
    }
}

pub enum PathEntry<'a> {
    Occupied(OccupiedEntry<'a>),
    Vacant(VacantEntry<'a>),
}

/// An index of files to layers, and package names to files.
#[derive(Default)]
pub struct ResolvedLayerFileIndex {
    /// A mapping of path to resolved layer
    files_to_layers: HashMap<RelativePathBuf, ResolvedLayer>,
    /// A mapping of package name to files
    pkg_name_to_files: HashMap<PkgNameBuf, HashSet<RelativePathBuf>>,
}

impl ResolvedLayerFileIndex {
    /// Return a package ident based on a file path.
    pub fn get_ident_by_path(&self, path: &RelativePathBuf) -> Option<&BuildIdent> {
        self.files_to_layers
            .get(path)
            .map(|layer| layer.spec.ident())
    }

    /// Iterate over all the files belonging to a package with the given name.
    pub fn iter_files_for_pkg_name<N>(
        &self,
        key: N,
    ) -> Option<std::collections::hash_set::Iter<'_, RelativePathBuf>>
    where
        N: Named,
    {
        let name = key.name();
        self.pkg_name_to_files.get(name).map(|files| files.iter())
    }

    /// Like `HashMap::entry`, indexed by file path.
    pub fn path_entry(&mut self, path: RelativePathBuf) -> PathEntry<'_> {
        match self.files_to_layers.entry(path) {
            std::collections::hash_map::Entry::Occupied(occupied) => {
                PathEntry::Occupied(OccupiedEntry { occupied })
            }
            std::collections::hash_map::Entry::Vacant(vacant) => PathEntry::Vacant(VacantEntry {
                pkg_name_to_files: &mut self.pkg_name_to_files,
                vacant,
            }),
        }
    }
}

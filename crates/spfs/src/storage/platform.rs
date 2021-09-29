// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use crate::{encoding, graph, Result};

pub trait PlatformStorage: graph::Database {
    /// Iterate the objects in this storage which are platforms.
    fn iter_platforms<'db>(
        &'db self,
    ) -> Box<dyn Iterator<Item = graph::Result<(encoding::Digest, graph::Platform)>> + 'db> {
        use graph::Object;
        Box::new(self.iter_objects().filter_map(|res| match res {
            Ok((digest, obj)) => match obj {
                Object::Platform(platform) => Some(Ok((digest, platform))),
                _ => None,
            },
            Err(err) => Some(Err(err)),
        }))
    }

    /// Return true if the identified platform exists in this storage.
    fn has_platform(&self, digest: &encoding::Digest) -> bool {
        self.read_platform(digest).is_ok()
    }

    /// Return the platform identified by the given digest.
    fn read_platform(&self, digest: &encoding::Digest) -> Result<graph::Platform> {
        use graph::Object;
        match self.read_object(digest) {
            Err(err) => Err(err),
            Ok(Object::Platform(platform)) => Ok(platform),
            Ok(_) => Err(format!("Object is not a platform: {:?}", digest).into()),
        }
    }

    /// Create and storage a new platform for the given platform.
    /// Layers are ordered bottom to top.
    fn create_platform(&mut self, layers: Vec<encoding::Digest>) -> Result<graph::Platform> {
        let platform = graph::Platform::new(layers.into_iter())?;
        let storable = graph::Object::Platform(platform);
        self.write_object(&storable)?;
        if let graph::Object::Platform(platform) = storable {
            Ok(platform)
        } else {
            panic!("this is impossible!");
        }
    }
}

impl<T: PlatformStorage> PlatformStorage for &mut T {}

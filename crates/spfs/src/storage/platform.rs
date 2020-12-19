use crate::{encoding, graph, Result};

pub trait PlatformStorage: graph::Database {
    /// Iterate the objects in this storage which are platforms.
    fn iter_platforms<'db>(
        &'db self,
    ) -> Box<dyn Iterator<Item = graph::Result<(encoding::Digest, &'db graph::Platform)>> + 'db>
    where
        Self: Sized,
    {
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
        match self.read_platform(digest) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    /// Return the platform identified by the given digest.
    fn read_platform<'db>(&'db self, digest: &encoding::Digest) -> Result<&'db graph::Platform> {
        use graph::Object;
        match self.read_object(digest) {
            Err(err) => Err(err.into()),
            Ok(Object::Platform(platform)) => Ok(platform),
            Ok(_) => Err(format!("Object is not a platform: {:?}", digest).into()),
        }
    }

    /// Create and storage a new platform for the given platform.
    /// Layers are ordered bottom to top.
    fn create_platform<E, I>(&mut self, layers: I) -> Result<graph::Platform>
    where
        E: encoding::Encodable,
        I: IntoIterator<Item = E>,
    {
        let platform = graph::Platform::new(layers)?;
        let storable = graph::Object::Platform(platform);
        self.write_object(&storable)?;
        if let graph::Object::Platform(platform) = storable {
            Ok(platform)
        } else {
            panic!("this is impossible!");
        }
    }
}

impl<T: graph::Database> PlatformStorage for T {}

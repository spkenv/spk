use std::os::unix::fs::PermissionsExt;

use crate::graph::{Blob, Layer, Manifest, Object, Platform};
use crate::{encoding, graph, tracking, Error};
use encoding::Encodable;
use graph::DatabaseView;

impl DatabaseView for super::FSRepository {
    fn read_object<'db>(&'db self, digest: &encoding::Digest) -> graph::Result<graph::Object> {
        let filepath = self.objects.build_digest_path(&digest);
        let mut reader = std::fs::File::open(&filepath).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => graph::UnknownObjectError::new(&digest),
            _ => Error::from(err),
        })?;
        Object::decode(&mut reader)
    }

    fn iter_digests<'db>(&'db self) -> Box<dyn Iterator<Item = graph::Result<encoding::Digest>>> {
        match self.objects.iter() {
            Ok(iter) => Box::new(iter),
            Err(err) => Box::new(vec![Err(Error::from(err))].into_iter()),
        }
    }
}

impl graph::Database for super::FSRepository {
    fn write_object(&mut self, obj: &graph::Object) -> graph::Result<()> {
        let filepath = self.objects.build_digest_path(&obj.digest()?);
        if filepath.exists() {
            return Ok(());
        }

        // we need to use a temporary file here, so that
        // other processes don't try to read our incomplete
        // object from the database
        let working_file = self.root().join(uuid::Uuid::new_v4().to_string());
        self.objects.ensure_base_dir(&filepath)?;
        let mut writer = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .open(&working_file)?;
        obj.encode(&mut writer)?;
        match std::fs::rename(&working_file, &filepath) {
            Ok(_) => Ok(()),
            Err(err) => {
                let _ = std::fs::remove_file(&working_file);
                match err.kind() {
                    std::io::ErrorKind::AlreadyExists => {
                        let _ = std::fs::remove_file(&working_file);
                        Ok(())
                    }
                    _ => Err(err.into()),
                }
            }
        }
    }

    fn remove_object(&mut self, digest: &encoding::Digest) -> crate::Result<()> {
        let filepath = self.objects.build_digest_path(&digest);

        // this might fail but we don't consider that fatal just yet
        let _ = std::fs::set_permissions(&filepath, std::fs::Permissions::from_mode(0o777));

        if let Err(err) = std::fs::remove_file(&filepath) {
            return match err.kind() {
                std::io::ErrorKind::NotFound => Ok(()),
                _ => Err(err.into()),
            };
        }
        Ok(())
    }
}

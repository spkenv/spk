// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use std::os::unix::fs::PermissionsExt;

use crate::graph::Object;
use crate::{encoding, graph, Error};
use encoding::{Decodable, Encodable};
use graph::DatabaseView;

impl DatabaseView for super::FSRepository {
    fn read_object(&self, digest: &encoding::Digest) -> graph::Result<graph::Object> {
        let filepath = self.objects.build_digest_path(digest);
        let mut reader = std::fs::File::open(&filepath).map_err(|err| match err.kind() {
            std::io::ErrorKind::NotFound => graph::UnknownObjectError::new_err(digest),
            _ => Error::from(err),
        })?;
        Object::decode(&mut reader)
    }

    fn iter_digests(&self) -> Box<dyn Iterator<Item = graph::Result<encoding::Digest>>> {
        match self.objects.iter() {
            Ok(iter) => Box::new(iter),
            Err(err) => Box::new(vec![Err(err)].into_iter()),
        }
    }

    fn iter_objects(&self) -> graph::DatabaseIterator<'_> {
        graph::DatabaseIterator::new(self)
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        graph::DatabaseWalker::new(self, *root)
    }

    fn resolve_full_digest(
        &self,
        partial: &encoding::PartialDigest,
    ) -> graph::Result<encoding::Digest> {
        self.objects.resolve_full_digest(partial)
    }
}

impl graph::Database for super::FSRepository {
    fn write_object(&mut self, obj: &graph::Object) -> graph::Result<()> {
        let digest = obj.digest()?;
        let filepath = self.objects.build_digest_path(&digest);
        if filepath.exists() {
            tracing::trace!(?digest, "object already exists");
            return Ok(());
        }
        tracing::trace!(?digest, kind = ?obj.kind(), "writing object to db");

        // we need to use a temporary file here, so that
        // other processes don't try to read our incomplete
        // object from the database
        let uuid = uuid::Uuid::new_v4().to_string();
        let working_file = self.objects.workdir().join(uuid);
        self.objects.ensure_base_dir(&working_file)?;
        let mut writer = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&working_file)?;
        if let Err(err) = obj.encode(&mut writer) {
            let _ = std::fs::remove_file(&working_file);
            return Err(err);
        }
        if let Err(err) = writer.sync_all() {
            let _ = std::fs::remove_file(&working_file);
            return Err(Error::wrap_io(err, "Failed to finalize object write"));
        }
        self.objects.ensure_base_dir(&filepath)?;
        match std::fs::rename(&working_file, &filepath) {
            Ok(_) => Ok(()),
            Err(err) => {
                let _ = std::fs::remove_file(&working_file);
                match err.kind() {
                    std::io::ErrorKind::AlreadyExists => Ok(()),
                    _ => Err(err.into()),
                }
            }
        }
    }

    fn remove_object(&mut self, digest: &encoding::Digest) -> crate::Result<()> {
        let filepath = self.objects.build_digest_path(digest);

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

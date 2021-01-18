use tar

use crate::{graph, encoding, prelude::*};
use super::payloads::TarPayloadStorage;



impl graph::DatabaseView for super::TarRepository {
    fn read_object(&self, digest: &encoding::Digest) -> graph::Result<graph::Object> {
        self.
        todo!()
    }

    fn iter_digests(&self) -> Box<dyn Iterator<Item = graph::Result<encoding::Digest>>> {
        todo!()
    }

    fn iter_objects<'db>(&'db self) -> graph::DatabaseIterator<'db> {
        graph::DatabaseIterator::new(Box::new(self))
    }

    fn walk_objects<'db>(&'db self, root: &encoding::Digest) -> graph::DatabaseWalker<'db> {
        graph::DatabaseWalker::new(Box::new(self))
    }
}

    def __init__(self, tar: tarfile.TarFile) -> None:

        super(TarDatabase, self).__init__(tar)
        self._prefix = "objects/"

    def read_object(self, digest: encoding.Digest) -> graph.Object:

        with self.open_payload(digest) as payload:
            reader = io.BytesIO(payload.read())

        try:
            encoding.consume_header(reader, _OBJECT_HEADER)
            kind = encoding.read_int(reader)
            if kind not in _OBJECT_KINDS:
                raise ValueError(f"Object is corrupt: unknown kind {kind} [{digest}]")
            return _OBJECT_KINDS[kind].decode(reader)
        finally:
            reader.close()

    def write_object(self, obj: graph.Object) -> None:

        for kind, cls in _OBJECT_KINDS.items():
            if isinstance(obj, cls):
                break
        else:
            raise ValueError(f"Unkown object kind, cannot store: {type(obj)}")

        filepath = self._build_digest_path(obj.digest())
        writer = io.BytesIO()
        encoding.write_header(writer, _OBJECT_HEADER)
        encoding.write_int(writer, kind)
        obj.encode(writer)
        writer.seek(0)
        info = tarfile.TarInfo(filepath)
        info.size = len(writer.getvalue())
        self._tar.addfile(info, writer)

    def remove_object(self, digest: encoding.Digest) -> None:

        self.remove_payload(digest)

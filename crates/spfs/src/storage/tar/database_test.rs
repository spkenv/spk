use rstest::rstest;
use super::TarDatabase;


class LargeObj(graph.Object):
    def digest(self) -> encoding.Digest:
        # all objs share one digest
        return encoding.NULL_DIGEST

    def encode(self, writer: BinaryIO) -> None:

        # simlulate a long write process
        time.sleep(2)

    @classmethod
    def decode(self, reader: BinaryIO) -> "LargeObj":
        return LargeObj()
}


fn try_sync(&mut db: TarDatabase) -> Option<str> {
    try:
        obj = LargeObj()
        if not db.has_object(obj.digest()):
            db.write_object(LargeObj())
    except Exception:
        return traceback.format_exc()
    return None
}


// #[test]
#[tokio::test]
fn test_database_race_condition(tmpdir: tempdir::TempDir) {

    pytest.skip("Tar archives are not concurrent-safe yet")

    db = TarDatabase(tmpdir.path().join("db.tar").strpath)

    with mock.patch.dict(_OBJECT_KINDS, {99: LargeObj}):

        with multiprocessing.Pool() as pool:
            results = pool.map(try_sync, itertools.repeat(db, 50))
            for err in results:
                if err is not None:
                    pytest.fail(err)
}

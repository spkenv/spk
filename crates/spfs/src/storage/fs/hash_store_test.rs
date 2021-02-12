use super::FSHashStore;
use crate::encoding;
use encoding::Encodable;

struct LargeObj {}

impl encoding::Encodable for LargeObj {
    fn digest(&self) -> crate::Result<encoding::Digest> {
        Ok(encoding::NULL_DIGEST.into())
    }

    fn encode(&self, _writer: &mut impl std::io::Write) -> crate::Result<()> {
        // simlulate a long write process
        std::thread::sleep(std::time::Duration::from_secs(2));
        Ok(())
    }
}

impl encoding::Decodable for LargeObj {
    fn decode(_reader: &mut impl std::io::Read) -> crate::Result<Self> {
        Ok(Self {})
    }
}

fn try_sync(store: &mut FSHashStore) -> Option<crate::Error> {
    let obj = LargeObj {};
    let mut buf = Vec::new();
    obj.encode(&mut buf).expect("failed to encode test obj");
    if !store.has_digest(&obj.digest().unwrap()) {
        match store.write_data(Box::new(&mut buf.as_slice())) {
            Ok(_) => None,
            Err(err) => Some(err),
        }
    } else {
        None
    }
}

// TODO: resurrect when adding async
// #[tokio::test]
// async fn test_database_race_condition(tmpdir: py.path.local)  {

//     db = FSDatabase(tmpdir.strpath)

//     with mock.patch.dict(_OBJECT_KINDS, {99: LargeObj}):

//         with multiprocessing.Pool() as pool:
//             results = pool.map(try_sync, itertools.repeat(db, 50))
//             for err in results:
//                 if err is not None:
//                     pytest.fail(str(err))
// }

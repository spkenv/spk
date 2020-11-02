mod binary;
pub use binary::{
    consume_header, read_digest, read_int, read_string, write_digest, write_header, write_int,
    write_string, INT_SIZE,
};

mod hash;
pub use hash::{parse_digest, Digest, Encodable, DIGEST_SIZE, EMPTY_DIGEST, NULL_DIGEST};

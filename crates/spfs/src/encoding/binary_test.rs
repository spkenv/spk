use rand::Rng;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};

use rstest::rstest;

use super::{consume_header, read_int, read_string, write_header, write_int, write_string};

macro_rules! assert_read_content {
    ($stream:expr, $expected:expr) => {
        let mut buf: Vec<u8> = Vec::new();
        $stream
            .read_to_end(&mut buf)
            .expect("failed to read rest of stream");
        assert_eq!(buf.as_slice(), $expected);
    };
}

#[test]
fn test_consume_header() {
    let mut stream = Cursor::new(Vec::from("HEADER\n".as_bytes()));
    consume_header(&mut stream, "HEADER".as_bytes()).expect("failed to read header");

    let nothing: &[u8] = &[];
    assert_read_content!(stream, nothing);
}

#[test]
fn test_write_read_header() {
    let header = b"HEADER";
    let mut stream = Cursor::new(Vec::<u8>::new());
    write_header(&mut stream, header).expect("failed to write header");
    stream.seek(SeekFrom::Start(0)).unwrap();
    consume_header(&mut stream, header).expect("failed to consume header");
    let mut remaining = String::new();
    stream.read_to_string(&mut remaining).unwrap();
    assert_eq!(remaining, "");
}

#[rstest(value, case(0), case(1), case(45), case(600))]
fn test_read_write_int(value: i64) {
    let mut stream = Cursor::new(Vec::<u8>::new());
    write_int(&mut stream, value).unwrap();
    stream.write_all(b"postfix").unwrap();
    stream.seek(SeekFrom::Start(0)).unwrap();
    assert_eq!(read_int(&mut stream).unwrap(), value);
    let mut remaining = String::new();
    stream.read_to_string(&mut remaining).unwrap();
    assert_eq!(remaining, "postfix");
}

fn random_word(length: usize) -> String {
    rand::thread_rng()
        .sample_iter(&rand::distributions::Alphanumeric)
        .take(length)
        .collect()
}

#[rstest(
    i,
    case(1),
    case(2),
    case(3),
    case(4),
    case(5),
    case(6),
    case(7),
    case(8),
    case(9),
    case(10)
)]
fn test_read_write_string(i: u64) {
    println!("running generated test #{}", i);
    let value = random_word(rand::thread_rng().gen_range(256, 1024));
    let postfix = random_word(rand::thread_rng().gen_range(256, 1024));

    let mut stream = Cursor::new(Vec::<u8>::new());
    write_string(&mut stream, &value).unwrap();
    write_string(&mut stream, &postfix).unwrap();
    stream.write(b"postfix").unwrap();
    stream.seek(SeekFrom::Start(0)).unwrap();
    assert_eq!(read_string(&mut stream).unwrap(), value);
    assert_eq!(read_string(&mut stream).unwrap(), postfix);
    let mut remaining = String::new();
    stream.read_to_string(&mut remaining).unwrap();
    assert_eq!(remaining, "postfix");
}

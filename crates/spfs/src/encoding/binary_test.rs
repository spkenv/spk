// Copyright (c) 2021 Sony Pictures Imageworks, et al.
// SPDX-License-Identifier: Apache-2.0
// https://github.com/imageworks/spk

use rand::Rng;
use std::io::{BufRead, Cursor, Read, Seek, SeekFrom, Write};

use rstest::rstest;

use super::{consume_header, read_int, read_string, write_header, write_int, write_string};

fn assert_read_content(stream: &mut impl Read, expected: &[u8]) {
    let mut buf: Vec<u8> = Vec::new();
    stream
        .read_to_end(&mut buf)
        .expect("failed to read rest of stream");
    assert_eq!(buf.as_slice(), expected);
}

#[rstest]
fn test_consume_header() {
    let mut stream = Cursor::new(Vec::from("HEADER\n".as_bytes()));
    consume_header(&mut stream, "HEADER".as_bytes()).expect("failed to read header");

    let nothing: &[u8] = &[];
    assert_read_content(&mut stream, nothing);
}

#[rstest]
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
    println!("running generated test #{i}");
    let value = random_word(rand::thread_rng().gen_range(256, 1024));
    let postfix = random_word(rand::thread_rng().gen_range(256, 1024));

    let mut stream = Cursor::new(Vec::<u8>::new());
    write_string(&mut stream, &value).unwrap();
    write_string(&mut stream, &postfix).unwrap();
    stream.write_all(b"postfix").unwrap();
    stream.seek(SeekFrom::Start(0)).unwrap();
    assert_eq!(read_string(&mut stream).unwrap(), value);
    assert_eq!(read_string(&mut stream).unwrap(), postfix);
    let mut remaining = String::new();
    stream.read_to_string(&mut remaining).unwrap();
    assert_eq!(remaining, "postfix");
}

const TEST_RAW_ERROR: i32 = 22;

#[derive(Debug, Default)]
struct TestStream {
    data: Vec<u8>,
    buffer: Vec<u8>,
    buffer_size: usize,
    fill_buf_fails: bool,
}

impl TestStream {
    fn new<S>(strings: Vec<S>, buffer_size: usize) -> Self
    where
        S: AsRef<str>,
    {
        let mut data = Vec::new();
        for s in strings {
            data.extend(s.as_ref().as_bytes());
            data.push(0);
        }
        Self {
            data,
            buffer_size,
            ..Default::default()
        }
    }
}

impl Read for TestStream {
    fn read(&mut self, _buf: &mut [u8]) -> std::io::Result<usize> {
        unreachable!()
    }
}

impl BufRead for TestStream {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        if self.fill_buf_fails {
            return Err(std::io::Error::from_raw_os_error(TEST_RAW_ERROR));
        } else if self.data.is_empty() {
            return Ok(&[]);
        } else if self.buffer.is_empty() {
            self.buffer = self
                .data
                .drain(..self.data.len().min(self.buffer_size))
                .collect();
        }
        Ok(&self.buffer[..])
    }

    fn consume(&mut self, amt: usize) {
        let remaining_in_buffer = self.buffer.len();
        if amt > remaining_in_buffer {
            panic!(
                "Invalid amt given to consume: {} > {}",
                amt, remaining_in_buffer
            )
        }
        self.buffer.drain(..amt);
    }
}

#[test]
fn test_read_string_failure() {
    let mut ts = TestStream {
        fill_buf_fails: true,
        ..Default::default()
    };
    let r = read_string(&mut ts);
    assert!(
        matches!(r, Err(crate::Error::EncodingReadError(io)) if io.raw_os_error() == Some(TEST_RAW_ERROR))
    );
}

#[test]
fn test_read_string_eof() {
    let mut ts = TestStream::default();
    let r = read_string(&mut ts);
    assert!(
        matches!(r, Err(crate::Error::EncodingReadError(io)) if io.kind() == std::io::ErrorKind::UnexpectedEof)
    );
}

#[test]
fn test_read_string_normal() {
    let test_string1 = "this is a test 1";
    let test_string2 = "this is a test 2";

    // This assertion is obviously true but it codifies that
    // this test is intended to exercise `read_string` hitting
    // the end of a buffer.
    let buffer_size = 8;
    assert!(buffer_size < test_string1.len() + test_string2.len() + 2);

    let mut ts = TestStream::new(vec![test_string1, test_string2], buffer_size);
    let r = read_string(&mut ts);
    assert!(matches!(r, Ok(s) if s == test_string1));
    let r = read_string(&mut ts);
    assert!(matches!(r, Ok(s) if s == test_string2));
}

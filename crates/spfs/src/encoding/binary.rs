use std::io::{Read, Write};

use unicode_reader::CodePoints;

use super::hash::{Digest, DIGEST_SIZE, NULL_DIGEST};
use crate::{Error, Result};

pub const INT_SIZE: usize = std::mem::size_of::<u64>();

#[cfg(test)]
#[path = "./binary_test.rs"]
mod binary_test;

/// Read and validate the given header from a binary stream.
pub fn consume_header(mut reader: impl Read, header: &[u8]) -> Result<()> {
    let mut buf = Vec::with_capacity(header.len() + 1);
    buf.resize(header.len() + 1, 0);
    reader.read_exact(buf.as_mut_slice())?;
    if buf[0..header.len()] != *header || buf.last() != Some(&('\n' as u8)) {
        Err(Error::from(format!(
            "Invalid header: expected {:?}, got {:?}",
            header, buf
        )))
    } else {
        Ok(())
    }
}

/// Write an identifiable header to the given binary stream.
pub fn write_header(mut writer: impl Write, header: &[u8]) -> Result<()> {
    writer.write_all(header)?;
    writer.write_all(b"\n")?;
    Ok(())
}

/// Write an integer to the given binary stream.
pub fn write_int(mut writer: impl Write, value: i64) -> Result<()> {
    writer.write_all(&value.to_be_bytes())?;
    Ok(())
}

/// Read an integer from the given binary stream.
pub fn read_int(mut reader: impl Read) -> Result<i64> {
    let mut buf: [u8; INT_SIZE] = [0, 0, 0, 0, 0, 0, 0, 0];
    reader.read_exact(&mut buf)?;
    Ok(i64::from_be_bytes(buf))
}

/// Write a digest to the given binary stream.
pub fn write_digest(mut writer: impl Write, digest: Digest) -> Result<()> {
    writer.write_all(digest.as_ref())?;
    Ok(())
}

/// Read a digest from the given binary stream.
pub fn read_digest(mut reader: impl Read) -> Result<Digest> {
    let mut buf: [u8; DIGEST_SIZE] = NULL_DIGEST.clone();
    reader.read_exact(buf.as_mut())?;
    Ok(Digest::from_bytes(&buf)?)
}

/// Write a string to the given binary stream.
pub fn write_string(mut writer: impl Write, string: &str) -> Result<()> {
    if string.contains("\x00") {
        return Err(Error::from(
            "Cannot encode string with null character".to_string(),
        ));
    }
    writer.write_all(string.as_bytes())?;
    writer.write_all("\x00".as_bytes())?;
    Ok(())
}

/// Read a string from the given binary stream.
pub fn read_string(reader: impl Read) -> Result<String> {
    let unicode_reader = CodePoints::from(reader);
    let text: std::result::Result<Vec<_>, _> = unicode_reader
        .take_while(|c| match c {
            Ok(c) => c != &'\x00',
            Err(_) => true,
        })
        .collect();
    Ok(text?.into_iter().collect())
}

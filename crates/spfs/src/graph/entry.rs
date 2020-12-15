use crate::encoding;
use crate::tracking;
use crate::Result;

#[derive(Eq, PartialEq)]
pub struct Entry {
    pub object: encoding::Digest,
    pub kind: tracking::EntryKind,
    pub mode: u32,
    pub size: u64,
    pub name: String,
}

impl Entry {
    pub fn from(name: String, entry: &tracking::Entry) -> Self {
        Self {
            object: entry.object,
            kind: entry.kind,
            mode: entry.mode,
            size: entry.size,
            name: name,
        }
    }
}

impl std::fmt::Display for Entry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_fmt(format_args!(
            "{:06o} {:?} {} {}",
            self.mode, self.kind, self.name, self.object
        ))
    }
}

impl PartialOrd for Entry {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Entry {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        if self.kind == other.kind {
            self.name.cmp(&other.name)
        } else {
            self.kind.cmp(&other.kind)
        }
    }
}

impl encoding::Encodable for Entry {
    fn encode(&self, mut writer: &mut impl std::io::Write) -> Result<()> {
        encoding::write_digest(&mut writer, &self.object)?;
        self.kind.encode(&mut writer)?;
        encoding::write_uint(&mut writer, self.mode as u64)?;
        encoding::write_uint(&mut writer, self.size)?;
        encoding::write_string(writer, self.name.as_str())?;
        Ok(())
    }

    fn decode(mut reader: &mut impl std::io::Read) -> Result<Self> {
        Ok(Entry {
            object: encoding::read_digest(&mut reader)?,
            kind: tracking::EntryKind::decode(&mut reader)?,
            mode: encoding::read_uint(&mut reader)? as u32,
            size: encoding::read_uint(&mut reader)?,
            name: encoding::read_string(reader)?,
        })
    }
}

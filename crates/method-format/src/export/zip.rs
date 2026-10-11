//! A minimal ZIP writer: STORED entries (no compression), no ZIP64, no
//! encryption, a fixed timestamp. Enough for an .xlsx package, which is a ZIP
//! of XML parts, without a compression crate.
//!
//! Layout (PKWARE APPNOTE 4.3): for each entry a local file header and its
//! bytes, then the central directory (one record per entry), then the end of
//! central directory record. All numbers are little-endian. Every entry has
//! the same modification time, 1980-01-01 00:00 (the earliest a DOS date can
//! say), so the same input always gives the same bytes.

/// Why a package can't be written as a plain (non-ZIP64) ZIP file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ZipError {
    /// More than 65,535 entries.
    TooManyEntries,
    /// An entry, or the whole archive, reaches 4 GiB.
    TooLarge,
    /// An entry name longer than 65,535 bytes.
    NameTooLong,
}

impl std::fmt::Display for ZipError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            ZipError::TooManyEntries => "too many parts for a ZIP file",
            ZipError::TooLarge => "too large for a ZIP file (4 GB or more)",
            ZipError::NameTooLong => "a part name is too long",
        })
    }
}

impl std::error::Error for ZipError {}

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL: u32 = 0x0605_4b50;
/// Version 2.0: what plain STORED and DEFLATE entries need.
const VERSION: u16 = 20;
/// 00:00:00 in DOS time.
const DOS_TIME: u16 = 0;
/// 1980-01-01 in DOS date: (year - 1980) << 9 | month << 5 | day.
const DOS_DATE: u16 = (1 << 5) | 1;

/// The CRC-32 lookup table (IEEE 802.3, reflected polynomial 0xEDB88320),
/// built at compile time.
const CRC_TABLE: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut i = 0;
    while i < 256 {
        let mut c = i as u32;
        let mut k = 0;
        while k < 8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
            k += 1;
        }
        table[i] = c;
        i += 1;
    }
    table
};

/// CRC-32 as ZIP (and PNG, gzip, Ethernet) use it.
pub fn crc32(bytes: &[u8]) -> u32 {
    let mut c = 0xFFFF_FFFFu32;
    for &b in bytes {
        c = CRC_TABLE[((c ^ u32::from(b)) & 0xFF) as usize] ^ (c >> 8);
    }
    c ^ 0xFFFF_FFFF
}

struct Entry {
    name: String,
    crc: u32,
    size: u32,
    offset: u32,
}

/// Builds a ZIP archive in memory, one entry at a time.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    entries: Vec<Entry>,
}

fn put16(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put32(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

impl ZipWriter {
    pub fn new() -> ZipWriter {
        ZipWriter::default()
    }

    /// Adds a stored (uncompressed) entry. `name` uses `/` between folders
    /// and is written as given (ASCII names need no UTF-8 flag).
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), ZipError> {
        if self.entries.len() >= usize::from(u16::MAX) {
            return Err(ZipError::TooManyEntries);
        }
        let name_len = u16::try_from(name.len()).map_err(|_| ZipError::NameTooLong)?;
        let size = u32::try_from(data.len()).map_err(|_| ZipError::TooLarge)?;
        let offset = u32::try_from(self.out.len()).map_err(|_| ZipError::TooLarge)?;
        let crc = crc32(data);
        // Bit 11 marks a UTF-8 name; plain ASCII doesn't need it.
        let flags: u16 = if name.is_ascii() { 0 } else { 1 << 11 };

        let out = &mut self.out;
        put32(out, LOCAL_HEADER);
        put16(out, VERSION);
        put16(out, flags);
        put16(out, 0); // method: stored
        put16(out, DOS_TIME);
        put16(out, DOS_DATE);
        put32(out, crc);
        put32(out, size); // compressed size
        put32(out, size); // uncompressed size
        put16(out, name_len);
        put16(out, 0); // extra field length
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        if u32::try_from(self.out.len()).is_err() {
            return Err(ZipError::TooLarge);
        }
        self.entries.push(Entry {
            name: name.to_string(),
            crc,
            size,
            offset,
        });
        Ok(())
    }

    /// Writes the central directory and returns the finished archive.
    pub fn finish(mut self) -> Result<Vec<u8>, ZipError> {
        let start = u32::try_from(self.out.len()).map_err(|_| ZipError::TooLarge)?;
        for e in &self.entries {
            let out = &mut self.out;
            let flags: u16 = if e.name.is_ascii() { 0 } else { 1 << 11 };
            put32(out, CENTRAL_HEADER);
            put16(out, VERSION); // made by: MS-DOS (0), version 2.0
            put16(out, VERSION); // needed to extract
            put16(out, flags);
            put16(out, 0); // method: stored
            put16(out, DOS_TIME);
            put16(out, DOS_DATE);
            put32(out, e.crc);
            put32(out, e.size);
            put32(out, e.size);
            // Checked when the entry was added.
            put16(out, e.name.len() as u16);
            put16(out, 0); // extra field length
            put16(out, 0); // comment length
            put16(out, 0); // disk number start
            put16(out, 0); // internal attributes
            put32(out, 0); // external attributes
            put32(out, e.offset);
            out.extend_from_slice(e.name.as_bytes());
        }
        let end = u32::try_from(self.out.len()).map_err(|_| ZipError::TooLarge)?;
        let count = self.entries.len() as u16;
        let out = &mut self.out;
        put32(out, END_OF_CENTRAL);
        put16(out, 0); // this disk
        put16(out, 0); // disk with the central directory
        put16(out, count); // entries on this disk
        put16(out, count); // entries in all
        put32(out, end - start); // central directory size
        put32(out, start); // central directory offset
        put16(out, 0); // comment length
        Ok(self.out)
    }
}

/// One entry read back by [`read_entries`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadEntry {
    pub name: String,
    pub data: Vec<u8>,
}

/// Reads a ZIP archive with stored entries and no archive comment (what
/// [`ZipWriter`] makes), checking every signature, size and CRC. For tests
/// and self-checks; it is not a general ZIP reader.
pub fn read_entries(zip: &[u8]) -> Result<Vec<ReadEntry>, String> {
    let u16_at = |at: usize| -> Result<u16, String> {
        zip.get(at..at + 2)
            .map(|b| u16::from_le_bytes([b[0], b[1]]))
            .ok_or_else(|| format!("truncated at byte {at}"))
    };
    let u32_at = |at: usize| -> Result<u32, String> {
        zip.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or_else(|| format!("truncated at byte {at}"))
    };
    let eocd = zip
        .len()
        .checked_sub(22)
        .ok_or("too short for a ZIP file")?;
    if u32_at(eocd)? != END_OF_CENTRAL {
        return Err("no end of central directory record".into());
    }
    let count = usize::from(u16_at(eocd + 10)?);
    if usize::from(u16_at(eocd + 8)?) != count {
        return Err("entry counts disagree".into());
    }
    let cd_size = u32_at(eocd + 12)? as usize;
    let cd_start = u32_at(eocd + 16)? as usize;
    if cd_start + cd_size != eocd {
        return Err("central directory is not where the end record says".into());
    }
    let mut out = Vec::with_capacity(count);
    let mut at = cd_start;
    for _ in 0..count {
        if u32_at(at)? != CENTRAL_HEADER {
            return Err(format!("bad central directory record at byte {at}"));
        }
        let method = u16_at(at + 10)?;
        let crc = u32_at(at + 16)?;
        let csize = u32_at(at + 20)? as usize;
        let usize_ = u32_at(at + 24)? as usize;
        let name_len = usize::from(u16_at(at + 28)?);
        let extra_len = usize::from(u16_at(at + 30)?);
        let comment_len = usize::from(u16_at(at + 32)?);
        let local = u32_at(at + 42)? as usize;
        let name = zip
            .get(at + 46..at + 46 + name_len)
            .ok_or("truncated name")?;
        let name = String::from_utf8(name.to_vec()).map_err(|e| e.to_string())?;
        if method != 0 || csize != usize_ {
            return Err(format!("{name}: not a stored entry"));
        }
        if u32_at(local)? != LOCAL_HEADER {
            return Err(format!("{name}: bad local header"));
        }
        if u32_at(local + 14)? != crc
            || u32_at(local + 18)? as usize != csize
            || usize::from(u16_at(local + 26)?) != name_len
            || zip.get(local + 30..local + 30 + name_len) != Some(name.as_bytes())
        {
            return Err(format!("{name}: local header disagrees with the directory"));
        }
        let data_at = local + 30 + name_len + usize::from(u16_at(local + 28)?);
        let data = zip
            .get(data_at..data_at + csize)
            .ok_or_else(|| format!("{name}: truncated data"))?;
        if crc32(data) != crc {
            return Err(format!("{name}: CRC mismatch"));
        }
        out.push(ReadEntry {
            name,
            data: data.to_vec(),
        });
        at += 46 + name_len + extra_len + comment_len;
    }
    if at != eocd {
        return Err("central directory size is wrong".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crc32_test_vectors() {
        assert_eq!(crc32(b""), 0);
        assert_eq!(crc32(b"a"), 0xE8B7_BE43);
        assert_eq!(crc32(b"abc"), 0x3524_41C2);
        assert_eq!(crc32(b"123456789"), 0xCBF4_3926);
        assert_eq!(
            crc32(b"The quick brown fox jumps over the lazy dog"),
            0x414F_A339
        );
        assert_eq!(crc32(&[0u8; 32]), 0x190A_55AD);
        assert_eq!(crc32(&[0xFFu8; 32]), 0xFF6C_AB0B);
    }

    #[test]
    fn writes_and_reads_back() {
        let mut z = ZipWriter::new();
        z.add("[Content_Types].xml", b"<Types/>").unwrap();
        z.add("xl/empty.xml", b"").unwrap();
        z.add("docs/caf\u{e9}.txt", "caf\u{e9}".as_bytes()).unwrap();
        let bytes = z.finish().unwrap();
        let entries = read_entries(&bytes).unwrap();
        let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["[Content_Types].xml", "xl/empty.xml", "docs/caf\u{e9}.txt"]
        );
        assert_eq!(entries[0].data, b"<Types/>");
        assert!(entries[1].data.is_empty());
        // Starts with a local header, ends with the end record.
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        assert_eq!(&bytes[bytes.len() - 22..bytes.len() - 18], b"PK\x05\x06");
        // Same input, same bytes.
        let mut again = ZipWriter::new();
        again.add("[Content_Types].xml", b"<Types/>").unwrap();
        again.add("xl/empty.xml", b"").unwrap();
        again
            .add("docs/caf\u{e9}.txt", "caf\u{e9}".as_bytes())
            .unwrap();
        assert_eq!(again.finish().unwrap(), bytes);
    }

    #[test]
    fn an_empty_archive_is_just_the_end_record() {
        let bytes = ZipWriter::new().finish().unwrap();
        assert_eq!(bytes.len(), 22);
        assert!(read_entries(&bytes).unwrap().is_empty());
    }

    #[test]
    fn corruption_is_caught() {
        let mut z = ZipWriter::new();
        z.add("a.xml", b"<a>hello</a>").unwrap();
        let good = z.finish().unwrap();
        let mut bad = good.clone();
        // Flip a byte of the data: the CRC no longer matches.
        bad[30 + 5 + 3] ^= 1;
        assert!(read_entries(&bad).unwrap_err().contains("CRC"));
        assert!(read_entries(&good[..good.len() - 1]).is_err());
    }
}

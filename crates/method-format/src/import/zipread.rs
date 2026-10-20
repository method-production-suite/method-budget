//! A ZIP reader for .xlsx packages: STORED and DEFLATE entries (see
//! [`super::inflate`]), found through the central directory.
//!
//! Layout (PKWARE APPNOTE 4.3): local headers and data, then the central
//! directory (one record per entry), then the end of central directory
//! record, optionally followed by an archive comment. All numbers are
//! little-endian.
//!
//! The archive is untrusted input:
//!
//! - every offset and length is bounds-checked against the bytes we have;
//! - an entry's data must lie before the central directory;
//! - an entry is decompressed only when asked for, into at most the size
//!   the directory declares; decoding more or less than that, or bytes whose
//!   CRC-32 differs, is an error;
//! - one archive gives out at most [`MAX_TOTAL`] decompressed bytes in all,
//!   and an entry whose declared size is over [`MAX_RATIO`] times its
//!   compressed size (beyond a small allowance) is refused before decoding:
//!   real XML compresses 5 to 50 times, a ZIP bomb thousands;
//! - at most [`MAX_ENTRIES`] entries; names must be unique (ignoring ASCII
//!   case, as package part names are);
//! - encrypted entries, other compression methods, archives split over
//!   several disks and ZIP64 archives (4 GB and up, which no budget
//!   spreadsheet is) are refused with a message saying so.

use super::inflate::inflate;
use super::ReadError;
use crate::export::zip::crc32;

/// Most decompressed bytes one archive may give out, over every entry read.
pub const MAX_TOTAL: usize = 200 << 20;
/// Most entries an archive may have.
pub const MAX_ENTRIES: usize = 10_000;
/// Largest declared expansion of an entry (beyond [`RATIO_ALLOWANCE`]).
pub const MAX_RATIO: u64 = 1_000;
/// Entries up to this size may expand any amount (tiny parts compress
/// extremely well and can't do harm).
pub const RATIO_ALLOWANCE: u64 = 1 << 20;

const LOCAL_HEADER: u32 = 0x0403_4b50;
const CENTRAL_HEADER: u32 = 0x0201_4b50;
const END_OF_CENTRAL: u32 = 0x0605_4b50;

#[derive(Clone, Debug)]
struct Entry {
    name: String,
    /// The name as parts are compared: no leading `/`, ASCII lowercase.
    key: String,
    method: u16,
    flags: u16,
    crc: u32,
    compressed: u64,
    size: u64,
    local: u64,
}

impl std::fmt::Debug for ZipArchive<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list().entries(self.names()).finish()
    }
}

/// An archive's directory, over its bytes.
pub struct ZipArchive<'a> {
    data: &'a [u8],
    entries: Vec<Entry>,
    /// Where the central directory starts: no entry's data may pass it.
    cd_start: usize,
    /// Decompressed bytes given out so far.
    given: usize,
    max_total: usize,
}

fn err(msg: impl Into<String>) -> ReadError {
    ReadError::new(msg)
}

fn damaged() -> ReadError {
    err("the file is damaged (its ZIP directory doesn't add up)")
}

fn u16_at(d: &[u8], at: usize) -> Result<u16, ReadError> {
    d.get(at..at.checked_add(2).ok_or_else(damaged)?)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .ok_or_else(damaged)
}

fn u32_at(d: &[u8], at: usize) -> Result<u32, ReadError> {
    d.get(at..at.checked_add(4).ok_or_else(damaged)?)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
        .ok_or_else(damaged)
}

/// How part names are compared: no leading `/`, ASCII case ignored.
pub fn part_key(name: &str) -> String {
    name.trim_start_matches('/').to_ascii_lowercase()
}

/// Where the end of central directory record starts: the last signature
/// whose comment length reaches exactly to the end of the file.
fn find_end(d: &[u8]) -> Result<usize, ReadError> {
    let last = d
        .len()
        .checked_sub(22)
        .ok_or_else(|| err("the file is too short to be a spreadsheet (.xlsx)"))?;
    let first = last.saturating_sub(u16::MAX as usize);
    for at in (first..=last).rev() {
        if u32_at(d, at)? == END_OF_CENTRAL && at + 22 + usize::from(u16_at(d, at + 20)?) == d.len()
        {
            return Ok(at);
        }
    }
    Err(err(
        "the file isn't a spreadsheet (.xlsx) (no ZIP directory found)",
    ))
}

impl<'a> ZipArchive<'a> {
    /// Reads the central directory. Nothing is decompressed yet.
    pub fn new(data: &'a [u8]) -> Result<ZipArchive<'a>, ReadError> {
        ZipArchive::with_limit(data, MAX_TOTAL)
    }

    /// Like [`ZipArchive::new`], giving out at most `max_total`
    /// decompressed bytes in all.
    pub fn with_limit(data: &'a [u8], max_total: usize) -> Result<ZipArchive<'a>, ReadError> {
        let end = find_end(data)?;
        let disk = u16_at(data, end + 4)?;
        let cd_disk = u16_at(data, end + 6)?;
        let on_disk = u16_at(data, end + 8)?;
        let count = u16_at(data, end + 10)?;
        let cd_size = u32_at(data, end + 12)?;
        let cd_offset = u32_at(data, end + 16)?;
        if count == u16::MAX || cd_size == u32::MAX || cd_offset == u32::MAX {
            return Err(err(
                "the file is a ZIP64 archive, which is too large to import",
            ));
        }
        if disk != 0 || cd_disk != 0 || on_disk != count {
            return Err(err(
                "the file is part of a split archive, which can't be imported",
            ));
        }
        let count = usize::from(count);
        if count > MAX_ENTRIES {
            return Err(err(
                "the file has too many parts to be a budget spreadsheet",
            ));
        }
        let cd_start = cd_offset as usize;
        if cd_start.checked_add(cd_size as usize) != Some(end) {
            return Err(damaged());
        }
        let mut entries: Vec<Entry> = Vec::with_capacity(count);
        let mut at = cd_start;
        for _ in 0..count {
            if u32_at(data, at)? != CENTRAL_HEADER {
                return Err(damaged());
            }
            let flags = u16_at(data, at + 8)?;
            let method = u16_at(data, at + 10)?;
            let crc = u32_at(data, at + 16)?;
            let compressed = u32_at(data, at + 20)?;
            let size = u32_at(data, at + 24)?;
            let name_len = usize::from(u16_at(data, at + 28)?);
            let extra_len = usize::from(u16_at(data, at + 30)?);
            let comment_len = usize::from(u16_at(data, at + 32)?);
            let local = u32_at(data, at + 42)?;
            if compressed == u32::MAX || size == u32::MAX || local == u32::MAX {
                return Err(err(
                    "the file is a ZIP64 archive, which is too large to import",
                ));
            }
            let raw = data.get(at + 46..at + 46 + name_len).ok_or_else(damaged)?;
            // Bit 11: the name is UTF-8. Otherwise it is code page 437,
            // which agrees with UTF-8 for the ASCII names packages use.
            let name = if flags & (1 << 11) != 0 {
                String::from_utf8(raw.to_vec()).map_err(|_| damaged())?
            } else {
                String::from_utf8_lossy(raw).into_owned()
            };
            if (local as usize) >= cd_start {
                return Err(damaged());
            }
            let key = part_key(&name);
            if entries.iter().any(|e| e.key == key) {
                return Err(err(format!("the file has two parts named {name}")));
            }
            entries.push(Entry {
                name,
                key,
                method,
                flags,
                crc,
                compressed: u64::from(compressed),
                size: u64::from(size),
                local: u64::from(local),
            });
            at = at + 46 + name_len + extra_len + comment_len;
        }
        if at != end {
            return Err(damaged());
        }
        Ok(ZipArchive {
            data,
            entries,
            cd_start,
            given: 0,
            max_total,
        })
    }

    /// Every entry's name, in directory order.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|e| e.name.as_str())
    }

    /// Whether there is an entry named `name` (ignoring a leading `/` and
    /// ASCII case).
    pub fn contains(&self, name: &str) -> bool {
        let key = part_key(name);
        self.entries.iter().any(|e| e.key == key)
    }

    /// The decompressed bytes of entry `name`, or `None` when there is no
    /// such entry.
    pub fn read(&mut self, name: &str) -> Result<Option<Vec<u8>>, ReadError> {
        let key = part_key(name);
        let Some(e) = self.entries.iter().find(|e| e.key == key).cloned() else {
            return Ok(None);
        };
        self.read_entry(&e).map(Some)
    }

    fn read_entry(&mut self, e: &Entry) -> Result<Vec<u8>, ReadError> {
        let name = &e.name;
        if e.flags & 1 != 0 {
            return Err(err(
                "the workbook is password-protected; save an unprotected copy to import it",
            ));
        }
        let size = usize::try_from(e.size).map_err(|_| err("the workbook is too large"))?;
        if size > self.max_total.saturating_sub(self.given) {
            return Err(err(format!(
                "the workbook is too large to import (more than {} MB of data)",
                (self.max_total >> 20).max(1)
            )));
        }
        if e.size > RATIO_ALLOWANCE && e.size / e.compressed.max(1) > MAX_RATIO {
            return Err(err(format!(
                "the workbook's part {name} expands suspiciously far, so it isn't read"
            )));
        }
        let d = self.data;
        let local = e.local as usize;
        if u32_at(d, local)? != LOCAL_HEADER {
            return Err(damaged());
        }
        let name_len = usize::from(u16_at(d, local + 26)?);
        let extra_len = usize::from(u16_at(d, local + 28)?);
        let start = local + 30 + name_len + extra_len;
        let stop = start
            .checked_add(e.compressed as usize)
            .filter(|&s| s <= self.cd_start)
            .ok_or_else(damaged)?;
        let raw = d.get(start..stop).ok_or_else(damaged)?;
        let out = match e.method {
            0 => {
                if e.compressed != e.size {
                    return Err(damaged());
                }
                raw.to_vec()
            }
            8 => inflate(raw, size).map_err(|why| {
                err(format!(
                    "the workbook's part {name} can't be decompressed: {why}"
                ))
            })?,
            m => {
                return Err(err(format!(
                    "the workbook's part {name} uses a compression method this app can't read ({m})"
                )))
            }
        };
        if out.len() != size {
            return Err(err(format!(
                "the workbook's part {name} isn't the size its directory says"
            )));
        }
        if crc32(&out) != e.crc {
            return Err(err(format!(
                "the workbook's part {name} is damaged (its checksum doesn't match)"
            )));
        }
        self.given += size;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::export::zip::ZipWriter;
    use crate::import::inflate::encode::{self, Blocks, Rng};

    /// A ZIP with deflated entries, written by hand (the export's writer
    /// only stores).
    fn deflated_zip(entries: &[(&str, &[u8])], comment: &[u8]) -> Vec<u8> {
        let mut rng = Rng(5);
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let z = encode::deflate(data, Blocks::Mixed, &mut rng);
            let offset = out.len() as u32;
            let crc = crc32(data);
            let header = |sig: u32, buf: &mut Vec<u8>, central: bool| {
                buf.extend_from_slice(&sig.to_le_bytes());
                if central {
                    buf.extend_from_slice(&20u16.to_le_bytes());
                }
                buf.extend_from_slice(&20u16.to_le_bytes());
                buf.extend_from_slice(&0u16.to_le_bytes()); // flags
                buf.extend_from_slice(&8u16.to_le_bytes()); // deflate
                buf.extend_from_slice(&0u32.to_le_bytes()); // time, date
                buf.extend_from_slice(&crc.to_le_bytes());
                buf.extend_from_slice(&(z.len() as u32).to_le_bytes());
                buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
                buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
                buf.extend_from_slice(&0u16.to_le_bytes()); // extra
                if central {
                    buf.extend_from_slice(&[0; 6]); // comment, disk, internal attributes
                    buf.extend_from_slice(&0u32.to_le_bytes()); // external attributes
                    buf.extend_from_slice(&offset.to_le_bytes());
                }
                buf.extend_from_slice(name.as_bytes());
            };
            header(LOCAL_HEADER, &mut out, false);
            out.extend_from_slice(&z);
            header(CENTRAL_HEADER, &mut central, true);
        }
        let start = out.len() as u32;
        out.extend_from_slice(&central);
        out.extend_from_slice(&END_OF_CENTRAL.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
        out.extend_from_slice(&(central.len() as u32).to_le_bytes());
        out.extend_from_slice(&start.to_le_bytes());
        out.extend_from_slice(&(comment.len() as u16).to_le_bytes());
        out.extend_from_slice(comment);
        out
    }

    #[test]
    fn reads_stored_entries_from_our_writer() {
        let mut z = ZipWriter::new();
        z.add("[Content_Types].xml", b"<Types/>").unwrap();
        z.add("xl/Workbook.xml", b"<workbook/>").unwrap();
        z.add("xl/empty.xml", b"").unwrap();
        let bytes = z.finish().unwrap();
        let mut a = ZipArchive::new(&bytes).unwrap();
        assert_eq!(
            a.names().collect::<Vec<_>>(),
            ["[Content_Types].xml", "xl/Workbook.xml", "xl/empty.xml"]
        );
        // Part names ignore case and a leading slash.
        assert!(a.contains("/XL/workbook.xml"));
        assert_eq!(a.read("xl/workbook.xml").unwrap().unwrap(), b"<workbook/>");
        assert_eq!(a.read("xl/empty.xml").unwrap().unwrap(), b"");
        assert_eq!(a.read("xl/missing.xml").unwrap(), None);
    }

    #[test]
    fn reads_deflated_entries_and_comments() {
        let big: Vec<u8> = "<row><c t=\"s\"><v>1</v></c></row>"
            .repeat(3000)
            .into_bytes();
        let bytes = deflated_zip(&[("a.xml", b"hello"), ("b.xml", &big)], b"a comment");
        let mut a = ZipArchive::new(&bytes).unwrap();
        assert_eq!(a.read("a.xml").unwrap().unwrap(), b"hello");
        assert_eq!(a.read("b.xml").unwrap().unwrap(), big);
    }

    #[test]
    fn damage_is_caught() {
        let bytes = deflated_zip(&[("a.xml", b"hello hello hello hello")], b"");
        // Every truncation fails to open or to read, never panics.
        for cut in 0..bytes.len() {
            if let Ok(mut a) = ZipArchive::new(&bytes[..cut]) {
                assert!(a.read("a.xml").is_err() || a.read("a.xml").unwrap().is_none());
            }
        }
        // Every single-bit flip either is caught or reads the right bytes.
        for at in 0..bytes.len() {
            for bit in 0..8 {
                let mut bad = bytes.clone();
                bad[at] ^= 1 << bit;
                if let Ok(mut a) = ZipArchive::new(&bad) {
                    if let Ok(Some(out)) = a.read("a.xml") {
                        assert_eq!(out, b"hello hello hello hello", "byte {at} bit {bit}");
                    }
                }
            }
        }
        // Not a ZIP at all.
        assert!(ZipArchive::new(b"Account,Description\r\n").is_err());
        assert!(ZipArchive::new(b"").is_err());
    }

    #[test]
    fn bombs_and_lies_are_refused() {
        // A declared size far beyond what the data could honestly hold.
        let z = encode::bomb(20 << 20);
        let mut honest = Vec::new();
        {
            let data_len = 20u32 << 20;
            let crc = crc32(&vec![0u8; data_len as usize]);
            let name = b"bomb.xml";
            let put_header = |buf: &mut Vec<u8>, central: bool, offset: u32| {
                buf.extend_from_slice(
                    &(if central {
                        CENTRAL_HEADER
                    } else {
                        LOCAL_HEADER
                    })
                    .to_le_bytes(),
                );
                if central {
                    buf.extend_from_slice(&20u16.to_le_bytes());
                }
                buf.extend_from_slice(&20u16.to_le_bytes());
                buf.extend_from_slice(&0u16.to_le_bytes());
                buf.extend_from_slice(&8u16.to_le_bytes());
                buf.extend_from_slice(&0u32.to_le_bytes());
                buf.extend_from_slice(&crc.to_le_bytes());
                buf.extend_from_slice(&(z.len() as u32).to_le_bytes());
                buf.extend_from_slice(&data_len.to_le_bytes());
                buf.extend_from_slice(&(name.len() as u16).to_le_bytes());
                buf.extend_from_slice(&0u16.to_le_bytes());
                if central {
                    buf.extend_from_slice(&[0; 10]);
                    buf.extend_from_slice(&offset.to_le_bytes());
                }
                buf.extend_from_slice(name);
            };
            put_header(&mut honest, false, 0);
            honest.extend_from_slice(&z);
            let start = honest.len() as u32;
            let mut central = Vec::new();
            put_header(&mut central, true, 0);
            honest.extend_from_slice(&central);
            honest.extend_from_slice(&END_OF_CENTRAL.to_le_bytes());
            honest.extend_from_slice(&[0, 0, 0, 0, 1, 0, 1, 0]);
            honest.extend_from_slice(&(central.len() as u32).to_le_bytes());
            honest.extend_from_slice(&start.to_le_bytes());
            honest.extend_from_slice(&0u16.to_le_bytes());
        }
        // 20 MB from about 130 KB is about 160 to 1: allowed, and it reads.
        let mut a = ZipArchive::new(&honest).unwrap();
        assert_eq!(a.read("bomb.xml").unwrap().unwrap().len(), 20 << 20);

        // The same stream declared as 10 bytes: decoding stops at 10 and
        // the entry is refused.
        let sizes_at = |b: &[u8]| {
            let end = b.len() - 22;
            let cd = u32::from_le_bytes(b[end + 16..end + 20].try_into().unwrap()) as usize;
            (cd + 24, 22usize) // central size field; local size field
        };
        let (central_size, local_size) = sizes_at(&honest);
        let mut liar = honest.clone();
        liar[central_size..central_size + 4].copy_from_slice(&10u32.to_le_bytes());
        liar[local_size..local_size + 4].copy_from_slice(&10u32.to_le_bytes());
        let mut a = ZipArchive::new(&liar).unwrap();
        let e = a.read("bomb.xml").unwrap_err();
        assert!(
            e.0.contains("can't be decompressed") || e.0.contains("size"),
            "{e}"
        );

        // Declared far bigger than the ratio allows: refused unread.
        let mut huge = honest.clone();
        huge[central_size..central_size + 4].copy_from_slice(&(190u32 << 20).to_le_bytes());
        let mut a = ZipArchive::new(&huge).unwrap();
        assert!(a.read("bomb.xml").unwrap_err().0.contains("suspiciously"));

        // Past the total for one archive: refused unread.
        let mut z = ZipWriter::new();
        let chunk = vec![b'x'; 600];
        for i in 0..4 {
            z.add(&format!("p{i}.xml"), &chunk).unwrap();
        }
        let four = z.finish().unwrap();
        let mut a = ZipArchive::with_limit(&four, 2_000).unwrap();
        for i in 0..3 {
            assert!(a.read(&format!("p{i}.xml")).unwrap().is_some());
        }
        assert!(a.read("p3.xml").unwrap_err().0.contains("too large"));
    }

    #[test]
    fn duplicate_names_and_encryption_are_refused() {
        let mut z = ZipWriter::new();
        z.add("xl/a.xml", b"1").unwrap();
        z.add("XL/A.xml", b"2").unwrap();
        let bytes = z.finish().unwrap();
        assert!(ZipArchive::new(&bytes).unwrap_err().0.contains("two parts"));

        let mut z = ZipWriter::new();
        z.add("a.xml", b"secret").unwrap();
        let mut bytes = z.finish().unwrap();
        // Set the encrypted flag in the central directory.
        let end = bytes.len() - 22;
        let cd = u32::from_le_bytes(bytes[end + 16..end + 20].try_into().unwrap()) as usize;
        bytes[cd + 8] |= 1;
        let mut a = ZipArchive::new(&bytes).unwrap();
        assert!(a.read("a.xml").unwrap_err().0.contains("password"));
    }
}

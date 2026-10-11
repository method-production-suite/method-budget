//! A DEFLATE decoder (RFC 1951), in plain Rust, for reading .xlsx packages
//! (a ZIP of compressed XML parts) without a compression crate.
//!
//! All three block types are handled: stored, fixed Huffman and dynamic
//! Huffman. The decoder follows the structure of Mark Adler's public-domain
//! reference decoder ("puff"): canonical Huffman codes kept as a count of
//! codes per length plus the symbols in code order, decoded a bit at a time.
//! It is not the fastest way, but it is short enough to check line by line,
//! and an .xlsx part is a few megabytes at most.
//!
//! Input is untrusted, so:
//!
//! - every read is bounds-checked: running out of input is
//!   [`InflateError::Truncated`], never a zero-filled read that could loop;
//! - every code table is checked the way the RFC requires (no
//!   over-subscribed codes, an incomplete code only where a single code of
//!   one bit is allowed, an end-of-block code present), and the symbols the
//!   RFC reserves (lengths 286 and 287, distances 30 and 31) are errors;
//! - a distance reaching back before the start of the output is an error;
//! - the output is capped by the caller (`limit`), so a small stream that
//!   expands to gigabytes stops at the cap with [`InflateError::TooLarge`]
//!   instead of exhausting memory;
//! - every block consumes input, so the decoder always finishes: there is
//!   no loop that can run without reading.

use std::fmt;

/// Longest Huffman code, in bits.
const MAX_BITS: usize = 15;
/// Most literal/length codes a dynamic block may declare.
const MAX_LCODES: usize = 286;
/// Most distance codes a dynamic block may declare.
const MAX_DCODES: usize = 30;
/// Literal/length codes in the fixed code (286 and 287 take part in the code
/// but never appear in valid data).
const FIX_LCODES: usize = 288;

/// Base lengths for length symbols 257..=285, and their extra bits.
const LBASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEXT: [u8; 29] = [
    0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0,
];
/// Base distances for distance symbols 0..=29, and their extra bits.
const DBASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DEXT: [u8; 30] = [
    0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13,
    13,
];
/// The order code-length code lengths are sent in (RFC 1951, 3.2.7).
const CL_ORDER: [usize; 19] = [
    16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15,
];

/// Why a stream could not be decoded.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InflateError {
    /// The input ended before the last block did.
    Truncated,
    /// Block type 3, which the format reserves.
    BadBlockType,
    /// A stored block's length and its one's complement disagree.
    StoredLength,
    /// A dynamic block's code lengths don't make a valid code.
    BadCodeLengths(&'static str),
    /// A bit pattern that is no code, or a reserved symbol.
    BadSymbol,
    /// A distance reaching back before the start of the output.
    DistanceTooFar,
    /// The output would pass `limit` bytes.
    TooLarge { limit: usize },
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InflateError::Truncated => f.write_str("the compressed data ends too soon"),
            InflateError::BadBlockType => {
                f.write_str("the compressed data has a reserved block type")
            }
            InflateError::StoredLength => f.write_str("a stored block's length is damaged"),
            InflateError::BadCodeLengths(why) => {
                write!(f, "the compressed data's codes are damaged ({why})")
            }
            InflateError::BadSymbol => f.write_str("the compressed data has an invalid code"),
            InflateError::DistanceTooFar => {
                f.write_str("the compressed data refers back past its start")
            }
            InflateError::TooLarge { limit } => {
                write!(f, "it expands to more than {limit} bytes")
            }
        }
    }
}

impl std::error::Error for InflateError {}

type Result<T> = std::result::Result<T, InflateError>;

/// Decodes a raw DEFLATE stream (no zlib or gzip wrapper), producing at most
/// `limit` bytes. Bytes after the final block are ignored.
pub fn inflate(input: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut s = State {
        bits: Bits::new(input),
        out: Vec::with_capacity(limit.min(input.len().saturating_mul(4)).min(1 << 20)),
        limit,
    };
    let mut fixed: Option<(Huffman, Huffman)> = None;
    loop {
        let last = s.bits.bits(1)?;
        match s.bits.bits(2)? {
            0 => s.stored()?,
            1 => {
                let (lencode, distcode) = fixed.get_or_insert_with(fixed_codes);
                s.codes(lencode, distcode)?;
            }
            2 => {
                let (lencode, distcode) = s.dynamic_codes()?;
                s.codes(&lencode, &distcode)?;
            }
            _ => return Err(InflateError::BadBlockType),
        }
        if last == 1 {
            return Ok(s.out);
        }
    }
}

/// Reads bits least significant first, as DEFLATE packs them.
struct Bits<'a> {
    data: &'a [u8],
    /// The next byte to load.
    pos: usize,
    buf: u64,
    /// Bits in `buf`.
    count: u32,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Bits<'a> {
        Bits {
            data,
            pos: 0,
            buf: 0,
            count: 0,
        }
    }

    /// Loads whole bytes while there is room and input left.
    fn fill(&mut self) {
        while self.count <= 56 {
            let Some(&b) = self.data.get(self.pos) else {
                break;
            };
            self.buf |= u64::from(b) << self.count;
            self.pos += 1;
            self.count += 8;
        }
    }

    /// The next `n` bits (0 to 16) as a number.
    fn bits(&mut self, n: u32) -> Result<u32> {
        debug_assert!(n <= 16);
        if self.count < n {
            self.fill();
            if self.count < n {
                return Err(InflateError::Truncated);
            }
        }
        let v = (self.buf & ((1u64 << n) - 1)) as u32;
        self.buf >>= n;
        self.count -= n;
        Ok(v)
    }

    /// Drops the bits left in the current byte and gives back the whole
    /// bytes still buffered, so reading continues at a byte boundary.
    fn align(&mut self) {
        let partial = self.count % 8;
        self.buf >>= partial;
        self.count -= partial;
        // Whole bytes in the buffer are the next bytes of the input, in order.
        self.pos -= (self.count / 8) as usize;
        self.buf = 0;
        self.count = 0;
    }
}

/// A canonical Huffman code: how many codes there are of each length, and
/// the symbols in code order (shorter codes first, then by symbol).
struct Huffman {
    count: [u16; MAX_BITS + 1],
    symbol: Vec<u16>,
}

/// The code for `lengths` (one per symbol, 0 for unused symbols), and how
/// many codes are left over: 0 for a complete code, more than 0 for an
/// incomplete one, below 0 for an over-subscribed one (not a code at all).
fn construct(lengths: &[u8]) -> (Huffman, i32) {
    let mut h = Huffman {
        count: [0; MAX_BITS + 1],
        symbol: vec![0; lengths.len()],
    };
    for &len in lengths {
        h.count[usize::from(len)] += 1;
    }
    if usize::from(h.count[0]) == lengths.len() {
        // No codes: complete, but every decode fails.
        return (h, 0);
    }
    let mut left: i32 = 1;
    for len in 1..=MAX_BITS {
        left <<= 1;
        left -= i32::from(h.count[len]);
        if left < 0 {
            return (h, left);
        }
    }
    let mut offs = [0u16; MAX_BITS + 1];
    for len in 1..MAX_BITS {
        offs[len + 1] = offs[len] + h.count[len];
    }
    for (sym, &len) in lengths.iter().enumerate() {
        if len != 0 {
            let at = &mut offs[usize::from(len)];
            h.symbol[usize::from(*at)] = sym as u16;
            *at += 1;
        }
    }
    (h, left)
}

/// The fixed literal/length and distance codes (RFC 1951, 3.2.6).
fn fixed_codes() -> (Huffman, Huffman) {
    let mut lengths = [0u8; FIX_LCODES];
    for (sym, len) in lengths.iter_mut().enumerate() {
        *len = match sym {
            0..=143 => 8,
            144..=255 => 9,
            256..=279 => 7,
            _ => 8,
        };
    }
    let (lencode, _) = construct(&lengths);
    // 30 distance codes of 5 bits: an incomplete code, so the two reserved
    // symbols (30 and 31) fail to decode.
    let (distcode, _) = construct(&[5u8; MAX_DCODES]);
    (lencode, distcode)
}

struct State<'a> {
    bits: Bits<'a>,
    out: Vec<u8>,
    limit: usize,
}

impl State<'_> {
    /// Room for `n` more output bytes, or [`InflateError::TooLarge`].
    fn room(&self, n: usize) -> Result<()> {
        if self.out.len().saturating_add(n) > self.limit {
            Err(InflateError::TooLarge { limit: self.limit })
        } else {
            Ok(())
        }
    }

    /// A stored block: LEN, NLEN (its complement) and LEN raw bytes.
    fn stored(&mut self) -> Result<()> {
        self.bits.align();
        let b = &self.bits;
        let header = b
            .data
            .get(b.pos..b.pos + 4)
            .ok_or(InflateError::Truncated)?;
        let len = u16::from_le_bytes([header[0], header[1]]);
        let nlen = u16::from_le_bytes([header[2], header[3]]);
        if len != !nlen {
            return Err(InflateError::StoredLength);
        }
        let start = b.pos + 4;
        let bytes = b
            .data
            .get(start..start + usize::from(len))
            .ok_or(InflateError::Truncated)?;
        self.room(bytes.len())?;
        self.out.extend_from_slice(bytes);
        self.bits.pos = start + usize::from(len);
        Ok(())
    }

    /// The next symbol in code `h`, reading its bits one at a time (codes
    /// are packed most significant bit first).
    fn decode(&mut self, h: &Huffman) -> Result<u16> {
        self.bits.fill();
        let avail = self.bits.count as usize;
        let mut buf = self.bits.buf;
        let mut code: i32 = 0; // the bits read so far
        let mut first: i32 = 0; // the first code of this length
        let mut index: i32 = 0; // where this length's symbols start
        for len in 1..=MAX_BITS {
            if len > avail {
                return Err(InflateError::Truncated);
            }
            code |= (buf & 1) as i32;
            buf >>= 1;
            let count = i32::from(h.count[len]);
            if code - count < first {
                self.bits.buf >>= len;
                self.bits.count -= len as u32;
                return h
                    .symbol
                    .get((index + code - first) as usize)
                    .copied()
                    .ok_or(InflateError::BadSymbol);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError::BadSymbol)
    }

    /// The literals and length/distance pairs of a Huffman block, up to and
    /// including its end-of-block code.
    fn codes(&mut self, lencode: &Huffman, distcode: &Huffman) -> Result<()> {
        loop {
            let sym = self.decode(lencode)?;
            if sym < 256 {
                self.room(1)?;
                self.out.push(sym as u8);
                continue;
            }
            if sym == 256 {
                return Ok(());
            }
            let i = usize::from(sym - 257);
            if i >= LBASE.len() {
                return Err(InflateError::BadSymbol);
            }
            let len = usize::from(LBASE[i]) + self.bits.bits(u32::from(LEXT[i]))? as usize;
            let d = usize::from(self.decode(distcode)?);
            if d >= DBASE.len() {
                return Err(InflateError::BadSymbol);
            }
            let dist = usize::from(DBASE[d]) + self.bits.bits(u32::from(DEXT[d]))? as usize;
            if dist > self.out.len() {
                return Err(InflateError::DistanceTooFar);
            }
            self.room(len)?;
            let start = self.out.len() - dist;
            if dist >= len {
                self.out.extend_from_within(start..start + len);
            } else {
                // The copy overlaps what it writes: a run, byte by byte.
                for k in 0..len {
                    let b = self.out[start + k];
                    self.out.push(b);
                }
            }
        }
    }

    /// Reads a dynamic block's code descriptions and builds its codes.
    fn dynamic_codes(&mut self) -> Result<(Huffman, Huffman)> {
        let nlen = self.bits.bits(5)? as usize + 257;
        let ndist = self.bits.bits(5)? as usize + 1;
        let ncode = self.bits.bits(4)? as usize + 4;
        if nlen > MAX_LCODES || ndist > MAX_DCODES {
            return Err(InflateError::BadCodeLengths("too many codes"));
        }
        let mut lengths = [0u8; MAX_LCODES + MAX_DCODES];
        for &at in &CL_ORDER[..ncode] {
            lengths[at] = self.bits.bits(3)? as u8;
        }
        let (lencode, left) = construct(&lengths[..19]);
        if left != 0 {
            return Err(InflateError::BadCodeLengths("code length code"));
        }
        let mut index = 0;
        while index < nlen + ndist {
            let sym = self.decode(&lencode)?;
            if sym < 16 {
                lengths[index] = sym as u8;
                index += 1;
                continue;
            }
            let (len, repeat) = match sym {
                16 => {
                    if index == 0 {
                        return Err(InflateError::BadCodeLengths("repeat with no length"));
                    }
                    (lengths[index - 1], 3 + self.bits.bits(2)? as usize)
                }
                17 => (0, 3 + self.bits.bits(3)? as usize),
                _ => (0, 11 + self.bits.bits(7)? as usize),
            };
            if index + repeat > nlen + ndist {
                return Err(InflateError::BadCodeLengths("too many lengths"));
            }
            lengths[index..index + repeat].fill(len);
            index += repeat;
        }
        if lengths[256] == 0 {
            return Err(InflateError::BadCodeLengths("no end-of-block code"));
        }
        // An incomplete code is allowed only when it is a single code of one
        // bit (RFC 1951, 3.2.7).
        let single = |h: &Huffman, n: usize| n == usize::from(h.count[0]) + usize::from(h.count[1]);
        let (lencode, left) = construct(&lengths[..nlen]);
        if left < 0 || (left > 0 && !single(&lencode, nlen)) {
            return Err(InflateError::BadCodeLengths("literal/length code"));
        }
        let (distcode, left) = construct(&lengths[nlen..nlen + ndist]);
        if left < 0 || (left > 0 && !single(&distcode, ndist)) {
            return Err(InflateError::BadCodeLengths("distance code"));
        }
        Ok((lencode, distcode))
    }
}

/// A small DEFLATE encoder for tests: it makes valid streams of every block
/// type, with LZ77 matches and randomly shaped (but valid) dynamic codes, so
/// the decoder can be checked on inputs no fixture covers. Not for real use:
/// it compresses poorly and slowly.
#[cfg(test)]
pub(crate) mod encode {
    use super::{CL_ORDER, DBASE, DEXT, LBASE, LEXT};

    /// xorshift64*: deterministic and dependency-free.
    pub struct Rng(pub u64);

    impl Rng {
        pub fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
        }

        pub fn below(&mut self, n: usize) -> usize {
            if n == 0 {
                0
            } else {
                (self.next() % n as u64) as usize
            }
        }
    }

    pub struct BitWriter {
        out: Vec<u8>,
        buf: u64,
        n: u32,
    }

    impl BitWriter {
        pub fn new() -> BitWriter {
            BitWriter {
                out: Vec::new(),
                buf: 0,
                n: 0,
            }
        }

        /// `bits` bits of `v`, least significant first.
        pub fn put(&mut self, v: u32, bits: u32) {
            self.buf |= u64::from(v) << self.n;
            self.n += bits;
            while self.n >= 8 {
                self.out.push(self.buf as u8);
                self.buf >>= 8;
                self.n -= 8;
            }
        }

        /// A Huffman code, most significant bit first.
        pub fn code(&mut self, code: u32, len: u8) {
            let mut rev = 0;
            for i in 0..len {
                rev |= ((code >> i) & 1) << (len - 1 - i);
            }
            self.put(rev, u32::from(len));
        }

        pub fn align(&mut self) {
            if self.n > 0 {
                self.out.push(self.buf as u8);
                self.buf = 0;
                self.n = 0;
            }
        }

        pub fn finish(mut self) -> Vec<u8> {
            self.align();
            self.out
        }
    }

    #[derive(Clone, Copy, Debug)]
    pub enum Tok {
        Lit(u8),
        Match { len: usize, dist: usize },
    }

    impl Tok {
        fn size(self) -> usize {
            match self {
                Tok::Lit(_) => 1,
                Tok::Match { len, .. } => len,
            }
        }
    }

    /// Literals and matches for `data`: at each place, the longest match
    /// among a few earlier places that start with the same byte.
    pub fn tokenize(data: &[u8], rng: &mut Rng) -> Vec<Tok> {
        let mut toks = Vec::new();
        let mut i = 0;
        while i < data.len() {
            let mut best = (0, 0);
            if rng.below(8) != 0 {
                for _ in 0..24 {
                    let back = 1 + rng.below(i.clamp(1, 32_768));
                    if back > i {
                        break;
                    }
                    let start = i - back;
                    let mut len = 0;
                    while len < 258 && i + len < data.len() && data[start + len] == data[i + len] {
                        len += 1;
                    }
                    if len > best.0 {
                        best = (len, back);
                    }
                }
                // Short distances find runs.
                if i > 0 {
                    let mut len = 0;
                    while len < 258 && i + len < data.len() && data[i - 1 + len] == data[i + len] {
                        len += 1;
                    }
                    if len > best.0 {
                        best = (len, 1);
                    }
                }
            }
            if best.0 >= 3 {
                let len = 3 + rng.below(best.0 - 2);
                toks.push(Tok::Match { len, dist: best.1 });
                i += len;
            } else {
                toks.push(Tok::Lit(data[i]));
                i += 1;
            }
        }
        toks
    }

    fn length_symbol(len: usize) -> (usize, u32, u32) {
        let i = (0..LBASE.len())
            .rev()
            .find(|&i| usize::from(LBASE[i]) <= len)
            .unwrap();
        (
            257 + i,
            (len - usize::from(LBASE[i])) as u32,
            u32::from(LEXT[i]),
        )
    }

    fn distance_symbol(dist: usize) -> (usize, u32, u32) {
        let i = (0..DBASE.len())
            .rev()
            .find(|&i| usize::from(DBASE[i]) <= dist)
            .unwrap();
        (i, (dist - usize::from(DBASE[i])) as u32, u32::from(DEXT[i]))
    }

    /// Canonical codes for `lengths` (RFC 1951, 3.2.2).
    pub fn canonical(lengths: &[u8]) -> Vec<u32> {
        let mut bl_count = [0u32; 16];
        for &l in lengths {
            bl_count[usize::from(l)] += 1;
        }
        bl_count[0] = 0;
        let mut next = [0u32; 16];
        let mut code = 0;
        for bits in 1..16 {
            code = (code + bl_count[bits - 1]) << 1;
            next[bits] = code;
        }
        lengths
            .iter()
            .map(|&l| {
                if l == 0 {
                    0
                } else {
                    let c = next[usize::from(l)];
                    next[usize::from(l)] += 1;
                    c
                }
            })
            .collect()
    }

    /// Random lengths making a complete code over `used` (symbol numbers out
    /// of `n`), no longer than `max`. A lone symbol gets a partner, since a
    /// complete code needs two.
    pub fn random_lengths(n: usize, used: &[usize], max: u8, rng: &mut Rng) -> Vec<u8> {
        let mut used = used.to_vec();
        if used.len() == 1 {
            let other = (0..n).find(|s| *s != used[0]).unwrap();
            used.push(other);
        }
        let mut depths = vec![0u8];
        while depths.len() < used.len() {
            let open: Vec<usize> = (0..depths.len()).filter(|&i| depths[i] < max).collect();
            let at = open[rng.below(open.len())];
            depths[at] += 1;
            depths.push(depths[at]);
        }
        // Shuffle which symbol gets which depth.
        for i in (1..depths.len()).rev() {
            depths.swap(i, rng.below(i + 1));
        }
        let mut lengths = vec![0u8; n];
        for (s, d) in used.iter().zip(depths) {
            lengths[*s] = d;
        }
        lengths
    }

    fn fixed_lengths() -> (Vec<u8>, Vec<u8>) {
        let ll = (0..288)
            .map(|s| match s {
                0..=143 => 8,
                144..=255 => 9,
                256..=279 => 7,
                _ => 8,
            })
            .collect();
        (ll, vec![5; 30])
    }

    fn write_tokens(w: &mut BitWriter, toks: &[Tok], ll: &[u8], dl: &[u8]) {
        let lc = canonical(ll);
        let dc = canonical(dl);
        for t in toks {
            match *t {
                Tok::Lit(b) => w.code(lc[usize::from(b)], ll[usize::from(b)]),
                Tok::Match { len, dist } => {
                    let (s, extra, bits) = length_symbol(len);
                    w.code(lc[s], ll[s]);
                    w.put(extra, bits);
                    let (s, extra, bits) = distance_symbol(dist);
                    w.code(dc[s], dl[s]);
                    w.put(extra, bits);
                }
            }
        }
        w.code(lc[256], ll[256]);
    }

    fn write_dynamic(w: &mut BitWriter, toks: &[Tok], last: bool, rng: &mut Rng) {
        let mut lit_used = vec![false; 286];
        let mut dist_used = [false; 30];
        lit_used[256] = true;
        for t in toks {
            match *t {
                Tok::Lit(b) => lit_used[usize::from(b)] = true,
                Tok::Match { len, dist } => {
                    lit_used[length_symbol(len).0] = true;
                    dist_used[distance_symbol(dist).0] = true;
                }
            }
        }
        // Sometimes give unused symbols codes too.
        for u in lit_used.iter_mut() {
            if rng.below(16) == 0 {
                *u = true;
            }
        }
        let lits: Vec<usize> = (0..286).filter(|&s| lit_used[s]).collect();
        let ll = random_lengths(286, &lits, 15, rng);
        let dists: Vec<usize> = (0..30).filter(|&s| dist_used[s]).collect();
        let dl = if dists.is_empty() {
            // No distance codes at all: one length of zero.
            vec![0u8; 1]
        } else {
            random_lengths(30, &dists, 15, rng)
        };
        let hlit = (257..=286)
            .rev()
            .find(|&n| ll[n - 1] != 0)
            .unwrap_or(257)
            .max(257);
        let hdist = (1..=dl.len()).rev().find(|&n| dl[n - 1] != 0).unwrap_or(1);
        let seq: Vec<u8> = ll[..hlit].iter().chain(&dl[..hdist]).copied().collect();

        // Run-length code the lengths, sometimes.
        let mut cl: Vec<(u8, u32, u32)> = Vec::new(); // (symbol, extra, extra bits)
        let mut i = 0;
        while i < seq.len() {
            let v = seq[i];
            let mut run = 1;
            while i + run < seq.len() && seq[i + run] == v {
                run += 1;
            }
            if v == 0 && run >= 11 && rng.below(2) == 0 {
                let r = run.min(138);
                cl.push((18, (r - 11) as u32, 7));
                i += r;
            } else if v == 0 && run >= 3 && rng.below(2) == 0 {
                let r = run.min(10);
                cl.push((17, (r - 3) as u32, 3));
                i += r;
            } else if i > 0 && seq[i - 1] == v && run >= 3 && rng.below(2) == 0 {
                let r = run.min(6);
                cl.push((16, (r - 3) as u32, 2));
                i += r;
            } else {
                cl.push((v, 0, 0));
                i += 1;
            }
        }
        let mut cl_used = [false; 19];
        for (s, _, _) in &cl {
            cl_used[usize::from(*s)] = true;
        }
        let used: Vec<usize> = (0..19).filter(|&s| cl_used[s]).collect();
        let cll = random_lengths(19, &used, 7, rng);
        let hclen = (4..=19)
            .rev()
            .find(|&n| cll[CL_ORDER[n - 1]] != 0)
            .unwrap_or(4);
        let clc = canonical(&cll);

        w.put(u32::from(last), 1);
        w.put(2, 2);
        w.put((hlit - 257) as u32, 5);
        w.put((hdist - 1) as u32, 5);
        w.put((hclen - 4) as u32, 4);
        for &s in &CL_ORDER[..hclen] {
            w.put(u32::from(cll[s]), 3);
        }
        for (s, extra, bits) in cl {
            w.code(clc[usize::from(s)], cll[usize::from(s)]);
            w.put(extra, bits);
        }
        let mut full_dl = dl.clone();
        full_dl.resize(30, 0);
        write_tokens(w, toks, &ll, &full_dl);
    }

    /// Which kind of block [`deflate`] writes.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Blocks {
        Stored,
        Fixed,
        Dynamic,
        /// Each block's type picked at random.
        Mixed,
    }

    /// `data` as a raw DEFLATE stream, cut into blocks of random sizes.
    pub fn deflate(data: &[u8], blocks: Blocks, rng: &mut Rng) -> Vec<u8> {
        let toks = tokenize(data, rng);
        let mut w = BitWriter::new();
        // Cut the tokens into blocks; each block also knows its raw bytes.
        let mut start_tok = 0;
        let mut start_byte = 0;
        loop {
            let n = (1 + rng.below(3000)).min(toks.len() - start_tok);
            let end_tok = start_tok + n;
            let bytes: usize = toks[start_tok..end_tok].iter().map(|t| t.size()).sum();
            let end_byte = start_byte + bytes;
            let last = end_tok == toks.len();
            let kind = match blocks {
                Blocks::Mixed => [Blocks::Stored, Blocks::Fixed, Blocks::Dynamic][rng.below(3)],
                k => k,
            };
            match kind {
                Blocks::Stored => {
                    let raw = &data[start_byte..end_byte];
                    let mut chunks: Vec<&[u8]> = raw.chunks(65_535).collect();
                    if chunks.is_empty() {
                        chunks.push(&[]);
                    }
                    let count = chunks.len();
                    for (k, c) in chunks.into_iter().enumerate() {
                        w.put(u32::from(last && k + 1 == count), 1);
                        w.put(0, 2);
                        w.align();
                        let len = c.len() as u16;
                        w.put(u32::from(len), 16);
                        w.put(u32::from(!len), 16);
                        for &b in c {
                            w.put(u32::from(b), 8);
                        }
                    }
                }
                Blocks::Fixed => {
                    w.put(u32::from(last), 1);
                    w.put(1, 2);
                    let (ll, dl) = fixed_lengths();
                    write_tokens(&mut w, &toks[start_tok..end_tok], &ll, &dl);
                }
                _ => write_dynamic(&mut w, &toks[start_tok..end_tok], last, rng),
            }
            start_tok = end_tok;
            start_byte = end_byte;
            if last {
                break;
            }
        }
        w.finish()
    }

    /// A fixed-Huffman stream of `n` zero bytes made almost entirely of
    /// 258-byte matches: a tiny input that expands enormously.
    pub fn bomb(n: usize) -> Vec<u8> {
        let mut toks = vec![Tok::Lit(0)];
        let mut left = n - 1;
        while left >= 258 + 3 {
            toks.push(Tok::Match { len: 258, dist: 1 });
            left -= 258;
        }
        while left > 0 {
            toks.push(Tok::Lit(0));
            left -= 1;
        }
        let mut w = BitWriter::new();
        w.put(1, 1);
        w.put(1, 2);
        let (ll, dl) = fixed_lengths();
        write_tokens(&mut w, &toks, &ll, &dl);
        w.finish()
    }
}

#[cfg(test)]
mod tests {
    use super::encode::{self, Blocks, Rng};
    use super::*;

    const LIMIT: usize = 64 << 20;

    #[test]
    fn empty_streams() {
        // A final fixed block holding only the end-of-block code (zlib's
        // output for no input): 0x03 0x00.
        assert_eq!(inflate(&[0x03, 0x00], LIMIT).unwrap(), b"");
        // A final stored block of length 0.
        assert_eq!(
            inflate(&[0x01, 0x00, 0x00, 0xFF, 0xFF], LIMIT).unwrap(),
            b""
        );
    }

    #[test]
    fn known_streams() {
        // zlib.compressobj(9, zlib.DEFLATED, -15) of b"hello hello hello hello"
        let hello = [0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01];
        assert_eq!(inflate(&hello, LIMIT).unwrap(), b"hello hello hello hello");
        // A stored block, then trailing bytes, which are ignored.
        let stored = [0x01, 0x03, 0x00, 0xFC, 0xFF, b'a', b'b', b'c', 0xAA, 0xBB];
        assert_eq!(inflate(&stored, LIMIT).unwrap(), b"abc");
    }

    #[test]
    fn every_block_type_round_trips() {
        let mut rng = Rng(0x1234_5678_9ABC_DEF1);
        let samples: Vec<Vec<u8>> = vec![
            Vec::new(),
            b"a".to_vec(),
            b"abcabcabcabcabcabcabcabcabcabc".to_vec(),
            (0..5000u32).map(|i| (i % 7) as u8).collect(),
            (0..70_000u32).map(|i| (i * 31 % 251) as u8).collect(),
            "<row r=\"1\"><c r=\"A1\" t=\"s\"><v>0</v></c></row>"
                .repeat(400)
                .into_bytes(),
        ];
        for data in &samples {
            for blocks in [
                Blocks::Stored,
                Blocks::Fixed,
                Blocks::Dynamic,
                Blocks::Mixed,
            ] {
                let z = encode::deflate(data, blocks, &mut rng);
                assert_eq!(&inflate(&z, LIMIT).unwrap(), data, "{blocks:?}");
            }
        }
    }

    #[test]
    fn random_data_round_trips() {
        let mut rng = Rng(42);
        for round in 0..300 {
            let n = rng.below(if round % 10 == 0 { 40_000 } else { 2_000 });
            // Small alphabets make long matches; full bytes make few.
            let alphabet = 1 + rng.below(256);
            let data: Vec<u8> = (0..n).map(|_| rng.below(alphabet) as u8).collect();
            let z = encode::deflate(&data, Blocks::Mixed, &mut rng);
            assert_eq!(inflate(&z, LIMIT).unwrap(), data, "round {round}");
        }
    }

    #[test]
    fn every_truncation_is_an_error() {
        let mut rng = Rng(7);
        let data: Vec<u8> = (0..3000u32).map(|i| (i * i % 97) as u8).collect();
        for blocks in [
            Blocks::Stored,
            Blocks::Fixed,
            Blocks::Dynamic,
            Blocks::Mixed,
        ] {
            let z = encode::deflate(&data, blocks, &mut rng);
            for cut in 0..z.len() {
                assert!(
                    inflate(&z[..cut], LIMIT).is_err(),
                    "{blocks:?} cut at {cut}"
                );
            }
        }
    }

    #[test]
    fn damage_never_panics_or_runs_away() {
        let mut rng = Rng(99);
        let data: Vec<u8> = (0..4000u32).map(|i| (i * 7 % 61) as u8).collect();
        let good = encode::deflate(&data, Blocks::Mixed, &mut rng);
        for _ in 0..3000 {
            let mut bad = good.clone();
            for _ in 0..1 + rng.below(4) {
                let at = rng.below(bad.len());
                bad[at] ^= 1 << rng.below(8);
            }
            if let Ok(out) = inflate(&bad, 100_000) {
                assert!(out.len() <= 100_000);
            }
        }
        // Pure noise.
        for _ in 0..3000 {
            let n = rng.below(600);
            let noise: Vec<u8> = (0..n).map(|_| rng.next() as u8).collect();
            if let Ok(out) = inflate(&noise, 100_000) {
                assert!(out.len() <= 100_000);
            }
        }
    }

    #[test]
    fn bombs_stop_at_the_limit() {
        let z = encode::bomb(50 << 20);
        assert!(z.len() * 150 < 50 << 20, "{} bytes", z.len());
        assert_eq!(
            inflate(&z, 1 << 20),
            Err(InflateError::TooLarge { limit: 1 << 20 })
        );
        // Under the limit it decodes in full.
        let small = encode::bomb(100_000);
        let out = inflate(&small, 100_000).unwrap();
        assert_eq!(out.len(), 100_000);
        assert!(out.iter().all(|&b| b == 0));
        assert_eq!(
            inflate(&small, 99_999),
            Err(InflateError::TooLarge { limit: 99_999 })
        );
        // A stored block over the limit is refused before it is copied.
        let stored = [0x01, 0x05, 0x00, 0xFA, 0xFF, 1, 2, 3, 4, 5];
        assert_eq!(
            inflate(&stored, 4),
            Err(InflateError::TooLarge { limit: 4 })
        );
    }

    #[test]
    fn malformed_streams_are_errors() {
        use encode::BitWriter;
        // Block type 3.
        assert_eq!(inflate(&[0x07], LIMIT), Err(InflateError::BadBlockType));
        // Stored length and complement disagree.
        assert_eq!(
            inflate(&[0x01, 0x03, 0x00, 0xFC, 0xFE, 1, 2, 3], LIMIT),
            Err(InflateError::StoredLength)
        );
        // Stored block shorter than it says.
        assert_eq!(
            inflate(&[0x01, 0x03, 0x00, 0xFC, 0xFF, 1, 2], LIMIT),
            Err(InflateError::Truncated)
        );
        // Not final, and nothing follows.
        assert_eq!(
            inflate(&[0x00, 0x00, 0x00, 0xFF, 0xFF], LIMIT),
            Err(InflateError::Truncated)
        );

        type Body<'a> = &'a dyn Fn(&mut BitWriter, &[u32], &[u32]);
        let fixed = |f: Body| {
            let (ll, dl): (Vec<u8>, Vec<u8>) = (
                (0..288)
                    .map(|s| match s {
                        0..=143 => 8,
                        144..=255 => 9,
                        256..=279 => 7,
                        _ => 8,
                    })
                    .collect(),
                vec![5; 30],
            );
            let lc = encode::canonical(&ll);
            let dc = encode::canonical(&dl);
            let mut w = BitWriter::new();
            w.put(1, 1);
            w.put(1, 2);
            f(&mut w, &lc, &dc);
            w.finish()
        };
        // A distance before the start: a match as the first thing.
        let z = fixed(&|w, lc, dc| {
            w.code(lc[257], 7); // length 3
            w.code(dc[0], 5); // distance 1
            w.code(lc[256], 7);
        });
        assert_eq!(inflate(&z, LIMIT), Err(InflateError::DistanceTooFar));
        // Reserved length symbols 286 and 287.
        for sym in [286, 287] {
            let z = fixed(&|w, lc, _| {
                w.code(lc[0], 8);
                w.code(lc[sym], 8);
                w.put(0, 16);
            });
            assert_eq!(inflate(&z, LIMIT), Err(InflateError::BadSymbol), "{sym}");
        }
        // Reserved distance symbols 30 and 31: five bits that are no code.
        for code in [30u32, 31] {
            let z = fixed(&|w, lc, _| {
                w.code(lc[0], 8);
                w.code(lc[257], 7);
                w.code(code, 5);
                w.put(0, 16);
            });
            assert_eq!(inflate(&z, LIMIT), Err(InflateError::BadSymbol), "{code}");
        }

        // Dynamic blocks with bad code descriptions.
        let dynamic =
            |hlit: u32, hdist: u32, cl: &[(usize, u32)], body: &dyn Fn(&mut BitWriter)| {
                let mut w = BitWriter::new();
                w.put(1, 1);
                w.put(2, 2);
                w.put(hlit, 5);
                w.put(hdist, 5);
                w.put(15, 4); // all 19 code length code lengths follow
                let mut lens = [0u32; 19];
                for &(s, l) in cl {
                    lens[s] = l;
                }
                for &s in &CL_ORDER {
                    w.put(lens[s], 3);
                }
                body(&mut w);
                w.put(0, 16);
                w.finish()
            };
        // Too many literal/length codes (HLIT 30 means 287).
        let z = dynamic(30, 0, &[(0, 1), (8, 1)], &|_| {});
        assert_eq!(
            inflate(&z, LIMIT),
            Err(InflateError::BadCodeLengths("too many codes"))
        );
        // Too many distance codes (HDIST 30 means 31).
        let z = dynamic(0, 30, &[(0, 1), (8, 1)], &|_| {});
        assert_eq!(
            inflate(&z, LIMIT),
            Err(InflateError::BadCodeLengths("too many codes"))
        );
        // An over-subscribed code length code: three codes of one bit.
        let z = dynamic(0, 0, &[(0, 1), (8, 1), (16, 1)], &|_| {});
        assert_eq!(
            inflate(&z, LIMIT),
            Err(InflateError::BadCodeLengths("code length code"))
        );
        // An incomplete code length code: one code of two bits.
        let z = dynamic(0, 0, &[(8, 2)], &|_| {});
        assert_eq!(
            inflate(&z, LIMIT),
            Err(InflateError::BadCodeLengths("code length code"))
        );
        // Code length code: 8 = code 0, 16 (repeat) = code 1 (canonical
        // codes go in symbol order). Repeating before any length is an error.
        let z = dynamic(0, 0, &[(16, 1), (8, 1)], &|w| {
            w.code(1, 1);
            w.put(0, 2);
        });
        assert_eq!(
            inflate(&z, LIMIT),
            Err(InflateError::BadCodeLengths("repeat with no length"))
        );
        // Every length 8 (257 + 1 codes of eight bits): over-subscribed,
        // and with symbol 256 present. 0 = code 0 for length 0, 8 = code 1.
        let z = dynamic(0, 0, &[(0, 1), (8, 1)], &|w| {
            for _ in 0..258 {
                w.code(1, 1);
            }
        });
        assert_eq!(
            inflate(&z, LIMIT),
            Err(InflateError::BadCodeLengths("literal/length code"))
        );
        // No end-of-block code: every length 0.
        let z = dynamic(0, 0, &[(0, 1), (8, 1)], &|w| {
            for _ in 0..258 {
                w.code(0, 1);
            }
        });
        assert_eq!(
            inflate(&z, LIMIT),
            Err(InflateError::BadCodeLengths("no end-of-block code"))
        );
        // Repeats running past the declared lengths: 8 = code 0, 17 (zeros)
        // = code 1; 26 runs of ten zeros pass 258.
        let z = dynamic(0, 0, &[(17, 1), (8, 1)], &|w| {
            for _ in 0..26 {
                w.code(1, 1);
                w.put(7, 3);
            }
        });
        assert_eq!(
            inflate(&z, LIMIT),
            Err(InflateError::BadCodeLengths("too many lengths"))
        );
    }

    #[test]
    fn a_single_one_bit_code_is_allowed() {
        use encode::BitWriter;
        // A dynamic block whose literal/length code is only the end-of-block
        // symbol, one bit long, and with no distance codes: valid, empty.
        let mut w = BitWriter::new();
        w.put(1, 1);
        w.put(2, 2);
        w.put(0, 5); // 257 literal/length codes
        w.put(0, 5); // 1 distance code
        w.put(15, 4);
        // Code length code: 0 -> code 0 (1 bit), 1 -> code 1 (1 bit),
        // 18 unused.
        let mut lens = [0u32; 19];
        lens[0] = 1;
        lens[1] = 1;
        for &s in &CL_ORDER {
            w.put(lens[s], 3);
        }
        // Lengths: 256 zeros, then 1 for the end-of-block, then 0 for the
        // one distance code.
        for _ in 0..256 {
            w.code(0, 1);
        }
        w.code(1, 1);
        w.code(0, 1);
        // The data: the end-of-block code, a single 0 bit.
        w.code(0, 1);
        assert_eq!(inflate(&w.finish(), LIMIT).unwrap(), b"");
    }
}

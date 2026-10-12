//! Finding a budget again after it was moved or renamed in the Finder
//! (roadmap 3.19; decisions.md, entry 64): the pinned list (src/start.rs)
//! and Open Recent (src/safety.rs) keep a macOS bookmark beside each path.
//! When the path no longer names a file, the bookmark is resolved and the
//! path follows the file to its new place.
//!
//! The bookmark is the system's own (`CFURLCreateBookmarkData`), called
//! straight through CoreFoundation, as src/pdf_mac.rs talks to AppKit: no
//! crate is added for it. The app is not sandboxed, so a plain bookmark is
//! enough (a security-scoped one needs the sandbox's entitlement). On Windows
//! and Linux there is no bookmark and a moved file shows as missing.
//!
//! A file moved to the Trash is treated as gone, not followed there.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A bookmark for the file at `path`, or `None` (not macOS, or the system
/// couldn't make one: the file is gone).
pub fn create(path: &Path) -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        mac::create(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

/// Where the file `mark` was made for is now, when it is still a file
/// outside the Trash, with a fresh bookmark when the old one is stale.
pub fn resolve(mark: &[u8]) -> Option<(PathBuf, Option<Vec<u8>>)> {
    #[cfg(target_os = "macos")]
    {
        let (path, stale) = mac::resolve(mark)?;
        if !path.is_file() || in_trash(&path) {
            return None;
        }
        let path = std::fs::canonicalize(&path).unwrap_or(path);
        let fresh = if stale { create(&path) } else { None };
        Some((path, fresh))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = mark;
        None
    }
}

/// Whether `path` is in a Trash folder (the user's or a volume's).
pub fn in_trash(path: &Path) -> bool {
    path.components().any(|c| {
        let c = c.as_os_str();
        c == ".Trash" || c == ".Trashes"
    })
}

/// A path kept with its bookmark.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Tracked {
    pub path: PathBuf,
    pub mark: Option<Vec<u8>>,
}

impl Tracked {
    /// `path` with a bookmark made now.
    pub fn new(path: PathBuf) -> Tracked {
        let mark = create(&path);
        Tracked { path, mark }
    }

    /// When the path names no file, follows the bookmark to where the file
    /// went. A file found without a bookmark gets one. True when anything
    /// changed (the caller saves its list).
    pub fn follow(&mut self) -> bool {
        if self.path.is_file() {
            if self.mark.is_none() {
                self.mark = create(&self.path);
                return self.mark.is_some();
            }
            return false;
        }
        let Some((path, fresh)) = self.mark.as_deref().and_then(resolve) else {
            return false;
        };
        if path == self.path {
            return false;
        }
        self.path = path;
        if let Some(fresh) = fresh {
            self.mark = Some(fresh);
        }
        true
    }
}

/// A kept list: `files` (paths, in order) with their `bookmarks` (hex, by
/// path). Blank paths are left out; a path without a bookmark has none.
pub fn load_list(files: Vec<String>, bookmarks: &BTreeMap<String, String>) -> Vec<Tracked> {
    files
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(|s| Tracked {
            mark: bookmarks.get(&s).and_then(|h| from_hex(h)),
            path: PathBuf::from(s),
        })
        .collect()
}

/// `list` to keep: its paths in order and their bookmarks by path.
pub fn save_list(list: &[Tracked]) -> (Vec<String>, BTreeMap<String, String>) {
    let mut files = Vec::with_capacity(list.len());
    let mut bookmarks = BTreeMap::new();
    for t in list {
        let Some(p) = t.path.to_str() else { continue };
        files.push(p.to_string());
        if let Some(m) = &t.mark {
            bookmarks.insert(p.to_string(), to_hex(m));
        }
    }
    (files, bookmarks)
}

/// Follows every entry of `list` (see [`Tracked::follow`]); one moved onto
/// a file listed already is dropped, so each file stays once. True when
/// anything changed.
pub fn follow_all(list: &mut Vec<Tracked>) -> bool {
    let mut changed = false;
    for t in list.iter_mut() {
        changed |= t.follow();
    }
    if changed {
        let mut seen = std::collections::HashSet::new();
        list.retain(|t| seen.insert(t.path.clone()));
    }
    changed
}

/// `bytes` as lowercase hex, for the JSON lists.
pub fn to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(s, "{b:02x}");
    }
    s
}

/// The bytes `hex` spells, or `None` when it isn't hex.
pub fn from_hex(hex: &str) -> Option<Vec<u8>> {
    if !hex.len().is_multiple_of(2) || !hex.is_ascii() {
        return None;
    }
    (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).ok())
        .collect()
}

#[cfg(target_os = "macos")]
mod mac {
    //! CoreFoundation's bookmark calls. Every object made here is released
    //! here; nothing is kept.

    use std::ffi::c_void;
    use std::os::unix::ffi::{OsStrExt, OsStringExt};
    use std::path::{Path, PathBuf};

    type CFTypeRef = *const c_void;
    type CFIndex = isize;
    type Boolean = u8;

    /// kCFURLBookmarkResolutionWithoutUIMask | WithoutMountingMask: never
    /// show anything or mount a volume to find a file.
    const RESOLVE_QUIETLY: usize = (1 << 8) | (1 << 9);
    const PATH_MAX: usize = 4096;

    #[link(name = "CoreFoundation", kind = "framework")]
    extern "C" {
        fn CFURLCreateFromFileSystemRepresentation(
            alloc: CFTypeRef,
            buffer: *const u8,
            len: CFIndex,
            is_directory: Boolean,
        ) -> CFTypeRef;
        fn CFURLCreateBookmarkData(
            alloc: CFTypeRef,
            url: CFTypeRef,
            options: usize,
            keys: CFTypeRef,
            relative_to: CFTypeRef,
            error: *mut CFTypeRef,
        ) -> CFTypeRef;
        fn CFURLCreateByResolvingBookmarkData(
            alloc: CFTypeRef,
            bookmark: CFTypeRef,
            options: usize,
            relative_to: CFTypeRef,
            keys: CFTypeRef,
            is_stale: *mut Boolean,
            error: *mut CFTypeRef,
        ) -> CFTypeRef;
        fn CFURLGetFileSystemRepresentation(
            url: CFTypeRef,
            resolve_against_base: Boolean,
            buffer: *mut u8,
            max_len: CFIndex,
        ) -> Boolean;
        fn CFDataCreate(alloc: CFTypeRef, bytes: *const u8, len: CFIndex) -> CFTypeRef;
        fn CFDataGetLength(data: CFTypeRef) -> CFIndex;
        fn CFDataGetBytePtr(data: CFTypeRef) -> *const u8;
        fn CFRelease(cf: CFTypeRef);
    }

    /// Releases a CoreFoundation object when dropped.
    struct Owned(CFTypeRef);

    impl Owned {
        fn new(p: CFTypeRef) -> Option<Owned> {
            // Built only for a non-null object: an `Owned` always releases.
            (!p.is_null()).then(|| Owned(p))
        }
    }

    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: only non-null objects this module created are wrapped.
            unsafe { CFRelease(self.0) }
        }
    }

    pub fn create(path: &Path) -> Option<Vec<u8>> {
        let bytes = path.as_os_str().as_bytes();
        let len = CFIndex::try_from(bytes.len()).ok()?;
        // SAFETY: `bytes` outlives the call; CoreFoundation copies it.
        let url = Owned::new(unsafe {
            CFURLCreateFromFileSystemRepresentation(std::ptr::null(), bytes.as_ptr(), len, 0)
        })?;
        // SAFETY: `url` is a valid CFURL; the null keys and base are allowed.
        let data = unsafe {
            CFURLCreateBookmarkData(
                std::ptr::null(),
                url.0,
                0,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
            )
        };
        let data = Owned::new(data)?;
        // SAFETY: `data` is a valid CFData; its bytes are copied out before
        // it is released.
        unsafe {
            let n = usize::try_from(CFDataGetLength(data.0)).ok()?;
            let p = CFDataGetBytePtr(data.0);
            if p.is_null() {
                return None;
            }
            Some(std::slice::from_raw_parts(p, n).to_vec())
        }
    }

    /// The path `mark` resolves to and whether the bookmark is stale.
    pub fn resolve(mark: &[u8]) -> Option<(PathBuf, bool)> {
        let len = CFIndex::try_from(mark.len()).ok()?;
        // SAFETY: CoreFoundation copies `mark`.
        let data = Owned::new(unsafe { CFDataCreate(std::ptr::null(), mark.as_ptr(), len) })?;
        let mut stale: Boolean = 0;
        // SAFETY: `data` is a valid CFData; `stale` lives on this frame; no
        // error is asked for.
        let url = unsafe {
            CFURLCreateByResolvingBookmarkData(
                std::ptr::null(),
                data.0,
                RESOLVE_QUIETLY,
                std::ptr::null(),
                std::ptr::null(),
                &mut stale,
                std::ptr::null_mut(),
            )
        };
        let url = Owned::new(url)?;
        let mut buf = vec![0u8; PATH_MAX];
        // SAFETY: `buf` holds PATH_MAX bytes and the call writes at most that.
        let ok = unsafe {
            CFURLGetFileSystemRepresentation(url.0, 1, buf.as_mut_ptr(), PATH_MAX as CFIndex)
        };
        if ok == 0 {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0)?;
        buf.truncate(end);
        let path = PathBuf::from(std::ffi::OsString::from_vec(buf));
        Some((path, stale != 0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trips() {
        let bytes = [0u8, 1, 0x7f, 0x80, 0xff];
        assert_eq!(to_hex(&bytes), "00017f80ff");
        assert_eq!(from_hex("00017f80ff").unwrap(), bytes);
        assert_eq!(from_hex("").unwrap(), Vec::<u8>::new());
        assert!(from_hex("abc").is_none());
        assert!(from_hex("zz").is_none());
        assert!(from_hex("éé").is_none());
    }

    #[test]
    fn the_trash_is_not_a_place_to_follow_a_file_to() {
        assert!(in_trash(Path::new("/Users/sam/.Trash/Big Show.mthd")));
        assert!(in_trash(Path::new("/Volumes/Work/.Trashes/501/a.mthd")));
        assert!(!in_trash(Path::new("/Users/sam/Films/Trash talk.mthd")));
    }

    #[test]
    fn a_moved_file_is_followed_where_the_system_can() {
        let dir = crate::safety::tests::temp_dir("bookmark");
        let dir = std::fs::canonicalize(&dir).unwrap();
        let a = dir.join("a.mthd");
        std::fs::write(&a, "{}").unwrap();
        let mut t = Tracked::new(a.clone());
        assert!(!t.follow(), "still there: nothing to do");
        std::fs::create_dir(dir.join("moved")).unwrap();
        let b = dir.join("moved").join("renamed.mthd");
        std::fs::rename(&a, &b).unwrap();
        if cfg!(target_os = "macos") {
            assert!(t.mark.is_some());
            assert!(t.follow(), "followed to its new place");
            assert_eq!(t.path, b);
        } else {
            assert!(t.mark.is_none());
            assert!(!t.follow(), "no bookmarks: shown as missing");
            assert_eq!(t.path, a);
        }
        // A deleted file isn't found again.
        std::fs::remove_file(&b).unwrap();
        let before = t.clone();
        assert!(!t.follow());
        assert!(!t.follow(), "and again");
        assert_eq!(t, before);
        assert!(create(&b).is_none(), "no bookmark for a missing file");
        assert!(resolve(b"not a bookmark").is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}

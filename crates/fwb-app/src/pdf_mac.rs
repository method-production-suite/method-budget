//! macOS: print the main window's web view straight into a PDF file, or to
//! the system print sheet with the report's page setup, through AppKit's
//! `NSPrintOperation` (`-[WKWebView printOperationWithPrintInfo:]`, macOS 11+).
//!
//! No crate offers this to us without adding dependencies (Tauri's own
//! `print()` always shows the print sheet with the shared page setup), so this
//! talks to the Objective-C runtime directly: `objc_msgSend` with an exact
//! function type per call, a small delegate class registered once at run time
//! to hear when the operation is over, and nothing else. No blocks, no struct
//! returns (so it is the same on Apple silicon and Intel).
//!
//! What gets printed is whatever the page shows for print media: the report
//! page renders the report into `#print-root` and marks `<html>` as printing
//! before it asks (ui/js/report.js), exactly as for `window.print()`.
//!
//! Everything here must run on the main thread (Tauri's `with_webview`
//! closure does).

use std::ffi::{c_char, c_void, CString};
use std::sync::OnceLock;

type Id = *mut c_void;
type Sel = *const c_void;

#[link(name = "objc")]
extern "C" {
    fn objc_getClass(name: *const c_char) -> Id;
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_msgSend();
    fn objc_allocateClassPair(superclass: Id, name: *const c_char, extra: usize) -> Id;
    fn objc_registerClassPair(cls: Id);
    fn class_addMethod(cls: Id, name: Sel, imp: *const c_void, types: *const c_char) -> i8;
}

#[link(name = "AppKit", kind = "framework")]
extern "C" {
    static NSPrintSaveJob: Id;
    static NSPrintJobSavingURL: Id;
}

/// Page setup for one print or PDF job.
#[derive(Clone, Debug, PartialEq)]
pub struct PageSetup {
    /// Paper width and height in points (1/72 inch), portrait.
    pub paper: [f64; 2],
    pub landscape: bool,
    /// Margins in points (1/72 inch): top, right, bottom, left.
    pub margins: [f64; 4],
    /// The job's name (the print queue, and the PDF's title).
    pub title: String,
}

/// How a job ends: `true` when it printed or wrote the file, `false` when it
/// was cancelled or failed.
pub type Done = Box<dyn FnOnce(bool) + Send>;

fn sel(name: &str) -> Sel {
    let c = CString::new(name).expect("selector");
    // SAFETY: a NUL-terminated selector name; the runtime interns it.
    unsafe { sel_registerName(c.as_ptr()) }
}

fn class(name: &str) -> Id {
    let c = CString::new(name).expect("class name");
    // SAFETY: a NUL-terminated class name; null when it does not exist.
    unsafe { objc_getClass(c.as_ptr()) }
}

// One function type per call shape. Calling objc_msgSend through the exact
// type of the method is what the runtime requires (it is not variadic).

unsafe fn send0(obj: Id, s: &str) -> Id {
    let f: unsafe extern "C" fn(Id, Sel) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s))
}

unsafe fn send_id(obj: Id, s: &str, a: Id) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, Id) -> Id = std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s), a)
}

unsafe fn send_id_id(obj: Id, s: &str, a: Id, b: Id) {
    let f: unsafe extern "C" fn(Id, Sel, Id, Id) = std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s), a, b)
}

unsafe fn send_bool(obj: Id, s: &str, v: bool) {
    let f: unsafe extern "C" fn(Id, Sel, i8) = std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s), i8::from(v))
}

unsafe fn send_isize(obj: Id, s: &str, v: isize) {
    let f: unsafe extern "C" fn(Id, Sel, isize) = std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s), v)
}

unsafe fn send_f64(obj: Id, s: &str, v: f64) {
    let f: unsafe extern "C" fn(Id, Sel, f64) = std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s), v)
}

#[repr(C)]
#[derive(Clone, Copy)]
struct Size {
    width: f64,
    height: f64,
}

unsafe fn send_size(obj: Id, s: &str, v: Size) {
    let f: unsafe extern "C" fn(Id, Sel, Size) = std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel(s), v)
}

unsafe fn responds(obj: Id, s: &str) -> bool {
    let f: unsafe extern "C" fn(Id, Sel, Sel) -> i8 =
        std::mem::transmute(objc_msgSend as *const ());
    f(obj, sel("respondsToSelector:"), sel(s)) != 0
}

/// An autoreleased `NSString`.
unsafe fn ns_string(s: &str) -> Id {
    // Interior NULs can't cross into C; a title or path never needs them.
    let c = CString::new(s.replace('\0', "")).expect("no NULs left");
    let f: unsafe extern "C" fn(Id, Sel, *const c_char) -> Id =
        std::mem::transmute(objc_msgSend as *const ());
    f(class("NSString"), sel("stringWithUTF8String:"), c.as_ptr())
}

/// The delegate class: one method, called when the operation is over.
fn delegate_class() -> Id {
    static CLASS: OnceLock<usize> = OnceLock::new();
    *CLASS.get_or_init(|| {
        // SAFETY: registers a plain NSObject subclass with one method whose
        // IMP matches the selector's signature (void, id, BOOL, void*).
        unsafe {
            let name = CString::new("FWBPrintJobDone").expect("name");
            let existing = objc_getClass(name.as_ptr());
            if !existing.is_null() {
                return existing as usize;
            }
            let cls = objc_allocateClassPair(class("NSObject"), name.as_ptr(), 0);
            let types = CString::new("v@:@c^v").expect("types");
            class_addMethod(
                cls,
                sel("printOperationDidRun:success:contextInfo:"),
                did_run as *const c_void,
                types.as_ptr(),
            );
            objc_registerClassPair(cls);
            cls as usize
        }
    }) as Id
}

/// What travels through `contextInfo`: the callback and the delegate object
/// (kept alive until now, then released).
struct Job {
    done: Done,
    delegate: Id,
}

extern "C" fn did_run(_this: Id, _cmd: Sel, _op: Id, success: i8, ctx: *mut c_void) {
    if ctx.is_null() {
        return;
    }
    // SAFETY: ctx is the Box<Job> leaked in `run`, handed back exactly once.
    let job = unsafe { Box::from_raw(ctx as *mut Job) };
    // SAFETY: the delegate was created with +1 in `run` and is released once.
    unsafe {
        send0(job.delegate, "release");
    }
    (job.done)(success != 0);
}

/// Prints `webview` (a `WKWebView*`) in `window` (its `NSWindow*`): into the
/// PDF file `pdf` without any panel, or, with `pdf` `None`, through the print
/// sheet with `setup` preselected. `done` runs once, on the main thread, when
/// it is over. Returns an error, without calling `done`, when this macOS
/// can't print a web view this way (before macOS 11).
///
/// # Safety
///
/// `webview` and `window` must be live `WKWebView` and `NSWindow` pointers,
/// and this must be called on the main thread.
pub unsafe fn run(
    webview: *mut c_void,
    window: *mut c_void,
    setup: &PageSetup,
    pdf: Option<&std::path::Path>,
    done: Done,
) -> Result<(), String> {
    if webview.is_null() || window.is_null() {
        return Err("The window isn't ready to print.".into());
    }
    if !responds(webview, "printOperationWithPrintInfo:") {
        return Err("Printing a report this way needs macOS 11 or later.".into());
    }
    // A copy, so the app-wide page setup isn't changed behind the user's back.
    let shared = send0(class("NSPrintInfo"), "sharedPrintInfo");
    let info = send0(shared, "copy");
    send0(info, "autorelease");
    let [width, height] = setup.paper;
    send_size(info, "setPaperSize:", Size { width, height });
    send_isize(info, "setOrientation:", isize::from(setup.landscape));
    let [top, right, bottom, left] = setup.margins;
    send_f64(info, "setTopMargin:", top);
    send_f64(info, "setRightMargin:", right);
    send_f64(info, "setBottomMargin:", bottom);
    send_f64(info, "setLeftMargin:", left);
    send_bool(info, "setHorizontallyCentered:", false);
    send_bool(info, "setVerticallyCentered:", false);
    if let Some(path) = pdf {
        let url = send_id(
            class("NSURL"),
            "fileURLWithPath:",
            ns_string(&path.to_string_lossy()),
        );
        send_id(info, "setJobDisposition:", NSPrintSaveJob);
        let dict = send0(info, "dictionary");
        send_id_id(dict, "setObject:forKey:", url, NSPrintJobSavingURL);
    }

    let op = send_id(webview, "printOperationWithPrintInfo:", info);
    if op.is_null() {
        return Err("The page could not be printed.".into());
    }
    send_id(op, "setJobTitle:", ns_string(&setup.title));
    send_bool(op, "setShowsPrintPanel:", pdf.is_none());
    send_bool(op, "setShowsProgressPanel:", pdf.is_none());

    let delegate = send0(send0(delegate_class(), "alloc"), "init");
    let ctx = Box::into_raw(Box::new(Job { done, delegate })) as *mut c_void;
    let f: unsafe extern "C" fn(Id, Sel, Id, Id, Sel, *mut c_void) =
        std::mem::transmute(objc_msgSend as *const ());
    f(
        op,
        sel("runOperationModalForWindow:delegate:didRunSelector:contextInfo:"),
        window,
        delegate,
        sel("printOperationDidRun:success:contextInfo:"),
        ctx,
    );
    Ok(())
}

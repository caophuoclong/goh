//! macOS font lookup by name: which file holds the font a terminal is set to.

use std::ffi::{c_void, CString};
use std::path::PathBuf;

type CFTypeRef = *mut c_void; // as scroll.rs declares CFRelease

#[link(name = "CoreText", kind = "framework")]
extern "C" {
    static kCTFontURLAttribute: CFTypeRef;
    fn CTFontCreateWithName(name: CFTypeRef, size: f64, matrix: *const c_void) -> CFTypeRef;
    fn CTFontCopyAttribute(font: CFTypeRef, attribute: CFTypeRef) -> CFTypeRef;
    fn CTFontCopyFamilyName(font: CFTypeRef) -> CFTypeRef;
    fn CTFontCopyPostScriptName(font: CFTypeRef) -> CFTypeRef;
    fn CTFontCopyFullName(font: CFTypeRef) -> CFTypeRef;
}

#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFRelease(cf: CFTypeRef);
    fn CFStringCreateWithCString(alloc: CFTypeRef, c: *const i8, encoding: u32) -> CFTypeRef;
    fn CFStringGetCString(s: CFTypeRef, buf: *mut u8, size: isize, encoding: u32) -> bool;
    fn CFURLGetFileSystemRepresentation(url: CFTypeRef, resolve: bool, buf: *mut u8, max: isize) -> bool;
}

const UTF8: u32 = 0x0800_0100;

struct Owned(CFTypeRef);
impl Drop for Owned {
    fn drop(&mut self) {
        // SAFETY: owns one reference to a non-null CF object.
        unsafe { CFRelease(self.0) }
    }
}

fn owned(p: CFTypeRef) -> Option<Owned> {
    (!p.is_null()).then_some(Owned(p))
}

fn string(s: &Owned) -> Option<String> {
    let mut buf = vec![0u8; 512];
    // SAFETY: the buffer length is passed alongside the pointer.
    unsafe { CFStringGetCString(s.0, buf.as_mut_ptr(), buf.len() as isize, UTF8) }
        .then(|| String::from_utf8_lossy(&buf[..buf.iter().position(|&b| b == 0).unwrap_or(buf.len())]).into_owned())
}

/// The font file for `name` (family, full or PostScript name). CoreText answers an unknown name with a
/// default font, so the result counts only when one of the font's own names matches what was asked.
pub fn font_file(name: &str, accepts: impl Fn(&str) -> bool) -> Option<PathBuf> {
    let c = CString::new(name).ok()?;
    // SAFETY: valid C string; null allocator means the default one. Every CF object below is owned and released.
    unsafe {
        let key = owned(CFStringCreateWithCString(std::ptr::null_mut(), c.as_ptr(), UTF8))?;
        let font = owned(CTFontCreateWithName(key.0, 13.0, std::ptr::null()))?;
        let names = [CTFontCopyFamilyName(font.0), CTFontCopyPostScriptName(font.0), CTFontCopyFullName(font.0)];
        let matched = names.into_iter().filter_map(owned).filter_map(|n| string(&n)).any(|n| accepts(&n));
        if !matched {
            return None;
        }
        let url = owned(CTFontCopyAttribute(font.0, kCTFontURLAttribute))?;
        let mut buf = vec![0u8; 4096];
        if !CFURLGetFileSystemRepresentation(url.0, true, buf.as_mut_ptr(), buf.len() as isize) {
            return None;
        }
        let end = buf.iter().position(|&b| b == 0)?;
        Some(PathBuf::from(String::from_utf8_lossy(&buf[..end]).into_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use goh_popup::font::{cell_width, normalize};

    #[test]
    fn a_installed_font_resolves_to_its_file_and_an_unknown_one_does_not() {
        let want = normalize("Menlo");
        let path = font_file("Menlo", |n| normalize(n) == want).expect("Menlo ships with macOS");
        let width = cell_width(&std::fs::read(path).unwrap(), 13.0).unwrap();
        assert!((width - 7.8).abs() < 0.2, "Menlo's cell at 13 pt is the popup's built-in cell width: {width}");
        let nope = normalize("No Such Font Family");
        assert_eq!(font_file("No Such Font Family", |n| normalize(n) == nope), None, "the default-font answer is rejected");
    }
}

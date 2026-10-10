//! `--capture <png>`: a PNG of the window's content, for screenshot
//! demos (no screen-recording permission needed). WebKit renders it
//! (`WKWebView takeSnapshotWithConfiguration:`), so it only exists on
//! macOS; the other platforms ignore the flag.

#[cfg(target_os = "macos")]
pub fn capture(webview: &wry::WebView, path: std::path::PathBuf, done: impl Fn(bool) + 'static) {
    use block2::RcBlock;
    use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
    use objc2_foundation::{NSDictionary, NSError, NSString};
    use wry::WebViewExtMacOS;

    fn write_png(image: &NSImage, path: &std::path::Path) -> Option<()> {
        let tiff = image.TIFFRepresentation()?;
        let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
        let png = unsafe {
            rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
        }?;
        let ns_path = NSString::from_str(path.to_str()?);
        png.writeToFile_atomically(&ns_path, true).then_some(())
    }

    let wk = webview.webview();
    let block = RcBlock::new(move |image: *mut NSImage, _error: *mut NSError| {
        let ok = unsafe { image.as_ref() }.is_some_and(|image| write_png(image, &path).is_some());
        done(ok);
    });
    unsafe { wk.takeSnapshotWithConfiguration_completionHandler(None, &block) };
}

#[cfg(not(target_os = "macos"))]
pub fn capture(_webview: &wry::WebView, _path: std::path::PathBuf, done: impl Fn(bool) + 'static) {
    done(false);
}

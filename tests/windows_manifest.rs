//! gpui is built with `windows-manifest-provided-by-host`, so quill.exe must
//! embed its own application manifest. These checks run on every platform:
//! the manifest carries the required settings and build.rs links it in.

#![allow(
    clippy::expect_used,
    reason = "integration test code is exempt (docs/contributing/panic-safety.md)"
)]

use std::path::Path;

fn read(rel: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(rel);
    std::fs::read_to_string(&path).expect("read repo file")
}

#[test]
fn manifest_declares_dpi_long_paths_utf8_and_common_controls() {
    let manifest = read("assets/windows/quill.manifest");
    for needle in [
        ">PerMonitorV2</dpiAwareness>",
        ">true</longPathAware>",
        ">UTF-8</activeCodePage>",
        "Microsoft.Windows.Common-Controls",
        "version=\"6.0.0.0\"",
        // Windows 10 and 11.
        "{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}",
        "level=\"asInvoker\"",
    ] {
        assert!(manifest.contains(needle), "manifest lacks {needle}");
    }
}

#[test]
fn build_script_embeds_the_manifest_as_rt_manifest() {
    let build = read("build.rs");
    assert!(build.contains("assets/windows/quill.manifest"));
    // Resource type 24 (RT_MANIFEST), id 1.
    assert!(build.contains("1 24 "));
}

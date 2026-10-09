//! The app's version. Release builds are stamped with the release date in
//! Cargo's version format, `YYYY.M.D` (`QUILL_RELEASE_VERSION`, set by
//! `.github/workflows/release.yml`); local and CI builds use `Cargo.toml`'s.

/// The version this binary reports, compares updates against and shows.
pub const APP: &str = match option_env!("QUILL_RELEASE_VERSION") {
    Some(version) if !version.is_empty() => version,
    _ => env!("CARGO_PKG_VERSION"),
};

#[cfg(test)]
mod tests {
    #[test]
    fn app_version_is_semver() {
        assert!(semver::Version::parse(super::APP).is_ok());
    }
}

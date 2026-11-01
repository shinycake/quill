//! Why nothing could draw the window on Linux, worked out without Vulkan:
//! whether the loader is installed, which driver manifests it would read,
//! and whether the GPU is a Virtio virtual GPU. Turns that into steps that
//! name the package to install.

use quill::lang::{keys, tr, tr_with};
use std::path::{Path, PathBuf};

/// PCI vendor id of Virtio devices (`/sys/class/drm/card*/device/vendor`).
const VIRTIO_VENDOR: &str = "0x1af4";

/// What the machine has for Vulkan.
#[derive(Debug, Default)]
pub(super) struct Setup {
    pub(super) loader: bool,
    pub(super) drivers: Vec<Driver>,
    pub(super) virtio_gpu: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DriverKind {
    /// lavapipe: draws on the CPU, works on any machine.
    Software,
    /// Venus: shows a device only when the VM host enables Venus.
    Virtio,
    Hardware,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Driver {
    pub(super) name: String,
    pub(super) kind: DriverKind,
}

/// What is missing when no adapter could draw the window.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Missing {
    Loader,
    Driver,
    /// Drivers are installed but none gave a device that can draw.
    /// `hardware`: one of them is for a real graphics card.
    UsableDevice {
        drivers: Vec<String>,
        hardware: bool,
    },
}

/// Whether wgpu found no adapter at all, or none that could draw the
/// window (`gpui-pre-wgpu` `select_adapter_and_device`).
fn is_no_adapter(detail: &str) -> bool {
    detail.to_lowercase().contains("no gpu adapter")
}

/// `None` when this is not a missing-dependency case: another GPU error, or
/// lavapipe is installed and still failed. Those keep the general message.
pub(super) fn diagnose(detail: &str, setup: &Setup) -> Option<Missing> {
    if !is_no_adapter(detail) {
        return None;
    }
    if !setup.loader {
        return Some(Missing::Loader);
    }
    if setup.drivers.is_empty() {
        return Some(Missing::Driver);
    }
    if setup
        .drivers
        .iter()
        .any(|driver| driver.kind == DriverKind::Software)
    {
        return None;
    }
    let mut drivers: Vec<String> = Vec::new();
    for driver in &setup.drivers {
        if !drivers.contains(&driver.name) {
            drivers.push(driver.name.clone());
        }
    }
    let hardware = setup
        .drivers
        .iter()
        .any(|driver| driver.kind == DriverKind::Hardware);
    Some(Missing::UsableDevice { drivers, hardware })
}

/// The alert body for `missing`.
pub(super) fn message(missing: &Missing, virtio_gpu: bool, detail: &str) -> String {
    let mut parts = vec![match missing {
        Missing::Loader => tr(keys::Q_MISC_VULKAN_NO_LOADER).to_owned(),
        Missing::Driver => tr(keys::Q_MISC_VULKAN_NO_DRIVER).to_owned(),
        Missing::UsableDevice { drivers, .. } => tr_with(
            keys::Q_MISC_VULKAN_NO_DEVICE,
            &[("drivers", &drivers.join(", "))],
        ),
    }];
    if *missing == Missing::Loader {
        parts.push(tr(keys::Q_MISC_VULKAN_INSTALL_LOADER).to_owned());
    }
    if virtio_gpu {
        parts.push(tr(keys::Q_MISC_VULKAN_VIRTIO).to_owned());
    }
    parts.push(tr(keys::Q_MISC_VULKAN_INSTALL_LAVAPIPE).to_owned());
    if matches!(missing, Missing::UsableDevice { hardware: true, .. }) {
        parts.push(tr(keys::Q_MISC_VULKAN_CHECK_HARDWARE).to_owned());
    } else if !virtio_gpu {
        parts.push(tr(keys::Q_MISC_VULKAN_INSTALL_HARDWARE).to_owned());
    }
    parts.push(tr_with(
        keys::Q_MISC_VULKAN_START_AGAIN,
        &[("detail", detail)],
    ));
    parts.join("\n\n")
}

/// What to install, short enough for a desktop notification.
pub(super) fn summary(missing: &Missing) -> &'static str {
    match missing {
        Missing::Loader => tr(keys::Q_MISC_VULKAN_SUMMARY_LOADER),
        // An installed card driver needs checking, not lavapipe first.
        Missing::UsableDevice { hardware: true, .. } => tr(keys::Q_MISC_VULKAN_SUMMARY_GPU),
        Missing::Driver | Missing::UsableDevice { .. } => tr(keys::Q_MISC_VULKAN_SUMMARY_LAVAPIPE),
    }
}

/// Read the machine's Vulkan setup.
#[cfg(target_os = "linux")]
pub(super) fn probe() -> Setup {
    Setup {
        loader: loader_installed(),
        drivers: manifest_files()
            .iter()
            .filter_map(|path| driver(path))
            .collect(),
        virtio_gpu: virtio_gpu(Path::new("/sys/class/drm")),
    }
}

#[cfg(target_os = "linux")]
fn loader_installed() -> bool {
    // SAFETY: the name is a NUL-terminated literal, and a non-null handle is
    // closed right away; wgpu has already loaded and initialised the library
    // when it is present.
    unsafe {
        let handle = libc::dlopen(
            c"libvulkan.so.1".as_ptr(),
            libc::RTLD_LAZY | libc::RTLD_LOCAL,
        );
        if handle.is_null() {
            return false;
        }
        libc::dlclose(handle);
    }
    true
}

/// The driver manifests the Vulkan loader would read: `VK_DRIVER_FILES` (or
/// the older `VK_ICD_FILENAMES`) replaces the search, `VK_ADD_DRIVER_FILES`
/// adds to it, otherwise the XDG config and data folders.
fn manifest_files() -> Vec<PathBuf> {
    let listed = |name: &str| -> Option<Vec<PathBuf>> {
        let value = std::env::var_os(name).filter(|value| !value.is_empty())?;
        Some(std::env::split_paths(&value).collect())
    };
    let mut roots = listed("VK_DRIVER_FILES")
        .or_else(|| listed("VK_ICD_FILENAMES"))
        .unwrap_or_else(search_folders);
    roots.extend(listed("VK_ADD_DRIVER_FILES").unwrap_or_default());
    let mut files = Vec::new();
    for root in roots {
        if root.is_dir() {
            let mut entries: Vec<PathBuf> = std::fs::read_dir(&root)
                .map(|dir| dir.flatten().map(|entry| entry.path()).collect())
                .unwrap_or_default();
            entries.sort();
            files.extend(
                entries
                    .into_iter()
                    .filter(|path| path.extension().is_some_and(|ext| ext == "json")),
            );
        } else if root.is_file() {
            files.push(root);
        }
    }
    files
}

fn search_folders() -> Vec<PathBuf> {
    let env_dirs = |name: &str, default: &str| -> Vec<PathBuf> {
        let value = std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .unwrap_or_else(|| default.into());
        std::env::split_paths(&value).collect()
    };
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let user_dir = |name: &str, fallback: &str| -> Option<PathBuf> {
        std::env::var_os(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| home.as_ref().map(|home| home.join(fallback)))
    };
    let mut bases = Vec::new();
    bases.extend(user_dir("XDG_CONFIG_HOME", ".config"));
    bases.extend(env_dirs("XDG_CONFIG_DIRS", "/etc/xdg"));
    bases.push(PathBuf::from("/etc"));
    bases.extend(user_dir("XDG_DATA_HOME", ".local/share"));
    bases.extend(env_dirs("XDG_DATA_DIRS", "/usr/local/share:/usr/share"));
    bases
        .into_iter()
        .map(|base| base.join("vulkan").join("icd.d"))
        .collect()
}

fn driver(manifest: &Path) -> Option<Driver> {
    let text = std::fs::read_to_string(manifest).ok()?;
    let json: serde_json::Value = serde_json::from_str(&text).ok()?;
    let library = json
        .get("ICD")
        .and_then(|icd| icd.get("library_path"))
        .and_then(serde_json::Value::as_str)?;
    let file_name = manifest.file_name()?.to_string_lossy();
    Some(classify(&file_name, library))
}

/// Name and kind from a manifest's file name (`lvp_icd.aarch64.json`) and
/// library (`/usr/lib/libvulkan_lvp.so`).
fn classify(file_name: &str, library: &str) -> Driver {
    let both = format!("{file_name} {library}").to_lowercase();
    let kind = if ["lvp", "lavapipe", "swrast", "llvmpipe"]
        .iter()
        .any(|needle| both.contains(needle))
    {
        DriverKind::Software
    } else if both.contains("virtio") {
        DriverKind::Virtio
    } else {
        DriverKind::Hardware
    };
    let name = match kind {
        DriverKind::Software => "lavapipe".to_owned(),
        DriverKind::Virtio => "Virtio (Venus)".to_owned(),
        DriverKind::Hardware => file_name
            .split(['_', '.'])
            .next()
            .filter(|name| !name.is_empty())
            .unwrap_or(file_name)
            .to_owned(),
    };
    Driver { name, kind }
}

/// Whether a DRM card under `drm` is a Virtio GPU.
fn virtio_gpu(drm: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(drm) else {
        return false;
    };
    entries.flatten().any(|entry| {
        std::fs::read_to_string(entry.path().join("device").join("vendor"))
            .is_ok_and(|vendor| vendor.trim() == VIRTIO_VENDOR)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO_DEVICE: &str = "No GPU adapter found that can configure the display surface";

    fn setup(loader: bool, drivers: &[(&str, &str)], virtio_gpu: bool) -> Setup {
        Setup {
            loader,
            drivers: drivers
                .iter()
                .map(|(file, library)| classify(file, library))
                .collect(),
            virtio_gpu,
        }
    }

    #[test]
    fn missing_loader_then_driver_then_device() {
        assert_eq!(
            diagnose(NO_DEVICE, &setup(false, &[], false)),
            Some(Missing::Loader)
        );
        assert_eq!(
            diagnose("No GPU adapters found", &setup(true, &[], true)),
            Some(Missing::Driver)
        );
        assert_eq!(
            diagnose(
                NO_DEVICE,
                &setup(
                    true,
                    &[("virtio_icd.json", "/usr/lib/libvulkan_virtio.so")],
                    true
                )
            ),
            Some(Missing::UsableDevice {
                drivers: vec!["Virtio (Venus)".to_owned()],
                hardware: false,
            })
        );
    }

    #[test]
    fn lavapipe_installed_or_other_errors_keep_the_general_message() {
        let with_lavapipe = setup(
            true,
            &[
                ("virtio_icd.json", "/usr/lib/libvulkan_virtio.so"),
                ("lvp_icd.aarch64.json", "/usr/lib/libvulkan_lvp.so"),
            ],
            true,
        );
        assert_eq!(diagnose(NO_DEVICE, &with_lavapipe), None);
        assert_eq!(diagnose("Device lost", &setup(true, &[], false)), None);
    }

    #[test]
    fn installed_card_driver_gets_a_check_instead_of_an_install() {
        let missing = diagnose(
            NO_DEVICE,
            &setup(
                true,
                &[
                    ("radeon_icd.x86_64.json", "libvulkan_radeon.so"),
                    ("radeon_icd.i686.json", "libvulkan_radeon.so"),
                ],
                false,
            ),
        );
        assert_eq!(
            missing,
            Some(Missing::UsableDevice {
                drivers: vec!["radeon".to_owned()],
                hardware: true,
            })
        );
        let text = missing.map(|missing| message(&missing, false, NO_DEVICE));
        assert!(text.is_some_and(|text| text.contains("/dev/dri")));
    }

    #[test]
    fn hardware_driver_names_come_from_the_manifest() {
        assert_eq!(
            classify("radeon_icd.x86_64.json", "/usr/lib/libvulkan_radeon.so"),
            Driver {
                name: "radeon".to_owned(),
                kind: DriverKind::Hardware
            }
        );
        assert_eq!(
            classify("nvidia_icd.json", "libGLX_nvidia.so.0").kind,
            DriverKind::Hardware
        );
    }

    #[test]
    fn messages_name_the_packages() {
        let virtio = message(&Missing::Driver, true, NO_DEVICE);
        for needle in [
            "vulkan-virtio",
            "Venus",
            "vulkan-swrast",
            "mesa-vulkan-drivers",
            NO_DEVICE,
        ] {
            assert!(virtio.contains(needle), "{needle} missing from {virtio}");
        }
        assert!(summary(&Missing::Driver).contains("vulkan-swrast"));
        assert!(summary(&Missing::Loader).contains("vulkan-icd-loader"));
        let loader = message(&Missing::Loader, false, NO_DEVICE);
        assert!(loader.contains("vulkan-icd-loader"));
        assert!(loader.contains("NVIDIA"));
    }

    #[test]
    fn virtio_card_found_by_pci_vendor() {
        let drm = quill::scratch::process_dir("startup-failure-drm");
        let device = drm.join("card0").join("device");
        let _ = std::fs::create_dir_all(&device);
        let _ = std::fs::write(device.join("vendor"), "0x1af4\n");
        assert!(virtio_gpu(&drm));
        let _ = std::fs::write(device.join("vendor"), "0x8086\n");
        assert!(!virtio_gpu(&drm));
    }
}

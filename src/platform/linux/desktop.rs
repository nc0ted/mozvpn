use std::path::PathBuf;

pub fn init_environment() {
    std::env::remove_var("WAYLAND_DISPLAY");
    ensure_desktop_entry();
}

pub fn ensure_desktop_entry() {
    if let Some(data_dir) = dirs::data_local_dir() {
        let apps_dir = data_dir.join("applications");
        let icons_dir = data_dir.join("icons").join("hicolor").join("256x256").join("apps");
        let desktop_file = apps_dir.join("outfox.desktop");
        let icon_file = icons_dir.join("outfox.png");

        let _ = std::fs::create_dir_all(&apps_dir);
        let _ = std::fs::create_dir_all(&icons_dir);
        let _ = std::fs::write(&icon_file, include_bytes!("../../../assets/icon.png"));

        let bin_dir = dirs::home_dir().map(|h| h.join(".local").join("bin"));
        let target_bin = bin_dir.as_ref().map(|b| b.join("outfox"));

        let source_exe = std::env::var("APPIMAGE")
            .ok()
            .map(PathBuf::from)
            .or_else(|| std::env::current_exe().ok());

        let final_exec = if let (Some(src), Some(dst)) = (source_exe.as_ref(), target_bin.as_ref()) {
            if let Some(b) = &bin_dir {
                let _ = std::fs::create_dir_all(b);
            }
            if src != dst {
                use std::os::unix::fs::PermissionsExt;
                let tmp = dst.with_extension("tmp");
                if std::fs::copy(src, &tmp).is_ok() {
                    let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o755));
                    let _ = std::fs::rename(&tmp, dst);
                }
            }
            dst.clone()
        } else if let Some(src) = source_exe {
            src
        } else {
            return;
        };

        let entry = format!(
            "[Desktop Entry]\nName=outfox\nTryExec={}\nExec={}\nIcon={}\nType=Application\nCategories=Network;\nTerminal=false\nStartupNotify=true\nStartupWMClass=outfox\n",
            final_exec.display(),
            final_exec.display(),
            icon_file.display()
        );
        let _ = std::fs::write(desktop_file, entry);
        let _ = std::process::Command::new("update-desktop-database")
            .arg(&apps_dir)
            .output();
    }
}

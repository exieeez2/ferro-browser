use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=src");
    println!("cargo:rerun-if-changed=build.rs");

    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| "/home/exieeez/NODE".to_string());
    let home = env::var("HOME").unwrap_or_else(|_| "/home/exieeez".to_string());
    let home_path = PathBuf::from(&home);

    let logo_src = PathBuf::from(&manifest_dir).join("src/ui/logo.png");
    let icon_512_dir = home_path.join(".local/share/icons/hicolor/512x512/apps");
    let icon_256_dir = home_path.join(".local/share/icons/hicolor/256x256/apps");
    let _ = fs::create_dir_all(&icon_512_dir);
    let _ = fs::create_dir_all(&icon_256_dir);

    let icon_512_path = icon_512_dir.join("ferro.png");
    let icon_256_path = icon_256_dir.join("ferro.png");
    if logo_src.exists() {
        let _ = fs::copy(&logo_src, &icon_512_path);
        let _ = fs::copy(&logo_src, &icon_256_path);
    }

    let bin_dir = home_path.join(".local/bin");
    let _ = fs::create_dir_all(&bin_dir);
    let launcher_path = bin_dir.join("ferro");
    let launcher_script = format!(
        r#"#!/usr/bin/env bash
REPO_DIR="{repo}"
REL_BIN="$REPO_DIR/target/release/ferro"
DBG_BIN="$REPO_DIR/target/debug/ferro"

if [ -x "$REL_BIN" ] && [ -x "$DBG_BIN" ]; then
  if [ "$DBG_BIN" -nt "$REL_BIN" ]; then
    exec "$DBG_BIN" "$@"
  else
    exec "$REL_BIN" "$@"
  fi
elif [ -x "$REL_BIN" ]; then
  exec "$REL_BIN" "$@"
elif [ -x "$DBG_BIN" ]; then
  exec "$DBG_BIN" "$@"
else
  cd "$REPO_DIR" || exit 1
  exec cargo run --release -- "$@"
fi
"#,
        repo = manifest_dir
    );
    if fs::write(&launcher_path, launcher_script).is_ok() {
        let _ = fs::set_permissions(&launcher_path, fs::Permissions::from_mode(0o755));
    }

    let apps_dir = home_path.join(".local/share/applications");
    let _ = fs::create_dir_all(&apps_dir);
    let desktop_path = apps_dir.join("ferro.desktop");
    let desktop_content = format!(
        "[Desktop Entry]\n\
Type=Application\n\
Name=Ferro\n\
GenericName=Web Browser\n\
GenericName[ru]=Веб-браузер\n\
Comment=Быстрый минималистичный браузер Ferro\n\
Exec={launcher} %U\n\
TryExec={launcher}\n\
Path={repo}\n\
Icon={icon}\n\
Terminal=false\n\
Categories=Network;WebBrowser;\n\
StartupWMClass=ferro\n\
StartupNotify=true\n\
MimeType=text/html;text/xml;application/xhtml+xml;x-scheme-handler/http;x-scheme-handler/https;\n",
        launcher = launcher_path.to_string_lossy(),
        repo = manifest_dir,
        icon = icon_512_path.to_string_lossy()
    );
    if fs::write(&desktop_path, desktop_content).is_ok() {
        let _ = fs::set_permissions(&desktop_path, fs::Permissions::from_mode(0o755));
    }

    let _ = Command::new("update-desktop-database")
        .arg(&apps_dir)
        .status();
}

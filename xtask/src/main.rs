use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail};

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    match args.next().as_deref() {
        Some("macos-bundle") => macos_bundle(parse_profile(args.collect())?),
        Some(command) => bail!("unknown xtask command: {command}"),
        None => {
            println!("AgentHouse xtask");
            println!("usage: cargo run -p xtask -- macos-bundle [--profile debug|release]");
            Ok(())
        }
    }
}

fn parse_profile(args: Vec<String>) -> anyhow::Result<String> {
    let mut profile = "debug".to_string();
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--release" => profile = "release".to_string(),
            "--profile" => {
                profile = iter.next().context("--profile requires a value")?;
                if profile != "debug" && profile != "release" {
                    bail!("unsupported profile: {profile}");
                }
            }
            other => bail!("unknown macos-bundle option: {other}"),
        }
    }
    Ok(profile)
}

#[cfg(target_os = "macos")]
fn macos_bundle(profile: String) -> anyhow::Result<()> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .context("xtask should live under the workspace root")?;
    let target_dir = repo_root.join("target").join(&profile);
    let executable = target_dir.join("ah-app");
    if !executable.exists() {
        bail!(
            "{} does not exist; run `cargo build -p ah-app{}` first",
            executable.display(),
            if profile == "release" {
                " --release"
            } else {
                ""
            }
        );
    }

    let bundle_dir = target_dir.join("AgentHouse.app");
    let contents_dir = bundle_dir.join("Contents");
    let macos_dir = contents_dir.join("MacOS");
    let resources_dir = contents_dir.join("Resources");
    if bundle_dir.exists() {
        fs::remove_dir_all(&bundle_dir)
            .with_context(|| format!("removing {}", bundle_dir.display()))?;
    }
    fs::create_dir_all(&macos_dir).with_context(|| format!("creating {}", macos_dir.display()))?;
    fs::create_dir_all(&resources_dir)
        .with_context(|| format!("creating {}", resources_dir.display()))?;

    let bundled_executable = macos_dir.join("ah-app");
    fs::copy(&executable, &bundled_executable).with_context(|| {
        format!(
            "copying {} to {}",
            executable.display(),
            bundled_executable.display()
        )
    })?;
    set_executable(&bundled_executable)?;

    let iconset_dir = target_dir.join("AgentHouse.iconset");
    if iconset_dir.exists() {
        fs::remove_dir_all(&iconset_dir)
            .with_context(|| format!("removing {}", iconset_dir.display()))?;
    }
    fs::create_dir_all(&iconset_dir)
        .with_context(|| format!("creating {}", iconset_dir.display()))?;
    build_iconset(
        &repo_root.join("crates/ah-app/assets/app-icon.jpg"),
        &iconset_dir,
    )?;
    run(Command::new("iconutil").args([
        "-c",
        "icns",
        iconset_dir
            .to_str()
            .context("iconset path is not valid UTF-8")?,
        "-o",
        resources_dir
            .join("AppIcon.icns")
            .to_str()
            .context("icns path is not valid UTF-8")?,
    ]))?;

    fs::write(contents_dir.join("Info.plist"), info_plist())
        .with_context(|| format!("writing {}", contents_dir.join("Info.plist").display()))?;

    println!("{}", bundle_dir.display());
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn macos_bundle(_profile: String) -> anyhow::Result<()> {
    bail!("macos-bundle can only run on macOS")
}

#[cfg(target_os = "macos")]
fn build_iconset(source: &Path, iconset_dir: &Path) -> anyhow::Result<()> {
    let sizes = [
        (16, "icon_16x16.png"),
        (32, "icon_16x16@2x.png"),
        (32, "icon_32x32.png"),
        (64, "icon_32x32@2x.png"),
        (128, "icon_128x128.png"),
        (256, "icon_128x128@2x.png"),
        (256, "icon_256x256.png"),
        (512, "icon_256x256@2x.png"),
        (512, "icon_512x512.png"),
        (1024, "icon_512x512@2x.png"),
    ];
    for (size, name) in sizes {
        let output = iconset_dir.join(name);
        run(Command::new("sips").args([
            "-z",
            &size.to_string(),
            &size.to_string(),
            "-s",
            "format",
            "png",
            source
                .to_str()
                .context("icon source path is not valid UTF-8")?,
            "--out",
            output
                .to_str()
                .context("icon output path is not valid UTF-8")?,
        ]))?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn info_plist() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>ah-app</string>
    <key>CFBundleIconFile</key>
    <string>AppIcon</string>
    <key>CFBundleIdentifier</key>
    <string>com.agenthouse.rs</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>AgentHouse</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleShortVersionString</key>
    <string>0.1.0</string>
    <key>CFBundleVersion</key>
    <string>0.1.0</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
"#
}

#[cfg(target_os = "macos")]
fn run(command: &mut Command) -> anyhow::Result<()> {
    let status = command
        .status()
        .with_context(|| format!("running {:?}", command))?;
    if !status.success() {
        bail!("command failed with {status}: {:?}", command);
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn set_executable(path: &PathBuf) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(path)
            .with_context(|| format!("reading metadata for {}", path.display()))?
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(path, permissions)
            .with_context(|| format!("setting executable permissions on {}", path.display()))?;
    }
    Ok(())
}

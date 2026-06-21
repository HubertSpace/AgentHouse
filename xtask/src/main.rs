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
    let executable = target_dir.join("agenthouse");
    if !executable.exists() {
        bail!(
            "{} does not exist; run `cargo build -p agenthouse{}` first",
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

    let bundled_executable = macos_dir.join("AgentHouse");
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
        &repo_root.join("crates/agenthouse/assets/app-icon.jpg"),
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
    copy_release_notices(repo_root, &resources_dir)?;

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
        render_macos_app_icon(source, &output, size)?;
    }
    Ok(())
}

#[cfg(target_os = "macos")]
fn render_macos_app_icon(source: &Path, output: &Path, size: u32) -> anyhow::Result<()> {
    let script = std::env::temp_dir().join("agenthouse-render-app-icon.swift");
    fs::write(&script, MACOS_ICON_RENDERER_SWIFT)
        .with_context(|| format!("writing {}", script.display()))?;
    run(Command::new("swift").args([
        script
            .to_str()
            .context("icon renderer path is not valid UTF-8")?,
        source
            .to_str()
            .context("icon source path is not valid UTF-8")?,
        output
            .to_str()
            .context("icon output path is not valid UTF-8")?,
        &size.to_string(),
    ]))
}

#[cfg(target_os = "macos")]
fn copy_release_notices(repo_root: &Path, resources_dir: &Path) -> anyhow::Result<()> {
    let notices = [
        ("LICENSE", "LICENSE"),
        ("THIRD_PARTY_NOTICES.md", "THIRD_PARTY_NOTICES.md"),
        (
            "crates/agenthouse/assets/fonts/geist/LICENSE.md",
            "GEIST_FONT_LICENSE.md",
        ),
    ];

    for (source, destination) in notices {
        let source = repo_root.join(source);
        if !source.exists() {
            bail!(
                "{} does not exist; release notice is missing",
                source.display()
            );
        }
        let destination = resources_dir.join(destination);
        fs::copy(&source, &destination).with_context(|| {
            format!(
                "copying release notice {} to {}",
                source.display(),
                destination.display()
            )
        })?;
    }

    Ok(())
}

#[cfg(target_os = "macos")]
const MACOS_ICON_RENDERER_SWIFT: &str = r#"
import AppKit

let args = CommandLine.arguments
guard args.count == 4, let size = Int(args[3]), size > 0 else {
    fputs("usage: render-icon source output size\n", stderr)
    exit(2)
}

let source = URL(fileURLWithPath: args[1])
let output = URL(fileURLWithPath: args[2])
guard let image = NSImage(contentsOf: source) else {
    fputs("failed to load source icon\n", stderr)
    exit(1)
}

let canvas = NSSize(width: size, height: size)
let inset = CGFloat(size) * 0.09
let rect = NSRect(x: inset, y: inset, width: CGFloat(size) - inset * 2.0, height: CGFloat(size) - inset * 2.0)
let corner = rect.width * 0.225
let composed = NSImage(size: canvas)
composed.lockFocus()
NSColor.clear.setFill()
NSRect(origin: .zero, size: canvas).fill()

let path = NSBezierPath(roundedRect: rect, xRadius: corner, yRadius: corner)
path.addClip()
image.draw(in: rect, from: .zero, operation: .copy, fraction: 1.0, respectFlipped: false, hints: [.interpolation: NSImageInterpolation.high])
composed.unlockFocus()

guard let tiff = composed.tiffRepresentation,
      let rep = NSBitmapImageRep(data: tiff),
      let png = rep.representation(using: .png, properties: [:]) else {
    fputs("failed to encode rendered icon\n", stderr)
    exit(1)
}

try png.write(to: output)
"#;

#[cfg(target_os = "macos")]
fn info_plist() -> &'static str {
    r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>AgentHouse</string>
    <key>CFBundleDisplayName</key>
    <string>AgentHouse</string>
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

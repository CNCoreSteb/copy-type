#[cfg(target_os = "macos")]
use std::fs;
#[cfg(target_os = "macos")]
use std::path::Path;

fn main() {
    // Windows 平台：设置程序资源
    #[cfg(windows)]
    {
        let mut res = winres::WindowsResource::new();
        res.set_icon("src/logo.ico");
        
        // 设置程序详细信息
        res.set("ProductName", "Copy&Type");
        res.set("FileDescription", "一款跨平台剪贴板监控和键盘输入模拟工具");
        res.set("CompanyName", "CN_CoreSteb");
        res.set("LegalCopyright", "Copyright © 2026 CN_CoreSteb. All rights reserved.");
        res.set("OriginalFilename", "copy-type.exe");
        
        res.compile().unwrap();
        println!("cargo:rerun-if-changed=src/logo.ico");
    }

    // macOS 平台：生成 Info.plist（版本号跟随 Cargo.toml，防止漂移）
    #[cfg(target_os = "macos")]
    {
        let version = env!("CARGO_PKG_VERSION");
        let info_plist = format!(r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>Copy&Type</string>
    <key>CFBundleDisplayName</key>
    <string>Copy&Type</string>
    <key>CFBundleIdentifier</key>
    <string>com.coresteb.copy-type</string>
    <key>CFBundleVersion</key>
    <string>{version}</string>
    <key>CFBundleShortVersionString</key>
    <string>{version}</string>
    <key>CFBundleExecutable</key>
    <string>copy-type</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleSignature</key>
    <string>????</string>
    <key>NSHumanReadableCopyright</key>
    <string>Copyright © 2026 CN_CoreSteb. All rights reserved.</string>
    <key>LSMinimumSystemVersion</key>
    <string>10.13</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>"#);

        let out_dir = std::env::var("OUT_DIR")
            .expect("OUT_DIR is not set; Cargo should provide it during builds");
        let info_plist_path = Path::new(&out_dir).join("Info.plist");
        fs::write(info_plist_path, info_plist)
            .expect("failed to write Info.plist to OUT_DIR");
        println!("cargo:rerun-if-changed=build.rs");
    }
}

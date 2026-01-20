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
    }

    // macOS 平台：生成 Info.plist
    #[cfg(target_os = "macos")]
    {
        use std::fs;
        use std::path::Path;
        
        let info_plist = r#"<?xml version="1.0" encoding="UTF-8"?>
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
    <string>1.1.0</string>
    <key>CFBundleShortVersionString</key>
    <string>1.1.0</string>
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
</plist>"#;
        
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let info_plist_path = Path::new(&out_dir).join("Info.plist");
        fs::write(info_plist_path, info_plist).unwrap();
        println!("cargo:rerun-if-changed=build.rs");
    }

    // Linux 平台：生成 .desktop 文件
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        use std::path::Path;
        
        let desktop_entry = r#"[Desktop Entry]
Type=Application
Name=Copy&Type
GenericName=Clipboard Monitor
Comment=跨平台剪贴板监控和键盘输入模拟工具
Exec=copy-type
Icon=copy-type
Terminal=false
Categories=Utility;
Keywords=clipboard;keyboard;typing;
"#;
        
        let out_dir = std::env::var("OUT_DIR").unwrap();
        let desktop_path = Path::new(&out_dir).join("copy-type.desktop");
        fs::write(desktop_path, desktop_entry).unwrap();
        println!("cargo:rerun-if-changed=build.rs");
    }
}

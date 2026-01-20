# 构建指南（Windows / Linux / macOS）

本文档说明如何在各自系统上编译对应平台的可执行文件。

## Windows 编译 Windows 版本

### 前置条件
- 安装 Rust 工具链（建议默认 MSVC 工具链）。
- 如果未安装过 C++ 编译环境，请安装 **Visual Studio Build Tools**（包含 MSVC 和 Windows SDK）。

### 步骤
```powershell
git clone https://github.com/CNCoreSteb/copy-type.git
cd copy-type
cargo build --release
```

### 产物位置
- `target\release\copy-type.exe`

## Linux 编译 Linux 版本

### 前置条件
- 安装 Rust 工具链。
- 安装依赖库（以下与 CI 保持一致）：

```bash
# Ubuntu/Debian
sudo apt-get update
sudo apt-get install -y \
  libglib2.0-dev \
  libgtk-3-dev \
  libxdo-dev \
  libx11-dev \
  libxkbcommon-dev \
  libwayland-dev \
  libxcb1-dev \
  libxcb-render0-dev \
  libxcb-shape0-dev \
  libxcb-xfixes0-dev \
  libxrandr-dev \
  libxi-dev \
  libxinerama-dev \
  libxcursor-dev \
  libxrender-dev \
  libgl1-mesa-dev \
  libegl1-mesa-dev \
  pkg-config

# Fedora
sudo dnf install -y \
  glib2-devel \
  gtk3-devel \
  libX11-devel \
  libxcb-devel \
  libxkbcommon-devel \
  wayland-devel \
  libXrandr-devel \
  libXi-devel \
  libXinerama-devel \
  libXcursor-devel \
  libXrender-devel \
  mesa-libGL-devel \
  mesa-libEGL-devel \
  pkgconf-pkg-config
```

### 步骤
```bash
git clone https://github.com/CNCoreSteb/copy-type.git
cd copy-type
cargo build --release
```

### 产物位置
- `target/release/copy-type`

## macOS 编译 macOS 版本

### 前置条件
- 安装 Xcode Command Line Tools：
  ```bash
  xcode-select --install
  ```
- 安装 Rust 工具链。

### 步骤（原生架构）
```bash
git clone https://github.com/CNCoreSteb/copy-type.git
cd copy-type
cargo build --release
```

### 步骤（指定目标架构）
```bash
# Intel Mac
rustup target add x86_64-apple-darwin
cargo build --release --target x86_64-apple-darwin

# Apple Silicon
rustup target add aarch64-apple-darwin
cargo build --release --target aarch64-apple-darwin
```

### 产物位置
- `target/release/copy-type`（原生架构）
- `target/<target>/release/copy-type`（指定目标架构）

## macOS .app packaging (Info.plist)
The build script writes `Info.plist` into `OUT_DIR`, which is not automatically included in the `.app` bundle.
Use one of the options below to package the macOS app correctly.

### Option A: cargo-bundle (recommended)
1. Install the tool:
   ```bash
   cargo install cargo-bundle
   ```
2. (Optional) Add bundle metadata to `Cargo.toml`:
   ```toml
   [package.metadata.bundle]
   name = "Copy&Type"
   identifier = "com.coresteb.copy-type"
   ```
3. Build the bundle:
   ```bash
   cargo bundle --release
   ```

### Option B: manual bundle assembly
1. Build the binary:
   ```bash
   cargo build --release
   ```
2. Create the bundle structure and copy artifacts:
   ```bash
   APP_NAME="Copy&Type.app"
   mkdir -p "$APP_NAME/Contents/MacOS"
   mkdir -p "$APP_NAME/Contents/Resources"
   cp "target/release/copy-type" "$APP_NAME/Contents/MacOS/copy-type"
   cp "target/release/build/<crate-hash>/out/Info.plist" "$APP_NAME/Contents/Info.plist"
   ```
   Replace `<crate-hash>` with the actual build directory under `target/release/build/`.
   If you build with a target triple, the path is `target/<target>/release/build/<crate-hash>/out/Info.plist`.

## Linux .desktop installation
The build script writes `copy-type.desktop` into `OUT_DIR`, which is not installed automatically.
Use one of the options below to put it where desktop environments can find it.

### Option A: user-local install
1. Build the binary:
   ```bash
   cargo build --release
   ```
2. Locate the generated desktop file:
   ```bash
   ls target/release/build/*/out/copy-type.desktop
   ```
3. Install it for the current user:
   ```bash
   mkdir -p ~/.local/share/applications
   cp target/release/build/*/out/copy-type.desktop ~/.local/share/applications/copy-type.desktop
   ```
4. (Optional) install an icon or update the `Icon=` field in the `.desktop` file to an absolute path.

### Option B: system-wide install
1. Build the binary:
   ```bash
   cargo build --release
   ```
2. Copy the `.desktop` file into `/usr/share/applications`:
   ```bash
   sudo cp target/release/build/*/out/copy-type.desktop /usr/share/applications/copy-type.desktop
   ```

### Option C: package it
Consider using a packaging tool like `cargo-deb` (Debian/Ubuntu) or `cargo-rpm` (Fedora/RHEL) to install the `.desktop` file, binary, and icons together.

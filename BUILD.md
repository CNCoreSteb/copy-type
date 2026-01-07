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

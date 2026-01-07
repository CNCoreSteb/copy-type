# Copy-Type

一个跨平台的剪贴板监控和模拟键盘输入工具，基于Rust。

## 功能

- **剪贴板监控**：自动检测并记录复制的文本内容
- **模拟键盘输入**：通过快捷键触发，模拟键盘逐字输入复制的内容
- **保留格式**：完整保留换行符、缩进等文本格式
- **跨平台支持**：支持 Windows（主要）、macOS 和 Linux
- **图形界面**：提供图形界面，支持启用/禁用、自定义快捷键、预览待输入文本
- **剪贴板历史**：存储最多100条最近剪贴板（暂时不推荐使用）

## 使用场景

- 在不允许粘贴的输入框中输入复制的文本
- 绕过某些网站对粘贴操作的限制
- 需要模拟真实键盘输入的场景
- 复制AI的输出并拟人地使用（bushi）

## 界面功能

- 启用/禁用程序开关
- 自定义快捷键设置
- 实时显示待输入的剪贴板内容
- 显示字符数和行数统计
- 手动触发输入按钮
- 清空剪贴板内容
- 输入速度调整
- 剪贴板历史开关

## 默认快捷键

| 快捷键 | 功能 |
|--------|------|
| `Ctrl+Shift+V` | 模拟键盘输入剪贴板内容（默认，可自定义）|

## 安装

### 从源码编译

1. 确保已安装 Rust 工具链：
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   ```

2. 克隆仓库并编译：
   ```bash
   git clone https://github.com/CNCoreSteb/copy-type.git
   cd copy-type
   cargo build --release
   ```

3. 运行程序：
   ```bash
   cargo run --release
   ```

## 使用方法

1. 启动程序，界面将会显示
2. 复制任意文本（Ctrl+C）
3. 在页面中可以看到待输入的文本预览
4. 将光标放置在需要输入的位置
5. 按下快捷键（默认 `Ctrl+Shift+V`）触发模拟输入

### 修改快捷键

1. 点击菜单 "设置" -> "快捷键设置"
2. 或点击主界面的 "修改" 按钮
3. 勾选需要的修饰键（Ctrl、Shift、Alt、Win/Cmd）
4. 选择主按键
5. 点击 "保存"

## 配置文件

配置文件保存在：
- Windows: `%APPDATA%\copy-type\config.json`
- macOS: `~/Library/Application Support/copy-type/config.json`
- Linux: `~/.config/copy-type/config.json`

## 平台依赖

### Windows
无需额外依赖

### Linux
需要安装 X11 开发库：
```bash
# Ubuntu/Debian
sudo apt-get install libx11-dev libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev

# Fedora
sudo dnf install libX11-devel libxcb-devel
```

### macOS
需要授予辅助功能权限

## 许可证

GPL-3.0 License

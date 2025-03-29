<div align="center">
  <img src="https://raw.githubusercontent.com/neoapps-dev/rustpack/main/assets/logo.png" alt="RustPack Logo" width="200"/>
  <h6>Logo by <a href="https://github.com/syntaxspin">@syntaxspin</a></h5>
<h1>RustPack</h1>
  <b>Bundle your Rust applications for seamless cross-platform execution</b>

  <p></p>
  <img src="https://ziadoua.github.io/m3-Markdown-Badges/badges/LicenceGPLv3/licencegplv31.svg"></img> <img src="https://ziadoua.github.io/m3-Markdown-Badges/badges/Rust/rust1.svg"></img> <img src="https://ziadoua.github.io/m3-Markdown-Badges/badges/Windows/windows1.svg"></img> <img src="https://ziadoua.github.io/m3-Markdown-Badges/badges/macOS/macos1.svg"></img> <center>+ GNU/Linux</center>
</div>

## 🚀 What is RustPack?

RustPack is a powerful tool that simplifies cross-platform Rust application distribution. Build once, run anywhere - without requiring your users to have Rust installed.

```bash
rustpack -i ./my-project -o my-awesome-app.rpack -t x86_64-apple-darwin,x86_64-pc-windows-msvc,x86_64-unknown-linux-gnu
```

## ✨ Features

- **Single Executable** - Package your Rust application as a standalone executable
- **Cross-Platform** - Automatically detects and runs the right binary for the user's platform
- **Zero Dependencies** - Users don't need Rust or any other dependencies installed
- **Multiple Architectures** - Build for various platforms in one operation
- **Offline Execution** - Apps run without requiring network connectivity

## 🛠️ Installation

```bash
git clone https://github.com/neoapps-dev/rustpack.git
cd rustpack
cargo build --release
cd target/release
pwd
# add the printed path to $PATH
```

## 📋 Usage

### Basic Usage

```bash
rustpack -i path/to/your/project -o output_name.rpack
```

### Specify Target Platforms

```bash
rustpack -i . -o myapp.rpack -t x86_64-apple-darwin,aarch64-apple-darwin,x86_64-unknown-linux-gnu
```

### Run Your Packaged App

```bash
./myapp.rpack
```

## 🔍 How It Works

RustPack creates a self-extracting archive with a smart bootstrap script that:

1. Detects the user's platform and architecture
2. Extracts the appropriate binary
3. Executes it with all command-line arguments passed through

## 📊 Supported Platforms

- 🍎 macOS (x86_64, aarch64)
- 🐧 GNU/Linux (x86_64, aarch64, arm, x86)
- 🪟 Windows (x86_64, x86)

## 🤝 Contributing

Contributions are welcome! Feel free to open an issue or submit a pull request.

## ❤️ Special Thanks

- [The Rust Programming Language](https://rust-lang.org)
- [SyntaxSpin](https://github.com/syntaxspin) for the logo :)

## 📝 License

This project is licensed under the GNU GPL-3.0 License - see the [LICENSE](LICENSE) file for details.

<img src="https://m3-markdown-badges.vercel.app/stars/1/1/neoapps-dev/rustpack"> <img src="https://m3-markdown-badges.vercel.app/issues/1/1/neoapps-dev/rustpack">
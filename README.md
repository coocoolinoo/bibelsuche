# Bibelsuche

Bibelsuche is a desktop app for macOS and Windows that helps users search Bible verses across translations with a fast and clean interface.

## Development

### Prerequisites

- Node.js 20+
- Rust toolchain (`rustup`)
- Tauri prerequisites for your OS

### Run in development mode

```bash
npm install
npm run tauri dev
```

### Build production app

```bash
npm run tauri build
```

Build outputs:

- macOS: `src-tauri/target/release/bundle/macos/*.app` and `src-tauri/target/release/bundle/dmg/*.dmg`
- Windows (run on Windows): `src-tauri\target\release\bundle\msi\*.msi` or `src-tauri\target\release\bundle\nsis\*.exe`

## Project Structure

```text
bibelsuche/
|- src/                 # React frontend (UI, styles, app logic)
|- public/              # Static frontend assets
|- src-tauri/           # Tauri + Rust backend and desktop packaging config
|- Zefania XML Bibelübersetzungen/  # Translation source files
|- build-windows.bat    # Windows build helper script
|- package.json         # Frontend scripts and dependencies
|- README.md
`- LICENSE
```

## License

This project is licensed under the MIT License. See the `LICENSE` file for details.

# Bibelsuche (Tauri + React + Typescript)

## macOS Build

```bash
npm install
npm run tauri build
```

Output:
- `src-tauri/target/release/bundle/dmg/*.dmg`
- `src-tauri/target/release/bundle/macos/*.app`

## Windows Build (auf Windows ausfuehren)

Einfach `build-windows.bat` per Doppelklick starten.

Oder manuell:

```bat
npm install
npm run tauri build
```

Output:
- `src-tauri\target\release\bundle\msi\*.msi`
- oder `src-tauri\target\release\bundle\nsis\*.exe`

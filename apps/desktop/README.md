# MAOF_G4 desktop

The TypeScript/Vite UI calls Rust through Tauri. From this directory:

```powershell
npm ci
npm run tauri dev
```

See the root [README](../../README.md), [architecture](../../doc/ARCHITECTURE.md)
and [testing](../../doc/TESTING.md). Demo mode needs no credentials.

Validate with `npm run build`. Build the executable using
`npm run tauri build -- --debug --no-bundle`; output is under `../../target/debug`.
`npm run test:ui` needs Microsoft Edge and the built Rust `ui_bridge` example.

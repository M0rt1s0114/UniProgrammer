# UniProgrammer (3.Software)

Frontend + Tauri backend workspace.

- Frontend: **Tauri 2** + **Vue 3** + **TypeScript** + **Pinia**
- Backend: Rust 模块源在 `modules/`，由 `tools/assemble.mjs` 组装到 `build/<profile>/src-tauri`
- UI build: `npm run build`
- Lint: `npm run lint` / format: `npm run format`
- Full release build: `npm run dist:libusb`（GPL 版）/ `npm run dist:dll`（本地版）
- Release outputs: `dist/<profile>/installer/`、`dist/<profile>/portable/`、`dist/<profile>/packages/`
- Dev wrapper: `npm run tauri -- dev --profile desktop-tauri-libusb`

See repository root documentation for project-level notes.

# Obsidian Local Sync Server (Tauri)

This is the background REST/WebSocket server for the Obsidian Local Sync Custom Plugin. It provides an SQLite database and a real-time WebSocket connection to synchronize Obsidian vaults across devices over a local network (Wi-Fi or Bluetooth Tethering).

## 🚀 Download & Install
1. Go to the [Releases](https://github.com/minseokk7/obsidian-local-sync-server/releases) page.
2. Download the `.exe` (NSIS) or `.msi` Windows installer.
3. Run the installer and launch the app.
4. Set the desired port and click **Start Server**.

## 🔌 Connecting your devices
Once the server is running on your desktop, note your desktop's local IP address (e.g. `192.168.0.39`).
Then, install the **Obsidian Local Sync Custom Plugin** on your other devices (laptop, phone, etc.) and enter `ws://<YOUR_IP>:8080/ws` in the plugin settings to connect.

## Development Setup
```bash
# Install dependencies
pnpm install

# Run in dev mode
pnpm tauri dev

# Build the release executable
pnpm tauri build
```

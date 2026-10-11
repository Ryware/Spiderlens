# Opening unsigned test builds

Builds from CI are not code-signed yet (signing arrives in milestone M3), so
each operating system warns before opening them. These steps allow the one app
without changing system-wide security settings.

Download the artifact for your system from the CI run of the change:
`spiderlens-desktop-<system>` for the app, `spiderlens-cli-<system>` for the
command-line tool.

## Windows

1. Unzip the artifact and run `SpiderLens_<version>_x64-setup.exe`.
2. If SmartScreen shows "Windows protected your PC", click **More info**, then
   **Run anyway**.
3. The app installs for your user only and appears in the Start menu as
   SpiderLens.

## macOS

Pick `macos-arm64` for Apple Silicon (M1 and later) or `macos-x64` for Intel.

1. Unzip the artifact, open the `.dmg` and drag SpiderLens to Applications.
2. Open SpiderLens once. macOS says it cannot verify the developer; click
   **Done**.
3. Open **System Settings → Privacy & Security**, scroll to Security, and click
   **Open Anyway** next to the SpiderLens message. Confirm with your password.

This allows only SpiderLens. For the CLI, run
`xattr -d com.apple.quarantine ./spiderlens` once in the folder you extracted it
to.

## Linux

1. Unzip the artifact.
2. `chmod +x SpiderLens_<version>_amd64.AppImage`
3. Run it. If it reports a missing FUSE library, either install `libfuse2`
   (Ubuntu 22.04+: `sudo apt install libfuse2t64` or `libfuse2`) or run with
   `APPIMAGE_EXTRACT_AND_RUN=1`.

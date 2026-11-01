# IPTV Player (Rust)

## Windows prerequisites

- Rust with Cargo and the MSVC build tools. Install Rust using
  [rustup](https://rustup.rs/) and follow its Windows prerequisite prompts.
- A Windows libmpv build matching the Rust application's architecture
  (normally x86-64).
- Windows `tar.exe` or 7-Zip for `.7z` downloads. The download script detects
  7-Zip on PATH or in its default installation directory, then falls back to tar.

## Get libmpv

The mpv binaries are intentionally excluded from Git. The local DLL is about
117 MiB, exceeding GitHub's 100 MiB regular-file limit.

From the workspace root in PowerShell:

```powershell
.\download-libmpv.ps1
```

The script downloads the following fixed third-party build from
[shinchiro's release 20261101](https://github.com/shinchiro/mpv-winbuild-cmake/releases/tag/20261101),
a provider linked on [mpv's installation page](https://mpv.io/installation/):

- Architecture: x86-64 (not the x86-64-v3 variant).
- mpv Git revision: `48fca90e8c`.
- Archive: `mpv-dev-x86_64-20261101-git-48fca90e8c.7z`.
- Archive SHA-256:
  `30d3f9fa121ce24638610a9ae52f8957743f9dea416dd37dc4ff7e43589ef843`.

It verifies the checksum before extraction and installs only `libmpv-2.dll`
into `libs/mpv/` at the repository root, independent of the current working directory. It does
not follow the latest release or silently replace an existing DLL. To replace
your current DLL with the pinned build:

```powershell
.\download-libmpv.ps1 -Force
```

The download and extraction have been verified on Windows; playback with this
specific build has not yet been tested. For another architecture, obtain a
matching libmpv archive manually from the provider and extract `libmpv-2.dll`
into `libs/mpv/`. A package containing only `mpv.exe` is not sufficient.

The Rust binding loads the DLL dynamically; `libmpv.dll.a` is not required.
If the chosen build requires additional runtime DLLs, install them according
to that provider's instructions.

To update the pin, change the release, asset name, and expected archive SHA-256
together in `download-libmpv.ps1`, verify the new download, and update this README.

## Build and run

From the workspace root in PowerShell:

```powershell
cargo run --locked
```

The app looks for `libmpv-2.dll` beside its executable first, then falls back to
`libs/mpv/libmpv-2.dll` in the build-time Rust project directory. Having the DLL
in `libs/mpv/` is sufficient for local development.

## Distribute a release

From the repository root:

```powershell
cargo build --release --locked
Copy-Item libs/mpv/libmpv-2.dll target/release/libmpv-2.dll -Force
```

Distribute `target/release/iptv-player.exe` together with `libmpv-2.dll` in the
same directory, plus any additional runtime dependencies required by the build.
The copy command assumes Cargo's default output directory; adjust it if using
`CARGO_TARGET_DIR` or an explicit `--target`.

The existing build script attempts to copy the DLL, but its output path does
not currently point to the executable directory. Use the explicit copy above
when packaging; it also replaces an old DLL after an upgrade.

Before redistributing mpv and its dependencies, review the selected build's
license and comply with its notice and source-distribution requirements.

## Troubleshooting

- **Cannot load DLL:** check the filename, location, architecture, and any
  additional DLL dependencies required by the downloaded build.
- **Missing symbol:** use a libmpv build providing both the client API and the
  OpenGL render API, rather than a standalone player executable.
- **An older DLL is still loading:** replace the copy beside the executable;
  it takes precedence over the copy in `libs/mpv/`.
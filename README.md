<h1 align="center">Spotifast Ninetale</h1>

![Spotifast Ninetale Home: the library on the left, the search box over dithered album art, shelves of playlists, and the player card at the bottom](docs/screenshot-ninetale.png)


**[Download the latest release](https://github.com/kycloudy/spotifast-ninetale/releases/latest)**

Based on Spotifast by Carmine Paolino. [MIT license](LICENSE).

Automatic update checks and downloads use this fork's
[GitHub releases](https://github.com/kycloudy/spotifast-ninetale/releases).

The sidebar wordmark pauses while the window is unfocused and resumes from
the same frame. Full-screen lyrics sit in a rounded, inset frame with a
dithered background drawn from the playing song's cover.

The sidebar toggle sits beside the wordmark, aligned with the Home icon below,
and stays in the same spot when hidden. Leaving Home clears its search, so
returning shows the Home shelves again.

## Build on Windows

These steps use PowerShell. They build `spotifast.exe` from source on
Windows 10 or 11, x64 or ARM.

### 1. Install the required tools

Run these commands once. Accept any prompts from the installers.

```powershell
winget install --id Git.Git -e
winget install --id Rustlang.Rustup -e
winget install --id Microsoft.VisualStudio.2022.BuildTools -e --override "--wait --passive --add Microsoft.VisualStudio.Workload.VCTools --includeRecommended"
```

The last command installs the MSVC C++ build tools and the Windows SDK.
Rust uses them to link the program.

Close PowerShell and open a new window, so that the new `PATH` applies.
Then get the source:

```powershell
git clone https://github.com/kycloudy/spotifast-ninetale
cd spotifast-ninetale
```

`rust-toolchain.toml` sets the Rust version. Cargo installs that version
the first time you build.

### 2. Quick build (without the MilkDrop visualizer)

This build needs only the tools from step 1.

```powershell
cargo build --release --no-default-features
```

The program is `target\release\spotifast.exe`. The first build takes
several minutes.

### 3. Full build (with the MilkDrop visualizer)

MilkDrop uses libprojectM, a C++ library that the build compiles from
source. It needs CMake, LLVM (for libclang), and the GLEW library from
vcpkg. Install them once:

```powershell
winget install --id Kitware.CMake -e
winget install --id LLVM.LLVM -e
git clone https://github.com/microsoft/vcpkg C:\vcpkg
C:\vcpkg\bootstrap-vcpkg.bat
C:\vcpkg\vcpkg install glew:x64-windows-static
```

On a Windows on ARM PC, use `glew:arm64-windows-static` instead.

Tell the build where vcpkg and libclang are. These commands save the
values for your user account:

```powershell
[Environment]::SetEnvironmentVariable("VCPKG_INSTALLATION_ROOT", "C:\vcpkg", "User")
[Environment]::SetEnvironmentVariable("LIBCLANG_PATH", "C:\Program Files\LLVM\bin", "User")
```

Open a new PowerShell window, go to the `spotifast-ninetale` folder, and
build:

```powershell
cargo build --release
```

The program is `target\release\spotifast.exe`. The first full build takes
longer, because it also compiles libprojectM.

### If the build fails

**`linker 'link.exe' not found`**: Rust cannot find the MSVC linker.

1. Make sure that the Build Tools have the C++ workload. Run the
   `winget install` command for `Microsoft.VisualStudio.2022.BuildTools`
   from step 1 again. If the Build Tools are already installed, open
   **Visual Studio Installer**, select **Modify** on Build Tools 2022, and
   select **Desktop development with C++**.
2. Close all PowerShell windows and open a new one.
3. Run `rustup show`. The default host must end in `-pc-windows-msvc`.
   If it ends in `-pc-windows-gnu`, run
   `rustup set default-host x86_64-pc-windows-msvc` (or
   `aarch64-pc-windows-msvc` on ARM), then build again.
4. If the error continues, open **Developer PowerShell for VS 2022** from
   the Start menu and build from there.

**`VCPKG_INSTALLATION_ROOT is not set`** or **GLEW not found**: do the
vcpkg steps in step 3, then open a new PowerShell window.

**`Unable to find libclang`**: install LLVM and set `LIBCLANG_PATH` as in
step 3.

**`could not find any instance of Visual Studio`** (from CMake): the full
build uses the Visual Studio 2022 generator by default. If you have a
different Visual Studio version, open its Developer PowerShell, run
`$env:CMAKE_GENERATOR = "Ninja"`, and build again.

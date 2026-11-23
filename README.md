# Java Control Centre (JCC)

A lightweight desktop control centre for observing and managing Java runtimes. JCC discovers installed JDKs, lists and manages JVM processes, samples live counters with `jcmd` (no agent in the target JVM), explores threads/classes/JIT/memory/GC, parses real HPROF heap dumps with reachability and dominators, and probes Spring Boot Actuator endpoints.

Built with **Tauri 2 + Rust + vanilla TypeScript + ECharts + SQLite**.

> Why vanilla TypeScript? The brief highlighted lightweight cross-platform stacks such as Svelte or SolidJS. Vanilla TS keeps the renderer dependency-free apart from ECharts and produces a small, fast bundle while retaining the same responsive UI capabilities.

## Features

| Module | What it does |
| --- | --- |
| Overview | Live snapshot of the selected JVM with heap history and recent GC events |
| JVM Monitor | PerfCounter-based CPU/heap/threads/classes/GC/JIT sampling with live charts |
| Memory Explorer | Heap generations, metaspace, compressed class space, code cache and memory pools |
| GC Monitor | Collection counts, cumulative pause time, detected GC deltas and reclaimed bytes |
| Thread Explorer | `Thread.print` stacks, state distribution, CPU time, expandable frames and deadlock detection |
| Class Loading | `VM.classloader_stats` loader tree and `GC.class_histogram` class histogram |
| JIT Explorer | Code heaps, compiler queue, compiled methods, bailouts and invalidations |
| Heap Analyzer | Real HPROF parsing, strong/weak reachability, dominant tree and retained sizes |
| Spring Boot Monitor | Actuator discovery, health, info, links and key JVM/HTTP metrics |
| JVM Processes | Process discovery, graceful/force shutdown, JFR control and runtime details |
| JDK Manager | Discover JDKs from `JAVA_HOME`, `PATH`, common install locations and running JVMs |
| Settings | Sample interval, retention, preferred JDK, local storage and pruning |

## How monitoring works

JCC never injects an agent into a target JVM. It uses:

- `jcmd <pid> PerfCounter.print` for heap generations, threads, classes, GC and JIT counters
- `jcmd <pid> Thread.print`, `VM.classloader_stats`, `GC.class_histogram`, `Compiler.codelist`, `Compiler.queue`, `CodeCache` and the JFR commands
- OS process metrics via `sysinfo`

HPROF files are parsed by a genuine two-pass streaming parser (`reader.rs` / `parse.rs`), followed by reachability analysis with weak-reference handling and a Lengauer–Tarjan dominator computation for retained sizes (`analysis.rs`). Very large dumps (> 1.5 M objects) skip dominator computation to stay interactive.

## Requirements

### Runtime

- **Windows 10/11**: nothing to install manually — the NSIS installer detects a JDK and installs Microsoft OpenJDK 21 (or Eclipse Temurin 21) if none is present. WebView2 is handled by the Tauri bootstrapper.
- **Linux**: WebView2 equivalent (`webkit2gtk-4.1` / `libayatana-appindicator-4` / `gtk3`) plus a JDK 17+ for `jcmd`. The `.deb` and AUR package declare these dependencies.
- **JDK 17+** is required for the full feature set. JCC still starts without Java, but process discovery, monitoring and heap dumps need `jcmd`.

### Building from source

- Rust stable (1.77+; developed with 1.99)
- Node.js 20+ and npm
- Tauri 2 prerequisites for your platform: <https://v2.tauri.app/start/prerequisites/>
- **Rust is build-time only.** Released installers do not require it.

```bash
npm install
npm run tauri:dev       # development
npm run tauri:build     # release bundles
npm run check           # TypeScript only
cargo test --lib        # from src-tauri/
```

## Installation

### Windows

Download `Java Control Centre_<version>_x64-setup.exe` from the Releases page and run it. The installer:

1. checks for `jcmd` on `PATH` or in `JAVA_HOME`;
2. if missing, tries `winget install Microsoft.OpenJDK.21`;
3. otherwise downloads and silently installs Eclipse Temurin 21 from Adoptium;
4. installs and starts JCC.

If automatic Java installation fails, the installer tells you what to install and still completes the app installation.

### Linux

- **Kali / Debian / Ubuntu**: install the `.deb` package.
- **Arch / Omarchy**: use the one-line installer, the binary AUR package, or build from source:

  ```bash
  curl -fsSL https://raw.githubusercontent.com/saswinkumar017/java-ctrl/master/installers/omarchy/install.sh | bash
  # or: JCC_VERSION=0.1.0 ./installers/omarchy/install.sh
  # or: yay -S java-control-centre-bin     # packaging/aur/PKGBUILD-bin
  ```

  The installer downloads the prebuilt AppImage from Releases, installs `webkit2gtk-4.1`, `gtk3`, `libayatana-appindicator` and `fuse2` with `pacman`, installs `jdk21-openjdk` if `jcmd` is missing, then registers a desktop entry and a `~/.local/bin/java-control-centre` launcher. No Rust or Node.js needed.

- **Any distro**: the AppImage is portable; install `webkit2gtk-4.1` and `libayatana-appindicator-4` if your distribution does not already provide them.

## Data and privacy

- All samples, GC events, dump metadata and settings are stored in a local SQLite database under the OS application data directory (e.g. `%APPDATA%\dev.javacontrolcentre.app` on Windows).
- Heap dumps and JFR recordings are written to the `dumps` subdirectory.
- Spring connection passwords are currently stored **unencrypted** in that local database. Use a dedicated, low-privilege monitoring account and restrict access to your user profile. A future release will move these to the platform credential store.
- No telemetry is sent anywhere.

## Demo JVM

A dependency-free demo app is included to exercise monitoring, GC detection, threads, heap dumps and Actuator probing:

```bash
javac demo/DemoApp.java -d demo
java -Dserver.port=8080 -Xmx256m -cp demo DemoApp
```

It allocates continuously and exposes a small Actuator-compatible endpoint on `http://localhost:8080/actuator`. Select the `DemoApp` process in JCC, then use **Discover** on the Spring Boot Monitor page.

For a **deterministic memory test**, use `demo/jcc-memory-lab` instead: it retains a known payload (50 MiB by default), allocates known garbage (2 MiB/s), instantiates eight beans through a reflection-based DI container, runs named threads and exposes custom `jcc.lab.*` Actuator metrics. See `demo/jcc-memory-lab/README.md` for the expected-heap arithmetic and which JCC page should show each effect.

## Limitations

- PerfCounter does not expose an installed-nmethod count, so JCC derives it from `CodeCache` output (`nmethods=`).
- GC events are detected only while JCC is sampling; for full histories use JFR or GC logs.
- `GC.class_histogram` can briefly safepoint the target JVM.
- HPROF analysis is performed entirely in memory; multi-gigabyte dumps need a machine with sufficient RAM.
- Weakly reachable objects are reported separately and are excluded from the strong-reachability retained-size tree.

## Repository layout

```text
src/                     Vanilla TypeScript UI (pages, charts, styles)
src-tauri/src/core/      Rust domain modules (jdk, process, monitor, hprof, spring, storage...)
src-tauri/src/commands.rs Tauri command surface
src-tauri/windows/       NSIS prerequisite hooks
packaging/aur/           AUR PKGBUILD templates (source + binary)
installers/omarchy/      One-line Omarchy / Arch AppImage installer
.github/workflows/       Release build matrix
```

## License

MIT — see [LICENSE](LICENSE).

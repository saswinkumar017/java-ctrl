# JCC Memory Lab

A deterministic Java workload designed to make Java Control Centre's graphs easy to interpret. It allocates a **known retained payload**, produces **known garbage**, wires its services through a **real dependency-injection container**, exposes named threads, and serves an **Actuator-compatible endpoint** with custom `jcc.lab.*` metrics.

## Build and run

```bash
javac -d out (Get-ChildItem -Recurse -Filter *.java src).FullName   # PowerShell
# or: javac -d out $(find src -name '*.java')                       # Bash

java -Xms256m -Xmx512m -XX:+UseG1GC -Dserver.port=8081 \
  -cp out dev.jcc.lab.LabApp \
  --block-kib 256 --retained-blocks 200 --churn-blocks 4 --threads 2
```

`--help` lists all options. The JVM stays up until stopped; use `--duration-secs 120` for a bounded run.

## What to expect in JCC

| JCC page | What you should see |
| --- | --- |
| **Overview / JVM Monitor** | CPU near zero, heap sawtooth between ~65 MiB and ~210 MiB, GC collections increasing |
| **Memory Explorer** | Eden/Survivor/Old usage, metaspace from all loaded `dev.jcc.lab.*` classes |
| **GC Monitor** | Collection events with reclaimed bytes roughly equal to `churn block size × blocks × ticks` |
| **Thread Explorer** | `jcc-lab-churn-1`, `jcc-lab-churn-2`, `jcc-lab-report` as user threads |
| **Class Loading** | `dev.jcc.lab.Container`, `Allocator`, `MemoryStore`, `Actuator`, … in the class histogram, created through reflection by DI |
| **JIT Explorer** | JIT compiles as the reflection and JSON paths warm up |
| **Spring Boot Monitor** | Probe `http://localhost:8081/actuator`, health `UP`, custom `jcc.lab.*` metrics |
| **Heap Analyzer** | Capture a heap dump and find ~200 retained `byte[]` blocks of 256 KiB (`~50 MiB`) |

## Expected heap arithmetic

With the defaults (`--block-kib 256 --retained-blocks 200`):

```
retained payload = 200 × 256 KiB            = 51,200 KiB = 50 MiB
array headers   = 200 × ~32 B               ≈ 6 KiB
DI + JVM base   = loaded classes, JIT, threads, HTTP server ≈ 15–25 MiB
---------------------------------------------------------------
expected steady heap after GC               ≈ 65–80 MiB
churn               = 4 blocks × 256 KiB × 2 ticks/s = ~2 MiB/s garbage
peak heap before GC ≈ 50 MiB + churn accumulated since last collection
```

The app prints its own accounting every two seconds:

```
heap used=74 MiB / max=512 MiB | retained=50 MiB (200 blocks) | allocated total=212 MiB | churn=162 MiB | cycles=162
```

`retained` stays flat (strong references held by `MemoryStore`), `allocated total` and `churn` grow monotonically, and `heap used` drops after each GC — this is the exact shape JCC's charts should show.

## Dependency injection

`dev.jcc.lab.Container` is a small reflection-based DI container:

- `@Component` / `@Singleton` declare beans
- `@Inject` injects fields, optionally with an implementation class (`@Inject(SystemClock.class) Clock`)
- `Container.init()` eagerly instantiates all eight beans, so the class loader and JIT see them immediately

All allocator threads and the actuator are injected services, not globals. That means DI-allocated classes and their allocations are visible in JCC's Class Loading, Thread Explorer, and heap histogram views.

## Verify from the command line

```bash
jcmd <pid> VM.classloader_stats | Select-String 'dev.jcc.lab'
jcmd <pid> Thread.print | Select-String 'jcc-lab'
curl http://localhost:8081/actuator/jcc/lab
```

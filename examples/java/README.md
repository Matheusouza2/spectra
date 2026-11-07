# RACoN Java example

A standalone (no GUI) Java program that drives a full RACoN analysis. It is
the stage-1 acceptance example from the HTTP/JSON spec: Java only knows the JNI
bootstrap plus the versioned HTTP contract.

## Build the native bridge

```bash
cargo build -p racon-jni --release
```

This produces `target/release/libracon_jni.so`.

## Compile and run

```bash
javac -d examples/java/out examples/java/racon/*.java
java -Djava.library.path=target/release -cp examples/java/out racon.RaconExample
```

Pass a MercuryScript path as the first argument to analyze your own model;
without an argument a small built-in cyclic model is used.

## What it demonstrates

1. `RaconRuntime.startServer()` via JNI (returns JSON `ServerInfo`).
2. `GET /v1/health`.
3. `POST /v1/nets/load` with a MercuryScript path → `session_id`.
4. `GET /v1/sessions/{id}/properties` → query `ORD`.
5. `POST /v1/sessions/{id}/state-space` → `analysis_id`.
6. `GET /v1/analyses/{id}/status` polling until the job finishes.
7. Query behavioral properties (`REV`) and `P`/`T` invariants.
8. `RaconRuntime.stopServer()`.

Java never sees Rust pointers, internal structures or Rayon internals.

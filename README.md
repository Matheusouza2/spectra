# RACoN — Reachability And Coverability Net Analyzer

RACoN is a desktop application for modeling and analyzing Petri nets. It brings
together a React interface, a Tauri desktop shell, and a Rust analysis engine to
help inspect a net's structure, explore its behavior, and check formal
properties.

## What you can do

- Inspect structural and behavioral properties of a Petri net.
- Calculate place and transition invariants and view the incidence matrix.
- Explore reachability graphs and coverability trees or sets.
- Check CTL formulas and query bad states.
- Review evidence, paths, and counterexamples when available.
- Play through enabled transitions in an interactive token game.
- Use the interface in English or Portuguese, with light and dark themes.

Results depend on the net class and selected algorithm. The algorithm catalog
identifies experimental methods and documents their preservation limits.

## How it works

The React frontend presents the net and its analysis tools. In the desktop app,
Tauri connects the interface to native Rust operations through IPC commands.
The Rust workspace separates the domain and algorithms into crates:
`racon-core` defines nets, markings, and mathematical structures; other crates
provide parsing, properties, invariants, state-space exploration, coverability,
CTL, reductions, evidence, an HTTP API, a CLI, and JNI integration.

A typical workflow is to load a net, inspect its properties, then select an
exploration or verification task. The engine validates the input, computes the
requested results, and returns tables, graphs, paths, or evidence to the UI.

### Input formats

The desktop interface opens `.pnt` files and accepts compatible Mercury scripts
as pasted text. The converter preserves supported extended features and rejects
timed GSPN semantics when converting them to PNT would lose information. The
Rust `racon-parsers` crate also provides PNML, PNT, and MercuryScript parsers;
not every parser format is currently available in the desktop file picker.

## Repository layout

- `src/` — React interface, components, IPC and mock services, translations,
  and styles.
- `src-tauri/` — Tauri desktop application and native commands.
- `crates/` — Rust engine, parsers, algorithms, HTTP API, CLI, and bindings.
- `tests/models/` — Mercury model corpus and expected results for behavior tests.
- `examples/java/` — Java integration examples for the RACoN service.
- `website/` — static bilingual project page deployed with GitHub Pages.

## Requirements

- Rust and Cargo with Rust 2021 edition support.
- Node.js and npm.
- Tauri's native development prerequisites for your operating system.

## Development

From the repository root:

```bash
# Install frontend dependencies
npm install

# Run the frontend in a browser without the Tauri backend
npm run dev

# Build the frontend assets
npm run build

# Run the desktop application (frontend and Rust/Tauri backend)
npm run tauri dev
```

To work on the Rust workspace:

```bash
cargo check --workspace
cargo test --workspace
cargo build --release -p racon-cli -p racon-jni
```

## Local HTTP service

The `racon-cli` crate starts a versioned HTTP service under `/v1/`. It binds to
loopback (`127.0.0.1`) and selects an available port by default. Use `--json` to
print connection details for another process to consume.

```bash
cargo run -p racon-cli -- start-server --json
```

The process stays active until Ctrl+C. `/v1/health` reports service status.
Analysis routes cover net loading, properties, invariants, state-space
exploration, the algorithm catalog, job status, and cancellation.

## Downloads and GitHub Pages

Pushing to `main` builds Windows, Linux, and macOS bundles and publishes them
as assets on a GitHub Release. The same workflow deploys the bilingual project
page to GitHub Pages. Enable Pages once in the repository settings under
**Settings → Pages → Build and deployment → Source → GitHub Actions**.

The macOS bundle uses an ad-hoc signature and is not notarized with Apple.
macOS may still ask you to allow the app in Privacy & Security before opening
it.

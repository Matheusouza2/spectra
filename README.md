# SPECTRA

**Reachability And Coverability Net Analyzer**

SPECTRA is a free desktop workbench for modeling and analyzing Petri nets. It
combines a React interface, a Tauri desktop shell, and a Rust analysis engine
(the RACoN engine) to inspect a net's structure, explore its behavior, and
check formal properties.

[Website](https://matheusouza2.github.io/spectra/) ·
[Download](https://github.com/Matheusouza2/spectra/releases/latest) ·
[Report an issue](https://github.com/Matheusouza2/spectra/issues)

## What you can do

- **Analysis:** inspect structural and behavioral properties of a net.
- **Invariants:** calculate place and transition invariants and view the
  incidence matrix.
- **Graphs:** explore reachability graphs and coverability trees or sets.
- **CTL and bad states:** check CTL formulas and query bad states.
- **Evidence:** review witnesses, paths, and counterexamples when available.
- **Reductions:** use stubborn sets, symmetries, and structural reductions to
  keep the state space tractable.
- **Token game:** play through enabled transitions interactively.
- Use the interface in English or Portuguese, with light and dark themes.

Results depend on the net class and the selected algorithm. The algorithm
catalog identifies experimental methods and documents their preservation
limits.

## Install

Download the installer for your system from the
[latest release](https://github.com/Matheusouza2/spectra/releases/latest):

| System  | Package                          |
| ------- | -------------------------------- |
| Windows | `.msi` or `.exe` (64-bit)        |
| macOS   | `.dmg` (Apple Silicon)           |
| Linux   | `.deb` or `.AppImage` (64-bit)   |

The installers are not code-signed yet. Windows SmartScreen may warn you on
first run. The macOS bundle uses an ad-hoc signature and is not notarized, so
macOS may ask you to allow the app under **Privacy & Security** before opening
it.

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

The desktop interface opens `.pnt` files (the INA net format) and accepts
compatible [Mercury](#references) scripts as pasted text. The converter
preserves supported extended features and rejects timed GSPN semantics when
converting them to PNT would lose information. The Rust `racon-parsers` crate
also provides PNML, PNT, and MercuryScript parsers; not every parser format is
currently available in the desktop file picker.

## Repository layout

- `src/` — React interface, components, IPC and mock services, translations,
  and styles.
- `src-tauri/` — Tauri desktop application and native commands.
- `crates/` — Rust engine, parsers, algorithms, HTTP API, CLI, and bindings.
- `tests/models/` — Mercury model corpus and expected results for behavior tests.
- `examples/java/` — Java integration examples for the RACoN service.
- `website/` — static bilingual project page deployed with GitHub Pages.

## Build from source

Requirements:

- Rust and Cargo with Rust 2021 edition support.
- Node.js 20 or later and npm.
- Tauri's native prerequisites for your operating system. On Debian and
  Ubuntu: `build-essential curl file libayatana-appindicator3-dev
  librsvg2-dev libssl-dev libwebkit2gtk-4.1-dev patchelf`.

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

# Build an installer for your system
npm run tauri build
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

## Releases and GitHub Pages

Installers are built by GitHub Actions only when a version tag is pushed. To
publish a release:

```bash
# 1. Set the new version in src-tauri/tauri.conf.json (and src-tauri/Cargo.toml)
git commit -am "chore: release X.Y.Z"
git tag vX.Y.Z
git push origin main
git push origin vX.Y.Z
```

The workflow builds the Windows, Linux, and macOS bundles and attaches them to
a GitHub Release. It fails if the tag does not match the version in
`src-tauri/tauri.conf.json`. A tag push can also be re-run manually with
`gh workflow run main.yml --ref vX.Y.Z`.

The project page in `website/` is deployed by a separate workflow whenever it
changes on `main`, and again when a release is published. Enable Pages once
under **Settings → Pages → Build and deployment → Source → GitHub Actions**.

## License

SPECTRA is free software licensed under the
[GNU Affero General Public License v3.0 or later](LICENSE) (AGPL-3.0-or-later).
You may use, study, modify, and redistribute it. If you distribute a modified
version, or offer one to users over a network, you must make the complete
corresponding source code available under the same license. Third-party
dependencies keep their own licenses.

Commercial licensing for use outside these terms is available from the author.

## How to cite

SPECTRA was developed as part of academic research. If you use it in a
publication, thesis, or derived work, please cite the associated dissertation
or article (see [`CITATION.cff`](CITATION.cff); GitHub shows a "Cite this
repository" button), in addition to the software itself. Plain-text and
BibTeX formats are generated from that file.

## References

SPECTRA reads Mercury scripts. If you use that support in your work, please
also cite the Mercury platform:

> T. Pinheiro, V. Almeida, M. Santana, J. Silva, M. Mialaret, and P. Maciel,
> "Mercury 6.1: a Unified Platform for Stochastic Modeling, Analysis, and
> Decision Support," in *2025 IEEE 36th International Symposium on Software
> Reliability Engineering Workshops (ISSREW)*, Los Alamitos, CA, USA: IEEE
> Computer Society, Oct. 2025, pp. 57–62.
> [doi:10.1109/ISSREW67781.2025.00046](https://doi.org/10.1109/ISSREW67781.2025.00046)

```bibtex
@inproceedings{pinheiro2025mercury,
  author    = {Pinheiro, Thiago and Almeida, Vinicius and Santana, Marcelo and Silva, Jonatas and Mialaret, Marco and Maciel, Paulo},
  booktitle = {2025 IEEE 36th International Symposium on Software Reliability Engineering Workshops (ISSREW)},
  title     = {{Mercury 6.1: a Unified Platform for Stochastic Modeling, Analysis, and Decision Support}},
  year      = {2025},
  pages     = {57--62},
  doi       = {10.1109/ISSREW67781.2025.00046},
  url       = {https://doi.ieeecomputersociety.org/10.1109/ISSREW67781.2025.00046},
  publisher = {IEEE Computer Society},
  address   = {Los Alamitos, CA, USA},
  month     = oct
}
```

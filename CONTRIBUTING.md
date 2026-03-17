# Contributing to Atlas

Thank you for your interest in contributing to Atlas! This guide will help you get started.

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/YOUR_USERNAME/atlas.git`
3. Create a feature branch: `git checkout -b feat/your-feature`
4. Make your changes
5. Run tests: `cargo test --workspace`
6. Run lints: `cargo clippy --workspace && cargo fmt --workspace -- --check`
7. Commit and push
8. Open a pull request

## Development Setup

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs/))
- Node.js 20+ (for frontend development)
- [pmtiles CLI](https://github.com/protomaps/go-pmtiles/releases) (for test data)

### First-time setup

```bash
# Build everything
cargo build --workspace

# Download test data (Ghana OSM + Accra tiles)
./scripts/download-test-data.sh

# Build geocoding indices
cargo run -p atlas-ingest -- --osm-dir ./data/osm --output-dir ./test-data
cargo run -p atlas-ingest -- --osm-dir ./data/osm --output-dir ./test-data --build-search-index

# Run tests
cargo test --workspace

# Start the server
cargo run -p atlas-server

# Start the frontend (separate terminal)
cd sdk/atlas-js && npm install && npm run dev
```

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix all warnings
- No inline comments unless the logic is non-obvious
- Prefer small, focused functions
- Use `thiserror` for error types
- Propagate errors with `?`, don't `unwrap()` on fallible operations

## Testing

- Write tests for new functionality
- Run the full test suite before submitting a PR: `cargo test --workspace`
- Integration tests for HTTP endpoints go in `crates/atlas-server/tests/`
- Unit tests go in the same file as the code they test (Rust convention)

## Commit Messages

Use the format: `type(scope): description`

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`, `perf`

Examples:
- `feat(atlas-geocode): add Hausa language support`
- `fix(atlas-route): handle disconnected road segments`
- `perf(atlas-tiles): reduce tile cache memory usage`

## Pull Requests

- Keep PRs focused on a single change
- Include tests for new functionality
- Update documentation if behavior changes
- Ensure CI passes (build, test, clippy, fmt)

## Areas for Contribution

- **Language support** — Add tokenizer rules for more African languages
- **Road profiles** — Improve routing profiles for African road conditions
- **Data sources** — Integrate additional African map data sources
- **Performance** — Optimize CH preprocessing, tile serving, geocoding
- **Frontend** — Improve the MapLibre UI, add mobile responsiveness
- **Documentation** — Improve API docs, add examples, translate docs

## Questions?

Open an issue on GitHub for questions, bug reports, or feature requests.

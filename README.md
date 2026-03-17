# Atlas

A Pan-African maps platform built entirely in Rust. Tile serving, geocoding, routing, and place search — optimized for African road networks, addressing patterns, and languages.

## Features

- **Tile Serving** — PMTiles vector tiles via S3 range requests with LRU cache
- **Geocoding** — Forward and reverse geocoding with multi-language support (English, French, Arabic, Swahili, Twi, Yoruba) and landmark-relative addressing ("near the MTN mast")
- **Routing** — Point-to-point routing with 4 profiles (car, motorcycle, bicycle, foot) and African road condition penalties (unpaved, seasonal closures)
- **Place Search** — POI discovery with distance scoring and category filtering
- **Turn-by-Turn Navigation** — Route instructions with simulated navigation camera
- **Community Contributions** — Users report wrong turns, road closures, and bad conditions. Reports feed back into routing as edge penalties.
- **Trip Telemetry & ETA Learning** — Collect real GPS traces from users who opt in. Aggregate actual travel speeds per road segment. ETAs improve over time based on what drivers actually experience, not just speed limits.
- **Auth & Rate Limiting** — DynamoDB API key authentication with per-key token bucket rate limiting
- **Metrics** — Prometheus endpoint with request counters, latency histograms, cache stats

## Quick Start

### Prerequisites

- Rust 1.75+ (`curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`)
- Node.js 20+ (for the frontend)
- [pmtiles CLI](https://github.com/protomaps/go-pmtiles/releases) (for downloading tile data)

### 1. Clone and build

```bash
git clone https://github.com/Augani/atlas.git
cd atlas
cargo build --release -p atlas-server -p atlas-ingest
```

### 2. Download test data

```bash
./scripts/download-test-data.sh
```

This downloads a small PMTiles extract for Accra and the Ghana OSM PBF (~105MB).

### 3. Build geocoding and search indices

```bash
cargo run --release -p atlas-ingest -- --osm-dir ./data/osm --output-dir ./test-data
cargo run --release -p atlas-ingest -- --osm-dir ./data/osm --output-dir ./test-data --build-search-index
```

### 4. Start the server

```bash
cargo run --release -p atlas-server
```

The server starts at `http://localhost:3001`. It loads tiles, geocoding index, search index, and builds a road graph from OSM data for on-demand routing.

### 5. Start the frontend

```bash
cd sdk/atlas-js
npm install
npm run dev
```

Open `http://localhost:5173` to see the map.

## API Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | `/v1/tiles/{tileset}/{z}/{x}/{y}.mvt` | Vector tiles |
| GET | `/v1/tiles/{tileset}/tilejson.json` | TileJSON metadata |
| GET | `/v1/geocode?q=Makola+Market` | Forward geocode |
| GET | `/v1/reverse?lat=5.55&lon=-0.21` | Reverse geocode |
| GET | `/v1/search?q=restaurant&lat=5.6&lon=-0.2` | Place search |
| POST | `/v1/route` | Point-to-point route |
| POST | `/v1/matrix` | N×M distance/duration matrix |
| POST | `/v1/contribute` | Report route issues |
| POST | `/v1/telemetry/start` | Start trip telemetry collection |
| POST | `/v1/telemetry/{id}/update` | Send GPS waypoints |
| POST | `/v1/telemetry/{id}/end` | End trip, trigger speed learning |
| GET | `/metrics` | Prometheus metrics |
| GET | `/health` | Health check |

### Route example

```bash
curl -X POST http://localhost:3001/v1/route \
  -H 'Content-Type: application/json' \
  -d '{"origin":{"lat":5.603,"lon":-0.187},"destination":{"lat":6.688,"lon":-1.624},"profile":"car"}'
```

## Project Structure

```
atlas/
├── crates/
│   ├── atlas-core/       # Shared types (Place, BBox, TileCoord, errors)
│   ├── atlas-tiles/      # Tile serving (PMTiles, S3, cache) + tile generator
│   ├── atlas-geocode/    # Geocoding engine (Tantivy, tokenizer, landmarks)
│   ├── atlas-search/     # POI search with distance scoring
│   ├── atlas-route/      # Routing (road graph, CH, Dijkstra, turn-by-turn)
│   ├── atlas-ingest/     # Data pipeline (Overture + OSM PBF)
│   ├── atlas-server/     # Axum HTTP server
│   └── atlas-cli/        # CLI pipeline orchestrator
├── sdk/atlas-js/         # React + MapLibre frontend
└── scripts/              # Data download scripts
```

## Configuration

All configuration is via environment variables. See [env.example](env.example) for the full list.

Key variables:

| Variable | Default | Description |
|----------|---------|-------------|
| `ATLAS_TILE_SOURCE` | `local` | `local` or `s3` |
| `ATLAS_TILE_DIR` | `./test-data` | PMTiles directory |
| `ATLAS_PORT` | `3001` | Server port |
| `ATLAS_OSM_DIR` | `./data/osm` | OSM PBF directory for routing |
| `ATLAS_AUTH_ENABLED` | `false` | Enable API key auth |

## Development

```bash
cargo test --workspace        # Run all tests
cargo clippy --workspace      # Lint
cargo fmt --workspace         # Format
cd sdk/atlas-js && npx tsc --noEmit  # Check frontend types
```

## Self-Hosting

### From Source (Recommended)

```bash
# 1. Install Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Install pmtiles CLI (for tile data)
# macOS: brew install pmtiles
# Linux: download from https://github.com/protomaps/go-pmtiles/releases

# 3. Clone and build
git clone https://github.com/Augani/atlas.git
cd atlas
cargo build --release -p atlas-server -p atlas-ingest

# 4. Download data for your region
./scripts/download-test-data.sh

# 5. Build indices
./target/release/atlas-ingest --osm-dir ./data/osm --output-dir ./test-data
./target/release/atlas-ingest --osm-dir ./data/osm --output-dir ./test-data --build-search-index

# 6. Run the server
./target/release/atlas-server
```

### Using Different Regions

Atlas defaults to Ghana data. To use a different African country:

1. Download the country's OSM PBF from [Geofabrik](https://download.geofabrik.de/africa.html):
   ```bash
   curl -L -o data/osm/kenya-latest.osm.pbf \
     https://download.geofabrik.de/africa/kenya-latest.osm.pbf
   ```

2. Download tiles for the region using pmtiles:
   ```bash
   pmtiles extract "https://build.protomaps.com/$(date +%Y%m%d).pmtiles" \
     test-data/kenya.pmtiles \
     --bbox="33.9,-4.7,41.9,5.5" \
     --maxzoom=15
   ```

3. Build indices and run:
   ```bash
   cargo run --release -p atlas-ingest -- --osm-dir ./data/osm --output-dir ./test-data
   cargo run --release -p atlas-server
   ```

### Production Checklist

- [ ] Enable API key auth: `ATLAS_AUTH_ENABLED=true` (requires DynamoDB table)
- [ ] Set `ATLAS_PUBLIC_URL` to your server's public URL
- [ ] Put behind a reverse proxy (nginx/Caddy) with TLS
- [ ] Set up monitoring on the `/metrics` endpoint
- [ ] For high traffic: preprocess contraction hierarchies for faster routing

### Hardware Requirements

| Data Scope | RAM | Disk | CPU |
|-----------|-----|------|-----|
| Single country (Ghana) | 2 GB | 10 GB | 2 cores |
| Regional (West Africa) | 4 GB | 30 GB | 4 cores |
| All of Africa | 8 GB | 50 GB | 4+ cores |

## Known Limitations

- **No live traffic data.** Routes are computed on static OSM road data. A road that's gridlocked or flooded right now looks the same as an empty one. The contribution API (`POST /v1/contribute`) is designed to close this gap — users can report road closures, bad conditions, and slow segments in real time, which can be fed back into route scoring over time.
- **OSM data quality varies by region.** Some African cities have excellent OpenStreetMap coverage, others don't. Atlas is only as good as the underlying data. Contributing to OSM directly improves Atlas for everyone.
- **Roundabout exit counting is approximate.** The roundabout detection uses OSM junction tags, which aren't always present or accurate. Community corrections help here too.

## Architecture

Atlas is built entirely in Rust for predictable latency, memory efficiency, and single-binary deployment.

Key design decisions:
- **Pure Rust** — No GC pauses, no JIT warmup, ~15-30MB binary
- **PMTiles** — Single-file tile archives with HTTP range requests
- **Tantivy** — Rust-native full-text search for geocoding
- **Contraction Hierarchies** — Precomputed routing for fast queries
- **On-demand Dijkstra** — A* routing fallback when CH not preprocessed

## License

[MIT](LICENSE)

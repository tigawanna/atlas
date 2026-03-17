#!/bin/bash
set -euo pipefail

DEST="test-data"
mkdir -p "$DEST"

# Download Accra PMTiles extract for tile serving
if [ -f "$DEST/accra.pmtiles" ]; then
    echo "test-data/accra.pmtiles already exists, skipping"
else
    echo "Downloading Accra area PMTiles extract..."
    PLANET_URL="https://build.protomaps.com/20260315.pmtiles"

    if command -v pmtiles &> /dev/null; then
        pmtiles extract "$PLANET_URL" "$DEST/accra.pmtiles" \
            --bbox="-0.30,5.50,-0.10,5.70" \
            --maxzoom=14
        echo "Done: $DEST/accra.pmtiles ($(du -h "$DEST/accra.pmtiles" | cut -f1))"
    else
        echo "pmtiles CLI not found. Install with: brew install pmtiles"
        echo "Or download from: https://github.com/protomaps/go-pmtiles/releases"
        exit 1
    fi
fi

# Download Ghana OSM PBF for geocoding data
OSM_DEST="data/osm"
mkdir -p "$OSM_DEST"

if [ -f "$OSM_DEST/ghana-latest.osm.pbf" ]; then
    echo "data/osm/ghana-latest.osm.pbf already exists, skipping"
else
    echo "Downloading Ghana OSM PBF..."
    curl -L -o "$OSM_DEST/ghana-latest.osm.pbf" \
        "https://download.geofabrik.de/africa/ghana-latest.osm.pbf"
    echo "Done: $OSM_DEST/ghana-latest.osm.pbf ($(du -h "$OSM_DEST/ghana-latest.osm.pbf" | cut -f1))"
fi

echo "All test data ready."

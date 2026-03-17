import { useState } from 'react'
import maplibregl from 'maplibre-gl'
import { MapView } from './Map'
import { SearchBar } from './SearchBar'
import { RoutePanel } from './RoutePanel'
import type { SelectedPlace } from './types'

export function App() {
  const [map, setMap] = useState<maplibregl.Map | null>(null)
  const [selectedPlace, setSelectedPlace] = useState<SelectedPlace | null>(null)

  return (
    <div className="app-shell">
      <div className="app-map-layer">
        <MapView onMapReady={setMap} />
        <div className="app-map-vignette" />
      </div>

      <div className="app-chrome">
        <aside className="app-sidebar">
          <section className="atlas-panel app-intro">
            <div className="app-intro__top">
              <span className="app-eyebrow">Atlas SDK</span>
              <span className="app-live-pill">Accra demo</span>
            </div>

            <h1>Search smarter. Build a route faster.</h1>
            <p>
              Explore nearby places, push a picked location straight into routing,
              and preview navigation in one polished workspace.
            </p>

            <div className="app-stat-grid">
              <div className="app-stat-card">
                <strong>Explore</strong>
                <span>Geocoding, categories, and nearby POIs around the live map.</span>
              </div>
              <div className="app-stat-card">
                <strong>Route</strong>
                <span>Pick points on the map or reuse a searched place in one tap.</span>
              </div>
              <div className="app-stat-card">
                <strong>Preview</strong>
                <span>Launch a more cinematic navigation simulation when ready.</span>
              </div>
            </div>
          </section>

          <SearchBar map={map} onPlaceSelected={setSelectedPlace} />
          <RoutePanel map={map} selectedPlace={selectedPlace} />
        </aside>

        <aside className="atlas-panel app-guide">
          <span className="app-guide__label">Quick flow</span>
          <h2 className="app-guide__title">Move from discovery to directions in three steps.</h2>

          <div className="app-guide__step">
            <span className="app-guide__num">1</span>
            <div className="app-guide__body">
              <strong>Search or filter</strong>
              <span>Look up a landmark or browse nearby categories around the current viewport.</span>
            </div>
          </div>

          <div className="app-guide__step">
            <span className="app-guide__num">2</span>
            <div className="app-guide__body">
              <strong>Reuse the selection</strong>
              <span>Send a searched place into the route panel or arm a map click for precision.</span>
            </div>
          </div>

          <div className="app-guide__step">
            <span className="app-guide__num">3</span>
            <div className="app-guide__body">
              <strong>Preview navigation</strong>
              <span>Generate the route, inspect the instruction cards, then start navigation.</span>
            </div>
          </div>
        </aside>
      </div>
    </div>
  )
}

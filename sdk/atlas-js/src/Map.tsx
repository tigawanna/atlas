import { useEffect, useRef } from 'react'
import maplibregl from 'maplibre-gl'
import 'maplibre-gl/dist/maplibre-gl.css'
import { layers, namedFlavor } from '@protomaps/basemaps'
import { reverseGeocode } from './api'

const TILESET = 'ghana'
const TILE_SOURCE = 'atlas'

interface MapViewProps {
  onMapReady?: (map: maplibregl.Map) => void
}

export function MapView({ onMapReady }: MapViewProps) {
  const containerRef = useRef<HTMLDivElement>(null)
  const mapRef = useRef<maplibregl.Map | null>(null)

  useEffect(() => {
    if (!containerRef.current || mapRef.current) return

    const map = new maplibregl.Map({
      container: containerRef.current,
      style: {
        version: 8,
        glyphs: 'https://protomaps.github.io/basemaps-assets/fonts/{fontstack}/{range}.pbf',
        sprite: 'https://protomaps.github.io/basemaps-assets/sprites/v4/dark',
        sources: {
          [TILE_SOURCE]: {
            type: 'vector',
            url: `/v1/tiles/${TILESET}/tilejson.json`,
          },
        },
        layers: layers(TILE_SOURCE, namedFlavor('dark'), { lang: 'en' }),
      },
      center: [-0.187, 5.603],
      zoom: 13,
      attributionControl: {},
    })

    map.addControl(new maplibregl.NavigationControl(), 'top-right')

    map.on('contextmenu', async (e) => {
      const { lat, lng } = e.lngLat
      const results = await reverseGeocode(lat, lng)
      const top = results[0]
      const html = top
        ? `<div class="atlas-popup-card">
             <span class="atlas-popup-kicker">Map lookup</span>
             <div class="atlas-popup-title">${top.name}</div>
             <div class="atlas-popup-meta">
               <span class="atlas-popup-tag">${top.category}</span>
               <span class="atlas-popup-dot"></span>
               <span>${Math.round(top.distance_m)}m away</span>
             </div>
           </div>`
        : '<div class="atlas-popup-empty">No results found for that point.</div>'

      new maplibregl.Popup({
        closeButton: false,
        maxWidth: '280px',
        className: 'atlas-popup',
        offset: 10,
      })
        .setLngLat([lng, lat])
        .setHTML(html)
        .addTo(map)
    })

    mapRef.current = map
    onMapReady?.(map)

    return () => {
      map.remove()
      mapRef.current = null
    }
  }, [onMapReady])

  return <div ref={containerRef} className="atlas-map-canvas" />
}

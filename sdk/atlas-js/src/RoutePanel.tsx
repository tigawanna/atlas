import { useCallback, useEffect, useRef, useState } from 'react'
import type { ReactNode } from 'react'
import maplibregl from 'maplibre-gl'
import { route, submitContribution, type RouteInstruction, type RouteResult } from './api'
import { NavigationView } from './NavigationView'
import type { SelectedPlace } from './types'

interface RoutePanelProps {
  map: maplibregl.Map | null
  selectedPlace: SelectedPlace | null
}

type ActiveField = 'origin' | 'destination' | null
type Profile = 'car' | 'motorcycle' | 'bicycle' | 'foot'

interface Waypoint {
  lat: number
  lon: number
  label: string
}

const ROUTE_SOURCE = 'route'
const ROUTE_LAYER = 'route-line'
const ROUTE_GLOW_LAYER = 'route-line-glow'

const INSTRUCTION_LABELS: Record<string, string> = {
  depart: 'Start',
  arrive: 'Arrive',
  left: 'Turn left',
  right: 'Turn right',
  slight_left: 'Slight left',
  slight_right: 'Slight right',
  sharp_left: 'Sharp left',
  sharp_right: 'Sharp right',
  straight: 'Continue',
  u_turn: 'U-turn',
  turn_left: 'Turn left',
  turn_right: 'Turn right',
  roundabout: 'Roundabout',
}

const PROFILES: { id: Profile; label: string; icon: ReactNode }[] = [
  {
    id: 'car',
    label: 'Car',
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="M19 17h2c.6 0 1-.4 1-1v-3c0-.9-.7-1.7-1.5-1.9C18.7 10.6 16 10 16 10s-1.3-1.4-2.2-2.3c-.5-.4-1.1-.7-1.8-.7H5c-.6 0-1.1.4-1.4.9l-1.4 2.9A3.7 3.7 0 0 0 2 12v4c0 .6.4 1 1 1h2" />
        <circle cx="7" cy="17" r="2" />
        <circle cx="17" cy="17" r="2" />
      </svg>
    ),
  },
  {
    id: 'motorcycle',
    label: 'Moto',
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="5" cy="18" r="3" />
        <circle cx="19" cy="18" r="3" />
        <path d="M10 18h5l2-12h-6l-1 2" />
        <path d="M19 18v-4l-2-1" />
        <path d="M13 6l2-4h2" />
      </svg>
    ),
  },
  {
    id: 'bicycle',
    label: 'Bike',
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <circle cx="5.5" cy="17.5" r="3.5" />
        <circle cx="18.5" cy="17.5" r="3.5" />
        <circle cx="15" cy="5" r="1" />
        <path d="M12 17.5V14l-3-3 4-3 2 3h2" />
      </svg>
    ),
  },
  {
    id: 'foot',
    label: 'Walk',
    icon: (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
        <path d="m10 5 3.5 4.5-1.5 6h2.5M10 20l1-5M15 20l-1-4" />
        <circle cx="14" cy="4" r="1" />
        <path d="M7 15V8l3-3" />
      </svg>
    ),
  },
]

function formatDistance(meters: number): string {
  return (meters / 1000).toFixed(1) + ' km'
}

function formatDistanceShort(meters: number): string {
  if (meters < 1000) return `${Math.round(meters)}m`
  return `${(meters / 1000).toFixed(1)}km`
}

function formatDuration(seconds: number): string {
  const hours = Math.floor(seconds / 3600)
  const minutes = Math.floor((seconds % 3600) / 60)
  return hours > 0 ? `${hours}h ${minutes.toString().padStart(2, '0')}m` : `${minutes}m`
}

function TurnIcon({ type }: { type: string }) {
  const color = '#0f766e'

  if (type === 'arrive') {
    return (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
        <circle cx="12" cy="12" r="9" stroke={color} strokeWidth="2" />
        <path d="M8 12l3 3 5-5" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'u_turn') {
    return (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
        <path d="M8 20V9a4 4 0 0 1 8 0v1" stroke={color} strokeWidth="2" strokeLinecap="round" />
        <path d="M5 17l3 3 3-3" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'left' || type === 'slight_left' || type === 'sharp_left' || type === 'turn_left') {
    return (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
        <path d="M12 5v7a3 3 0 0 1-3 3H5" stroke={color} strokeWidth="2" strokeLinecap="round" />
        <path d="M8 12l-3 3 3 3" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M12 5l3 3-3 3" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'right' || type === 'slight_right' || type === 'sharp_right' || type === 'turn_right') {
    return (
      <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
        <path d="M12 5v7a3 3 0 0 0 3 3h4" stroke={color} strokeWidth="2" strokeLinecap="round" />
        <path d="M16 12l3 3-3 3" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M12 5l-3 3 3 3" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
      <path d="M12 19V5" stroke={color} strokeWidth="2" strokeLinecap="round" />
      <path d="M6 11l6-6 6 6" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function toWaypoint(place: SelectedPlace): Waypoint {
  return {
    lat: place.lat,
    lon: place.lon,
    label: place.label,
  }
}

export function RoutePanel({ map, selectedPlace }: RoutePanelProps) {
  const [origin, setOrigin] = useState<Waypoint | null>(null)
  const [destination, setDestination] = useState<Waypoint | null>(null)
  const [activeField, setActiveField] = useState<ActiveField>(null)
  const [profile, setProfile] = useState<Profile>('car')
  const [result, setResult] = useState<RouteResult | null>(null)
  const [loading, setLoading] = useState(false)
  const [navigating, setNavigating] = useState(false)
  const [showReportForm, setShowReportForm] = useState(false)
  const [reportIssueType, setReportIssueType] = useState('wrong_turn')
  const [reportDescription, setReportDescription] = useState('')
  const [reportSubmitting, setReportSubmitting] = useState(false)
  const [reportSubmitted, setReportSubmitted] = useState(false)
  const [routeError, setRouteError] = useState<string | null>(null)

  const originMarkerRef = useRef<maplibregl.Marker | null>(null)
  const destMarkerRef = useRef<maplibregl.Marker | null>(null)
  const routeInitialized = useRef(false)

  const clearRenderedRoute = useCallback(() => {
    if (!map) return
    const source = map.getSource(ROUTE_SOURCE) as maplibregl.GeoJSONSource | undefined
    source?.setData({ type: 'FeatureCollection', features: [] })
  }, [map])

  const clearRoutePreview = useCallback(() => {
    setResult(null)
    setNavigating(false)
    setLoading(false)
    setShowReportForm(false)
    setReportSubmitted(false)
    setReportDescription('')
    setReportIssueType('wrong_turn')
    setRouteError(null)
    clearRenderedRoute()
  }, [clearRenderedRoute])

  const placeMarker = useCallback(
    (field: Exclude<ActiveField, null>, waypoint: Waypoint) => {
      if (!map) return

      const markerRef = field === 'origin' ? originMarkerRef : destMarkerRef
      const color = field === 'origin' ? '#0f766e' : '#d97706'

      markerRef.current?.remove()
      markerRef.current = new maplibregl.Marker({ color })
        .setLngLat([waypoint.lon, waypoint.lat])
        .addTo(map)
    },
    [map]
  )

  const setWaypoint = useCallback(
    (field: Exclude<ActiveField, null>, waypoint: Waypoint, options?: { flyTo?: boolean }) => {
      clearRoutePreview()

      if (field === 'origin') {
        setOrigin(waypoint)
      } else {
        setDestination(waypoint)
      }

      placeMarker(field, waypoint)
      setActiveField(null)

      if (map && options?.flyTo !== false) {
        map.flyTo({
          center: [waypoint.lon, waypoint.lat],
          zoom: Math.max(map.getZoom(), 14),
          duration: 900,
          essential: true,
        })
      }
    },
    [clearRoutePreview, map, placeMarker]
  )

  const initRouteLayer = useCallback((instance: maplibregl.Map) => {
    if (routeInitialized.current) return

    instance.addSource(ROUTE_SOURCE, {
      type: 'geojson',
      data: { type: 'FeatureCollection', features: [] },
    })

    instance.addLayer({
      id: ROUTE_GLOW_LAYER,
      type: 'line',
      source: ROUTE_SOURCE,
      layout: { 'line-join': 'round', 'line-cap': 'round' },
      paint: {
        'line-color': '#f59e0b',
        'line-width': 11,
        'line-opacity': 0.18,
      },
    })

    instance.addLayer({
      id: ROUTE_LAYER,
      type: 'line',
      source: ROUTE_SOURCE,
      layout: { 'line-join': 'round', 'line-cap': 'round' },
      paint: {
        'line-color': '#0f766e',
        'line-width': 5.5,
        'line-opacity': 0.95,
      },
    })

    routeInitialized.current = true
  }, [])

  useEffect(() => {
    if (!map) return

    if (map.loaded()) {
      initRouteLayer(map)
    } else {
      map.once('load', () => initRouteLayer(map))
    }

    return () => {
      if (!routeInitialized.current) return

      if (map.getLayer(ROUTE_LAYER)) map.removeLayer(ROUTE_LAYER)
      if (map.getLayer(ROUTE_GLOW_LAYER)) map.removeLayer(ROUTE_GLOW_LAYER)
      if (map.getSource(ROUTE_SOURCE)) map.removeSource(ROUTE_SOURCE)
      routeInitialized.current = false
    }
  }, [map, initRouteLayer])

  useEffect(() => {
    return () => {
      originMarkerRef.current?.remove()
      destMarkerRef.current?.remove()
    }
  }, [])

  useEffect(() => {
    if (!map) return
    const canvas = map.getCanvas()
    canvas.style.cursor = activeField ? 'crosshair' : ''

    return () => {
      canvas.style.cursor = ''
    }
  }, [map, activeField])

  useEffect(() => {
    if (!map) return

    const handleClick = (event: maplibregl.MapMouseEvent) => {
      if (!activeField) return

      const { lat, lng } = event.lngLat
      const waypoint: Waypoint = {
        lat,
        lon: lng,
        label: `${lat.toFixed(5)}, ${lng.toFixed(5)}`,
      }

      setWaypoint(activeField, waypoint, { flyTo: false })
    }

    map.on('click', handleClick)
    return () => {
      map.off('click', handleClick)
    }
  }, [map, activeField, setWaypoint])

  const handleProfileChange = (nextProfile: Profile) => {
    if (nextProfile === profile) return
    clearRoutePreview()
    setProfile(nextProfile)
  }

  const handleSwap = () => {
    if (!origin || !destination) return

    clearRoutePreview()

    setOrigin(destination)
    setDestination(origin)
    placeMarker('origin', destination)
    placeMarker('destination', origin)
  }

  const handleGetRoute = async () => {
    if (!origin || !destination || !map) return

    setLoading(true)
    setRouteError(null)

    const response = await route(origin, destination, profile)
    setLoading(false)

    if (!response) {
      setRouteError('Atlas could not build a route for those points. Try moving the pins or changing the profile.')
      clearRenderedRoute()
      return
    }

    setResult(response)
    setShowReportForm(false)
    setReportSubmitted(false)
    setReportDescription('')

    initRouteLayer(map)

    const source = map.getSource(ROUTE_SOURCE) as maplibregl.GeoJSONSource | undefined
    source?.setData({
      type: 'Feature',
      properties: {},
      geometry: response.geometry as GeoJSON.Geometry,
    })

    try {
      if (response.geometry.coordinates.length >= 2) {
        const coordinates = response.geometry.coordinates
        let minLng = coordinates[0][0]
        let maxLng = coordinates[0][0]
        let minLat = coordinates[0][1]
        let maxLat = coordinates[0][1]

        for (const coordinate of coordinates) {
          if (coordinate[0] < minLng) minLng = coordinate[0]
          if (coordinate[0] > maxLng) maxLng = coordinate[0]
          if (coordinate[1] < minLat) minLat = coordinate[1]
          if (coordinate[1] > maxLat) maxLat = coordinate[1]
        }

        map.fitBounds(
          [
            [minLng, minLat],
            [maxLng, maxLat],
          ],
          { padding: 100 }
        )
      }
    } catch (error) {
      console.warn('fitBounds failed:', error)
    }
  }

  const handleReportSubmit = async () => {
    if (!origin || !destination || !result) return

    setReportSubmitting(true)
    await submitContribution({
      route_origin: { lat: origin.lat, lon: origin.lon },
      route_destination: { lat: destination.lat, lon: destination.lon },
      profile,
      issue_type: reportIssueType,
      description: reportDescription || undefined,
    })
    setReportSubmitting(false)
    setReportSubmitted(true)

    setTimeout(() => {
      setShowReportForm(false)
      setReportSubmitted(false)
      setReportDescription('')
      setReportIssueType('wrong_turn')
    }, 2000)
  }

  const handleClear = () => {
    clearRoutePreview()
    setOrigin(null)
    setDestination(null)
    setActiveField(null)

    originMarkerRef.current?.remove()
    originMarkerRef.current = null
    destMarkerRef.current?.remove()
    destMarkerRef.current = null
  }

  const canRoute = origin !== null && destination !== null

  if (navigating && result) {
    return (
      <NavigationView
        map={map}
        route={result}
        origin={origin ? { lat: origin.lat, lon: origin.lon } : undefined}
        destination={destination ? { lat: destination.lat, lon: destination.lon } : undefined}
        profile={profile}
        onExit={() => setNavigating(false)}
      />
    )
  }

  return (
    <section className="atlas-panel route-panel">
      <div className="panel-title-row">
        <div>
          <span className="panel-kicker">Route</span>
          <h2 className="panel-title">Build a trip without losing context.</h2>
          <p className="panel-note">Use a searched place below, or arm a point and click the map when you need a precise origin or destination.</p>
        </div>
        <span className="panel-chip">{result ? 'Route ready' : 'Planner'}</span>
      </div>

      <div className={`route-helper-card${activeField ? ' is-active' : ''}`}>
        <strong>{activeField ? `Picking the ${activeField}` : 'Map picking is built in.'}</strong>
        <p>
          {activeField
            ? `Click anywhere on the map to drop the ${activeField} pin.`
            : 'Tap a field to arm it, then click the map. Right-click on the map whenever you want a nearby place preview.'}
        </p>
      </div>

      {selectedPlace && (
        <div className="route-picked-card">
          <strong>{selectedPlace.label}</strong>
          <p>Reuse the latest search selection without re-pinning it on the map.</p>

          <div className="route-picked__meta">
            {selectedPlace.category && <span className="search-meta-pill">{selectedPlace.category}</span>}
            {selectedPlace.address && <span className="search-meta-pill">{selectedPlace.address}</span>}
          </div>

          <div className="route-picked__actions">
            <button type="button" className="route-chip-btn" onClick={() => setWaypoint('origin', toWaypoint(selectedPlace))}>
              Use as origin
            </button>
            <button type="button" className="route-primary-ghost" onClick={() => setWaypoint('destination', toWaypoint(selectedPlace))}>
              Use as destination
            </button>
          </div>
        </div>
      )}

      <div className="route-stop-list">
        <div className={`route-stop${activeField === 'origin' ? ' is-active' : ''}`}>
          <span className="route-stop__marker" style={{ background: 'var(--route-origin)' }} />
          <div className="route-stop__content">
            <span className="route-stop__label">Origin</span>
            <span className={`route-stop__value${origin ? '' : ' is-placeholder'}`}>
              {origin?.label ?? 'Pick a starting point on the map'}
            </span>
          </div>
          <button type="button" className="route-stop__action" onClick={() => setActiveField('origin')}>
            {activeField === 'origin' ? 'Armed' : 'Pick'}
          </button>
        </div>

        <div className={`route-stop${activeField === 'destination' ? ' is-active' : ''}`}>
          <span className="route-stop__marker" style={{ background: 'var(--route-destination)' }} />
          <div className="route-stop__content">
            <span className="route-stop__label">Destination</span>
            <span className={`route-stop__value${destination ? '' : ' is-placeholder'}`}>
              {destination?.label ?? 'Pick an arrival point on the map'}
            </span>
          </div>
          <button type="button" className="route-stop__action" onClick={() => setActiveField('destination')}>
            {activeField === 'destination' ? 'Armed' : 'Pick'}
          </button>
        </div>
      </div>

      {origin && destination && (
        <button type="button" className="route-swap-btn" onClick={handleSwap}>
          Swap stops
        </button>
      )}

      <div className="profile-grid">
        {PROFILES.map((item) => (
          <button
            key={item.id}
            type="button"
            className={`profile-button${profile === item.id ? ' is-active' : ''}`}
            onClick={() => handleProfileChange(item.id)}
            aria-pressed={profile === item.id}
          >
            <span className="profile-button__icon">{item.icon}</span>
            <span className="profile-button__label">{item.label}</span>
          </button>
        ))}
      </div>

      <div className="route-cta-row">
        <button
          type="button"
          className="route-primary-btn"
          disabled={!canRoute || loading}
          onClick={handleGetRoute}
        >
          {loading ? 'Building route…' : 'Build route'}
        </button>
        <button type="button" className="route-secondary-btn" onClick={handleClear}>
          Reset
        </button>
      </div>

      {routeError && <div className="route-error">{routeError}</div>}

      {!result && (
        <div className="route-empty-state">
          <strong>Once a route is ready, the turn cards appear here.</strong>
          <p>Set two points, pick a travel profile, and Atlas will draw the route preview on the map.</p>
        </div>
      )}

      {result && (
        <>
          <div className="route-result-wrap">
            <div className="route-summary-grid">
              <div className="route-summary-card">
                <small>Distance</small>
                <strong>{formatDistance(result.distance_m)}</strong>
              </div>
              <div className="route-summary-card">
                <small>Duration</small>
                <strong>{formatDuration(result.duration_s)}</strong>
              </div>
            </div>

            <div className="route-actions-row">
              <button type="button" className="route-start-btn" onClick={() => setNavigating(true)}>
                Start navigation
              </button>
              <button type="button" className="route-secondary-btn" onClick={() => setShowReportForm((visible) => !visible)}>
                {showReportForm ? 'Hide report' : 'Report issue'}
              </button>
            </div>

            <div className="route-instructions">
              {result.instructions.map((instruction: RouteInstruction, index: number) => (
                <div key={`${instruction.type}-${index}`} className="route-instruction">
                  <div className="route-instruction__icon">
                    <TurnIcon type={instruction.type} />
                  </div>
                  <div className="route-instruction__body">
                    <span className="route-instruction__title">
                      {INSTRUCTION_LABELS[instruction.type] ?? instruction.type}
                    </span>
                    {instruction.road && <span className="route-instruction__road">{instruction.road}</span>}
                  </div>
                  <span className="route-instruction__distance">
                    {instruction.distance_m > 0 ? formatDistanceShort(instruction.distance_m) : ''}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {showReportForm && (
            <div className="route-report-card">
              <strong>Report a routing issue</strong>
              <p>Flag wrong turns, closed roads, or anything else that should improve this route in the future.</p>

              {reportSubmitted ? (
                <div className="route-report-success">Thanks. Your route feedback has been captured.</div>
              ) : (
                <>
                  <select value={reportIssueType} onChange={(event) => setReportIssueType(event.target.value)}>
                    <option value="wrong_turn">Wrong Turn</option>
                    <option value="road_closed">Road Closed</option>
                    <option value="better_route">Better Route</option>
                    <option value="roundabout_error">Roundabout Error</option>
                    <option value="missing_road">Missing Road</option>
                    <option value="speed_wrong">Speed Wrong</option>
                    <option value="other">Other</option>
                  </select>

                  <textarea
                    placeholder="Describe the issue (optional)"
                    value={reportDescription}
                    onChange={(event) => setReportDescription(event.target.value)}
                  />

                  <div className="route-report-actions">
                    <button type="button" className="route-report-cancel" onClick={() => setShowReportForm(false)}>
                      Cancel
                    </button>
                    <button
                      type="button"
                      className="route-report-submit"
                      disabled={reportSubmitting}
                      onClick={handleReportSubmit}
                    >
                      {reportSubmitting ? 'Sending…' : 'Send report'}
                    </button>
                  </div>
                </>
              )}
            </div>
          )}
        </>
      )}
    </section>
  )
}

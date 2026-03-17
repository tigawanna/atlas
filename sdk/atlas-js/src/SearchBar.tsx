import { useCallback, useEffect, useRef, useState } from 'react'
import type { KeyboardEvent, ReactNode } from 'react'
import maplibregl from 'maplibre-gl'
import { geocode, type GeocodeResult, search, type SearchResult } from './api'
import type { SelectedPlace } from './types'

interface SearchBarProps {
  map: maplibregl.Map | null
  onPlaceSelected?: (place: SelectedPlace) => void
}

type AnyResult = GeocodeResult | SearchResult

const CATEGORY_ICONS: Record<string, ReactNode> = {
  Restaurant: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 2v7c0 1.1.9 2 2 2h4a2 2 0 0 0 2-2V2" />
      <path d="M7 2v20" />
      <path d="M21 15V2v0a5 5 0 0 0-5 5v6c0 1.1.9 2 2 2h3Zm0 0v7" />
    </svg>
  ),
  Hotel: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 22v-20h18v20" />
      <path d="M18 22v-4a2 2 0 0 0-2-2H8a2 2 0 0 0-2 2v4" />
      <path d="M10 8h4" />
      <path d="M10 12h4" />
    </svg>
  ),
  Hospital: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M12 6v12" />
      <path d="M6 12h12" />
      <rect x="2" y="4" width="20" height="16" rx="2" />
    </svg>
  ),
  Bank: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 21h18" />
      <path d="M3 10h18" />
      <path d="M5 6l7-4 7 4" />
      <path d="M4 10v11" />
      <path d="M20 10v11" />
      <path d="M8 14v3" />
      <path d="M12 14v3" />
      <path d="M16 14v3" />
    </svg>
  ),
  Mosque: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M22 21H2" />
      <path d="M12 17V3" />
      <path d="M7 17v-4a5 5 0 0 1 10 0v4" />
      <path d="M18 21v-8a1 1 0 0 0-1-1H7a1 1 0 0 0-1 1v8" />
    </svg>
  ),
  Market: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <circle cx="9" cy="21" r="1" />
      <circle cx="20" cy="21" r="1" />
      <path d="M1 1h4l2.68 13.39a2 2 0 0 0 2 1.61h9.72a2 2 0 0 0 2-1.61L23 6H6" />
    </svg>
  ),
  Fuel: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M3 22V7" />
      <path d="M4 7h11" />
      <path d="M14 22V7" />
      <path d="M15 7l4 4" />
      <path d="M19 11v10" />
      <rect x="6" y="10" width="5" height="4" rx="0.5" />
    </svg>
  ),
  School: (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round">
      <path d="M22 10v6" />
      <path d="M2 10l10-5 10 5-10 5-10-5Z" />
      <path d="M6 12v5c3 3 9 3 12 0v-5" />
    </svg>
  ),
}

const CATEGORIES = ['Restaurant', 'Hotel', 'Hospital', 'Bank', 'Mosque', 'Market', 'Fuel', 'School'] as const
type Category = (typeof CATEGORIES)[number]

const CATEGORY_DOT_COLORS: Record<string, string> = {
  restaurant: '#dc2626',
  hotel: '#b45309',
  hospital: '#dc2626',
  bank: '#0f766e',
  mosque: '#059669',
  market: '#ea580c',
  fuel: '#2563eb',
  school: '#7c3aed',
  place: '#64748b',
  road: '#94a3b8',
}

function getCategoryColor(category: string): string {
  return CATEGORY_DOT_COLORS[category.toLowerCase()] ?? '#64748b'
}

function formatDistance(m: number | null): string | null {
  if (m == null) return null
  if (m < 1000) return `${Math.round(m)}m away`
  return `${(m / 1000).toFixed(1)}km away`
}

function isSearchResult(result: AnyResult): result is SearchResult {
  return 'score' in result
}

function SearchIcon() {
  return (
    <svg width="18" height="18" viewBox="0 0 24 24" fill="none">
      <circle cx="11" cy="11" r="7" stroke="currentColor" strokeWidth="2.1" />
      <path d="M20 20l-3.5-3.5" stroke="currentColor" strokeWidth="2.1" strokeLinecap="round" />
    </svg>
  )
}

function CloseIcon() {
  return (
    <svg width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2.3" strokeLinecap="round">
      <path d="M6 6l12 12" />
      <path d="M18 6 6 18" />
    </svg>
  )
}

export function SearchBar({ map, onPlaceSelected }: SearchBarProps) {
  const [query, setQuery] = useState('')
  const [results, setResults] = useState<AnyResult[]>([])
  const [hoveredIndex, setHoveredIndex] = useState(-1)
  const [activeCategory, setActiveCategory] = useState<Category | null>(null)
  const [loading, setLoading] = useState(false)
  const markerRef = useRef<maplibregl.Marker | null>(null)
  const poiMarkersRef = useRef<maplibregl.Marker[]>([])
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null)
  const requestSeqRef = useRef(0)

  const clearPoiMarkers = useCallback(() => {
    poiMarkersRef.current.forEach((marker) => marker.remove())
    poiMarkersRef.current = []
  }, [])

  const clearSelectedMarker = useCallback(() => {
    markerRef.current?.remove()
    markerRef.current = null
  }, [])

  const dropPoiMarkers = useCallback(
    (items: SearchResult[]) => {
      if (!map) return
      clearPoiMarkers()
      poiMarkersRef.current = items.map((item) =>
        new maplibregl.Marker({ color: getCategoryColor(item.category) })
          .setLngLat([item.lon, item.lat])
          .setPopup(new maplibregl.Popup({ offset: 18, className: 'atlas-popup' }).setHTML(
            `<div class="atlas-popup-card"><span class="atlas-popup-kicker">Nearby place</span><div class="atlas-popup-title">${item.name}</div><div class="atlas-popup-meta"><span class="atlas-popup-tag">${item.category}</span></div></div>`
          ))
          .addTo(map)
      )
    },
    [map, clearPoiMarkers]
  )

  const getMapCenter = useCallback((): { lat: number; lon: number } | null => {
    if (!map) return null
    const center = map.getCenter()
    return { lat: center.lat, lon: center.lng }
  }, [map])

  const runSearch = useCallback(
    async (rawQuery: string, category: Category | null) => {
      const trimmedQuery = rawQuery.trim()
      const requestId = ++requestSeqRef.current

      if (trimmedQuery.length < 2 && !category) {
        setResults([])
        setLoading(false)
        clearPoiMarkers()
        return
      }

      setLoading(true)

      if (category) {
        const center = getMapCenter()
        const found = await search({
          q: trimmedQuery.length >= 2 ? trimmedQuery : undefined,
          lat: center?.lat,
          lon: center?.lon,
          category: category.toLowerCase(),
          limit: 10,
        })

        if (requestId !== requestSeqRef.current) return
        setResults(found)
        setLoading(false)
        dropPoiMarkers(found)
        return
      }

      clearPoiMarkers()

      if (trimmedQuery.length < 2) {
        setResults([])
        setLoading(false)
        return
      }

      const found = await geocode(trimmedQuery)
      if (requestId !== requestSeqRef.current) return
      setResults(found)
      setLoading(false)
    },
    [clearPoiMarkers, dropPoiMarkers, getMapCenter]
  )

  useEffect(() => {
    if (debounceRef.current) clearTimeout(debounceRef.current)
    debounceRef.current = setTimeout(() => {
      void runSearch(query, activeCategory)
    }, 260)

    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current)
    }
  }, [query, activeCategory, runSearch])

  useEffect(() => {
    return () => {
      requestSeqRef.current += 1
      clearPoiMarkers()
      clearSelectedMarker()
    }
  }, [clearPoiMarkers, clearSelectedMarker])

  const selectResult = (result: AnyResult) => {
    if (!map) return

    const place: SelectedPlace = {
      lat: result.lat,
      lon: result.lon,
      label: result.name,
      category: result.category,
      address: result.address ?? null,
    }

    clearPoiMarkers()
    clearSelectedMarker()

    markerRef.current = new maplibregl.Marker({ color: '#0f766e' })
      .setLngLat([place.lon, place.lat])
      .addTo(map)

    map.flyTo({
      center: [place.lon, place.lat],
      zoom: 15,
      essential: true,
      duration: 900,
    })

    setQuery(place.label)
    setHoveredIndex(-1)
    onPlaceSelected?.(place)
  }

  const handleInputKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === 'Escape') {
      setQuery('')
      setResults([])
      setHoveredIndex(-1)
      setLoading(false)
      setActiveCategory(null)
      clearPoiMarkers()
      clearSelectedMarker()
      return
    }

    if (event.key === 'Enter' && results.length > 0) {
      const selected = results[hoveredIndex] ?? results[0]
      if (selected) {
        event.preventDefault()
        selectResult(selected)
      }
    }
  }

  const toggleCategory = (category: Category) => {
    setHoveredIndex(-1)
    setActiveCategory((current) => (current === category ? null : category))
  }

  const resetSearch = () => {
    requestSeqRef.current += 1
    setQuery('')
    setResults([])
    setHoveredIndex(-1)
    setLoading(false)
    setActiveCategory(null)
    clearPoiMarkers()
    clearSelectedMarker()
  }

  const showingSearchState = activeCategory !== null || query.trim().length >= 2
  const helperMessage = activeCategory
    ? `Browsing ${activeCategory.toLowerCase()} close to the center of the map.`
    : 'Search a landmark, road, or address. You can also jump into one of the nearby categories below.'

  return (
    <section className="atlas-panel search-panel">
      <div className="panel-title-row">
        <div>
          <span className="panel-kicker">Explore</span>
          <h2 className="panel-title">Find places around the live map.</h2>
          <p className="panel-note">Search by name, or switch to nearby category browsing when you want quick inspiration.</p>
        </div>
        <span className="panel-chip">{activeCategory ? `${activeCategory} nearby` : 'Geocode + POIs'}</span>
      </div>

      <div className="search-input-shell">
        <SearchIcon />
        <input
          className="search-input"
          type="text"
          placeholder="Search markets, landmarks, or addresses"
          value={query}
          onChange={(event) => setQuery(event.target.value)}
          onKeyDown={handleInputKeyDown}
          aria-label="Search places"
        />
        {(query || activeCategory) && (
          <button className="search-clear" type="button" onClick={resetSearch} aria-label="Clear search">
            <CloseIcon />
          </button>
        )}
      </div>

      <div className="search-caption">{helperMessage}</div>

      <div className="search-chip-grid">
        {CATEGORIES.map((category) => (
          <button
            key={category}
            type="button"
            className={`search-chip${activeCategory === category ? ' is-active' : ''}`}
            onClick={() => toggleCategory(category)}
            aria-pressed={activeCategory === category}
          >
            <span>{CATEGORY_ICONS[category]}</span>
            <span>{category}</span>
          </button>
        ))}
      </div>

      <div className="search-results-frame">
        {!showingSearchState && (
          <div className="search-empty-state">
            <strong>Start with a place you know.</strong>
            <p>Try a landmark like Makola Market, Korle Bu, or Independence Square, or use a category to browse nearby options.</p>
            <div className="search-suggestions">
              <span className="search-suggestion">Makola Market</span>
              <span className="search-suggestion">Osu Castle</span>
              <span className="search-suggestion">Korle Bu</span>
            </div>
          </div>
        )}

        {showingSearchState && loading && (
          <div className="search-empty-state">
            <strong>Searching the map…</strong>
            <p>Atlas is looking up places and nearby points of interest for the current view.</p>
          </div>
        )}

        {showingSearchState && !loading && results.length === 0 && (
          <div className="search-empty-state">
            <strong>No matches yet.</strong>
            <p>Pan the map a little, change the wording, or switch categories to widen the search.</p>
          </div>
        )}

        {showingSearchState && !loading && results.length > 0 && (
          <div className="search-results-list" role="listbox" aria-label="Search results">
            {results.map((result, index) => {
              const distanceText = isSearchResult(result) ? formatDistance(result.distance_m) : null
              const confidenceText = !isSearchResult(result) ? `${Math.round(result.confidence * 100)}% match` : null
              const categoryColor = getCategoryColor(result.category)

              return (
                <div
                  key={`${result.name}-${result.lat}-${result.lon}`}
                  className={`search-result${index === hoveredIndex ? ' is-hovered' : ''}`}
                  onMouseEnter={() => setHoveredIndex(index)}
                  onMouseLeave={() => setHoveredIndex(-1)}
                  onClick={() => selectResult(result)}
                  role="option"
                  aria-selected={index === hoveredIndex}
                >
                  <div className="search-result__header">
                    <div>
                      <p className="search-result__name">{result.name}</p>
                      <div className="search-meta-row">
                        <span className="search-meta-pill">
                          <span className="search-dot" style={{ background: categoryColor }} />
                          <span>{result.category}</span>
                        </span>
                        {distanceText && <span className="search-meta-pill">{distanceText}</span>}
                        {confidenceText && <span className="search-meta-pill">{confidenceText}</span>}
                      </div>
                    </div>
                    <span className="search-result__jump" aria-hidden="true">
                      ↗
                    </span>
                  </div>

                  {result.address && <p className="search-result__address">{result.address}</p>}
                </div>
              )
            })}
          </div>
        )}
      </div>
    </section>
  )
}

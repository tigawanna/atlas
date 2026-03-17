export interface GeocodeResult {
  name: string
  lat: number
  lon: number
  category: string
  address: string | null
  confidence: number
}

export interface ReverseResult {
  name: string
  lat: number
  lon: number
  distance_m: number
  category: string
}

export async function geocode(query: string, limit = 5): Promise<GeocodeResult[]> {
  const params = new URLSearchParams({ q: query, limit: String(limit) })
  const resp = await fetch(`/v1/geocode?${params}`)
  if (!resp.ok) return []
  const data = await resp.json()
  return data.results ?? []
}

export async function reverseGeocode(lat: number, lon: number, limit = 1): Promise<ReverseResult[]> {
  const params = new URLSearchParams({ lat: String(lat), lon: String(lon), limit: String(limit) })
  const resp = await fetch(`/v1/reverse?${params}`)
  if (!resp.ok) return []
  const data = await resp.json()
  return data.results ?? []
}

export interface SearchResult {
  name: string
  lat: number
  lon: number
  category: string
  address: string | null
  distance_m: number | null
  score: number
}

export async function search(params: {
  q?: string
  lat?: number
  lon?: number
  category?: string
  radius_km?: number
  limit?: number
}): Promise<SearchResult[]> {
  const qs = new URLSearchParams()
  if (params.q) qs.set('q', params.q)
  if (params.lat != null) qs.set('lat', String(params.lat))
  if (params.lon != null) qs.set('lon', String(params.lon))
  if (params.category) qs.set('category', params.category)
  if (params.radius_km) qs.set('radius_km', String(params.radius_km))
  if (params.limit) qs.set('limit', String(params.limit))
  const resp = await fetch(`/v1/search?${qs}`)
  if (!resp.ok) return []
  const data = await resp.json()
  return data.results ?? []
}

export interface RouteInstruction {
  type: string
  road: string | null
  distance_m: number
  bearing: number
}

export interface RouteResult {
  distance_m: number
  duration_s: number
  geometry: { type: string; coordinates: number[][] }
  instructions: RouteInstruction[]
}

export async function route(
  origin: { lat: number; lon: number },
  destination: { lat: number; lon: number },
  profile: string
): Promise<RouteResult | null> {
  const resp = await fetch('/v1/route', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ origin, destination, profile }),
  })
  if (!resp.ok) return null
  return resp.json()
}

export async function submitContribution(data: {
  route_origin: { lat: number; lon: number }
  route_destination: { lat: number; lon: number }
  profile: string
  issue_type: string
  description?: string
}): Promise<{ id: string } | null> {
  const resp = await fetch('/v1/contribute', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!resp.ok) return null
  return resp.json()
}

export interface TelemetryWaypoint {
  lat: number
  lon: number
  timestamp: string
  speed_kmh?: number
  bearing?: number
}

export async function startTrip(
  profile: string,
  origin: { lat: number; lon: number },
  destination: { lat: number; lon: number }
): Promise<{ trip_id: string } | null> {
  const resp = await fetch('/v1/telemetry/start', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ profile, origin, destination }),
  })
  if (!resp.ok) return null
  return resp.json()
}

export async function sendTelemetry(
  tripId: string,
  waypoints: TelemetryWaypoint[]
): Promise<void> {
  await fetch(`/v1/telemetry/${tripId}/update`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ waypoints }),
  })
}

export async function endTrip(
  tripId: string
): Promise<{ status: string; duration_s: number; distance_m: number } | null> {
  const resp = await fetch(`/v1/telemetry/${tripId}/end`, {
    method: 'POST',
  })
  if (!resp.ok) return null
  return resp.json()
}

import { useEffect, useRef, useState, useCallback } from 'react'
import maplibregl from 'maplibre-gl'
import { RouteResult, submitContribution } from './api'

interface NavigationViewProps {
  map: maplibregl.Map | null
  route: RouteResult
  origin?: { lat: number; lon: number }
  destination?: { lat: number; lon: number }
  profile?: string
  onExit: () => void
}

type SpeedMultiplier = 1 | 2 | 5 | 10

const NAV_PITCH = 60
const NAV_ZOOM = 17
const TURN_ALERT_DISTANCE_M = 100
const SIMULATED_BASE_SPEED_MS = 13.89

const INSTRUCTION_LABELS: Record<string, string> = {
  depart: 'Head out',
  arrive: 'Arrive at destination',
  left: 'Turn left',
  right: 'Turn right',
  slight_left: 'Slight left',
  slight_right: 'Slight right',
  sharp_left: 'Sharp left',
  sharp_right: 'Sharp right',
  straight: 'Continue straight',
  u_turn: 'Make a U-turn',
  turn_left: 'Turn left',
  turn_right: 'Turn right',
  roundabout: 'Roundabout',
}

function TurnArrow({ type, size = 28 }: { type: string; size?: number }) {
  const color = '#fff'
  const s = size

  if (type === 'arrive') {
    return (
      <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
        <circle cx="12" cy="12" r="10" stroke={color} strokeWidth="2" />
        <path d="M8 12l3 3 5-5" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'u_turn') {
    return (
      <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
        <path d="M8 20V9a4 4 0 0 1 8 0v1" stroke={color} strokeWidth="2.5" strokeLinecap="round" />
        <path d="M5 17l3 3 3-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'left' || type === 'turn_left') {
    return (
      <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
        <path d="M12 5v8a4 4 0 0 1-4 4H4" stroke={color} strokeWidth="2.5" strokeLinecap="round" />
        <path d="M7 14l-3 3 3 3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M12 5l3 3-3 3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'right' || type === 'turn_right') {
    return (
      <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
        <path d="M12 5v8a4 4 0 0 0 4 4h4" stroke={color} strokeWidth="2.5" strokeLinecap="round" />
        <path d="M17 14l3 3-3 3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M12 5l-3 3 3 3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'slight_left') {
    return (
      <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
        <path d="M12 19V10a3 3 0 0 0-3-3H5" stroke={color} strokeWidth="2.5" strokeLinecap="round" />
        <path d="M8 10l-3-3 3-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M12 19l-2-3 2-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'slight_right') {
    return (
      <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
        <path d="M12 19V10a3 3 0 0 1 3-3h4" stroke={color} strokeWidth="2.5" strokeLinecap="round" />
        <path d="M16 10l3-3-3-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M12 19l2-3-2-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'sharp_left') {
    return (
      <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
        <path d="M12 19v-8a1 1 0 0 0-1-1H4" stroke={color} strokeWidth="2.5" strokeLinecap="round" />
        <path d="M7 13l-3-3 3-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M12 19l-2-3 2-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  if (type === 'sharp_right') {
    return (
      <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
        <path d="M12 19v-8a1 1 0 0 1 1-1h7" stroke={color} strokeWidth="2.5" strokeLinecap="round" />
        <path d="M17 13l3-3-3-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
        <path d="M12 19l2-3-2-3" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    )
  }

  return (
    <svg width={s} height={s} viewBox="0 0 24 24" fill="none">
      <path d="M12 19V5" stroke={color} strokeWidth="2.5" strokeLinecap="round" />
      <path d="M6 11l6-6 6 6" stroke={color} strokeWidth="2.5" strokeLinecap="round" strokeLinejoin="round" />
    </svg>
  )
}

function toRadians(deg: number): number {
  return (deg * Math.PI) / 180
}

function haversineDistance(a: [number, number], b: [number, number]): number {
  const R = 6371000
  const dLat = toRadians(b[1] - a[1])
  const dLon = toRadians(b[0] - a[0])
  const lat1 = toRadians(a[1])
  const lat2 = toRadians(b[1])
  const sinDlat = Math.sin(dLat / 2)
  const sinDlon = Math.sin(dLon / 2)
  const h = sinDlat * sinDlat + Math.cos(lat1) * Math.cos(lat2) * sinDlon * sinDlon
  return 2 * R * Math.asin(Math.sqrt(h))
}

function bearingBetween(a: [number, number], b: [number, number]): number {
  const lat1 = toRadians(a[1])
  const lat2 = toRadians(b[1])
  const dLon = toRadians(b[0] - a[0])
  const y = Math.sin(dLon) * Math.cos(lat2)
  const x = Math.cos(lat1) * Math.sin(lat2) - Math.sin(lat1) * Math.cos(lat2) * Math.cos(dLon)
  return ((Math.atan2(y, x) * 180) / Math.PI + 360) % 360
}

interface SegmentInfo {
  cumDist: number
  segDist: number
}

function buildSegmentTable(coords: number[][]): SegmentInfo[] {
  const table: SegmentInfo[] = []
  let cumDist = 0
  for (let i = 0; i < coords.length - 1; i++) {
    const a = coords[i] as [number, number]
    const b = coords[i + 1] as [number, number]
    const segDist = haversineDistance(a, b)
    table.push({ cumDist, segDist })
    cumDist += segDist
  }
  return table
}

function interpolateAlongLine(
  coords: number[][],
  segTable: SegmentInfo[],
  totalDist: number,
  fraction: number
): { point: [number, number]; bearing: number } {
  const target = fraction * totalDist
  for (let i = 0; i < segTable.length; i++) {
    const { cumDist, segDist } = segTable[i]
    if (target <= cumDist + segDist || i === segTable.length - 1) {
      const t = segDist > 0 ? Math.min((target - cumDist) / segDist, 1) : 0
      const a = coords[i] as [number, number]
      const b = coords[i + 1] as [number, number]
      const point: [number, number] = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
      const bearing = bearingBetween(a, b)
      return { point, bearing }
    }
  }
  const last = coords[coords.length - 1] as [number, number]
  const prev = coords[coords.length - 2] as [number, number]
  return { point: last, bearing: bearingBetween(prev, last) }
}

function buildCumulativeInstructionDistances(route: RouteResult): number[] {
  let cumDist = 0
  return route.instructions.map((instr) => {
    const dist = cumDist
    cumDist += instr.distance_m
    return dist
  })
}

function formatDistanceShort(meters: number): string {
  if (meters < 1000) return `${Math.round(meters)}m`
  return `${(meters / 1000).toFixed(1)}km`
}

function formatDuration(seconds: number): string {
  const h = Math.floor(seconds / 3600)
  const m = Math.ceil((seconds % 3600) / 60)
  return h > 0 ? `${h}h ${m.toString().padStart(2, '0')}m` : `${m}m`
}

function createMarkerElement(): HTMLDivElement {
  const el = document.createElement('div')
  el.style.cssText = `
    width: 36px;
    height: 36px;
    border-radius: 50%;
    background: #2563eb;
    border: 3px solid #fff;
    box-shadow: 0 2px 8px rgba(0,0,0,0.4);
    display: flex;
    align-items: center;
    justify-content: center;
    position: relative;
  `
  el.innerHTML = `
    <svg width="16" height="16" viewBox="0 0 24 24" fill="none">
      <path d="M12 20V4" stroke="white" stroke-width="3" stroke-linecap="round"/>
      <path d="M6 10l6-6 6 6" stroke="white" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"/>
    </svg>
  `
  return el
}

export function NavigationView({ map, route, origin, destination, profile, onExit }: NavigationViewProps) {
  const [progress, setProgress] = useState(0)
  const [speed, setSpeed] = useState<SpeedMultiplier>(1)
  const [paused, setPaused] = useState(false)
  const [currentInstructionIndex, setCurrentInstructionIndex] = useState(0)
  const [distanceToNextTurn, setDistanceToNextTurn] = useState(0)
  const [showReport, setShowReport] = useState(false)
  const [reportSent, setReportSent] = useState(false)

  const progressRef = useRef(0)
  const speedRef = useRef<SpeedMultiplier>(1)
  const pausedRef = useRef(false)
  const rafRef = useRef<number | null>(null)
  const lastTimeRef = useRef<number | null>(null)
  const markerRef = useRef<maplibregl.Marker | null>(null)
  const savedCameraRef = useRef<{
    center: maplibregl.LngLatLike
    zoom: number
    pitch: number
    bearing: number
  } | null>(null)

  const coords = route.geometry.coordinates
  const segTableRef = useRef(buildSegmentTable(coords))
  const totalDistRef = useRef(
    segTableRef.current.reduce((sum, s) => sum + s.segDist, 0)
  )
  const instrCumDistsRef = useRef(buildCumulativeInstructionDistances(route))

  const stopAnimation = useCallback(() => {
    if (rafRef.current !== null) {
      cancelAnimationFrame(rafRef.current)
      rafRef.current = null
    }
  }, [])

  const startAnimation = useCallback(() => {
    if (!map) return
    lastTimeRef.current = null

    const tick = (now: number) => {
      if (lastTimeRef.current === null) {
        lastTimeRef.current = now
      }
      const elapsed = (now - lastTimeRef.current) / 1000
      lastTimeRef.current = now

      if (!pausedRef.current && progressRef.current < 1) {
        const distPerSecond = SIMULATED_BASE_SPEED_MS * speedRef.current
        const totalDist = totalDistRef.current
        const delta = totalDist > 0 ? (distPerSecond * elapsed) / totalDist : 0
        progressRef.current = Math.min(progressRef.current + delta, 1)

        const { point, bearing } = interpolateAlongLine(
          coords,
          segTableRef.current,
          totalDistRef.current,
          progressRef.current
        )

        map.easeTo({
          center: point,
          bearing,
          pitch: NAV_PITCH,
          zoom: NAV_ZOOM,
          duration: 100,
        })

        if (markerRef.current) {
          markerRef.current.setLngLat(point)
          const el = markerRef.current.getElement()
          el.style.transform = `rotate(${bearing}deg)`
        }

        const traveledDist = progressRef.current * totalDistRef.current
        const instrDists = instrCumDistsRef.current
        let activeIdx = 0
        for (let i = instrDists.length - 1; i >= 0; i--) {
          if (traveledDist >= instrDists[i]) {
            activeIdx = i
            break
          }
        }

        const nextInstrIdx = Math.min(activeIdx + 1, route.instructions.length - 1)
        const nextInstrStartDist = instrDists[nextInstrIdx] ?? totalDistRef.current
        const distToNext = Math.max(0, nextInstrStartDist - traveledDist)

        setProgress(progressRef.current)
        setCurrentInstructionIndex(activeIdx)
        setDistanceToNextTurn(distToNext)
      }

      if (progressRef.current < 1) {
        rafRef.current = requestAnimationFrame(tick)
      }
    }

    rafRef.current = requestAnimationFrame(tick)
  }, [map, coords, route.instructions.length])

  useEffect(() => {
    if (!map) return

    savedCameraRef.current = {
      center: map.getCenter(),
      zoom: map.getZoom(),
      pitch: map.getPitch(),
      bearing: map.getBearing(),
    }

    const markerEl = createMarkerElement()
    const startCoord = coords[0] as [number, number]
    const marker = new maplibregl.Marker({ element: markerEl, anchor: 'center' })
      .setLngLat(startCoord)
      .addTo(map)
    markerRef.current = marker

    const { bearing: initialBearing } = interpolateAlongLine(
      coords,
      segTableRef.current,
      totalDistRef.current,
      0
    )
    map.easeTo({
      center: startCoord,
      bearing: initialBearing,
      pitch: NAV_PITCH,
      zoom: NAV_ZOOM,
      duration: 800,
    })

    startAnimation()

    return () => {
      stopAnimation()
      markerRef.current?.remove()
      markerRef.current = null
      if (savedCameraRef.current) {
        map.easeTo({
          ...savedCameraRef.current,
          duration: 600,
        })
      }
    }
  }, [map, coords, startAnimation, stopAnimation])

  useEffect(() => {
    speedRef.current = speed
  }, [speed])

  useEffect(() => {
    pausedRef.current = paused
  }, [paused])

  const handleReport = async () => {
    if (!origin || !destination) return
    setReportSent(true)
    await submitContribution({
      route_origin: origin,
      route_destination: destination,
      profile: profile ?? 'car',
      issue_type: 'roundabout_error',
      description: `Reported during navigation at instruction ${currentInstructionIndex}`,
    })
    setTimeout(() => {
      setShowReport(false)
      setReportSent(false)
    }, 2000)
  }

  const handleExit = () => {
    stopAnimation()
    onExit()
  }

  const currentInstr = route.instructions[currentInstructionIndex]
  const nextInstr = route.instructions[currentInstructionIndex + 1] ?? null
  const displayInstr = nextInstr ?? currentInstr
  const isApproachingTurn = distanceToNextTurn > 0 && distanceToNextTurn <= TURN_ALERT_DISTANCE_M
  const remainingDist = (1 - progress) * route.distance_m
  const remainingTime = (1 - progress) * route.duration_s
  const arrived = progress >= 1
  const instrType = arrived ? 'arrive' : (displayInstr?.type ?? 'straight')
  const instrLabel = arrived
    ? 'You have arrived'
    : (INSTRUCTION_LABELS[instrType] ?? instrType)
  const profileLabel = profile === 'motorcycle'
    ? 'Moto'
    : profile
      ? profile.charAt(0).toUpperCase() + profile.slice(1)
      : 'Car'
  const progressPct = Math.round(progress * 100)

  return (
    <div style={navStyles.overlay}>
      <div style={navStyles.floatingCard(isApproachingTurn, arrived)}>
        <div style={navStyles.headerRow}>
          <span style={navStyles.statusBadge(isApproachingTurn, arrived)}>
            {arrived ? 'Trip complete' : isApproachingTurn ? 'Turn incoming' : `${profileLabel} preview`}
          </span>
          <div style={navStyles.headerActions}>
            <button
              style={navStyles.headerBtn}
              onClick={() => setShowReport((visible) => !visible)}
              aria-label="Report issue"
            >
              Report
            </button>
            <button style={navStyles.headerBtn} onClick={handleExit} aria-label="Exit navigation">
              Exit
            </button>
          </div>
        </div>

        {showReport && (
          <div style={navReportStyles.reportOverlay}>
            {reportSent ? (
              <div style={navReportStyles.reportSuccess}>Report sent!</div>
            ) : (
              <button style={navReportStyles.reportSubmitBtn} onClick={handleReport}>
                Report Issue Here
              </button>
            )}
          </div>
        )}

        <div style={navStyles.arrowRow}>
          <div style={navStyles.arrowContainer(isApproachingTurn)}>
            <TurnArrow type={instrType} size={28} />
          </div>
          <div style={navStyles.distanceBlock}>
            {!arrived && distanceToNextTurn > 0 && (
              <div style={navStyles.distanceLarge}>{formatDistanceShort(distanceToNextTurn)}</div>
            )}
            {arrived && <div style={navStyles.distanceLarge}>--</div>}
          </div>
        </div>

        <div style={navStyles.instrLabel}>{instrLabel}</div>

        {displayInstr?.road && !arrived && (
          <div style={navStyles.roadName}>{displayInstr.road}</div>
        )}

        {nextInstr && !arrived && currentInstr && (
          <div style={navStyles.nextInstr}>
            Then: {INSTRUCTION_LABELS[nextInstr.type] ?? nextInstr.type}
            {nextInstr.road ? ` onto ${nextInstr.road}` : ''}
          </div>
        )}

        <div style={navStyles.progressRow}>
          <div style={navStyles.progressTrack}>
            <div style={navStyles.progressBar(progressPct)} />
          </div>
          <span style={navStyles.progressLabel}>{progressPct}%</span>
        </div>

        <div style={navStyles.speedRow}>
          <button
            style={navStyles.pauseBtn}
            onClick={() => setPaused((p) => !p)}
            aria-label={paused ? 'Resume' : 'Pause'}
          >
            {paused ? '▶' : '⏸'}
          </button>
          {([1, 2, 5, 10] as SpeedMultiplier[]).map((s) => (
            <button
              key={s}
              style={navStyles.speedBtn(speed === s)}
              onClick={() => setSpeed(s)}
            >
              {s}x
            </button>
          ))}
        </div>
      </div>

      <div style={navStyles.summaryStrip}>
        <span style={navStyles.summaryText}>
          {formatDistanceShort(remainingDist)} · {formatDuration(remainingTime)} remaining
        </span>
      </div>
    </div>
  )
}

const cardBg = 'rgba(10,18,27,0.88)'
const cardBorder = 'rgba(255,255,255,0.1)'

const navStyles = {
  overlay: {
    position: 'fixed' as const,
    inset: 0,
    zIndex: 30,
    pointerEvents: 'none' as const,
    display: 'flex',
    flexDirection: 'column' as const,
    justifyContent: 'flex-end',
    alignItems: 'flex-end',
  },
  floatingCard: (alert: boolean, arrived: boolean) => ({
    pointerEvents: 'auto' as const,
    background: arrived
      ? 'rgba(7,94,84,0.94)'
      : alert
        ? 'rgba(146,64,14,0.96)'
        : cardBg,
    backdropFilter: 'blur(18px)',
    WebkitBackdropFilter: 'blur(18px)',
    border: `1px solid ${alert ? 'rgba(251,191,36,0.32)' : cardBorder}`,
    borderRadius: 24,
    padding: '16px 18px 16px',
    margin: '0 20px 82px',
    width: 'min(360px, calc(100vw - 24px))',
    boxShadow: '0 18px 52px rgba(0,0,0,0.36)',
    position: 'relative' as const,
  }),
  headerRow: {
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'space-between',
    gap: 12,
    marginBottom: 12,
  },
  statusBadge: (alert: boolean, arrived: boolean) => ({
    display: 'inline-flex',
    alignItems: 'center',
    borderRadius: 999,
    padding: '8px 10px',
    background: arrived
      ? 'rgba(255,255,255,0.18)'
      : alert
        ? 'rgba(251,191,36,0.18)'
        : 'rgba(255,255,255,0.12)',
    color: '#fff',
    fontSize: 11,
    fontWeight: 800,
    letterSpacing: '0.12em',
    textTransform: 'uppercase' as const,
  }),
  headerActions: {
    display: 'flex',
    gap: 8,
  },
  headerBtn: {
    border: 'none',
    borderRadius: 999,
    padding: '9px 12px',
    background: 'rgba(255,255,255,0.12)',
    color: '#fff',
    fontSize: 12,
    fontWeight: 700,
    cursor: 'pointer',
  },
  arrowRow: {
    display: 'flex',
    alignItems: 'center',
    gap: 14,
    marginBottom: 12,
  },
  arrowContainer: (alert: boolean) => ({
    width: 58,
    height: 58,
    borderRadius: 18,
    background: alert ? 'rgba(251,191,36,0.24)' : 'rgba(20,184,166,0.24)',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    flexShrink: 0,
    boxShadow: 'inset 0 1px 0 rgba(255,255,255,0.1)',
  }),
  distanceBlock: {
    flex: 1,
  },
  distanceLarge: {
    color: '#fff',
    fontSize: 34,
    fontWeight: 800,
    fontFamily: 'system-ui, sans-serif',
    fontVariantNumeric: 'tabular-nums' as const,
    lineHeight: 1,
  },
  instrLabel: {
    color: '#fff',
    fontSize: 18,
    fontWeight: 700,
    fontFamily: 'system-ui, sans-serif',
    lineHeight: 1.3,
    marginBottom: 4,
  },
  roadName: {
    color: 'rgba(255,255,255,0.76)',
    fontSize: 13,
    fontFamily: 'system-ui, sans-serif',
    marginBottom: 6,
  },
  nextInstr: {
    color: 'rgba(255,255,255,0.62)',
    fontSize: 12,
    fontFamily: 'system-ui, sans-serif',
    borderTop: '1px solid rgba(255,255,255,0.1)',
    paddingTop: 10,
    marginTop: 8,
    marginBottom: 10,
  },
  progressRow: {
    display: 'flex',
    alignItems: 'center',
    gap: 10,
  },
  progressTrack: {
    flex: 1,
    height: 8,
    borderRadius: 999,
    overflow: 'hidden' as const,
    background: 'rgba(255,255,255,0.12)',
  },
  progressBar: (progressPct: number) => ({
    width: `${progressPct}%`,
    height: '100%',
    borderRadius: 999,
    background: 'linear-gradient(90deg, #34d399 0%, #f59e0b 100%)',
  }),
  progressLabel: {
    color: 'rgba(255,255,255,0.72)',
    fontSize: 12,
    fontWeight: 700,
    fontVariantNumeric: 'tabular-nums' as const,
  },
  speedRow: {
    display: 'flex',
    gap: 5,
    alignItems: 'center',
    marginTop: 10,
    borderTop: '1px solid rgba(255,255,255,0.1)',
    paddingTop: 10,
  },
  pauseBtn: {
    background: 'rgba(255,255,255,0.15)',
    color: '#fff',
    border: '1px solid rgba(255,255,255,0.2)',
    borderRadius: 20,
    padding: '6px 12px',
    fontSize: 12,
    fontWeight: 600,
    fontFamily: 'system-ui, sans-serif',
    cursor: 'pointer',
    marginRight: 2,
  },
  speedBtn: (active: boolean) => ({
    background: active ? '#14b8a6' : 'rgba(255,255,255,0.1)',
    color: '#fff',
    border: active ? '1px solid rgba(255,255,255,0.2)' : '1px solid transparent',
    borderRadius: 20,
    padding: '6px 12px',
    fontSize: 12,
    fontWeight: 600,
    fontFamily: 'system-ui, sans-serif',
    cursor: 'pointer',
    transition: 'background 0.15s',
  }),
  summaryStrip: {
    pointerEvents: 'none' as const,
    position: 'fixed' as const,
    bottom: 18,
    left: '50%',
    transform: 'translateX(-50%)',
    background: 'rgba(10,18,27,0.72)',
    border: '1px solid rgba(255,255,255,0.12)',
    backdropFilter: 'blur(12px)',
    WebkitBackdropFilter: 'blur(12px)',
    padding: '10px 16px',
    display: 'flex',
    alignItems: 'center',
    justifyContent: 'center',
    zIndex: 31,
    borderRadius: 999,
    boxShadow: '0 16px 36px rgba(0,0,0,0.28)',
    maxWidth: 'calc(100vw - 24px)',
  },
  summaryText: {
    color: 'rgba(255,255,255,0.85)',
    fontSize: 13,
    fontWeight: 600,
    fontFamily: 'system-ui, sans-serif',
    fontVariantNumeric: 'tabular-nums' as const,
    letterSpacing: '0.01em',
  },
}

const navReportStyles = {
  reportOverlay: {
    marginBottom: 12,
    padding: '10px 0 12px',
    borderBottom: '1px solid rgba(255,255,255,0.1)',
  },
  reportSubmitBtn: {
    width: '100%',
    padding: '10px 12px',
    background: 'rgba(251,191,36,0.18)',
    color: '#fde68a',
    border: '1px solid rgba(251,191,36,0.28)',
    borderRadius: 12,
    cursor: 'pointer',
    fontSize: 12,
    fontWeight: 600 as const,
    fontFamily: 'system-ui, sans-serif',
  },
  reportSuccess: {
    padding: '10px 12px',
    color: '#bbf7d0',
    fontSize: 12,
    fontWeight: 600 as const,
    fontFamily: 'system-ui, sans-serif',
    textAlign: 'center' as const,
  },
}

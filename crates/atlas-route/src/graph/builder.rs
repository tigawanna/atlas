use std::collections::{HashMap, HashSet};
use std::path::Path;

use atlas_core::bbox::AFRICA;
use atlas_core::AtlasError;
use osmpbf::{Element, ElementReader};

use super::edge::{make_flags, Access, Edge, RoadClass, Surface};
use super::road_network::{RoadGeometry, RoadGraph};

struct WayData {
    node_refs: Vec<i64>,
    road_class: RoadClass,
    surface: Surface,
    oneway: OnewayDirection,
    roundabout: bool,
    foot: Access,
    bicycle: Access,
    name: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OnewayDirection {
    Bidirectional,
    Forward,
    Reverse,
}

fn haversine_m(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let r = 6_371_000.0_f32;
    let dlat = (lat2 - lat1).to_radians();
    let dlon = (lon2 - lon1).to_radians();
    let a = (dlat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (dlon / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

fn parse_highway_tag(value: &str) -> Option<RoadClass> {
    match value {
        "motorway" | "motorway_link" => Some(RoadClass::Motorway),
        "trunk" | "trunk_link" => Some(RoadClass::Trunk),
        "primary" | "primary_link" => Some(RoadClass::Primary),
        "secondary" | "secondary_link" => Some(RoadClass::Secondary),
        "tertiary" | "tertiary_link" => Some(RoadClass::Tertiary),
        "unclassified" | "residential" | "living_street" | "service" => {
            Some(RoadClass::Residential)
        }
        "track" => Some(RoadClass::Track),
        "path" | "footway" | "cycleway" => Some(RoadClass::Path),
        _ => None,
    }
}

fn parse_surface_tag(value: &str) -> Surface {
    match value {
        "paved" | "asphalt" | "concrete" | "concrete:plates" | "concrete:lanes" | "sett"
        | "cobblestone" | "paving_stones" => Surface::Paved,
        "unpaved" | "gravel" | "fine_gravel" | "compacted" | "dirt" | "earth" | "sand" | "mud"
        | "ground" => Surface::Unpaved,
        "grass" | "grass_paver" => Surface::Track,
        _ => Surface::Unknown,
    }
}

fn parse_access_tag(value: &str) -> Access {
    match value {
        "yes" | "permissive" => Access::Yes,
        "no" | "private" => Access::No,
        "designated" => Access::Designated,
        _ => Access::Yes,
    }
}

fn default_speed_kmh(road_class: RoadClass) -> f64 {
    match road_class {
        RoadClass::Motorway => 120.0,
        RoadClass::Trunk => 100.0,
        RoadClass::Primary => 80.0,
        RoadClass::Secondary => 60.0,
        RoadClass::Tertiary => 50.0,
        RoadClass::Residential => 30.0,
        RoadClass::Track => 20.0,
        RoadClass::Path => 10.0,
    }
}

fn compute_time_ds(distance_m: f32, speed_kmh: f64) -> u32 {
    if speed_kmh <= 0.0 {
        return u32::MAX;
    }
    let speed_ms = speed_kmh * 1000.0 / 3600.0;
    let time_ds = (distance_m as f64 / speed_ms) * 10.0;
    (time_ds as u64).min(u32::MAX as u64) as u32
}

pub fn build_road_graph(osm_dir: &Path) -> Result<(RoadGraph, RoadGeometry), AtlasError> {
    let pbf_files = collect_pbf_files(osm_dir)?;
    if pbf_files.is_empty() {
        return Err(AtlasError::StoreError(format!(
            "no *.osm.pbf files found in {}",
            osm_dir.display()
        )));
    }

    let (ways, referenced_nodes) = pass1_collect_ways(&pbf_files)?;
    tracing::info!(
        "pass 1 complete: {} ways, {} referenced nodes",
        ways.len(),
        referenced_nodes.len()
    );

    let node_coords = pass1_collect_nodes(&pbf_files, &referenced_nodes)?;
    tracing::info!("resolved {} node coordinates", node_coords.len());

    let node_ref_count = count_node_references(&ways);
    let (graph, geometry) = pass2_build_edges(&ways, &node_coords, &node_ref_count)?;
    tracing::info!(
        "graph built: {} nodes, {} edges, {} geometry segments",
        graph.num_nodes(),
        graph.num_edges(),
        geometry.num_segments()
    );

    Ok((graph, geometry))
}

fn collect_pbf_files(dir: &Path) -> Result<Vec<std::path::PathBuf>, AtlasError> {
    let entries = std::fs::read_dir(dir).map_err(|e| {
        AtlasError::StoreError(format!("failed to read directory {}: {}", dir.display(), e))
    })?;

    let mut files = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|e| AtlasError::StoreError(e.to_string()))?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("pbf")
            && path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|n| n.contains(".osm."))
                .unwrap_or(false)
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn pass1_collect_ways(
    pbf_files: &[std::path::PathBuf],
) -> Result<(Vec<WayData>, HashSet<i64>), AtlasError> {
    let mut ways = Vec::new();
    let mut referenced_nodes = HashSet::new();

    for pbf_path in pbf_files {
        let reader = ElementReader::from_path(pbf_path).map_err(|e| {
            AtlasError::StoreError(format!("failed to open PBF {}: {}", pbf_path.display(), e))
        })?;

        let mut file_ways: Vec<WayData> = Vec::new();
        reader
            .for_each(|element| {
                if let Element::Way(way) = element {
                    let mut highway = None;
                    let mut surface_tag = None;
                    let mut oneway_tag = None;
                    let mut foot_tag = None;
                    let mut bicycle_tag = None;
                    let mut junction_tag = None;
                    let mut name = None;

                    for (key, value) in way.tags() {
                        match key {
                            "highway" => highway = Some(value.to_string()),
                            "surface" => surface_tag = Some(value.to_string()),
                            "oneway" => oneway_tag = Some(value.to_string()),
                            "foot" => foot_tag = Some(value.to_string()),
                            "bicycle" => bicycle_tag = Some(value.to_string()),
                            "junction" => junction_tag = Some(value.to_string()),
                            "name" => name = Some(value.to_string()),
                            _ => {}
                        }
                    }

                    let road_class = match highway.as_deref().and_then(parse_highway_tag) {
                        Some(rc) => rc,
                        None => return,
                    };

                    let node_refs: Vec<i64> = way.refs().collect();
                    if node_refs.len() < 2 {
                        return;
                    }

                    let surface = surface_tag
                        .as_deref()
                        .map(parse_surface_tag)
                        .unwrap_or(Surface::Unknown);

                    let is_roundabout = junction_tag.as_deref() == Some("roundabout");

                    let is_motorway = matches!(road_class, RoadClass::Motorway);
                    let oneway = match oneway_tag.as_deref() {
                        Some("yes") | Some("1") | Some("true") => OnewayDirection::Forward,
                        Some("-1") => OnewayDirection::Reverse,
                        _ if is_motorway || is_roundabout => OnewayDirection::Forward,
                        _ => OnewayDirection::Bidirectional,
                    };

                    let foot = foot_tag
                        .as_deref()
                        .map(parse_access_tag)
                        .unwrap_or(Access::Yes);
                    let bicycle = bicycle_tag
                        .as_deref()
                        .map(parse_access_tag)
                        .unwrap_or(Access::Yes);

                    file_ways.push(WayData {
                        node_refs,
                        road_class,
                        surface,
                        oneway,
                        roundabout: is_roundabout,
                        foot,
                        bicycle,
                        name,
                    });
                }
            })
            .map_err(|e| AtlasError::StoreError(format!("PBF read error: {}", e)))?;

        for way in &file_ways {
            for &node_id in &way.node_refs {
                referenced_nodes.insert(node_id);
            }
        }
        ways.extend(file_ways);
    }

    Ok((ways, referenced_nodes))
}

fn pass1_collect_nodes(
    pbf_files: &[std::path::PathBuf],
    referenced: &HashSet<i64>,
) -> Result<HashMap<i64, (f32, f32)>, AtlasError> {
    let mut node_coords: HashMap<i64, (f32, f32)> = HashMap::with_capacity(referenced.len());

    for pbf_path in pbf_files {
        let reader = ElementReader::from_path(pbf_path).map_err(|e| {
            AtlasError::StoreError(format!("failed to open PBF {}: {}", pbf_path.display(), e))
        })?;

        let mut file_nodes: Vec<(i64, f32, f32)> = Vec::new();
        reader
            .for_each(|element| match element {
                Element::Node(node) => {
                    let id = node.id();
                    if referenced.contains(&id) {
                        file_nodes.push((id, node.lat() as f32, node.lon() as f32));
                    }
                }
                Element::DenseNode(node) => {
                    let id = node.id;
                    if referenced.contains(&id) {
                        file_nodes.push((id, node.lat() as f32, node.lon() as f32));
                    }
                }
                _ => {}
            })
            .map_err(|e| AtlasError::StoreError(format!("PBF read error: {}", e)))?;

        for (id, lat, lon) in file_nodes {
            node_coords.insert(id, (lat, lon));
        }
    }

    Ok(node_coords)
}

fn count_node_references(ways: &[WayData]) -> HashMap<i64, u32> {
    let mut counts: HashMap<i64, u32> = HashMap::new();
    for way in ways {
        for &node_id in &way.node_refs {
            *counts.entry(node_id).or_insert(0) += 1;
        }
    }
    counts
}

struct RawEdge {
    source: u32,
    target: u32,
    distance_m: f32,
    road_class: RoadClass,
    surface: Surface,
    oneway: bool,
    roundabout: bool,
    foot: Access,
    bicycle: Access,
    segment_lats: Vec<f32>,
    segment_lons: Vec<f32>,
    name: Option<String>,
}

fn make_raw_edge(
    source: u32,
    target: u32,
    distance_m: f32,
    way: &WayData,
    segment_lats: Vec<f32>,
    segment_lons: Vec<f32>,
) -> RawEdge {
    RawEdge {
        source,
        target,
        distance_m,
        road_class: way.road_class,
        surface: way.surface,
        oneway: !matches!(way.oneway, OnewayDirection::Bidirectional),
        roundabout: way.roundabout,
        foot: way.foot,
        bicycle: way.bicycle,
        segment_lats,
        segment_lons,
        name: way.name.clone(),
    }
}

fn pass2_build_edges(
    ways: &[WayData],
    node_coords: &HashMap<i64, (f32, f32)>,
    node_ref_count: &HashMap<i64, u32>,
) -> Result<(RoadGraph, RoadGeometry), AtlasError> {
    let mut osm_to_compact: HashMap<i64, u32> = HashMap::new();
    let mut compact_lats: Vec<f32> = Vec::new();
    let mut compact_lons: Vec<f32> = Vec::new();
    let mut raw_edges: Vec<RawEdge> = Vec::new();

    let get_or_insert_node = |osm_id: i64,
                              coords: &HashMap<i64, (f32, f32)>,
                              lats: &mut Vec<f32>,
                              lons: &mut Vec<f32>,
                              map: &mut HashMap<i64, u32>|
     -> Option<u32> {
        if let Some(&compact_id) = map.get(&osm_id) {
            return Some(compact_id);
        }
        let &(lat, lon) = coords.get(&osm_id)?;
        if !AFRICA.contains(lon as f64, lat as f64) {
            return None;
        }
        let id = lats.len() as u32;
        lats.push(lat);
        lons.push(lon);
        map.insert(osm_id, id);
        Some(id)
    };

    for way in ways {
        let is_intersection = |idx: usize| -> bool {
            if idx == 0 || idx == way.node_refs.len() - 1 {
                return true;
            }
            node_ref_count
                .get(&way.node_refs[idx])
                .map(|&c| c >= 2)
                .unwrap_or(false)
        };

        let mut segment_start = 0;
        for idx in 1..way.node_refs.len() {
            if !is_intersection(idx) {
                continue;
            }

            let start_osm = way.node_refs[segment_start];
            let end_osm = way.node_refs[idx];

            let source = match get_or_insert_node(
                start_osm,
                node_coords,
                &mut compact_lats,
                &mut compact_lons,
                &mut osm_to_compact,
            ) {
                Some(id) => id,
                None => {
                    segment_start = idx;
                    continue;
                }
            };

            let target = match get_or_insert_node(
                end_osm,
                node_coords,
                &mut compact_lats,
                &mut compact_lons,
                &mut osm_to_compact,
            ) {
                Some(id) => id,
                None => {
                    segment_start = idx;
                    continue;
                }
            };

            let mut distance = 0.0_f32;
            let mut seg_lats = Vec::new();
            let mut seg_lons = Vec::new();
            let mut all_coords_valid = true;

            for i in segment_start..=idx {
                let osm_id = way.node_refs[i];
                if let Some(&(lat, lon)) = node_coords.get(&osm_id) {
                    if i > segment_start {
                        let prev_osm = way.node_refs[i - 1];
                        if let Some(&(plat, plon)) = node_coords.get(&prev_osm) {
                            distance += haversine_m(plat, plon, lat, lon);
                        }
                    }
                    seg_lats.push(lat);
                    seg_lons.push(lon);
                } else {
                    all_coords_valid = false;
                    break;
                }
            }

            if !all_coords_valid || distance <= 0.0 {
                segment_start = idx;
                continue;
            }

            if !matches!(way.oneway, OnewayDirection::Reverse) {
                raw_edges.push(make_raw_edge(
                    source,
                    target,
                    distance,
                    way,
                    seg_lats.clone(),
                    seg_lons.clone(),
                ));
            }

            if !matches!(way.oneway, OnewayDirection::Forward) && source != target {
                let mut rev_lats = seg_lats;
                let mut rev_lons = seg_lons;
                rev_lats.reverse();
                rev_lons.reverse();

                raw_edges.push(make_raw_edge(
                    target, source, distance, way, rev_lats, rev_lons,
                ));
            }

            segment_start = idx;
        }
    }

    let num_nodes = compact_lats.len();

    raw_edges.sort_by_key(|e| e.source);

    let mut first_edge = vec![0u32; num_nodes + 1];
    let mut edges = Vec::with_capacity(raw_edges.len());
    let mut geo_first_point = Vec::with_capacity(raw_edges.len() + 1);
    let mut geo_lats = Vec::new();
    let mut geo_lons = Vec::new();
    let mut road_names = Vec::with_capacity(raw_edges.len());

    let mut current_node = 0u32;
    for (edge_idx, raw) in raw_edges.iter().enumerate() {
        while current_node <= raw.source {
            first_edge[current_node as usize] = edge_idx as u32;
            current_node += 1;
        }

        let dist_m = raw.distance_m.min(u32::MAX as f32) as u32;
        let speed = default_speed_kmh(raw.road_class);
        let time_ds = compute_time_ds(raw.distance_m, speed);
        let flags = make_flags(
            raw.road_class,
            raw.surface,
            raw.oneway,
            false,
            raw.foot,
            raw.bicycle,
            raw.roundabout,
        );

        let geo_index = geo_first_point.len() as u32;
        geo_first_point.push(geo_lats.len() as u32);
        geo_lats.extend_from_slice(&raw.segment_lats);
        geo_lons.extend_from_slice(&raw.segment_lons);
        road_names.push(raw.name.clone());

        edges.push(Edge {
            target: raw.target,
            geo_index,
            shortcut_mid: 0,
            distance_m: dist_m,
            time_ds,
            flags,
            _padding: 0,
        });
    }

    while (current_node as usize) <= num_nodes {
        first_edge[current_node as usize] = edges.len() as u32;
        current_node += 1;
    }

    geo_first_point.push(geo_lats.len() as u32);

    let graph = RoadGraph {
        first_edge,
        edges,
        node_lat: compact_lats,
        node_lon: compact_lons,
    };

    let geometry = RoadGeometry {
        first_point: geo_first_point,
        coords_lat: geo_lats,
        coords_lon: geo_lons,
        road_names,
    };

    Ok((graph, geometry))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn haversine_known_distance() {
        let dist = haversine_m(5.6037, -0.1870, 6.6885, -1.6244);
        assert!((dist - 199_500.0).abs() < 1_000.0);
    }

    #[test]
    fn parse_highway_tags() {
        assert_eq!(parse_highway_tag("motorway"), Some(RoadClass::Motorway));
        assert_eq!(
            parse_highway_tag("motorway_link"),
            Some(RoadClass::Motorway)
        );
        assert_eq!(parse_highway_tag("trunk"), Some(RoadClass::Trunk));
        assert_eq!(parse_highway_tag("primary"), Some(RoadClass::Primary));
        assert_eq!(
            parse_highway_tag("secondary_link"),
            Some(RoadClass::Secondary)
        );
        assert_eq!(
            parse_highway_tag("residential"),
            Some(RoadClass::Residential)
        );
        assert_eq!(
            parse_highway_tag("living_street"),
            Some(RoadClass::Residential)
        );
        assert_eq!(parse_highway_tag("service"), Some(RoadClass::Residential));
        assert_eq!(parse_highway_tag("track"), Some(RoadClass::Track));
        assert_eq!(parse_highway_tag("footway"), Some(RoadClass::Path));
        assert_eq!(parse_highway_tag("cycleway"), Some(RoadClass::Path));
        assert_eq!(parse_highway_tag("path"), Some(RoadClass::Path));
        assert_eq!(parse_highway_tag("steps"), None);
        assert_eq!(parse_highway_tag("construction"), None);
    }

    #[test]
    fn parse_surface_tags() {
        assert_eq!(parse_surface_tag("asphalt"), Surface::Paved);
        assert_eq!(parse_surface_tag("concrete"), Surface::Paved);
        assert_eq!(parse_surface_tag("gravel"), Surface::Unpaved);
        assert_eq!(parse_surface_tag("dirt"), Surface::Unpaved);
        assert_eq!(parse_surface_tag("grass"), Surface::Track);
        assert_eq!(parse_surface_tag("something_weird"), Surface::Unknown);
    }

    #[test]
    fn compute_time_ds_reasonable() {
        let time = compute_time_ds(1000.0, 60.0);
        let expected = (1000.0 / (60.0 * 1000.0 / 3600.0)) * 10.0;
        assert!((time as f64 - expected).abs() < 2.0);
    }

    #[test]
    fn compute_time_ds_zero_speed() {
        assert_eq!(compute_time_ds(1000.0, 0.0), u32::MAX);
    }

    #[test]
    fn reverse_oneway_creates_reverse_only_edge() {
        let way = WayData {
            node_refs: vec![1, 2],
            road_class: RoadClass::Primary,
            surface: Surface::Paved,
            oneway: OnewayDirection::Reverse,
            roundabout: false,
            foot: Access::Yes,
            bicycle: Access::Yes,
            name: Some("Reverse Only".to_string()),
        };
        let node_coords = HashMap::from([(1_i64, (5.0_f32, -0.1_f32)), (2_i64, (5.001, -0.101))]);
        let node_ref_count = HashMap::from([(1_i64, 1_u32), (2_i64, 1_u32)]);

        let (graph, geometry) = pass2_build_edges(&[way], &node_coords, &node_ref_count).unwrap();

        assert_eq!(graph.num_edges(), 1);
        assert!(graph.edges_of(0).is_empty());
        assert_eq!(graph.edges_of(1)[0].target, 0);
        assert_eq!(geometry.num_segments(), 1);
    }
}

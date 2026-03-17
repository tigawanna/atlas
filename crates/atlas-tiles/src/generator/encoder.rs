use std::collections::HashMap;

pub mod vector_tile {
    include!(concat!(env!("OUT_DIR"), "/vector_tile.rs"));
}

use vector_tile::tile;

const MVT_EXTENT: u32 = 4096;
const CMD_MOVE_TO: u32 = 1;
const CMD_LINE_TO: u32 = 2;
const CMD_CLOSE_PATH: u32 = 7;

#[derive(Debug, Clone, PartialEq)]
pub enum GeomType {
    Point,
    LineString,
    Polygon,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PropertyValue {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
}

#[derive(Debug, Clone)]
pub struct TileFeature {
    pub id: u64,
    pub geom_type: GeomType,
    pub geometry: Vec<(f64, f64)>,
    pub properties: Vec<(String, PropertyValue)>,
}

pub fn zigzag_encode(n: i32) -> u32 {
    ((n << 1) ^ (n >> 31)) as u32
}

pub fn command(id: u32, count: u32) -> u32 {
    (id & 0x7) | (count << 3)
}

pub fn wgs84_to_tile_coords(lat: f64, lon: f64, z: u8, x: u32, y: u32, extent: u32) -> (i32, i32) {
    let n = (1u64 << z) as f64;
    let tile_x_frac = (lon + 180.0) / 360.0 * n - x as f64;
    let lat_rad = lat.to_radians();
    let tile_y_frac =
        (1.0 - (lat_rad.tan() + 1.0 / lat_rad.cos()).ln() / std::f64::consts::PI) / 2.0 * n
            - y as f64;

    let px = (tile_x_frac * extent as f64).round() as i32;
    let py = (tile_y_frac * extent as f64).round() as i32;
    (px, py)
}

pub fn encode_geometry_point(x: i32, y: i32) -> Vec<u32> {
    vec![command(CMD_MOVE_TO, 1), zigzag_encode(x), zigzag_encode(y)]
}

pub fn encode_geometry_linestring(coords: &[(i32, i32)]) -> Vec<u32> {
    if coords.len() < 2 {
        return vec![];
    }

    let mut result = Vec::with_capacity(3 + (coords.len() - 1) * 2 + 1);

    result.push(command(CMD_MOVE_TO, 1));
    result.push(zigzag_encode(coords[0].0));
    result.push(zigzag_encode(coords[0].1));

    result.push(command(CMD_LINE_TO, (coords.len() - 1) as u32));
    let mut prev_x = coords[0].0;
    let mut prev_y = coords[0].1;

    for &(cx, cy) in &coords[1..] {
        let dx = cx - prev_x;
        let dy = cy - prev_y;
        result.push(zigzag_encode(dx));
        result.push(zigzag_encode(dy));
        prev_x = cx;
        prev_y = cy;
    }

    result
}

pub fn encode_geometry_polygon(coords: &[(i32, i32)]) -> Vec<u32> {
    if coords.len() < 3 {
        return vec![];
    }

    let mut result = Vec::with_capacity(3 + (coords.len() - 1) * 2 + 1 + 1);

    result.push(command(CMD_MOVE_TO, 1));
    result.push(zigzag_encode(coords[0].0));
    result.push(zigzag_encode(coords[0].1));

    let line_to_count = coords.len() as u32 - 1;
    result.push(command(CMD_LINE_TO, line_to_count));
    let mut prev_x = coords[0].0;
    let mut prev_y = coords[0].1;

    for &(cx, cy) in &coords[1..] {
        let dx = cx - prev_x;
        let dy = cy - prev_y;
        result.push(zigzag_encode(dx));
        result.push(zigzag_encode(dy));
        prev_x = cx;
        prev_y = cy;
    }

    result.push(command(CMD_CLOSE_PATH, 1));

    result
}

fn property_to_value(pv: &PropertyValue) -> tile::Value {
    match pv {
        PropertyValue::String(s) => tile::Value {
            string_value: Some(s.clone()),
            ..Default::default()
        },
        PropertyValue::Int(i) => tile::Value {
            int_value: Some(*i),
            ..Default::default()
        },
        PropertyValue::Float(f) => tile::Value {
            double_value: Some(*f),
            ..Default::default()
        },
        PropertyValue::Bool(b) => tile::Value {
            bool_value: Some(*b),
            ..Default::default()
        },
    }
}

pub fn encode_tile(features: &[TileFeature], layer_name: &str, z: u8, x: u32, y: u32) -> Vec<u8> {
    let mut keys_map: HashMap<String, u32> = HashMap::new();
    let mut keys_vec: Vec<String> = Vec::new();
    let mut values_map: HashMap<String, u32> = HashMap::new();
    let mut values_vec: Vec<tile::Value> = Vec::new();
    let mut pb_features: Vec<tile::Feature> = Vec::new();

    for feat in features {
        let tile_coords: Vec<(i32, i32)> = feat
            .geometry
            .iter()
            .map(|&(lat, lon)| wgs84_to_tile_coords(lat, lon, z, x, y, MVT_EXTENT))
            .collect();

        let geometry = match feat.geom_type {
            GeomType::Point => {
                if let Some(&(px, py)) = tile_coords.first() {
                    encode_geometry_point(px, py)
                } else {
                    continue;
                }
            }
            GeomType::LineString => {
                let encoded = encode_geometry_linestring(&tile_coords);
                if encoded.is_empty() {
                    continue;
                }
                encoded
            }
            GeomType::Polygon => {
                let encoded = encode_geometry_polygon(&tile_coords);
                if encoded.is_empty() {
                    continue;
                }
                encoded
            }
        };

        let geom_type = match feat.geom_type {
            GeomType::Point => tile::GeomType::Point,
            GeomType::LineString => tile::GeomType::Linestring,
            GeomType::Polygon => tile::GeomType::Polygon,
        };

        let mut tags = Vec::with_capacity(feat.properties.len() * 2);
        for (key, val) in &feat.properties {
            let key_idx = match keys_map.get(key) {
                Some(&idx) => idx,
                None => {
                    let idx = keys_vec.len() as u32;
                    keys_vec.push(key.clone());
                    keys_map.insert(key.clone(), idx);
                    idx
                }
            };

            let val_key = format!("{val:?}");
            let val_idx = match values_map.get(&val_key) {
                Some(&idx) => idx,
                None => {
                    let idx = values_vec.len() as u32;
                    values_vec.push(property_to_value(val));
                    values_map.insert(val_key, idx);
                    idx
                }
            };

            tags.push(key_idx);
            tags.push(val_idx);
        }

        pb_features.push(tile::Feature {
            id: Some(feat.id),
            tags,
            r#type: Some(geom_type.into()),
            geometry,
        });
    }

    let layer = tile::Layer {
        version: 2,
        name: layer_name.to_string(),
        features: pb_features,
        keys: keys_vec,
        values: values_vec,
        extent: Some(MVT_EXTENT),
    };

    let tile_msg = vector_tile::Tile {
        layers: vec![layer],
    };

    prost::Message::encode_to_vec(&tile_msg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zigzag_positive_values() {
        assert_eq!(zigzag_encode(0), 0);
        assert_eq!(zigzag_encode(1), 2);
        assert_eq!(zigzag_encode(2), 4);
        assert_eq!(zigzag_encode(100), 200);
    }

    #[test]
    fn zigzag_negative_values() {
        assert_eq!(zigzag_encode(-1), 1);
        assert_eq!(zigzag_encode(-2), 3);
        assert_eq!(zigzag_encode(-100), 199);
    }

    #[test]
    fn command_encoding() {
        assert_eq!(command(CMD_MOVE_TO, 1), (1 & 0x7) | (1 << 3));
        assert_eq!(command(CMD_LINE_TO, 3), (2 & 0x7) | (3 << 3));
        assert_eq!(command(CMD_CLOSE_PATH, 1), (7 & 0x7) | (1 << 3));
    }

    #[test]
    fn encode_point_feature_roundtrip() {
        let features = vec![TileFeature {
            id: 1,
            geom_type: GeomType::Point,
            geometry: vec![(5.603, -0.187)],
            properties: vec![
                (
                    "name".to_string(),
                    PropertyValue::String("Accra".to_string()),
                ),
                ("population".to_string(), PropertyValue::Int(2_000_000)),
            ],
        }];

        let encoded = encode_tile(&features, "places", 10, 511, 496);
        assert!(!encoded.is_empty());

        let decoded: vector_tile::Tile =
            prost::Message::decode(encoded.as_slice()).expect("valid protobuf");

        assert_eq!(decoded.layers.len(), 1);
        let layer = &decoded.layers[0];
        assert_eq!(layer.name, "places");
        assert_eq!(layer.version, 2);
        assert_eq!(layer.extent, Some(MVT_EXTENT));
        assert_eq!(layer.features.len(), 1);

        let feat = &layer.features[0];
        assert_eq!(feat.id, Some(1));
        assert_eq!(feat.r#type(), tile::GeomType::Point);
        assert_eq!(feat.geometry.len(), 3);
        assert_eq!(feat.geometry[0], command(CMD_MOVE_TO, 1));

        assert_eq!(layer.keys.len(), 2);
        assert!(layer.keys.contains(&"name".to_string()));
        assert!(layer.keys.contains(&"population".to_string()));
    }

    #[test]
    fn wgs84_accra_at_z10() {
        let (px, py) = wgs84_to_tile_coords(5.603, -0.187, 10, 511, 496, MVT_EXTENT);
        assert!(
            px >= 0 && px <= MVT_EXTENT as i32,
            "px={px} out of tile extent"
        );
        assert!(
            py >= 0 && py <= MVT_EXTENT as i32,
            "py={py} out of tile extent"
        );
    }

    #[test]
    fn encode_linestring_commands() {
        let coords = vec![(0, 0), (10, 10), (20, 10)];
        let cmds = encode_geometry_linestring(&coords);

        assert_eq!(cmds[0], command(CMD_MOVE_TO, 1));
        assert_eq!(cmds[1], zigzag_encode(0));
        assert_eq!(cmds[2], zigzag_encode(0));
        assert_eq!(cmds[3], command(CMD_LINE_TO, 2));
        assert_eq!(cmds[4], zigzag_encode(10));
        assert_eq!(cmds[5], zigzag_encode(10));
        assert_eq!(cmds[6], zigzag_encode(10));
        assert_eq!(cmds[7], zigzag_encode(0));
    }

    #[test]
    fn encode_polygon_has_close_path() {
        let coords = vec![(0, 0), (10, 0), (10, 10), (0, 10)];
        let cmds = encode_geometry_polygon(&coords);

        let last = cmds.last().copied().unwrap();
        assert_eq!(last, command(CMD_CLOSE_PATH, 1));
    }

    #[test]
    fn empty_linestring_returns_empty() {
        assert!(encode_geometry_linestring(&[]).is_empty());
        assert!(encode_geometry_linestring(&[(0, 0)]).is_empty());
    }

    #[test]
    fn empty_polygon_returns_empty() {
        assert!(encode_geometry_polygon(&[]).is_empty());
        assert!(encode_geometry_polygon(&[(0, 0), (1, 1)]).is_empty());
    }
}

use serde::ser::{SerializeMap, Serializer};
use serde::Serialize;

use crate::path::PathSegment;

#[derive(Debug, Clone, Serialize)]
pub struct Instruction {
    #[serde(flatten)]
    pub instruction_type: InstructionType,
    pub road: Option<String>,
    pub distance_m: f64,
    pub bearing: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub enum InstructionType {
    Depart,
    Straight,
    SlightLeft,
    SlightRight,
    Left,
    Right,
    SharpLeft,
    SharpRight,
    UTurn,
    Arrive,
    Roundabout { exit_number: u8 },
}

impl Serialize for InstructionType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            InstructionType::Roundabout { exit_number } => {
                let mut map = serializer.serialize_map(Some(2))?;
                map.serialize_entry("type", "roundabout")?;
                map.serialize_entry("exit", exit_number)?;
                map.end()
            }
            other => {
                let mut map = serializer.serialize_map(Some(1))?;
                let type_str = match other {
                    InstructionType::Depart => "depart",
                    InstructionType::Straight => "straight",
                    InstructionType::SlightLeft => "slight_left",
                    InstructionType::SlightRight => "slight_right",
                    InstructionType::Left => "left",
                    InstructionType::Right => "right",
                    InstructionType::SharpLeft => "sharp_left",
                    InstructionType::SharpRight => "sharp_right",
                    InstructionType::UTurn => "u_turn",
                    InstructionType::Arrive => "arrive",
                    InstructionType::Roundabout { .. } => unreachable!(),
                };
                map.serialize_entry("type", type_str)?;
                map.end()
            }
        }
    }
}

pub fn generate_instructions(segments: &[PathSegment]) -> Vec<Instruction> {
    if segments.is_empty() {
        return Vec::new();
    }

    let mut instructions = Vec::new();
    let first_bearing = segment_bearing(&segments[0]);

    instructions.push(Instruction {
        instruction_type: InstructionType::Depart,
        road: segments[0].road_name.clone(),
        distance_m: segments[0].distance_m,
        bearing: first_bearing,
    });

    let mut accumulated_distance = segments[0].distance_m;
    let mut prev_bearing = first_bearing;
    let mut prev_road = segments[0].road_name.clone();
    let mut idx = 1;

    while idx < segments.len() {
        let segment = &segments[idx];

        if segment.is_roundabout {
            let roundabout_start = idx;
            let mut exit_count: u8 = 0;
            let mut roundabout_distance = 0.0;

            while idx < segments.len() && segments[idx].is_roundabout {
                roundabout_distance += segments[idx].distance_m;

                if idx + 1 < segments.len() && !segments[idx + 1].is_roundabout {
                    exit_count += 1;
                    break;
                }

                if idx + 1 < segments.len() && segments[idx + 1].is_roundabout {
                    let current_name = &segments[idx].road_name;
                    let next_name = &segments[idx + 1].road_name;
                    if current_name != next_name {
                        exit_count += 1;
                    }
                }

                idx += 1;
            }

            exit_count = exit_count.max(1);

            let exit_road = if idx < segments.len() && !segments[idx].is_roundabout {
                segments[idx].road_name.clone()
            } else if idx + 1 < segments.len() {
                segments[idx + 1].road_name.clone()
            } else {
                segments[roundabout_start.saturating_sub(1)]
                    .road_name
                    .clone()
            };

            let exit_bearing = if idx < segments.len() {
                segment_bearing(&segments[idx])
            } else {
                prev_bearing
            };

            instructions.push(Instruction {
                instruction_type: InstructionType::Roundabout {
                    exit_number: exit_count,
                },
                road: exit_road,
                distance_m: accumulated_distance,
                bearing: exit_bearing,
            });

            accumulated_distance = roundabout_distance;
            prev_bearing = exit_bearing;
            prev_road = if idx < segments.len() {
                segments[idx].road_name.clone()
            } else {
                None
            };
            idx += 1;
            continue;
        }

        let current_bearing = segment_bearing(segment);
        let road_changed = segment.road_name != prev_road;
        let bearing_delta = normalize_bearing_delta(current_bearing - prev_bearing);
        let significant_turn = bearing_delta.abs() > 15.0;

        if road_changed || significant_turn {
            let instruction_type = classify_turn(bearing_delta);
            instructions.push(Instruction {
                instruction_type,
                road: segment.road_name.clone(),
                distance_m: accumulated_distance,
                bearing: current_bearing,
            });
            accumulated_distance = segment.distance_m;
        } else {
            accumulated_distance += segment.distance_m;
        }

        prev_bearing = current_bearing;
        prev_road = segment.road_name.clone();
        idx += 1;
    }

    let last_segment = &segments[segments.len() - 1];
    let last_bearing = segment_bearing(last_segment);
    instructions.push(Instruction {
        instruction_type: InstructionType::Arrive,
        road: last_segment.road_name.clone(),
        distance_m: accumulated_distance,
        bearing: last_bearing,
    });

    instructions
}

fn classify_turn(delta: f64) -> InstructionType {
    let abs_delta = delta.abs();
    if abs_delta <= 15.0 {
        InstructionType::Straight
    } else if abs_delta <= 45.0 {
        if delta > 0.0 {
            InstructionType::SlightRight
        } else {
            InstructionType::SlightLeft
        }
    } else if abs_delta <= 135.0 {
        if delta > 0.0 {
            InstructionType::Right
        } else {
            InstructionType::Left
        }
    } else if abs_delta <= 170.0 {
        if delta > 0.0 {
            InstructionType::SharpRight
        } else {
            InstructionType::SharpLeft
        }
    } else {
        InstructionType::UTurn
    }
}

pub fn compute_bearing(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let lat1_r = lat1.to_radians();
    let lat2_r = lat2.to_radians();
    let dlon = (lon2 - lon1).to_radians();

    let x = dlon.cos() * lat2_r.cos();
    let y = lat1_r.cos() * lat2_r.sin() - lat1_r.sin() * x;
    let x2 = dlon.sin() * lat2_r.cos();

    let bearing = x2.atan2(y).to_degrees();
    (bearing + 360.0) % 360.0
}

fn segment_bearing(segment: &PathSegment) -> f64 {
    if segment.points.len() < 2 {
        return 0.0;
    }
    let (lat1, lon1) = segment.points[0];
    let (lat2, lon2) = segment.points[1];
    compute_bearing(lat1, lon1, lat2, lon2)
}

fn normalize_bearing_delta(delta: f64) -> f64 {
    let mut d = delta % 360.0;
    if d > 180.0 {
        d -= 360.0;
    } else if d < -180.0 {
        d += 360.0;
    }
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearing_north() {
        let b = compute_bearing(0.0, 0.0, 1.0, 0.0);
        assert!((b - 0.0).abs() < 1.0);
    }

    #[test]
    fn bearing_east() {
        let b = compute_bearing(0.0, 0.0, 0.0, 1.0);
        assert!((b - 90.0).abs() < 1.0);
    }

    #[test]
    fn bearing_south() {
        let b = compute_bearing(1.0, 0.0, 0.0, 0.0);
        assert!((b - 180.0).abs() < 1.0);
    }

    #[test]
    fn classify_right_turn() {
        assert_eq!(classify_turn(90.0), InstructionType::Right);
    }

    #[test]
    fn classify_left_turn() {
        assert_eq!(classify_turn(-90.0), InstructionType::Left);
    }

    #[test]
    fn classify_straight() {
        assert_eq!(classify_turn(5.0), InstructionType::Straight);
    }

    #[test]
    fn classify_uturn() {
        assert_eq!(classify_turn(175.0), InstructionType::UTurn);
    }

    #[test]
    fn classify_slight_right() {
        assert_eq!(classify_turn(30.0), InstructionType::SlightRight);
    }

    #[test]
    fn classify_slight_left() {
        assert_eq!(classify_turn(-30.0), InstructionType::SlightLeft);
    }

    #[test]
    fn classify_sharp_right() {
        assert_eq!(classify_turn(150.0), InstructionType::SharpRight);
    }

    #[test]
    fn classify_sharp_left() {
        assert_eq!(classify_turn(-150.0), InstructionType::SharpLeft);
    }

    #[test]
    fn instructions_depart_and_arrive() {
        let segments = vec![PathSegment {
            points: vec![(0.0, 0.0), (1.0, 0.0)],
            road_name: Some("Main St".to_string()),
            distance_m: 100.0,
            is_roundabout: false,
        }];
        let instructions = generate_instructions(&segments);
        assert!(instructions.len() >= 2);
        assert_eq!(instructions[0].instruction_type, InstructionType::Depart);
        assert_eq!(
            instructions.last().unwrap().instruction_type,
            InstructionType::Arrive
        );
    }

    #[test]
    fn instructions_name_change_generates_instruction() {
        let segments = vec![
            PathSegment {
                points: vec![(0.0, 0.0), (1.0, 0.0)],
                road_name: Some("Main St".to_string()),
                distance_m: 100.0,
                is_roundabout: false,
            },
            PathSegment {
                points: vec![(1.0, 0.0), (2.0, 0.0)],
                road_name: Some("Highway 1".to_string()),
                distance_m: 200.0,
                is_roundabout: false,
            },
        ];
        let instructions = generate_instructions(&segments);
        assert!(instructions.len() >= 3);
        assert_eq!(instructions[0].instruction_type, InstructionType::Depart);
        assert_eq!(instructions[1].instruction_type, InstructionType::Straight);
        assert_eq!(instructions[2].instruction_type, InstructionType::Arrive);
    }

    #[test]
    fn empty_segments_produces_no_instructions() {
        let instructions = generate_instructions(&[]);
        assert!(instructions.is_empty());
    }

    #[test]
    fn roundabout_produces_exit_instruction() {
        let segments = vec![
            PathSegment {
                points: vec![(5.600, -0.180), (5.601, -0.180)],
                road_name: Some("Approach Rd".to_string()),
                distance_m: 100.0,
                is_roundabout: false,
            },
            PathSegment {
                points: vec![(5.601, -0.180), (5.6015, -0.179)],
                road_name: Some("Roundabout".to_string()),
                distance_m: 30.0,
                is_roundabout: true,
            },
            PathSegment {
                points: vec![(5.6015, -0.179), (5.602, -0.178)],
                road_name: Some("Roundabout".to_string()),
                distance_m: 30.0,
                is_roundabout: true,
            },
            PathSegment {
                points: vec![(5.602, -0.178), (5.603, -0.177)],
                road_name: Some("Exit Rd".to_string()),
                distance_m: 100.0,
                is_roundabout: false,
            },
        ];
        let instructions = generate_instructions(&segments);
        let roundabout_instr = instructions
            .iter()
            .find(|i| matches!(i.instruction_type, InstructionType::Roundabout { .. }));
        assert!(
            roundabout_instr.is_some(),
            "expected a roundabout instruction"
        );
        if let InstructionType::Roundabout { exit_number } =
            roundabout_instr.unwrap().instruction_type
        {
            assert!(exit_number >= 1);
        }
    }

    #[test]
    fn roundabout_serialization() {
        let instr = Instruction {
            instruction_type: InstructionType::Roundabout { exit_number: 3 },
            road: Some("Ring Road".to_string()),
            distance_m: 50.0,
            bearing: 90.0,
        };
        let json = serde_json::to_value(&instr).unwrap();
        assert_eq!(json["type"], "roundabout");
        assert_eq!(json["exit"], 3);
        assert_eq!(json["road"], "Ring Road");
    }

    #[test]
    fn non_roundabout_serialization() {
        let instr = Instruction {
            instruction_type: InstructionType::Left,
            road: Some("Main St".to_string()),
            distance_m: 100.0,
            bearing: 270.0,
        };
        let json = serde_json::to_value(&instr).unwrap();
        assert_eq!(json["type"], "left");
        assert!(json.get("exit").is_none());
    }
}

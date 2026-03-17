use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::AtlasError;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ContributionType {
    WrongTurn,
    RoadClosed,
    BetterRoute,
    RoundaboutError,
    MissingRoad,
    SpeedWrong,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RouteContribution {
    pub id: String,
    pub route_origin: LatLon,
    pub route_destination: LatLon,
    pub profile: String,
    pub issue_type: ContributionType,
    pub description: Option<String>,
    pub suggested_waypoints: Option<Vec<LatLon>>,
    pub created_at: String,
}

pub struct ContributionStore {
    dir: PathBuf,
}

impl ContributionStore {
    pub fn new(dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
        }
    }

    pub fn save(&self, contribution: &RouteContribution) -> Result<(), AtlasError> {
        std::fs::create_dir_all(&self.dir).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to create contributions dir {}: {}",
                self.dir.display(),
                e
            ))
        })?;

        let file_path = self.dir.join(format!("{}.json", contribution.id));
        let json = serde_json::to_string_pretty(contribution).map_err(|e| {
            AtlasError::StoreError(format!("failed to serialize contribution: {}", e))
        })?;

        std::fs::write(&file_path, json).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to write contribution file {}: {}",
                file_path.display(),
                e
            ))
        })?;

        Ok(())
    }

    pub fn list(&self) -> Result<Vec<RouteContribution>, AtlasError> {
        if !self.dir.exists() {
            return Ok(Vec::new());
        }

        let entries = std::fs::read_dir(&self.dir).map_err(|e| {
            AtlasError::StoreError(format!(
                "failed to read contributions dir {}: {}",
                self.dir.display(),
                e
            ))
        })?;

        let mut contributions = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| AtlasError::StoreError(e.to_string()))?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let contents = std::fs::read_to_string(&path).map_err(|e| {
                AtlasError::StoreError(format!(
                    "failed to read contribution file {}: {}",
                    path.display(),
                    e
                ))
            })?;
            let contribution: RouteContribution = serde_json::from_str(&contents).map_err(|e| {
                AtlasError::StoreError(format!(
                    "failed to parse contribution file {}: {}",
                    path.display(),
                    e
                ))
            })?;
            contributions.push(contribution);
        }

        Ok(contributions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contribution_type_serialization() {
        let ct = ContributionType::WrongTurn;
        let json = serde_json::to_string(&ct).unwrap();
        assert_eq!(json, "\"wrong_turn\"");

        let parsed: ContributionType = serde_json::from_str("\"roundabout_error\"").unwrap();
        assert_eq!(parsed, ContributionType::RoundaboutError);
    }

    #[test]
    fn store_save_and_list() {
        let dir = std::env::temp_dir().join(format!("atlas-contrib-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let store = ContributionStore::new(&dir);

        let contribution = RouteContribution {
            id: "test-uuid-123".to_string(),
            route_origin: LatLon {
                lat: 5.603,
                lon: -0.187,
            },
            route_destination: LatLon {
                lat: 5.55,
                lon: -0.21,
            },
            profile: "car".to_string(),
            issue_type: ContributionType::WrongTurn,
            description: Some("Turn instruction was wrong".to_string()),
            suggested_waypoints: None,
            created_at: "2026-03-17T10:00:00Z".to_string(),
        };

        store.save(&contribution).unwrap();

        let listed = store.list().unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "test-uuid-123");
        assert_eq!(listed[0].issue_type, ContributionType::WrongTurn);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_list_empty_dir() {
        let dir =
            std::env::temp_dir().join(format!("atlas-contrib-empty-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);

        let store = ContributionStore::new(&dir);
        let listed = store.list().unwrap();
        assert!(listed.is_empty());
    }
}

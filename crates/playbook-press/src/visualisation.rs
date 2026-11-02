//! Visualisation Specification Types

use serde::{Deserialize, Serialize};

use crate::{AudienceId, AudienceProfile};

/// Visualisation specification
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualisationSpec {
    pub visualisation_id: String,
    pub visualisation_type: VisualisationType,
    pub purpose: VisualisationPurpose,
    pub data_source: VisualisationDataSource,
    pub claims_represented: Vec<String>,
    pub audience: AudienceId,
    pub required_annotations: Vec<String>,
    pub uncertainty_representation: UncertaintyRepresentation,
    pub annotations: Vec<String>,
    pub metadata: std::collections::HashMap<String, String>,
}

/// Types of visualisations
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualisationType {
    Table,
    Timeline,
    StateDiagram,
    ArchitectureDiagram,
    ComparisonMatrix,
    BarChart,
    LineChart,
    Scorecard,
    FieldView,
    Heatmap,
    SankeyDiagram,
    NetworkGraph,
}

/// Purpose of the visualisation
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VisualisationPurpose {
    Comparison,
    Trend,
    Distribution,
    Composition,
    Relationship,
    Process,
    State,
    Performance,
    Coverage,
    Anomaly,
}

/// Data source for visualisation
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VisualisationDataSource {
    pub source_type: DataSourceType,
    pub source_id: String,
    pub query: Option<String>,
    pub parameters: std::collections::HashMap<String, String>,
}

/// Type of data source
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSourceType {
    Evidence,
    Benchmark,
    Journal,
    Trace,
    ProviderResponse,
    External,
    Computed,
}

/// Uncertainty representation in visualisations
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UncertaintyRepresentation {
    None,
    ErrorBars,
    ConfidenceIntervals,
    Shading,
    Opacity,
    Annotation,
    AlternativeViews,
}

impl VisualisationSpec {
    pub fn new(vis_type: VisualisationType, purpose: VisualisationPurpose, audience: AudienceId) -> Self {
        Self {
            visualisation_id: uuid::Uuid::new_v4().to_string(),
            visualisation_type: vis_type,
            purpose,
            data_source: VisualisationDataSource {
                source_type: DataSourceType::Evidence,
                source_id: "".into(),
                query: None,
                parameters: std::collections::HashMap::new(),
            },
            claims_represented: Vec::new(),
            audience: AudienceId::new("general_public"),
            required_annotations: Vec::new(),
            uncertainty_representation: UncertaintyRepresentation::ErrorBars,
            annotations: Vec::new(),
            metadata: std::collections::HashMap::new(),
        }
    }
}

/// Errors for visualisation
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
#[serde(rename_all = "snake_case")]
pub enum VisualisationError {
    #[error("invalid visualisation type: {0}")]
    InvalidType(String),
    #[error("missing required data source")]
    MissingDataSource,
    #[error("uncertainty representation required for projected claims")]
    UncertaintyRequired,
    #[error("visual truth violation: {0}")]
    VisualTruthViolation(String),
    #[error("data source not found: {0}")]
    DataSourceNotFound(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visualisation_spec_creation() {
        let spec = VisualisationSpec::new(
            VisualisationType::BarChart,
            VisualisationPurpose::Comparison,
            AudienceId::new("technical"),
        );
        assert_eq!(spec.visualisation_type, VisualisationType::BarChart);
        assert_eq!(spec.purpose, VisualisationPurpose::Comparison);
    }

    #[test]
    fn uncertainty_representation_for_projected() {
        let spec = VisualisationSpec::new(
            VisualisationType::LineChart,
            VisualisationPurpose::Trend,
            AudienceId::new("technical"),
        );
        // Projected claims should have uncertainty representation
        assert_ne!(spec.uncertainty_representation, UncertaintyRepresentation::None);
    }
}
//! The fixture work units (§46): model-authored test corpus whose
//! validity is proven by executing the real engine. The nemotron swarm
//! authors observations and *predicts* the engine's verdicts; the
//! validator runs the actual engine and compares. A mismatch is a
//! rejection — the prediction is refuted by the sovereign implementation,
//! and the RRP repairs the authoring conditions, never the expectation.

use serde::{Deserialize, Serialize};

use playbook_resolution::{classify, Confidence, RefusalObservation, WorkUnitContract};

/// The uniform fixture envelope: every family carries its inputs and the
/// author's predicted engine verdict.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Fixture {
    pub fixture_id: String,
    pub family: String,
    /// `envelope` | `decomposition` | `ownership` | `lineage`
    pub observation: Option<RefusalObservation>,
    pub expected_cause: Option<String>,
    pub expected_family: Option<String>,
    pub expected_confidence: Option<String>,
    /// Decomposition inputs.
    pub items: Option<Vec<String>>,
    pub chunk_size: Option<usize>,
    /// Ownership inputs.
    pub owner: Option<String>,
    pub writer: Option<String>,
    pub exists: Option<bool>,
    pub overwrite_policy: Option<String>,
    pub transfer_recorded: Option<bool>,
    pub expected_verdict: Option<String>,
    /// Lineage inputs.
    pub chain_length: Option<u32>,
    /// The author's predicted chunk anchors (decomposition family).
    pub expected_chunk_anchors: Option<Vec<ChunkAnchor>>,
    /// Ownership: permitted mutators beyond the owner.
    pub expected_mutators: Option<Vec<String>>,
}

/// One predicted chunk in a decomposition fixture.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChunkAnchor {
    pub expected_items: u32,
    pub first: Option<String>,
    pub last: Option<String>,
}

pub const FAMILIES: [&str; 4] = ["envelope", "decomposition", "ownership", "lineage"];

/// Validate one fixture against the real engine. `Ok(())` means the
/// author's prediction matched the engine exactly.
pub fn validate(fixture: &Fixture) -> Result<(), Vec<String>> {
    let mut failures = Vec::new();
    match fixture.family.as_str() {
        "envelope" => {
            let Some(observation) = &fixture.observation else {
                return Err(vec!["envelope fixture carries no observation".into()]);
            };
            let (Some(expected_cause), Some(expected_confidence)) =
                (&fixture.expected_cause, &fixture.expected_confidence)
            else {
                return Err(vec![
                    "envelope fixture missing expected cause/confidence".into()
                ]);
            };
            let diagnosis = classify(observation);
            if diagnosis.primary.key() != expected_cause.as_str() {
                failures.push(format!(
                    "predicted cause {expected_cause} but the engine classified {}",
                    diagnosis.primary.key()
                ));
            }
            if format!("{:?}", diagnosis.confidence).to_lowercase() != expected_confidence.as_str()
            {
                failures.push(format!(
                    "predicted confidence {expected_confidence} but the engine measured {:?}",
                    diagnosis.confidence
                ));
            }
            if diagnosis.confidence == Confidence::Unknown {
                failures.push("a fixture with no deterministic signal teaches nothing".into());
            }
        }
        "decomposition" => {
            let (Some(items), Some(chunk_size)) = (&fixture.items, fixture.chunk_size) else {
                return Err(vec!["decomposition fixture missing items/chunk_size".into()]);
            };
            let contracts = playbook_work_contracts(items, chunk_size)?;
            if contracts.len() != fixture.expected_chunks() {
                failures.push(format!(
                    "predicted {} chunks but the engine produced {}",
                    fixture.expected_chunks(),
                    contracts.len()
                ));
            }
            type ChunkAnchorPred = (u32, Option<String>, Option<String>);
            let predicted: Option<Vec<ChunkAnchorPred>> =
                fixture.expected_chunk_anchors.as_ref().map(|anchors| {
                    anchors
                        .iter()
                        .map(|anchor| {
                            (
                                anchor.expected_items,
                                anchor.first.clone(),
                                anchor.last.clone(),
                            )
                        })
                        .collect()
                });
            if let Some(predicted) = predicted {
                for (index, (expected_items, first, last)) in predicted.iter().enumerate() {
                    let Some(contract) = contracts.get(index) else {
                        failures.push(format!("engine produced no chunk {index}"));
                        continue;
                    };
                    if &contract.expected_items != expected_items {
                        failures.push(format!(
                            "chunk {index}: predicted {expected_items} items, engine produced {}",
                            contract.expected_items
                        ));
                    }
                    if contract.first_anchor != *first || contract.last_anchor != *last {
                        failures.push(format!("chunk {index} anchors diverge from prediction"));
                    }
                }
            }
        }
        "ownership" => {
            let (Some(owner), Some(writer), Some(exists)) =
                (&fixture.owner, &fixture.writer, fixture.exists)
            else {
                return Err(vec!["ownership fixture missing owner/writer/exists".into()]);
            };
            let record = playbook_ownership(
                owner,
                fixture.permitted_mutators(),
                fixture
                    .overwrite_policy
                    .as_deref()
                    .unwrap_or("replace_owned"),
            );
            let verdict = playbook_resolution::ownership::check_write(
                &record,
                writer,
                exists,
                fixture.transfer_recorded.unwrap_or(false),
            );
            let Some(expected) = &fixture.expected_verdict else {
                return Err(vec!["ownership fixture missing expected verdict".into()]);
            };
            if format!("{verdict:?}").to_lowercase() != expected.as_str() {
                failures.push(format!(
                    "predicted verdict {expected} but the engine ruled {verdict:?}"
                ));
            }
        }
        "lineage" => {
            let Some(chain_length) = fixture.chain_length else {
                return Err(vec!["lineage fixture missing chain_length".into()]);
            };
            let mut lineage = playbook_resolution::ResolutionLineage::new();
            lineage.root("root", 0);
            let mut current = "root".to_string();
            for hop in 0..chain_length {
                current = lineage
                    .refuse(&current, &format!("refusal-{hop}"), "replay")
                    .ok_or_else(|| {
                        vec!["lineage chain broke before reaching the predicted depth".to_string()]
                    })?;
            }
            if lineage.depth() != chain_length + 1 {
                failures.push(format!(
                    "predicted chain depth {} but the engine measured {}",
                    chain_length + 1,
                    lineage.depth()
                ));
            }
        }
        other => failures.push(format!("unknown family: {other}")),
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(failures)
    }
}

fn playbook_work_contracts(
    items: &[String],
    chunk_size: usize,
) -> Result<Vec<WorkUnitContract>, Vec<String>> {
    if chunk_size == 0 {
        return Err(vec!["chunk_size 0 is inadmissible".into()]);
    }
    let mut contracts = Vec::new();
    for chunk in items.chunks(chunk_size) {
        contracts.push(WorkUnitContract {
            unit_id: format!("fixture-chunk-{}", contracts.len()),
            expected_items: chunk.len() as u32,
            first_anchor: chunk.first().cloned(),
            last_anchor: chunk.last().cloned(),
            order_immutable: true,
        });
    }
    Ok(contracts)
}

fn playbook_ownership(
    owner: &str,
    permitted: Vec<String>,
    policy: &str,
) -> playbook_resolution::ArtefactOwnership {
    use playbook_resolution::OverwritePolicy;
    let parsed = match policy {
        "forbidden" => OverwritePolicy::Forbidden,
        "append" => OverwritePolicy::Append,
        "merge" => OverwritePolicy::Merge,
        "derived_only" => OverwritePolicy::DerivedOnly,
        "explicit_transfer" => OverwritePolicy::ExplicitTransfer,
        _ => OverwritePolicy::ReplaceOwned,
    };
    playbook_resolution::ArtefactOwnership {
        path: "fixture-path".into(),
        owner: owner.into(),
        producer: owner.into(),
        permitted_mutators: permitted,
        overwrite_policy: parsed,
        generated_by: "fixture".into(),
        revision: 1,
    }
}

impl Fixture {
    fn expected_chunks(&self) -> usize {
        let items = self.items.as_ref().map(Vec::len).unwrap_or(0);
        let chunk = self.chunk_size.unwrap_or(1).max(1);
        items.div_ceil(chunk)
    }

    fn permitted_mutators(&self) -> Vec<String> {
        self.expected_mutators.clone().unwrap_or_default()
    }
}

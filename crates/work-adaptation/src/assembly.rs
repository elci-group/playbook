//! Assembly verification (§12): before any expensive semantic
//! inspection, check structurally that the delivered chunks satisfy every
//! work-unit contract, arrive in order, and duplicate nothing across
//! chunk boundaries.

use playbook_resolution::{check_anchors, AnchorVerdict, WorkUnitContract};

/// Why the assembly failed. Names the failing contract and the verdict.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssemblyError {
    /// A different number of chunks was delivered than contracted.
    ChunkCountMismatch { expected: usize, delivered: usize },
    /// The chunk at the given index violated its contract.
    ContractFailed {
        index: usize,
        unit_id: String,
        verdict: AnchorVerdict,
    },
    /// The same item appeared in more than one chunk.
    DuplicateAcrossChunks {
        item: String,
        first_index: usize,
        second_index: usize,
    },
}

impl std::fmt::Display for AssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ChunkCountMismatch {
                expected,
                delivered,
            } => write!(f, "expected {expected} chunks, {delivered} were delivered"),
            Self::ContractFailed {
                index,
                unit_id,
                verdict,
            } => write!(f, "contract {unit_id} (chunk {index}) failed: {verdict:?}"),
            Self::DuplicateAcrossChunks {
                item,
                first_index,
                second_index,
            } => write!(
                f,
                "item {item:?} appears in both chunk {first_index} and chunk {second_index}"
            ),
        }
    }
}

impl std::error::Error for AssemblyError {}

/// Verify a fully delivered assembly: every contract satisfied via
/// `check_anchors`, chunks in contract order, no item duplicated across
/// chunks. Contracts are matched to delivered chunks positionally —
/// contract 0 must be delivered as chunk 0, and so on (§12).
pub fn verify_assembly(
    contracts: &[WorkUnitContract],
    delivered: &[Vec<String>],
) -> Result<(), AssemblyError> {
    if contracts.len() != delivered.len() {
        return Err(AssemblyError::ChunkCountMismatch {
            expected: contracts.len(),
            delivered: delivered.len(),
        });
    }
    let mut seen: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for (index, (contract, chunk)) in contracts.iter().zip(delivered).enumerate() {
        let verdict = check_anchors(contract, chunk);
        if verdict != AnchorVerdict::Satisfied {
            return Err(AssemblyError::ContractFailed {
                index,
                unit_id: contract.unit_id.clone(),
                verdict,
            });
        }
        for item in chunk {
            match seen.get(item.as_str()) {
                Some(&first_index) => {
                    return Err(AssemblyError::DuplicateAcrossChunks {
                        item: item.clone(),
                        first_index,
                        second_index: index,
                    });
                }
                None => {
                    seen.insert(item, index);
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::decompose::decompose;

    fn items(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("item-{i}")).collect()
    }

    fn full_delivery(items: &[String], chunk_size: usize) -> Vec<Vec<String>> {
        items.chunks(chunk_size).map(<[String]>::to_vec).collect()
    }

    #[test]
    fn faithful_assembly_passes() {
        let items = items(38);
        let contracts = decompose("p", &items, 8).unwrap();
        assert!(verify_assembly(&contracts, &full_delivery(&items, 8)).is_ok());
    }

    #[test]
    fn empty_assembly_passes() {
        assert!(verify_assembly(&[], &[]).is_ok());
    }

    #[test]
    fn first_item_omitted_is_detected() {
        let items = items(16);
        let contracts = decompose("p", &items, 8).unwrap();
        let mut delivered = full_delivery(&items, 8);
        delivered[0].remove(0);
        delivered[0].push(items[15].clone()); // keep cardinality plausible
        match verify_assembly(&contracts, &delivered) {
            Err(AssemblyError::ContractFailed {
                index: 0,
                unit_id,
                verdict,
            }) => {
                assert_eq!(unit_id, "p-chunk-0");
                assert_eq!(verdict, AnchorVerdict::FirstMissing);
            }
            other => panic!("expected ContractFailed, got {other:?}"),
        }
    }

    #[test]
    fn last_item_omitted_is_detected() {
        let items = items(16);
        let contracts = decompose("p", &items, 8).unwrap();
        let mut delivered = full_delivery(&items, 8);
        delivered[1].pop();
        match verify_assembly(&contracts, &delivered) {
            Err(AssemblyError::ContractFailed {
                index: 1,
                unit_id,
                verdict,
            }) => {
                assert_eq!(unit_id, "p-chunk-1");
                assert_eq!(verdict, AnchorVerdict::CardinalityMismatch);
            }
            other => panic!("expected ContractFailed, got {other:?}"),
        }
    }

    #[test]
    fn duplicate_across_chunks_is_detected() {
        let items = items(16);
        let contracts = decompose("p", &items, 8).unwrap();
        let mut delivered = full_delivery(&items, 8);
        delivered[1][3] = delivered[0][0].clone();
        match verify_assembly(&contracts, &delivered) {
            Err(AssemblyError::DuplicateAcrossChunks {
                item,
                first_index,
                second_index,
            }) => {
                assert_eq!(item, "item-0");
                assert_eq!((first_index, second_index), (0, 1));
            }
            other => panic!("expected DuplicateAcrossChunks, got {other:?}"),
        }
    }

    #[test]
    fn reordered_chunks_are_detected() {
        let items = items(16);
        let contracts = decompose("p", &items, 8).unwrap();
        let mut delivered = full_delivery(&items, 8);
        delivered.swap(0, 1);
        match verify_assembly(&contracts, &delivered) {
            Err(AssemblyError::ContractFailed {
                index: 0,
                unit_id,
                verdict,
            }) => {
                assert_eq!(unit_id, "p-chunk-0");
                assert_eq!(verdict, AnchorVerdict::FirstMissing);
            }
            other => panic!("expected ContractFailed, got {other:?}"),
        }
    }

    #[test]
    fn missing_chunk_is_detected() {
        let items = items(16);
        let contracts = decompose("p", &items, 8).unwrap();
        let delivered = full_delivery(&items, 8);
        let delivered = &delivered[..1];
        match verify_assembly(&contracts, delivered) {
            Err(AssemblyError::ChunkCountMismatch {
                expected: 2,
                delivered: 1,
            }) => {}
            other => panic!("expected ChunkCountMismatch, got {other:?}"),
        }
    }
}

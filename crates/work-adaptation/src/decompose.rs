//! Decomposition contracts (§11): split an oversized work unit into
//! independently verifiable children. Deterministic: the same items and
//! chunk size always yield the same contracts. The acceptance standard is
//! unchanged — the same items must all appear, only the envelope per
//! attempt shrinks.

use playbook_resolution::WorkUnitContract;

/// Why a decomposition request could not be honoured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecompositionError {
    /// A zero chunk size can never make progress (§42: loop prevention).
    ZeroChunkSize,
}

impl std::fmt::Display for DecompositionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ZeroChunkSize => write!(f, "chunk size must be at least 1"),
        }
    }
}

impl std::error::Error for DecompositionError {}

/// Deterministically chunk `items` into work-unit contracts with
/// first/last anchors per chunk. Unit ids are `{parent}-chunk-{n}`,
/// zero-based. An empty item list yields no contracts. `chunk_size` of 0
/// is a panic-free error.
pub fn decompose(
    parent: &str,
    items: &[String],
    chunk_size: usize,
) -> Result<Vec<WorkUnitContract>, DecompositionError> {
    if chunk_size == 0 {
        return Err(DecompositionError::ZeroChunkSize);
    }
    let mut contracts = Vec::new();
    for (n, chunk) in items.chunks(chunk_size).enumerate() {
        contracts.push(WorkUnitContract {
            unit_id: format!("{parent}-chunk-{n}"),
            expected_items: chunk.len() as u32,
            first_anchor: chunk.first().cloned(),
            last_anchor: chunk.last().cloned(),
            order_immutable: true,
        });
    }
    Ok(contracts)
}

#[cfg(test)]
mod tests {
    use super::*;
    use playbook_resolution::{check_anchors, AnchorVerdict};

    fn items(n: usize) -> Vec<String> {
        (0..n).map(|i| format!("item-{i}")).collect()
    }

    #[test]
    fn empty_list_yields_no_contracts() {
        assert!(decompose("p", &[], 8).unwrap().is_empty());
    }

    #[test]
    fn zero_chunk_size_is_an_error_not_a_panic() {
        assert_eq!(
            decompose("p", &items(3), 0),
            Err(DecompositionError::ZeroChunkSize)
        );
    }

    #[test]
    fn exact_multiple_yields_full_chunks() {
        let contracts = decompose("p", &items(16), 8).unwrap();
        assert_eq!(contracts.len(), 2);
        assert!(contracts.iter().all(|c| c.expected_items == 8));
    }

    #[test]
    fn thirty_eight_items_chunk_eight() {
        let items = items(38);
        let contracts = decompose("parent", &items, 8).unwrap();
        assert_eq!(contracts.len(), 5);
        let sizes: Vec<u32> = contracts.iter().map(|c| c.expected_items).collect();
        assert_eq!(sizes, [8, 8, 8, 8, 6]);
        assert_eq!(contracts[0].unit_id, "parent-chunk-0");
        assert_eq!(contracts[4].unit_id, "parent-chunk-4");
        assert_eq!(contracts[0].first_anchor.as_deref(), Some("item-0"));
        assert_eq!(contracts[4].last_anchor.as_deref(), Some("item-37"));
        assert_eq!(contracts[4].first_anchor.as_deref(), Some("item-32"));
        assert!(contracts.iter().all(|c| c.order_immutable));
    }

    #[test]
    fn every_item_in_exactly_one_contract_and_contracts_verify() {
        let items = items(38);
        let contracts = decompose("parent", &items, 8).unwrap();
        let mut seen = std::collections::BTreeSet::new();
        for (n, contract) in contracts.iter().enumerate() {
            let chunk: Vec<String> =
                items[n * 8..(n * 8 + contract.expected_items as usize)].to_vec();
            assert_eq!(check_anchors(contract, &chunk), AnchorVerdict::Satisfied);
            for item in chunk {
                assert!(seen.insert(item), "item duplicated across contracts");
            }
        }
        assert_eq!(seen.len(), 38);
    }

    #[test]
    fn single_item_chunk_has_identical_anchors() {
        let items = items(3);
        let contracts = decompose("p", &items, 2).unwrap();
        assert_eq!(contracts.len(), 2);
        assert_eq!(contracts[1].first_anchor.as_deref(), Some("item-2"));
        assert_eq!(contracts[1].last_anchor.as_deref(), Some("item-2"));
    }
}

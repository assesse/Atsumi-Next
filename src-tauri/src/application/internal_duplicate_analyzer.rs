use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    DuplicatePageHash, DuplicatePagePair, HashProfile, InternalDuplicateGroup, InternalGroupRecord,
    InternalMatchKind, InternalPageEvidence,
};

use super::duplicate_analyzer::HashedArtifact;

pub(crate) const INTERNAL_DUPLICATE_ALGORITHM_VERSION: u32 = 2;
const DETAIL_HASH_BYTES: usize = 128;
const DETAIL_HASH_BITS: u32 = 1024;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InternalDetection {
    pub groups: Vec<InternalGroupRecord>,
    pub compared_pairs: u64,
}

#[derive(Clone)]
struct PreparedInternalPage {
    original: DuplicatePageHash,
    detail_bytes: Option<[u8; DETAIL_HASH_BYTES]>,
}

#[derive(Clone)]
struct Edge {
    left: usize,
    right: usize,
    evidence: DuplicatePagePair,
}

#[derive(Clone)]
struct PairRun {
    edges: Vec<usize>,
    average_similarity: f64,
    exact_count: usize,
    cumulative_gap: usize,
}

#[derive(Default)]
struct SceneBlock {
    rows: Vec<BTreeSet<usize>>,
    page_rows: BTreeMap<usize, usize>,
}

/// Hash features remain unchanged. Pair edges are aligned monotonically, then only runs that
/// share at least two ordered rows can attach a new edition track to a scene block.
pub(crate) fn detect_internal_groups(
    run_id: &str,
    artifact: &HashedArtifact,
    profile: &HashProfile,
) -> InternalDetection {
    let prepared = artifact
        .pages
        .iter()
        .cloned()
        .map(|original| PreparedInternalPage {
            detail_bytes: decode_detail_hash(&original.detail_d_hash_hex),
            original,
        })
        .collect::<Vec<_>>();
    let mut edges = Vec::new();
    let mut compared_pairs = 0_u64;
    for left in 0..prepared.len() {
        for right in (left + 1)..prepared.len() {
            compared_pairs = compared_pairs.saturating_add(1);
            if let Some(evidence) = compare_prepared(&prepared[left], &prepared[right], profile) {
                // Blank/divider exact bytes are retained as standalone exact groups, never as
                // sequence bridges between editions.
                if !evidence.low_information {
                    edges.push(Edge {
                        left,
                        right,
                        evidence,
                    });
                }
            }
        }
    }
    let runs = monotonic_runs(&edges);
    let blocks = merge_runs(&runs, &edges);
    let mut groups = rows_to_groups(run_id, artifact, &prepared, &edges, blocks);

    let scene_pages = groups
        .iter()
        .flat_map(|record| record.group.pages.iter().map(|page| page.source_page))
        .collect::<BTreeSet<_>>();
    let mut exact_classes = BTreeMap::<&str, Vec<usize>>::new();
    for (index, page) in prepared.iter().enumerate() {
        exact_classes
            .entry(page.original.artifact_sha256.as_str())
            .or_default()
            .push(index);
    }
    let mut block_number = groups
        .iter()
        .map(|record| record.group.block_id.clone())
        .collect::<BTreeSet<_>>()
        .len() as u32;
    for indices in exact_classes.values().filter(|indices| indices.len() >= 2) {
        if indices
            .iter()
            .any(|index| scene_pages.contains(&prepared[*index].original.source_page_number.get()))
        {
            continue;
        }
        block_number = block_number.saturating_add(1);
        let pages = indices
            .iter()
            .map(|index| evidence_for_exact(&prepared[*index].original))
            .collect();
        groups.push(group_record(
            run_id,
            artifact,
            block_number,
            0,
            InternalMatchKind::Exact,
            1.0,
            pages,
        ));
    }
    groups.sort_by_key(|record| {
        (
            record.group.gallery_id,
            record.group.block_id.clone(),
            record.group.sequence_index,
        )
    });
    InternalDetection {
        groups,
        compared_pairs,
    }
}

fn monotonic_runs(edges: &[Edge]) -> Vec<PairRun> {
    let positions = edges
        .iter()
        .enumerate()
        .map(|(index, edge)| ((edge.left, edge.right), index))
        .collect::<BTreeMap<_, _>>();
    let mut best = vec![(1_usize, None::<usize>, 0_usize); edges.len()];
    for (index, edge) in edges.iter().enumerate() {
        for left_gap in 1..=3 {
            for right_gap in 1..=3 {
                let Some(left) = edge.left.checked_sub(left_gap) else {
                    continue;
                };
                let Some(right) = edge.right.checked_sub(right_gap) else {
                    continue;
                };
                let Some(&previous) = positions.get(&(left, right)) else {
                    continue;
                };
                let (length, _, gap) = best[previous];
                let candidate = (length + 1, Some(previous), gap + left_gap + right_gap - 2);
                if candidate.0 > best[index].0
                    || (candidate.0 == best[index].0 && candidate.2 < best[index].2)
                {
                    best[index] = candidate;
                }
            }
        }
    }
    let predecessors = best
        .iter()
        .filter_map(|(_, previous, _)| *previous)
        .collect::<BTreeSet<_>>();
    let mut seen = BTreeSet::new();
    let mut runs = Vec::new();
    for terminal in 0..edges.len() {
        if predecessors.contains(&terminal) || best[terminal].0 < 2 {
            continue;
        }
        let mut chain = Vec::new();
        let mut cursor = Some(terminal);
        while let Some(index) = cursor {
            chain.push(index);
            cursor = best[index].1;
        }
        chain.reverse();
        if !seen.insert(chain.clone()) {
            continue;
        }
        let exact_count = chain
            .iter()
            .filter(|&&index| edges[index].evidence.exact_sha256)
            .count();
        let average_similarity = chain
            .iter()
            .map(|&index| edges[index].evidence.visual_similarity)
            .sum::<f64>()
            / chain.len() as f64;
        runs.push(PairRun {
            edges: chain,
            average_similarity,
            exact_count,
            cumulative_gap: best[terminal].2,
        });
    }
    runs.sort_by(|left, right| {
        right
            .edges
            .len()
            .cmp(&left.edges.len())
            .then_with(|| right.average_similarity.total_cmp(&left.average_similarity))
            .then_with(|| right.exact_count.cmp(&left.exact_count))
            .then_with(|| left.cumulative_gap.cmp(&right.cumulative_gap))
            .then_with(|| left.edges.cmp(&right.edges))
    });
    runs
}

fn merge_runs(runs: &[PairRun], edges: &[Edge]) -> Vec<SceneBlock> {
    let page_count = edges
        .iter()
        .flat_map(|edge| [edge.left, edge.right])
        .max()
        .map_or(0, |index| index + 1);
    let mut parent = (0..page_count).collect::<Vec<_>>();
    let mut qualifying = BTreeSet::new();
    for run in runs {
        if run.edges.len() >= 2 && run_has_local_edition_offset(run, edges) {
            qualifying.extend(run.edges.iter().copied());
        }
    }
    for edge_index in qualifying {
        let edge = &edges[edge_index];
        union(&mut parent, edge.left, edge.right);
    }
    let mut components = BTreeMap::<usize, BTreeSet<usize>>::new();
    for page in 0..page_count {
        let root = find(&mut parent, page);
        components.entry(root).or_default().insert(page);
    }
    let mut rows = components
        .into_values()
        .filter(|row| row.len() >= 2)
        .collect::<Vec<_>>();
    rows.sort_by_key(|row| row.iter().next().copied().unwrap_or(usize::MAX));
    let mut blocks: Vec<SceneBlock> = Vec::new();
    for row in rows {
        if let Some(block) = blocks.last_mut() {
            if block
                .rows
                .last()
                .is_some_and(|previous| rows_follow(previous, &row))
            {
                let row_index = block.rows.len();
                for page in &row {
                    block.page_rows.insert(*page, row_index);
                }
                block.rows.push(row);
                continue;
            }
        }
        let page_rows = row.iter().map(|page| (*page, 0)).collect();
        blocks.push(SceneBlock {
            rows: vec![row],
            page_rows,
        });
    }
    blocks
        .into_iter()
        .filter(|block| block.rows.len() >= 2)
        .collect()
}

/// A repeated cycle can produce a perfectly monotonic edge chain to the same
/// position in the next cycle.  Those long jumps are not another edition of
/// the current scene sequence.  Keep the bounded edition offsets that can be
/// supported by the run itself (including a small missing-page allowance),
/// while leaving the separate cycle as its own block.
fn run_has_local_edition_offset(run: &PairRun, edges: &[Edge]) -> bool {
    let offset_sum = run
        .edges
        .iter()
        .map(|index| edges[*index].right.abs_diff(edges[*index].left))
        .sum::<usize>();
    let average_offset = offset_sum / run.edges.len();
    average_offset <= run.edges.len().saturating_mul(3).saturating_add(3)
}

fn rows_follow(previous: &BTreeSet<usize>, next: &BTreeSet<usize>) -> bool {
    previous
        .iter()
        .flat_map(|left| next.iter().map(move |right| (*left, *right)))
        .filter(|(left, right)| *right > *left && *right - *left <= 3)
        .count()
        >= 2
}

fn find(parent: &mut [usize], index: usize) -> usize {
    if parent[index] != index {
        let root = find(parent, parent[index]);
        parent[index] = root;
    }
    parent[index]
}
fn union(parent: &mut [usize], left: usize, right: usize) {
    let left_root = find(parent, left);
    let right_root = find(parent, right);
    if left_root != right_root {
        parent[right_root] = left_root;
    }
}

fn rows_to_groups(
    run_id: &str,
    artifact: &HashedArtifact,
    prepared: &[PreparedInternalPage],
    edges: &[Edge],
    blocks: Vec<SceneBlock>,
) -> Vec<InternalGroupRecord> {
    let mut records = Vec::new();
    for (block_index, block) in blocks.into_iter().enumerate() {
        for (sequence, row) in block.rows.into_iter().enumerate() {
            let mut indexes = row.into_iter().collect::<Vec<_>>();
            indexes.sort_by_key(|index| prepared[*index].original.source_page_number);
            let representative = indexes[0];
            let pages = indexes
                .iter()
                .map(|&index| row_evidence(representative, index, prepared, edges))
                .collect::<Vec<_>>();
            let relation = if pages.iter().all(|page| page.exact_sha256) {
                InternalMatchKind::Exact
            } else {
                InternalMatchKind::TranslationVisual
            };
            let confidence = pages
                .iter()
                .map(|page| page.visual_similarity)
                .fold(1.0_f64, f64::min);
            records.push(group_record(
                run_id,
                artifact,
                block_index as u32 + 1,
                sequence as u32,
                relation,
                confidence,
                pages,
            ));
        }
    }
    records
}

fn row_evidence(
    representative: usize,
    index: usize,
    prepared: &[PreparedInternalPage],
    edges: &[Edge],
) -> InternalPageEvidence {
    if representative == index {
        return evidence_for_exact(&prepared[index].original);
    }
    let (left, right) = if representative < index {
        (representative, index)
    } else {
        (index, representative)
    };
    let evidence = edges
        .iter()
        .find(|edge| edge.left == left && edge.right == right)
        .map(|edge| &edge.evidence);
    evidence.map_or_else(
        || evidence_for_exact(&prepared[index].original),
        |value| InternalPageEvidence {
            source_page: prepared[index].original.source_page_number.get(),
            exact_sha256: value.exact_sha256,
            visual_similarity: value.visual_similarity,
            detail_hash_distance: value.detail_hash_distance,
            low_information: value.low_information,
        },
    )
}
fn evidence_for_exact(page: &DuplicatePageHash) -> InternalPageEvidence {
    InternalPageEvidence {
        source_page: page.source_page_number.get(),
        exact_sha256: true,
        visual_similarity: 1.0,
        detail_hash_distance: 0,
        low_information: page.low_information,
    }
}

fn compare_prepared(
    left: &PreparedInternalPage,
    right: &PreparedInternalPage,
    profile: &HashProfile,
) -> Option<DuplicatePagePair> {
    let exact = left.original.artifact_sha256 == right.original.artifact_sha256;
    let low = left.original.low_information || right.original.low_information;
    if exact {
        return Some(pair(
            &left.original,
            &right.original,
            true,
            0,
            0,
            0,
            1.0,
            low,
        ));
    }
    if low {
        return None;
    }
    let coarse = (left.original.coarse_d_hash ^ right.original.coarse_d_hash).count_ones();
    let phash = (left.original.p_hash ^ right.original.p_hash).count_ones();
    let edge = similarity(
        left.original.edge_density,
        right.original.edge_density,
        0.20,
    );
    let content = similarity(
        left.original.non_uniform_ratio,
        right.original.non_uniform_ratio,
        0.75,
    );
    if coarse > 20 || phash > 16 || edge < 0.62 || content < 0.60 {
        return None;
    }
    let detail = hamming(left.detail_bytes.as_ref()?, right.detail_bytes.as_ref()?);
    let central = central_hamming(left.detail_bytes.as_ref()?, right.detail_bytes.as_ref()?);
    let standard = similarity(left.original.std_dev, right.original.std_dev, 96.);
    let visual = ((1.0 - coarse as f64 / 64.0) * 0.15
        + (1.0 - phash as f64 / 64.0) * 0.25
        + (1.0 - detail as f64 / DETAIL_HASH_BITS as f64) * 0.35
        + edge * 0.15
        + standard * 0.05
        + content * 0.05)
        .clamp(0.0, 1.0);
    if detail > 260 || central > 48 || visual < profile.visual_match_threshold {
        return None;
    }
    Some(pair(
        &left.original,
        &right.original,
        false,
        coarse,
        phash,
        detail,
        visual,
        false,
    ))
}
#[allow(clippy::too_many_arguments)]
fn pair(
    left: &DuplicatePageHash,
    right: &DuplicatePageHash,
    exact_sha256: bool,
    d_hash_distance: u32,
    p_hash_distance: u32,
    detail_hash_distance: u32,
    visual_similarity: f64,
    low_information: bool,
) -> DuplicatePagePair {
    DuplicatePagePair {
        parent_source_page: left.source_page_number.get(),
        candidate_source_page: right.source_page_number.get(),
        exact_sha256,
        d_hash_distance,
        p_hash_distance,
        detail_hash_distance,
        edge_similarity: if exact_sha256 {
            1.0
        } else {
            similarity(left.edge_density, right.edge_density, 0.20)
        },
        visual_similarity,
        low_information,
    }
}
fn decode_detail_hash(value: &str) -> Option<[u8; DETAIL_HASH_BYTES]> {
    if value.len() != DETAIL_HASH_BYTES * 2 {
        return None;
    };
    let mut bytes = [0; DETAIL_HASH_BYTES];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&value[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}
fn hamming(left: &[u8; DETAIL_HASH_BYTES], right: &[u8; DETAIL_HASH_BYTES]) -> u32 {
    left.iter()
        .zip(right)
        .map(|(a, b)| (a ^ b).count_ones())
        .sum()
}
fn central_hamming(left: &[u8; DETAIL_HASH_BYTES], right: &[u8; DETAIL_HASH_BYTES]) -> u32 {
    let mut d = 0;
    for y in 7..25 {
        for x in 7..25 {
            let bit = y * 32 + x;
            d += u32::from(
                (left[bit / 8] & (1 << (bit % 8))) != (right[bit / 8] & (1 << (bit % 8))),
            );
        }
    }
    d
}
fn similarity(left: f64, right: f64, scale: f64) -> f64 {
    (1.0 - (left - right).abs() / scale).clamp(0.0, 1.0)
}

#[allow(clippy::too_many_arguments)]
fn group_record(
    run_id: &str,
    artifact: &HashedArtifact,
    block_number: u32,
    sequence_index: u32,
    relation: InternalMatchKind,
    confidence: f64,
    mut pages: Vec<InternalPageEvidence>,
) -> InternalGroupRecord {
    pages.sort_by_key(|page| page.source_page);
    let block_id = format!(
        "internal-a{}-p{}-g{}-b{}",
        INTERNAL_DUPLICATE_ALGORITHM_VERSION,
        artifact
            .pages
            .first()
            .map_or(1, |page| page.profile_version),
        artifact.gallery.gallery_id.get(),
        block_number
    );
    InternalGroupRecord {
        run_id: run_id.into(),
        group: InternalDuplicateGroup {
            group_id: format!("{block_id}-r{sequence_index}"),
            block_id,
            sequence_index,
            revision: 0,
            entry_id: artifact.gallery.entry_id.clone(),
            gallery_id: artifact.gallery.gallery_id,
            relation,
            confidence: confidence.clamp(0., 1.),
            recommended_keep_source_page: pages
                .iter()
                .map(|page| page.source_page)
                .min()
                .unwrap_or(1),
            pages,
            resolved: false,
            created_at: String::new(),
            updated_at: String::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{ArtifactSha256, DuplicateGalleryRef, GalleryId, SourcePageNumber};
    fn page(number: u32, scene: u64) -> DuplicatePageHash {
        DuplicatePageHash {
            entry_id: "entry-1".into(),
            gallery_id: GalleryId::new(1).unwrap(),
            source_page_number: SourcePageNumber::new(number).unwrap(),
            profile_version: 1,
            artifact_sha256: ArtifactSha256::new(format!("{scene:064x}")).unwrap(),
            coarse_d_hash: scene,
            p_hash: scene,
            detail_d_hash_hex: format!("{:02x}", scene.saturating_mul(37)).repeat(128),
            mean_luma: 120.0,
            std_dev: 44.0,
            non_uniform_ratio: 0.7,
            edge_density: 0.2,
            width: 100,
            height: 100,
            low_information: false,
        }
    }
    fn artifact(pages: Vec<DuplicatePageHash>) -> HashedArtifact {
        HashedArtifact {
            gallery: DuplicateGalleryRef {
                gallery_id: GalleryId::new(1).unwrap(),
                entry_id: "entry-1".into(),
                title: "fixture".into(),
                artist: None,
                group: None,
                page_count: pages.len() as u32,
            },
            pages,
        }
    }
    #[test]
    fn four_editions_form_five_nway_rows() {
        let pages = (0..4)
            .flat_map(|edition| {
                (0..5).map(move |scene| page(edition * 5 + scene + 1, u64::from(scene + 1)))
            })
            .collect();
        let found = detect_internal_groups("run", &artifact(pages), &HashProfile::current());
        assert_eq!(found.groups.len(), 5);
        for (scene, row) in found.groups.iter().enumerate() {
            assert_eq!(
                row.group
                    .pages
                    .iter()
                    .map(|p| p.source_page)
                    .collect::<Vec<_>>(),
                vec![
                    scene as u32 + 1,
                    scene as u32 + 6,
                    scene as u32 + 11,
                    scene as u32 + 16
                ]
            );
        }
    }
    #[test]
    fn one_shared_panel_does_not_form_a_block() {
        let mut shared_but_reencoded = page(3, 1);
        shared_but_reencoded.artifact_sha256 = ArtifactSha256::new(format!("{:064x}", 99)).unwrap();
        let found = detect_internal_groups(
            "run",
            &artifact(vec![page(1, 1), page(2, 2), shared_but_reencoded]),
            &HashProfile::current(),
        );
        assert!(found.groups.is_empty());
    }
    #[test]
    fn repeated_edition_cycles_remain_separate_blocks() {
        let pages = (0..2)
            .flat_map(|cycle| {
                (0..4).flat_map(move |edition| {
                    (0..5).map(move |scene| {
                        page(
                            cycle * 20 + edition * 5 + scene + 1,
                            u64::from(cycle * 100 + scene + 1),
                        )
                    })
                })
            })
            .collect();
        let found = detect_internal_groups("run", &artifact(pages), &HashProfile::current());
        assert_eq!(found.groups.len(), 10);
        let block_ids = found
            .groups
            .iter()
            .map(|group| group.group.block_id.clone())
            .collect::<BTreeSet<_>>();
        assert_eq!(block_ids.len(), 2);
        assert_eq!(
            found.groups[5]
                .group
                .pages
                .iter()
                .map(|page| page.source_page)
                .collect::<Vec<_>>(),
            vec![21, 26, 31, 36]
        );
    }
}

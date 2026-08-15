use std::collections::{BTreeMap, BTreeSet};

use crate::domain::{
    InternalDuplicateGroup, InternalGroupRecord, InternalMatchKind, InternalPageEvidence,
};

use super::duplicate_analyzer::{compare_page_evidence, HashedArtifact};
use crate::domain::HashProfile;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct InternalDetection {
    pub groups: Vec<InternalGroupRecord>,
    pub compared_pairs: u64,
}

#[derive(Debug, Clone)]
struct VisualPair {
    left_index: usize,
    right_index: usize,
    evidence: crate::domain::DuplicatePagePair,
}

pub(crate) fn detect_internal_groups(
    run_id: &str,
    artifact: &HashedArtifact,
    profile: &HashProfile,
) -> InternalDetection {
    let mut groups = Vec::new();
    let mut used_pages = BTreeSet::new();
    let mut exact_groups = BTreeMap::<&str, Vec<usize>>::new();
    for (index, page) in artifact.pages.iter().enumerate() {
        exact_groups
            .entry(page.artifact_sha256.as_str())
            .or_default()
            .push(index);
    }

    let mut block_number = 0_u32;
    for indices in exact_groups.values().filter(|indices| indices.len() >= 2) {
        block_number += 1;
        let pages = indices
            .iter()
            .map(|index| {
                let page = &artifact.pages[*index];
                used_pages.insert(page.source_page_number.get());
                InternalPageEvidence {
                    source_page: page.source_page_number.get(),
                    exact_sha256: true,
                    visual_similarity: 1.0,
                    detail_hash_distance: 0,
                    low_information: page.low_information,
                }
            })
            .collect::<Vec<_>>();
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

    let mut visual_pairs = Vec::new();
    let mut compared_pairs = 0_u64;
    for left_index in 0..artifact.pages.len() {
        for right_index in (left_index + 1)..artifact.pages.len() {
            compared_pairs = compared_pairs.saturating_add(1);
            let left = &artifact.pages[left_index];
            let right = &artifact.pages[right_index];
            if left.artifact_sha256 == right.artifact_sha256
                || used_pages.contains(&left.source_page_number.get())
                || used_pages.contains(&right.source_page_number.get())
            {
                continue;
            }
            let Some(evidence) = compare_page_evidence(left, right, profile) else {
                continue;
            };
            if evidence.exact_sha256 || evidence.low_information {
                continue;
            }
            visual_pairs.push(VisualPair {
                left_index,
                right_index,
                evidence,
            });
        }
    }
    visual_pairs.sort_by_key(|pair| (pair.left_index, pair.right_index));

    let mut consumed_pairs = BTreeSet::new();
    for seed_index in 0..visual_pairs.len() {
        if consumed_pairs.contains(&seed_index) {
            continue;
        }
        let seed = &visual_pairs[seed_index];
        if pair_uses_page(seed, &artifact.pages, &used_pages) {
            continue;
        }
        let mut block = vec![seed_index];
        let mut block_pages = BTreeSet::from([seed.left_index, seed.right_index]);
        let (mut last_left, mut last_right) = (seed.left_index, seed.right_index);
        loop {
            let next = visual_pairs
                .iter()
                .enumerate()
                .filter(|(index, pair)| {
                    !consumed_pairs.contains(index)
                        && !block.contains(index)
                        && pair.left_index > last_left
                        && pair.right_index > last_right
                        && pair.left_index - last_left <= 3
                        && pair.right_index - last_right <= 3
                        && !block_pages.contains(&pair.left_index)
                        && !block_pages.contains(&pair.right_index)
                        && !pair_uses_page(pair, &artifact.pages, &used_pages)
                })
                .max_by(|(_, left), (_, right)| {
                    left.evidence
                        .visual_similarity
                        .total_cmp(&right.evidence.visual_similarity)
                        .then_with(|| right.left_index.cmp(&left.left_index))
                        .then_with(|| right.right_index.cmp(&left.right_index))
                })
                .map(|(index, _)| index);
            let Some(next) = next else { break };
            block.push(next);
            block_pages.insert(visual_pairs[next].left_index);
            block_pages.insert(visual_pairs[next].right_index);
            last_left = visual_pairs[next].left_index;
            last_right = visual_pairs[next].right_index;
        }
        // A single perceptual page can be a shared panel or watermark.  Exact
        // SHA groups may be single-row, but visual scene blocks require two
        // monotonic rows before they enter Review.
        if block.len() < 2 {
            continue;
        }
        block_number += 1;
        for (sequence_index, pair_index) in block.into_iter().enumerate() {
            consumed_pairs.insert(pair_index);
            let pair = &visual_pairs[pair_index];
            let left = &artifact.pages[pair.left_index];
            let right = &artifact.pages[pair.right_index];
            used_pages.insert(left.source_page_number.get());
            used_pages.insert(right.source_page_number.get());
            groups.push(group_record(
                run_id,
                artifact,
                block_number,
                u32::try_from(sequence_index).unwrap_or(u32::MAX),
                InternalMatchKind::TranslationVisual,
                pair.evidence.visual_similarity,
                vec![
                    InternalPageEvidence {
                        source_page: left.source_page_number.get(),
                        exact_sha256: false,
                        visual_similarity: pair.evidence.visual_similarity,
                        detail_hash_distance: pair.evidence.detail_hash_distance,
                        low_information: pair.evidence.low_information,
                    },
                    InternalPageEvidence {
                        source_page: right.source_page_number.get(),
                        exact_sha256: false,
                        visual_similarity: pair.evidence.visual_similarity,
                        detail_hash_distance: pair.evidence.detail_hash_distance,
                        low_information: pair.evidence.low_information,
                    },
                ],
            ));
        }
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

fn pair_uses_page(
    pair: &VisualPair,
    pages: &[crate::domain::DuplicatePageHash],
    used_pages: &BTreeSet<u32>,
) -> bool {
    used_pages.contains(&pages[pair.left_index].source_page_number.get())
        || used_pages.contains(&pages[pair.right_index].source_page_number.get())
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
    let gallery_id = artifact.gallery.gallery_id;
    let block_id = format!(
        "internal-p{}-g{}-b{}",
        artifact
            .pages
            .first()
            .map_or(1, |page| page.profile_version),
        gallery_id.get(),
        block_number
    );
    InternalGroupRecord {
        run_id: run_id.to_owned(),
        group: InternalDuplicateGroup {
            group_id: format!("{block_id}-r{sequence_index}"),
            block_id,
            sequence_index,
            revision: 0,
            entry_id: artifact.gallery.entry_id.clone(),
            gallery_id,
            relation,
            confidence: confidence.clamp(0.0, 1.0),
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
    use crate::domain::{
        ArtifactSha256, DuplicateGalleryRef, DuplicatePageHash, GalleryId, SourcePageNumber,
    };

    use super::*;

    fn page(number: u32, feature: u64, sha_seed: u64) -> DuplicatePageHash {
        DuplicatePageHash {
            entry_id: "entry-1".into(),
            gallery_id: GalleryId::new(1).unwrap(),
            source_page_number: SourcePageNumber::new(number).unwrap(),
            profile_version: 1,
            artifact_sha256: ArtifactSha256::new(format!("{sha_seed:064x}")).unwrap(),
            coarse_d_hash: feature,
            detail_d_hash_hex: if feature == 0 {
                "00".repeat(128)
            } else {
                "ff".repeat(128)
            },
            p_hash: feature,
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
                title: "Internal fixture".into(),
                artist: None,
                group: None,
                page_count: pages.len() as u32,
            },
            pages,
        }
    }

    #[test]
    fn exact_repeated_pages_form_one_group_and_keep_original_source_numbers() {
        let detection = detect_internal_groups(
            "run",
            &artifact(vec![page(2, 0, 7), page(8, u64::MAX, 9), page(14, 0, 7)]),
            &HashProfile::current(),
        );
        assert_eq!(detection.groups.len(), 1);
        assert_eq!(detection.groups[0].group.relation, InternalMatchKind::Exact);
        assert_eq!(
            detection.groups[0]
                .group
                .pages
                .iter()
                .map(|page| page.source_page)
                .collect::<Vec<_>>(),
            vec![2, 14]
        );
    }

    #[test]
    fn two_row_visual_sequence_is_gap_tolerant_and_keeps_source_numbers() {
        let pages = vec![
            page(1, 0, 1),
            page(2, u64::MAX, 2),
            page(5, 0, 3),
            page(7, u64::MAX, 4),
        ];
        let detection = detect_internal_groups("run", &artifact(pages), &HashProfile::current());
        assert_eq!(detection.groups.len(), 2);
        assert_eq!(
            detection.groups[0].group.block_id,
            detection.groups[1].group.block_id
        );
        let all_pages = detection
            .groups
            .iter()
            .flat_map(|group| group.group.pages.iter().map(|page| page.source_page))
            .collect::<BTreeSet<_>>();
        assert_eq!(all_pages, BTreeSet::from([1, 2, 5, 7]));
    }

    #[test]
    fn one_visual_pair_is_ignored_as_a_possible_shared_panel() {
        let detection = detect_internal_groups(
            "run",
            &artifact(vec![page(1, 0, 1), page(5, 0, 2), page(9, u64::MAX, 3)]),
            &HashProfile::current(),
        );
        assert!(detection.groups.is_empty());
    }
}

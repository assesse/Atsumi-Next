use std::{
    collections::{HashMap, VecDeque},
    io::Cursor,
    sync::{Arc, Mutex},
    time::Duration,
};

use reqwest::Url;

use crate::{
    application::SearchRepository,
    domain::{Language, SearchRequest, SearchSort},
    source::{
        hitomi::{
            galleryinfo_script_url, gg_script_url, parse_galleryinfo_script, parse_gg_routing,
            webp_thumbnail_candidates, ThumbnailSize, HITOMI_METADATA_ORIGIN,
        },
        SourceContractError,
    },
    thumbnail::{CancellationToken, ThumbnailKey, ThumbnailResolver},
};

use super::{
    http::{validate_source_url, HttpPayload, HttpRequest, HttpTransport},
    search::{prefixed_nozomi_path, tag_nozomi_path},
    HitomiLiveAdapter, HitomiLiveConfig,
};

const GALLERY_SCRIPT: &str = include_str!("../../../fixtures/hitomi/galleryinfo-normal.js");
const GG_SCRIPT: &str = include_str!("../../../fixtures/hitomi/gg-current.js");

#[derive(Default)]
struct FakeTransport {
    responses: Mutex<HashMap<String, VecDeque<Result<HttpPayload, SourceContractError>>>>,
    calls: Mutex<Vec<String>>,
}

impl FakeTransport {
    fn respond(&self, url: String, content_type: &str, bytes: Vec<u8>) {
        self.responses
            .lock()
            .unwrap()
            .entry(url)
            .or_default()
            .push_back(Ok(HttpPayload {
                bytes,
                content_type: content_type.to_owned(),
            }));
    }

    fn call_count(&self, url: &str) -> usize {
        self.calls
            .lock()
            .unwrap()
            .iter()
            .filter(|call| call.as_str() == url)
            .count()
    }

    fn was_called(&self, url: &str) -> bool {
        self.call_count(url) > 0
    }
}

impl HttpTransport for FakeTransport {
    fn execute(&self, request: HttpRequest) -> Result<HttpPayload, SourceContractError> {
        self.calls.lock().unwrap().push(request.url.clone());
        self.responses
            .lock()
            .unwrap()
            .get_mut(&request.url)
            .and_then(VecDeque::pop_front)
            .unwrap_or_else(|| {
                Err(SourceContractError::not_found(
                    format!("fake response for {}", request.url),
                    Some(404),
                ))
            })
    }
}

#[test]
fn source_allowlist_rejects_lookalike_and_plain_http_hosts() {
    assert!(validate_source_url(
        &Url::parse("https://w1.gold-usergeneratedcontent.net/path.webp").unwrap()
    )
    .is_ok());
    assert!(validate_source_url(
        &Url::parse("https://gold-usergeneratedcontent.net.attacker.invalid/path").unwrap()
    )
    .is_err());
    assert!(validate_source_url(
        &Url::parse("http://ltn.gold-usergeneratedcontent.net/index-all.nozomi").unwrap()
    )
    .is_err());
}

#[test]
fn structured_tag_paths_preserve_hitomi_gender_namespace() {
    assert_eq!(
        tag_nozomi_path("female:long_hair").as_deref(),
        Some("n/tag/female%3Along%20hair-all.nozomi")
    );
    assert_eq!(
        tag_nozomi_path("full color").as_deref(),
        Some("n/tag/full%20color-all.nozomi")
    );
    assert_eq!(
        prefixed_nozomi_path("series:rain_archives").as_deref(),
        Some("n/series/rain%20archives-all.nozomi")
    );
    assert_eq!(
        prefixed_nozomi_path("character:mira_lane").as_deref(),
        Some("n/character/mira%20lane-all.nozomi")
    );
}

#[test]
fn search_and_thumbnail_share_the_same_metadata_cache_without_live_network() {
    let transport = Arc::new(FakeTransport::default());
    let nozomi = 424_242_u32.to_be_bytes().to_vec();
    transport.respond(
        format!("{HITOMI_METADATA_ORIGIN}/n/index-english.nozomi"),
        "application/x-nozomi",
        nozomi.clone(),
    );
    transport.respond(
        format!("{HITOMI_METADATA_ORIGIN}/n/tag/landscape-all.nozomi"),
        "application/x-nozomi",
        nozomi,
    );
    let gallery_url = galleryinfo_script_url(424_242).unwrap();
    transport.respond(
        gallery_url.clone(),
        "text/javascript",
        GALLERY_SCRIPT.as_bytes().to_vec(),
    );
    transport.respond(
        gg_script_url(),
        "text/javascript",
        GG_SCRIPT.as_bytes().to_vec(),
    );
    let metadata = parse_galleryinfo_script(GALLERY_SCRIPT).unwrap();
    let routing = parse_gg_routing(GG_SCRIPT).unwrap();
    let candidate = webp_thumbnail_candidates(
        metadata.pages.first().unwrap(),
        &routing,
        ThumbnailSize::Large,
    )
    .unwrap()
    .remove(0);
    transport.respond(candidate.url, "image/png", one_pixel_png());

    let config = HitomiLiveConfig {
        request_start_interval: Duration::ZERO,
        ..HitomiLiveConfig::default()
    };
    let adapter = HitomiLiveAdapter::with_transport(config, transport.clone());
    let submission = adapter
        .search_submit(&SearchRequest {
            text: String::new(),
            include_tags: vec!["landscape".to_owned()],
            exclude_tags: Vec::new(),
            languages: vec![Language::English],
            sort: SearchSort::Recent,
            page_size: 20,
        })
        .unwrap();
    assert_eq!(submission.first_page.items.len(), 1);
    assert_eq!(submission.first_page.items[0].series, vec!["original"]);
    assert_eq!(
        submission.first_page.items[0].characters,
        vec!["Example Character"]
    );

    let thumbnail = adapter
        .resolve(
            &ThumbnailKey::gallery_cover(424_242).unwrap(),
            &CancellationToken::new(),
        )
        .unwrap();
    assert_eq!((thumbnail.width, thumbnail.height), (1, 1));
    assert_eq!(thumbnail.content_type, "image/png");
    assert_eq!(transport.call_count(&gallery_url), 1);
}

#[test]
fn live_search_contract_covers_paging_filters_popular_and_related_without_network() {
    let transport = Arc::new(FakeTransport::default());
    let origin = HITOMI_METADATA_ORIGIN;
    transport.respond(
        format!("{origin}/n/index-english.nozomi"),
        "application/x-nozomi",
        nozomi(&[1001, 1002, 1003]),
    );
    transport.respond(
        format!("{origin}/n/index-english.nozomi"),
        "application/x-nozomi",
        nozomi(&[1001, 1002, 1003]),
    );
    transport.respond(
        format!("{origin}/n/tag/landscape-all.nozomi"),
        "application/x-nozomi",
        nozomi(&[1002, 1003]),
    );
    transport.respond(
        format!("{origin}/n/tag/female%3Ablue%20sky-all.nozomi"),
        "application/x-nozomi",
        nozomi(&[1002]),
    );
    transport.respond(
        format!("{origin}/n/popular/week-english.nozomi"),
        "application/x-nozomi",
        nozomi(&[1001, 1003]),
    );

    for (id, title, related) in [
        (1001, "Quiet Night Fixture", "[]"),
        (1002, "Excluded Blue Fixture", "[]"),
        (1003, "Sunlit Archive Fixture", "[1002, 1999]"),
    ] {
        transport.respond(
            galleryinfo_script_url(id).unwrap(),
            "text/javascript",
            gallery_script(id, title, related).into_bytes(),
        );
    }

    let adapter = HitomiLiveAdapter::with_transport(
        HitomiLiveConfig {
            request_start_interval: Duration::ZERO,
            ..HitomiLiveConfig::default()
        },
        transport.clone(),
    );

    let recent = adapter
        .search_submit(&SearchRequest {
            text: String::new(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            languages: vec![Language::English],
            sort: SearchSort::Recent,
            page_size: 1,
        })
        .unwrap();
    assert_eq!(recent.first_page.total_pages, 3);
    assert_eq!(recent.first_page.items[0].id.get(), 1003);
    let second = adapter
        .search_page_get(&recent.query_id, 2)
        .unwrap()
        .expect("cached query exists");
    assert_eq!(second.items[0].id.get(), 1002);

    let filtered = adapter
        .search_submit(&SearchRequest {
            text: "Sunlit".into(),
            include_tags: vec!["landscape".into()],
            exclude_tags: vec!["female:blue_sky".into()],
            languages: vec![Language::English],
            sort: SearchSort::Recent,
            page_size: 20,
        })
        .unwrap();
    assert_eq!(
        filtered
            .first_page
            .items
            .iter()
            .map(|gallery| gallery.id.get())
            .collect::<Vec<_>>(),
        vec![1003]
    );
    assert!(!transport.was_called(&format!("{origin}/n/index-korean.nozomi")));

    let popular = adapter
        .search_submit(&SearchRequest {
            text: String::new(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            languages: vec![Language::English],
            sort: SearchSort::PopularWeek,
            page_size: 20,
        })
        .unwrap();
    assert_eq!(
        popular
            .first_page
            .items
            .iter()
            .map(|gallery| gallery.id.get())
            .collect::<Vec<_>>(),
        vec![1001, 1003]
    );
    assert!(popular.first_page.items[0].popularity > popular.first_page.items[1].popularity);

    let detail = adapter
        .gallery_detail_get(crate::domain::GalleryId::new(1003).unwrap())
        .unwrap()
        .expect("detail exists");
    assert_eq!(detail.summary.title, "Sunlit Archive Fixture");
    assert_eq!(detail.summary.series, vec!["original"]);
    assert_eq!(detail.summary.characters, vec!["Example Character"]);
    assert_eq!(
        detail
            .related
            .iter()
            .map(|gallery| gallery.id.get())
            .collect::<Vec<_>>(),
        vec![1002]
    );
}

fn nozomi(ids: &[u32]) -> Vec<u8> {
    ids.iter().flat_map(|id| id.to_be_bytes()).collect()
}

fn gallery_script(id: u64, title: &str, related: &str) -> String {
    GALLERY_SCRIPT
        .replace("\"id\": \"424242\"", &format!("\"id\": \"{id}\""))
        .replace("Fixture } Landscape Collection", title)
        .replace("[424240, \"424241\", 424240]", related)
}

#[test]
#[ignore = "opt-in live Hitomi network smoke; run through tools/verify.ps1 -LiveSmoke"]
fn live_hitomi_smoke() {
    assert_eq!(
        std::env::var("ATSUMI_ALLOW_LIVE_SMOKE").as_deref(),
        Ok("1"),
        "live network access requires ATSUMI_ALLOW_LIVE_SMOKE=1"
    );
    let adapter = HitomiLiveAdapter::new(HitomiLiveConfig {
        max_candidate_ids: 3,
        query_cache_capacity: 1,
        related_gallery_limit: 1,
        ..HitomiLiveConfig::default()
    })
    .expect("construct live adapter");
    let result = adapter
        .search_submit(&SearchRequest {
            text: String::new(),
            include_tags: Vec::new(),
            exclude_tags: Vec::new(),
            languages: vec![Language::Korean],
            sort: SearchSort::Recent,
            page_size: 1,
        })
        .expect("live recent search");
    assert_eq!(result.first_page.items.len(), 1);
    assert!(result.first_page.items[0].id.get() > 0);
}

fn one_pixel_png() -> Vec<u8> {
    let mut bytes = Cursor::new(Vec::new());
    image::DynamicImage::new_rgba8(1, 1)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    bytes.into_inner()
}

const sheetPositions = ["pos-0", "pos-1", "pos-2", "pos-3", "pos-4", "pos-5"];
const sortOptions = [
  ["recent", "최신순"],
  ["popular_today", "인기순 · 오늘"],
  ["popular_week", "인기순 · 이번 주"],
  ["popular_month", "인기순 · 이번 달"],
  ["popular_year", "인기순 · 올해"],
  ["random", "무작위"],
];

const galleries = [
  {
    id: 4051038,
    title: "Archive of Rain",
    subtitle: "비 내리는 도시의 기록",
    artist: "serein",
    pages: 64,
    score: 98,
    date: "26-08-12",
    image: 0,
    tags: ["female:glasses", "female:long_hair", "male:suit", "mystery", "full_color"],
    favorite: true,
    status: "review",
  },
  {
    id: 4051027,
    title: "Summer Pool Notes",
    subtitle: "여름 수영부 노트",
    artist: "mizuno",
    pages: 84,
    score: 86,
    date: "26-08-12",
    image: 1,
    tags: ["female:swimsuit", "male:swimsuit", "female:short_hair", "school", "full_color"],
    status: "downloading",
    progress: 41,
  },
  {
    id: 4050974,
    title: "The Green Window",
    subtitle: "초록 창문의 비밀",
    artist: "paperlane",
    pages: 42,
    score: 71,
    date: "26-08-11",
    image: 2,
    tags: ["female:schoolgirl_uniform", "female:short_hair", "library", "mystery"],
    status: "completed",
  },
  {
    id: 4050932,
    title: "Platform 19",
    subtitle: "열아홉 번째 승강장",
    artist: "railbird",
    pages: 55,
    score: 92,
    date: "26-08-11",
    image: 3,
    tags: ["male:uniform", "male:short_hair", "science_fiction", "train", "full_color"],
  },
  {
    id: 4050891,
    title: "Festival Letter",
    subtitle: "축제에서 온 편지",
    artist: "akari",
    pages: 38,
    score: 78,
    date: "26-08-10",
    image: 4,
    tags: ["female:kimono", "female:hair_ornament", "festival", "romance", "full_color"],
    favorite: true,
  },
  {
    id: 4050806,
    title: "Paper Garden",
    subtitle: "종이 위에 핀 정원",
    artist: "sumie",
    pages: 29,
    score: 67,
    date: "26-08-10",
    image: 5,
    tags: ["male:dark_hair", "flowers", "monochrome", "artbook"],
    status: "completed",
  },
  {
    id: 4050754,
    title: "The Last Tram",
    subtitle: "마지막 전차",
    artist: "serein",
    pages: 76,
    score: 83,
    date: "26-08-09",
    image: 0,
    tags: ["female:coat", "male:suit", "rain", "drama", "night"],
    favorite: true,
    status: "failed",
  },
  {
    id: 4050642,
    title: "Blue Lane",
    subtitle: "푸른 레인의 끝",
    artist: "mizuno",
    pages: 48,
    score: 64,
    date: "26-08-09",
    image: 1,
    tags: ["female:swimsuit", "male:swimsuit", "sports", "school"],
  },
];

const viewConfig = {
  explore: { eyebrow: "EXPLORE", title: "갤러리 탐색", placeholder: "앨범, 작가, 태그 검색" },
  "auto-find": { eyebrow: "AUTO FIND", title: "즐겨찾기 작가 자동 탐색", placeholder: "현재 후보에서 검색" },
  downloads: { eyebrow: "DOWNLOADS", title: "다운로드 목록", placeholder: "다운로드 목록에서 검색" },
};

const state = {
  view: "explore",
  selected: new Set(),
  anchorId: null,
  query: "",
  localQueries: { "auto-find": "", downloads: "" },
  detailTabs: [],
  activeDetailId: null,
  detailMinimized: false,
  downloadFilter: "all",
};

const el = (id) => document.getElementById(id);
const grid = el("galleryGrid");
const searchInput = el("searchInput");
const suggestions = el("suggestions");
const selectionToolbar = el("selectionToolbar");

function galleryById(id) {
  return galleries.find((gallery) => gallery.id === Number(id));
}

function displayItems() {
  let items = galleries;
  if (state.view === "auto-find") items = galleries.filter((gallery) => gallery.favorite);
  if (state.view === "downloads") items = galleries.filter((gallery) => gallery.status);

  const query = state.view === "explore" ? state.query : state.localQueries[state.view];
  if (query.trim()) {
    const needle = query.trim().toLowerCase();
    items = items.filter((gallery) =>
      [gallery.title, gallery.subtitle, gallery.artist, ...gallery.tags].join(" ").toLowerCase().includes(needle),
    );
  }

  if (state.view === "downloads" && state.downloadFilter !== "all") {
    items = items.filter((gallery) => {
      if (state.downloadFilter === "active") return ["downloading", "queued"].includes(gallery.status);
      if (state.downloadFilter === "review") return gallery.status === "review";
      if (state.downloadFilter === "failed") return gallery.status === "failed";
      return gallery.status === "completed";
    });
  }
  return items;
}

function tagParts(tag) {
  const [namespace, ...rest] = tag.split(":");
  return { namespace: rest.length ? namespace : "tag", label: rest.length ? rest.join(":").replaceAll("_", " ") : tag.replaceAll("_", " ") };
}

function tagHtml(tag) {
  const parts = tagParts(tag);
  const cls = parts.namespace === "female" ? "female" : parts.namespace === "male" ? "male" : "";
  const favorite = ["glasses", "kimono"].includes(parts.label) ? " favorite" : "";
  return `<button class="tag ${cls}${favorite}" data-tag="${tag}" title="${parts.label} · 좌클릭 검색 / 우클릭 즐겨찾기">${parts.label}</button>`;
}

function statusHtml(gallery) {
  if (gallery.status === "review") return `<button class="status-pill" data-open-review="${gallery.id}">중복 의심</button>`;
  if (gallery.status === "failed") return `<button class="status-pill failed" data-status-detail="${gallery.id}">실패</button>`;
  if (gallery.status === "downloading") return `<span class="status-pill">받는 중 ${gallery.progress}%</span>`;
  return "";
}

function cardHtml(gallery) {
  const selected = state.selected.has(gallery.id) ? " is-selected" : "";
  const favorite = gallery.favorite ? " is-favorite" : "";
  const progress = gallery.status === "completed" ? 100 : gallery.progress || 0;
  return `
    <article class="gallery-card${selected}${favorite}" data-gallery-id="${gallery.id}" tabindex="0">
      <div class="cover ${sheetPositions[gallery.image]}">
        <span class="language-flag"><img src="../../assets/flags/kr.svg" alt="한국어" /></span>
        ${state.view === "explore" && gallery.status === "completed" ? '<span class="download-check">✓</span>' : ""}
      </div>
      <div class="card-content">
        <div class="card-title">${gallery.title}<span class="title-sub">${gallery.subtitle}</span></div>
        <button class="artist meta-chip${gallery.favorite ? " favorite" : ""}" data-meta="artist:${gallery.artist}">${gallery.artist}</button>
        <div class="tag-list">${gallery.tags.slice(0, 7).map(tagHtml).join("")}</div>
      </div>
      <div class="meta-column">
        <span>${gallery.date}</span>
        <span class="score" title="인기도 점수">${gallery.score}</span>
        ${statusHtml(gallery)}
        <div class="meta-bottom"><span>${gallery.pages}p</span><span>#${gallery.id}</span></div>
      </div>
      ${state.view === "downloads" ? `<div class="progress-track"><span style="width:${progress}%"></span></div>` : ""}
    </article>`;
}

function render() {
  const config = viewConfig[state.view];
  el("viewEyebrow").textContent = config.eyebrow;
  el("viewTitle").textContent = config.title;
  searchInput.placeholder = config.placeholder;
  searchInput.value = state.view === "explore" ? state.query : state.localQueries[state.view];
  document.querySelectorAll(".nav-item").forEach((button) => button.classList.toggle("is-active", button.dataset.view === state.view));

  renderHeadingActions();
  renderContext();
  renderSelection();
  const items = displayItems();
  grid.innerHTML = items.map(cardHtml).join("");
  el("contextSummary").textContent = `${items.length}개 결과`;
  el("pager").hidden = state.view !== "explore";
  bindGalleryEvents();
}

function renderHeadingActions() {
  const host = el("headingActions");
  if (state.view === "auto-find") {
    host.innerHTML = `
      <div class="segmented"><button class="is-active">전체</button><button>작가별</button></div>
      <button class="text-button" data-heading-action="refresh-authors"><span class="fluent">&#xE72C;</span> 즐겨찾기 작가 갱신</button>
      <button class="text-button dark" data-heading-action="queue-candidates"><span class="fluent">&#xE896;</span> 후보 다운로드</button>`;
  } else if (state.view === "downloads") {
    host.innerHTML = `
      <div class="segmented"><button class="is-active">전체</button><button>작가별</button></div>
      <button class="text-button" data-heading-action="internal-review"><span class="fluent">&#xE9D9;</span> 내부 중복 검사</button>
      <button class="text-button primary" data-heading-action="download-all"><span class="fluent">&#xE896;</span> 전체 다운로드</button>`;
  } else {
    host.innerHTML = "";
  }

  host.querySelectorAll(".segmented button").forEach((button) => {
    button.addEventListener("click", () => {
      button.parentElement.querySelectorAll("button").forEach((item) => item.classList.toggle("is-active", item === button));
      showToast(`${button.textContent} 보기로 전환했습니다.`);
    });
  });
  host.querySelectorAll("[data-heading-action]").forEach((button) => button.addEventListener("click", () => runHeadingAction(button.dataset.headingAction)));
}

function renderContext() {
  const host = el("contextLeft");
  if (state.view === "downloads") {
    host.innerHTML = `<div class="segmented status-filter">
      <button data-filter="all" class="${state.downloadFilter === "all" ? "is-active" : ""}">전체</button>
      <button data-filter="active" class="${state.downloadFilter === "active" ? "is-active" : ""}">작업 중</button>
      <button data-filter="review" class="${state.downloadFilter === "review" ? "is-active" : ""}">검토</button>
      <button data-filter="failed" class="${state.downloadFilter === "failed" ? "is-active" : ""}">실패</button>
      <button data-filter="complete" class="${state.downloadFilter === "complete" ? "is-active" : ""}">완료</button>
    </div>`;
    host.querySelectorAll("[data-filter]").forEach((button) => button.addEventListener("click", () => {
      state.downloadFilter = button.dataset.filter;
      state.selected.clear();
      render();
    }));
  } else if (state.view === "auto-find") {
    host.innerHTML = `<span class="context-summary">마지막 갱신 · 오늘 02:18</span>`;
  } else {
    host.innerHTML = `<div class="select-control"><label for="sortSelectProxy">정렬</label><select id="sortSelectProxy">${sortOptions.map(([value, label]) => `<option value="${value}">${label}</option>`).join("")}</select></div>`;
  }
}

function renderSelection() {
  const count = state.selected.size;
  selectionToolbar.classList.toggle("is-visible", count > 0);
  if (!count) {
    selectionToolbar.innerHTML = "";
    return;
  }
  const primary = state.view === "downloads" ? "선택 파일 다운로드" : "다운로드";
  const destructive = state.view === "downloads" ? "제거" : "제외";
  selectionToolbar.innerHTML = `
    <strong>${count}개 선택됨</strong>
    <button class="text-button" data-selection="all">전체 선택</button>
    <button class="text-button primary" data-selection="primary"><span class="fluent">&#xE896;</span> ${primary}</button>
    <button class="text-button danger-button" data-selection="delete">${destructive}</button>`;
  selectionToolbar.querySelectorAll("[data-selection]").forEach((button) => button.addEventListener("click", () => runSelectionAction(button.dataset.selection)));
}

function bindGalleryEvents() {
  grid.querySelectorAll(".gallery-card").forEach((card) => {
    card.addEventListener("click", onCardClick);
    card.addEventListener("dblclick", onCardDoubleClick);
    card.addEventListener("contextmenu", onCardContextMenu);
  });
  grid.querySelectorAll("[data-meta], [data-tag]").forEach((chip) => {
    chip.addEventListener("click", onMetadataSearch);
    chip.addEventListener("contextmenu", onMetadataFavorite);
  });
  grid.querySelectorAll("[data-open-review]").forEach((button) => button.addEventListener("click", (event) => {
    event.stopPropagation();
    openReview(button.dataset.openReview);
  }));
  grid.querySelectorAll("[data-status-detail]").forEach((button) => button.addEventListener("click", (event) => {
    event.stopPropagation();
    el("activityPanel").hidden = false;
  }));
}

function onCardClick(event) {
  if (event.target.closest("button")) return;
  const id = Number(event.currentTarget.dataset.galleryId);
  const items = displayItems();
  if (event.shiftKey && state.anchorId) {
    const start = items.findIndex((item) => item.id === state.anchorId);
    const end = items.findIndex((item) => item.id === id);
    if (start >= 0 && end >= 0) {
      const [from, to] = start < end ? [start, end] : [end, start];
      items.slice(from, to + 1).forEach((item) => state.selected.add(item.id));
    }
  } else if (event.ctrlKey) {
    state.selected.has(id) ? state.selected.delete(id) : state.selected.add(id);
    state.anchorId = id;
  } else if (state.selected.size === 1 && state.selected.has(id)) {
    state.selected.clear();
    state.anchorId = null;
  } else {
    state.selected = new Set([id]);
    state.anchorId = id;
  }
  render();
}

function onCardDoubleClick(event) {
  if (event.target.closest("button")) return;
  const gallery = galleryById(event.currentTarget.dataset.galleryId);
  if (state.view === "downloads" && gallery.status === "completed") showToast(`${gallery.title}의 첫 이미지를 기본 뷰어로 엽니다.`);
  else openDetail(gallery.id);
}

function onCardContextMenu(event) {
  if (event.target.closest("button")) return;
  event.preventDefault();
  openDetail(event.currentTarget.dataset.galleryId);
}

function onMetadataSearch(event) {
  event.stopPropagation();
  const value = event.currentTarget.dataset.meta || event.currentTarget.dataset.tag;
  state.view = "explore";
  state.query = value;
  state.selected.clear();
  render();
  showToast(`${value} 검색으로 이동했습니다.`);
}

function onMetadataFavorite(event) {
  event.preventDefault();
  event.stopPropagation();
  event.currentTarget.classList.toggle("favorite");
  showToast(event.currentTarget.classList.contains("favorite") ? "즐겨찾기에 추가했습니다." : "즐겨찾기에서 제거했습니다.");
}

function runSelectionAction(action) {
  if (action === "all") displayItems().forEach((gallery) => state.selected.add(gallery.id));
  if (action === "primary") showToast(`${state.selected.size}개 항목을 ${state.view === "downloads" ? "다운로드" : "대기열에 추가"}합니다.`);
  if (action === "delete") showToast(`${state.selected.size}개 항목의 ${state.view === "downloads" ? "격리 계획" : "제외 확인"}을 엽니다.`);
  render();
}

function runHeadingAction(action) {
  if (action === "refresh-authors") {
    el("refreshButton").classList.add("is-loading");
    showToast("즐겨찾기 작가의 새 작품을 확인하고 있습니다.");
    setTimeout(() => showToast("4개의 새 후보를 찾았습니다."), 900);
  } else if (action === "queue-candidates") showToast("현재 후보를 다운로드 대기열에 추가했습니다.");
  else if (action === "internal-review") showToast("검사할 다운로드 항목을 하나 선택하세요.");
  else showToast("대기와 실패 항목의 다운로드를 시작합니다.");
}

function suggestionItems(query) {
  const all = [
    { type: "HISTORY", value: "artist:serein", extra: "최근 검색어", favorite: true },
    { type: "HISTORY", value: "language:korean", extra: "최근 검색어" },
    { type: "ARTIST", value: "artist:akari", extra: "218 galleries", favorite: true },
    { type: "ARTIST", value: "artist:paperlane", extra: "87 galleries" },
    { type: "TAG", value: "female:glasses", extra: "12,482 galleries", favorite: true },
    { type: "TAG", value: "female:swimsuit", extra: "31,028 galleries" },
  ];
  if (!query) return all.filter((item) => item.type === "HISTORY").slice(0, 7);
  const needle = query.toLowerCase();
  return all.filter((item) => item.value.toLowerCase().includes(needle)).sort((a, b) => {
    const aPrefix = a.value.toLowerCase().startsWith(needle) ? 0 : 1;
    const bPrefix = b.value.toLowerCase().startsWith(needle) ? 0 : 1;
    return aPrefix - bPrefix;
  });
}

function renderSuggestions() {
  const items = suggestionItems(searchInput.value.trim());
  suggestions.hidden = !items.length;
  suggestions.innerHTML = items.map((item, index) => `
    <button type="button" class="suggestion${item.favorite ? " is-favorite" : ""}" data-suggestion-index="${index}">
      <span class="suggestion-type">${item.type}</span>
      <strong>${item.favorite ? "★ " : ""}${item.value}</strong>
      <small>${item.extra}</small>
    </button>`).join("");
  suggestions.querySelectorAll(".suggestion").forEach((button) => button.addEventListener("click", () => {
    searchInput.value = items[Number(button.dataset.suggestionIndex)].value;
    suggestions.hidden = true;
    searchInput.focus();
  }));
}

function submitSearch() {
  suggestions.hidden = true;
  if (state.view === "explore") state.query = searchInput.value.trim();
  else state.localQueries[state.view] = searchInput.value.trim();
  state.selected.clear();
  render();
  showToast(state.view === "explore" ? "웹 검색 결과를 불러왔습니다." : "현재 탭의 결과를 필터했습니다.");
}

function openDetail(id, parentId = null) {
  id = Number(id);
  if (!state.detailTabs.includes(id)) {
    const parentIndex = parentId ? state.detailTabs.indexOf(Number(parentId)) : -1;
    if (parentIndex >= 0) state.detailTabs.splice(parentIndex + 1, 0, id);
    else state.detailTabs.push(id);
  }
  state.activeDetailId = id;
  state.detailMinimized = false;
  renderDetail();
}

function renderDetail() {
  const workspace = el("detailWorkspace");
  workspace.hidden = !state.detailTabs.length || state.detailMinimized;
  el("detailRestore").hidden = !state.detailMinimized || !state.detailTabs.length;
  el("detailRestoreLabel").textContent = `상세 탭 ${state.detailTabs.length}`;
  if (!state.detailTabs.length) return;

  el("detailTabs").innerHTML = state.detailTabs.map((id) => {
    const gallery = galleryById(id);
    return `<button class="detail-tab${id === state.activeDetailId ? " is-active" : ""}" data-detail-tab="${id}">
      <span>${gallery.title}</span><span class="tab-close" data-close-tab="${id}">×</span>
    </button>`;
  }).join("");

  el("detailTabs").querySelectorAll("[data-detail-tab]").forEach((tab) => tab.addEventListener("click", (event) => {
    const close = event.target.closest("[data-close-tab]");
    if (close) {
      event.stopPropagation();
      closeDetailTab(Number(close.dataset.closeTab));
      return;
    }
    state.activeDetailId = Number(tab.dataset.detailTab);
    renderDetail();
  }));

  const gallery = galleryById(state.activeDetailId);
  const related = galleries.filter((item) => item.id !== gallery.id).slice(0, 5);
  el("detailBody").innerHTML = `
    <div class="detail-layout">
      <section class="detail-media">
        <div class="detail-cover ${sheetPositions[gallery.image]}"></div>
        <div class="preview-grid">${Array.from({ length: 24 }, (_, index) => `<button class="preview-thumb ${sheetPositions[(gallery.image + index) % 6]}" title="${index + 1}페이지 확대"><span>${index + 1}</span></button>`).join("")}</div>
      </section>
      <section class="detail-info">
        <div class="detail-title-row">
          <div><span class="eyebrow">FLOATING DETAIL</span><h2>${gallery.title}<br />${gallery.subtitle}</h2><p>#${gallery.id} · ${gallery.pages} pages</p></div>
          <button class="icon-button" title="다운로드" aria-label="다운로드"><span class="fluent">&#xE896;</span></button>
        </div>
        <div class="metadata-grid">
          ${metadataBox("작가", [gallery.artist], "artist", gallery.favorite)}
          ${metadataBox("그룹", ["paper studio"], "group")}
          ${metadataBox("언어", ["Korean"], "language")}
          ${metadataBox("시리즈", ["original"], "series")}
          ${metadataBox("캐릭터", [], "character")}
          <div class="metadata-box tags-box"><span>태그</span><div class="metadata-value">${gallery.tags.map(tagHtml).join("")}</div></div>
        </div>
        <section class="related-section"><div class="section-heading"><h3>Related galleries</h3><span>5</span></div><div class="related-list">
          ${related.map((item) => relatedHtml(item)).join("")}
        </div></section>
      </section>
    </div>`;

  el("detailBody").querySelectorAll("[data-related-id]").forEach((button) => button.addEventListener("contextmenu", (event) => {
    event.preventDefault();
    openDetail(button.dataset.relatedId, gallery.id);
  }));
  el("detailBody").querySelectorAll("[data-related-id]").forEach((button) => button.addEventListener("dblclick", () => openDetail(button.dataset.relatedId, gallery.id)));
  el("detailBody").querySelectorAll("[data-meta], [data-tag]").forEach((chip) => {
    chip.addEventListener("click", onMetadataSearch);
    chip.addEventListener("contextmenu", onMetadataFavorite);
  });
  el("detailBody").querySelectorAll(".preview-thumb").forEach((button) => button.addEventListener("click", () => showToast(`${button.title} 미리보기를 엽니다.`)));
  el("detailBody").scrollTop = 0;
}

function metadataBox(label, values, type, favorite = false) {
  return `<div class="metadata-box"><span>${label}</span><div class="metadata-value">${values.map((value) => `<button class="meta-chip${favorite ? " favorite" : ""}" data-meta="${type}:${value}">${value}</button>`).join("")}</div></div>`;
}

function relatedHtml(gallery) {
  return `<article class="related-card" data-related-id="${gallery.id}" title="더블클릭 또는 우클릭으로 상세 열기">
    <div class="related-cover ${sheetPositions[gallery.image]}"></div>
    <div class="related-copy"><strong>${gallery.title} | ${gallery.subtitle}</strong><button class="artist meta-chip${gallery.favorite ? " favorite" : ""}" data-meta="artist:${gallery.artist}">${gallery.artist}</button><div class="tag-list">${gallery.tags.slice(0, 4).map(tagHtml).join("")}</div></div>
    <div class="related-meta"><span>${gallery.pages}p</span><span>#${gallery.id}</span></div>
  </article>`;
}

function closeDetailTab(id) {
  const index = state.detailTabs.indexOf(id);
  state.detailTabs = state.detailTabs.filter((item) => item !== id);
  if (state.activeDetailId === id) state.activeDetailId = state.detailTabs[index] || state.detailTabs[index - 1] || null;
  if (!state.detailTabs.length) state.detailMinimized = false;
  renderDetail();
}

function openReview(id) {
  const parent = galleryById(id) || galleries[0];
  const candidate = galleries.find((item) => item.id !== parent.id && item.artist === parent.artist) || galleries[6];
  el("reviewColumns").innerHTML = reviewCardHtml(parent, "부모 상세") + reviewCardHtml(candidate, "후보 상세");
  el("pairStrip").innerHTML = Array.from({ length: 14 }, (_, index) => `<div class="pair"><div class="pair-image ${sheetPositions[(parent.image + index) % 6]}"><span>${index + 1}</span></div><div class="pair-image ${sheetPositions[(candidate.image + index) % 6]}"><span>${index + 3}</span></div></div>`).join("");
  el("reviewDialog").showModal();
}

function reviewCardHtml(gallery, label) {
  return `<section class="review-card"><h3>${label}</h3><div class="review-hero ${sheetPositions[gallery.image]}"></div><dl class="review-fields">
    <dt>제목</dt><dd>${gallery.title}<br />${gallery.subtitle}</dd>
    <dt>작가</dt><dd>${gallery.artist}</dd>
    <dt>언어</dt><dd>korean</dd>
    <dt>페이지</dt><dd>${gallery.pages}p</dd>
    <dt>EH ID</dt><dd>#${gallery.id}</dd>
    <dt>first gid</dt><dd>#${gallery.id - 137} · 일치</dd>
    <dt>parent gid</dt><dd>- · 불일치</dd>
  </dl></section>`;
}

function showToast(message) {
  const toast = el("toast");
  toast.textContent = message;
  toast.hidden = false;
  clearTimeout(showToast.timer);
  showToast.timer = setTimeout(() => { toast.hidden = true; }, 2200);
}

document.querySelectorAll(".nav-item").forEach((button) => button.addEventListener("click", () => {
  state.view = button.dataset.view;
  state.selected.clear();
  state.anchorId = null;
  suggestions.hidden = true;
  render();
}));

el("sidebarToggle").addEventListener("click", () => {
  el("appShell").classList.toggle("sidebar-collapsed");
  el("sidebarToggle").title = el("appShell").classList.contains("sidebar-collapsed") ? "메뉴 펼치기" : "메뉴 접기";
});

el("searchForm").addEventListener("submit", (event) => { event.preventDefault(); submitSearch(); });
el("searchButton").addEventListener("click", submitSearch);
searchInput.addEventListener("focus", renderSuggestions);
searchInput.addEventListener("input", renderSuggestions);
document.addEventListener("pointerdown", (event) => {
  if (!event.target.closest(".search-box")) suggestions.hidden = true;
  if (!event.target.closest(".menu-anchor")) {
    el("languageMenu").hidden = true;
    el("languageButton").setAttribute("aria-expanded", "false");
  }
});

el("languageButton").addEventListener("click", () => {
  const menu = el("languageMenu");
  menu.hidden = !menu.hidden;
  el("languageButton").setAttribute("aria-expanded", String(!menu.hidden));
});

el("languageMenu").addEventListener("change", () => {
  const count = el("languageMenu").querySelectorAll("input:checked").length;
  el("languageCount").textContent = count;
});

el("refreshButton").addEventListener("click", () => showToast(state.view === "explore" ? "현재 검색을 다시 불러옵니다." : "현재 화면을 갱신했습니다."));
el("activityButton").addEventListener("click", () => { el("activityPanel").hidden = !el("activityPanel").hidden; });
el("activityClose").addEventListener("click", () => { el("activityPanel").hidden = true; });
el("activityPanel").querySelectorAll("[data-open-review]").forEach((button) => button.addEventListener("click", () => openReview(button.dataset.openReview)));
el("settingsButton").addEventListener("click", () => el("settingsDialog").showModal());

el("detailMinimize").addEventListener("click", () => { state.detailMinimized = true; renderDetail(); });
el("detailCloseAll").addEventListener("click", () => { state.detailTabs = []; state.activeDetailId = null; renderDetail(); });
el("detailRestore").addEventListener("click", () => { state.detailMinimized = false; renderDetail(); });

el("reviewDialog").querySelectorAll("[data-review-action]").forEach((button) => button.addEventListener("click", () => {
  showToast(`${button.textContent} 판정 계획을 저장했습니다.`);
  el("reviewDialog").close();
}));

el("columnsRange").addEventListener("input", (event) => {
  document.documentElement.style.setProperty("--max-columns", event.target.value);
  el("columnsOutput").textContent = `${event.target.value}열`;
});
el("previewRange").addEventListener("input", (event) => {
  document.documentElement.style.setProperty("--preview-width", `${event.target.value}px`);
  el("previewOutput").textContent = `${event.target.value}px`;
});
el("cacheRange").addEventListener("input", (event) => { el("cacheOutput").textContent = `${event.target.value}GB`; });

document.addEventListener("keydown", (event) => {
  if (["INPUT", "TEXTAREA", "SELECT"].includes(document.activeElement.tagName)) return;
  if (event.key === "Escape") {
    state.selected.clear();
    render();
  }
  if (event.key === "Enter" && state.selected.size) runSelectionAction("primary");
  if (event.key === "Delete" && state.selected.size) runSelectionAction("delete");
});

render();

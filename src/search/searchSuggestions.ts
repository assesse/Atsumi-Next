import type { FavoriteRecord, SearchHistoryEntry, SearchRequest } from "../api/contracts";
import type { Gallery } from "../core/types";
import { activeSearchToken, canonicalSearchToken, normalizeTokenValue, searchTokenKind, type SearchTokenKind } from "./searchTokens";

export type SearchSuggestionType = "HISTORY" | "TITLE" | "ARTIST" | "GROUP" | "SERIES" | "CHARACTER" | "TAG" | "FEMALE" | "MALE";

export type SearchSuggestion = Readonly<{
  type: SearchSuggestionType;
  token: string;
  label: string;
  extra: string;
  favorite?: boolean;
  observedCount?: number;
  historyUseCount?: number;
  lastUsedAt?: string;
  request?: SearchRequest;
}>;

type CatalogInput = Readonly<{
  history: readonly SearchHistoryEntry[];
  favorites: readonly FavoriteRecord[];
  galleries: Iterable<Gallery>;
}>;

const typeForKind: Record<SearchTokenKind, SearchSuggestionType> = {
  artist: "ARTIST",
  group: "GROUP",
  series: "SERIES",
  character: "CHARACTER",
  tag: "TAG",
  female: "FEMALE",
  male: "MALE",
};

const readable = (value: string): string => value.replaceAll("_", " ");
const tagToken = (value: string): string => canonicalSearchToken(value, searchTokenKind(value) ?? "tag");

export function historyDisplayToken(entry: SearchHistoryEntry): string {
  if (entry.text.trim()) return entry.text;
  const include = entry.includeTags.at(0);
  if (include) return tagToken(include);
  const exclude = entry.excludeTags.at(0);
  return exclude ? `-${tagToken(exclude)}` : "";
}

function favoriteToken(record: FavoriteRecord): string {
  return record.namespace === "tag" ? tagToken(record.value) : canonicalSearchToken(`${record.namespace}:${record.value}`);
}

function candidateLabel(token: string, type: SearchSuggestionType): string {
  if (type === "TITLE") return token;
  const value = token.replace(/^-?(artist|group|series|character|tag|female|male):/i, "");
  return readable(value);
}

function candidateFromToken(token: string, type: SearchSuggestionType, extra: string): SearchSuggestion {
  return { type, token, label: candidateLabel(token, type), extra };
}

export function buildSearchSuggestionCatalog(input: CatalogInput): SearchSuggestion[] {
  const history = new Map<string, SearchSuggestion>();
  for (const entry of input.history) {
    const token = historyDisplayToken(entry);
    if (!token) continue;
    const key = JSON.stringify([entry.text, entry.includeTags, entry.excludeTags, entry.languages, entry.sort, entry.pageSize]);
    const previous = history.get(key);
    if (previous && (previous.historyUseCount ?? 0) >= entry.useCount) continue;
    const conditions = entry.includeTags.length + entry.excludeTags.length;
    history.set(key, {
      type: "HISTORY",
      token,
      label: token,
      extra: `최근 검색 · ${entry.useCount}회${conditions ? ` · 태그 조건 ${conditions}개` : ""}`,
      historyUseCount: entry.useCount,
      lastUsedAt: entry.lastUsedAt,
      request: { text: entry.text, includeTags: [...entry.includeTags], excludeTags: [...entry.excludeTags], languages: [...entry.languages], sort: entry.sort, pageSize: entry.pageSize },
    });
  }

  const favoriteTokens = new Set(input.favorites.map(favoriteToken));
  const catalog = new Map<string, SearchSuggestion>();
  const add = (token: string, type: SearchSuggestionType, extra: string, observed = true) => {
    const canonical = type === "TITLE" ? token.trim() : canonicalSearchToken(token);
    if (!canonical) return;
    const key = type === "TITLE" ? `title:${normalizeTokenValue(canonical)}` : canonical;
    const existing = catalog.get(key);
    if (existing) {
      catalog.set(key, { ...existing, observedCount: (existing.observedCount ?? 0) + (observed ? 1 : 0), favorite: existing.favorite || favoriteTokens.has(canonical) });
      return;
    }
    catalog.set(key, { ...candidateFromToken(canonical, type, extra), observedCount: observed ? 1 : 0, favorite: favoriteTokens.has(canonical) });
  };

  for (const favorite of input.favorites) {
    const token = favoriteToken(favorite);
    const kind = searchTokenKind(token);
    add(token, kind ? typeForKind[kind] : "TAG", `${favorite.namespace} 즐겨찾기`, false);
  }
  for (const gallery of input.galleries) {
    add(gallery.title, "TITLE", "현재 metadata 제목");
    add(`artist:${gallery.artist}`, "ARTIST", "현재 metadata 작가");
    if (gallery.group) add(`group:${gallery.group}`, "GROUP", "현재 metadata 그룹");
    gallery.series.forEach((value) => add(`series:${value}`, "SERIES", "현재 metadata 시리즈"));
    gallery.characters.forEach((value) => add(`character:${value}`, "CHARACTER", "현재 metadata 캐릭터"));
    gallery.tags.forEach((value) => {
      const kind = searchTokenKind(value);
      add(kind ? value : `tag:${value}`, kind ? typeForKind[kind] : "TAG", "현재 metadata 태그");
    });
  }
  return [...history.values(), ...catalog.values()];
}

type ActivePrefix = Readonly<{ kind?: SearchTokenKind; negative: boolean; needle: string; token: string }>;

function activePrefix(input: string, caretStart: number, caretEnd: number): ActivePrefix {
  const raw = activeSearchToken(input, caretStart, caretEnd).value;
  const negative = raw.startsWith("-");
  const token = raw.replace(/^-/, "");
  const kind = searchTokenKind(token);
  return { kind: kind ?? undefined, negative, needle: (kind ? token.slice(token.indexOf(":") + 1) : token).toLocaleLowerCase(), token: raw };
}

function matchRank(candidate: SearchSuggestion, needle: string): number | null {
  const normalizedNeedle = normalizeTokenValue(needle);
  if (!normalizedNeedle) return 4;
  const tokenValue = candidate.token.replace(/^-?(artist|group|series|character|tag|female|male):/i, "");
  const label = normalizeTokenValue(candidate.label);
  if (tokenValue === normalizedNeedle || label === normalizedNeedle) return 0;
  if (tokenValue.startsWith(normalizedNeedle) || label.startsWith(normalizedNeedle)) return 1;
  if (tokenValue.split("_").some((word) => word.startsWith(normalizedNeedle)) || label.split("_").some((word) => word.startsWith(normalizedNeedle))) return 2;
  if (tokenValue.includes(normalizedNeedle) || label.includes(normalizedNeedle)) return 3;
  return null;
}

function typeMatchesPrefix(type: SearchSuggestionType, kind?: SearchTokenKind): boolean {
  return !kind || type === typeForKind[kind];
}

export function filterSearchSuggestions(catalog: readonly SearchSuggestion[], input: string, caretStart: number, caretEnd = caretStart): SearchSuggestion[] {
  const active = activePrefix(input, caretStart, caretEnd);
  if (!active.token.trim()) {
    const history = catalog.filter((item) => item.type === "HISTORY").sort((a, b) => (b.historyUseCount ?? 0) - (a.historyUseCount ?? 0) || (b.lastUsedAt ?? "").localeCompare(a.lastUsedAt ?? ""));
    const favorites = catalog.filter((item) => item.type !== "HISTORY" && item.favorite);
    return [...history.slice(0, 4), ...favorites].slice(0, 8);
  }
  const ranked = catalog
    .filter((item) => typeMatchesPrefix(item.type, active.kind))
    .map((item) => ({ item, rank: matchRank(item, active.needle) }))
    .filter((entry): entry is { item: SearchSuggestion; rank: number } => entry.rank !== null)
    .sort((a, b) => a.rank - b.rank
      || Number(Boolean(b.item.favorite)) - Number(Boolean(a.item.favorite))
      || (b.item.historyUseCount ?? 0) - (a.item.historyUseCount ?? 0)
      || (b.item.lastUsedAt ?? "").localeCompare(a.item.lastUsedAt ?? "")
      || (b.item.observedCount ?? 0) - (a.item.observedCount ?? 0)
      || a.item.label.localeCompare(b.item.label));
  const suggestions = ranked.map((entry) => entry.item).slice(0, 8);
  if (active.kind && active.needle && suggestions.length === 0) {
    const token = `${active.negative ? "-" : ""}${active.kind}:${normalizeTokenValue(active.needle)}`;
    suggestions.unshift({ type: typeForKind[active.kind], token, label: readable(normalizeTokenValue(active.needle)), extra: active.kind === "tag" ? "입력한 태그로 전역 검색" : "입력한 조건으로 전역 검색" });
  }
  return suggestions.slice(0, 8);
}

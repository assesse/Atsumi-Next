import { useEffect, useMemo, useRef, useState, type FormEvent, type KeyboardEvent } from "react";
import type { SearchRequest } from "../api/contracts";
import type { Language, SearchUi, ViewId } from "../core/types";
import { languageOrder, languagePresentation } from "../data/languages";
import { FluentIcon } from "./FluentIcon";

export type SearchSuggestion = {
  type: "HISTORY" | "ARTIST" | "TAG";
  value: string;
  extra: string;
  favorite?: boolean;
  request?: SearchRequest;
};

const languageOptions = languageOrder.map((value) => ({ value, ...languagePresentation[value] }));

const placeholders: Record<ViewId, string> = {
  explore: "앨범, 작가, 태그 검색",
  "auto-find": "현재 후보에서 검색",
  downloads: "다운로드 목록에서 검색",
};

type ViewHeaderProps = {
  view: ViewId;
  search: SearchUi;
  suggestions: SearchSuggestion[];
  activityCount: number;
  activityOpen: boolean;
  onDraft: (value: string) => void;
  onSuggestions: (open: boolean, active?: number | null) => void;
  onCommit: (value?: string) => void;
  onSelectSuggestion: (suggestion: SearchSuggestion) => void;
  onLanguages: (languages: Language[]) => void;
  onRefresh: () => void;
  onActivity: () => void;
  onSettings: () => void;
};

export function ViewHeader({
  view,
  search,
  suggestions,
  activityCount,
  activityOpen,
  onDraft,
  onSuggestions,
  onCommit,
  onSelectSuggestion,
  onLanguages,
  onRefresh,
  onActivity,
  onSettings,
}: ViewHeaderProps) {
  const host = useRef<HTMLElement>(null);
  const languageButton = useRef<HTMLButtonElement>(null);
  const [languageOpen, setLanguageOpen] = useState(false);
  const visibleSuggestions = useMemo(() => {
    const needle = search.draft.trim().toLocaleLowerCase();
    if (!needle) return suggestions.slice(0, 7);
    return suggestions
      .filter((item) => item.value.toLocaleLowerCase().includes(needle))
      .sort((left, right) => {
        const leftPrefix = left.value.toLocaleLowerCase().startsWith(needle) ? 0 : 1;
        const rightPrefix = right.value.toLocaleLowerCase().startsWith(needle) ? 0 : 1;
        return leftPrefix - rightPrefix;
      });
  }, [search.draft, suggestions]);

  useEffect(() => {
    const closeTransient = (event: PointerEvent) => {
      if (!host.current?.contains(event.target as Node)) {
        setLanguageOpen(false);
        onSuggestions(false);
      }
    };
    const closeOnEscape = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape" || (!languageOpen && !search.suggestionsOpen)) return;
      event.preventDefault();
      event.stopImmediatePropagation();
      const restoreLanguageFocus = languageOpen;
      setLanguageOpen(false);
      onSuggestions(false);
      if (restoreLanguageFocus) window.requestAnimationFrame(() => languageButton.current?.focus());
    };
    document.addEventListener("pointerdown", closeTransient);
    document.addEventListener("keydown", closeOnEscape);
    return () => {
      document.removeEventListener("pointerdown", closeTransient);
      document.removeEventListener("keydown", closeOnEscape);
    };
  }, [languageOpen, onSuggestions, search.suggestionsOpen]);

  const submit = (event: FormEvent) => {
    event.preventDefault();
    if (search.activeSuggestion !== null) {
      const item = visibleSuggestions[search.activeSuggestion];
      if (item) {
        onSelectSuggestion(item);
        return;
      }
    }
    onCommit();
  };

  const keyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "ArrowDown" && visibleSuggestions.length) {
      event.preventDefault();
      const next = search.activeSuggestion === null ? 0 : (search.activeSuggestion + 1) % visibleSuggestions.length;
      onSuggestions(true, next);
    } else if (event.key === "ArrowUp" && visibleSuggestions.length) {
      event.preventDefault();
      const next = search.activeSuggestion === null
        ? visibleSuggestions.length - 1
        : (search.activeSuggestion - 1 + visibleSuggestions.length) % visibleSuggestions.length;
      onSuggestions(true, next);
    } else if (event.key === "Escape") {
      onSuggestions(false);
    }
  };

  const toggleLanguage = (language: Language) => {
    const languages = search.languages.includes(language)
      ? search.languages.filter((item) => item !== language)
      : [...search.languages, language];
    onLanguages(languages);
  };

  return (
    <header className="view-header" ref={host}>
      <form id="gallery-search-form" className="search-box" autoComplete="off" onSubmit={submit}>
        <FluentIcon glyph="\uE721" />
        <input
          type="search"
          role="combobox"
          aria-autocomplete="list"
          value={search.draft}
          placeholder={placeholders[view]}
          aria-label="검색"
          aria-controls="search-suggestions"
          aria-expanded={search.suggestionsOpen && visibleSuggestions.length > 0}
          aria-activedescendant={
            search.activeSuggestion === null ? undefined : `search-suggestion-${search.activeSuggestion}`
          }
          onFocus={() => onSuggestions(true)}
          onChange={(event) => {
            onDraft(event.target.value);
            onSuggestions(true);
          }}
          onKeyDown={keyDown}
        />
        {search.suggestionsOpen && visibleSuggestions.length ? (
          <div className="suggestions" id="search-suggestions" role="listbox" aria-label="검색 제안">
            {visibleSuggestions.map((item, index) => (
              <button
                key={`${item.type}-${item.value}`}
                id={`search-suggestion-${index}`}
                type="button"
                role="option"
                tabIndex={-1}
                aria-selected={search.activeSuggestion === index}
                className={`suggestion${item.favorite ? " is-favorite" : ""}${
                  search.activeSuggestion === index ? " is-active" : ""
                }`}
                onMouseDown={(event) => event.preventDefault()}
                onClick={() => {
                  onSelectSuggestion(item);
                }}
              >
                <span className="suggestion-type">{item.type}</span>
                <strong>{item.favorite ? `★ ${item.value}` : item.value}</strong>
                <small>{item.extra}</small>
              </button>
            ))}
          </div>
        ) : null}
      </form>
      <button type="submit" form="gallery-search-form" className="icon-button primary-soft" title="검색" aria-label="검색">
        <FluentIcon glyph="\uE721" />
      </button>
      <div className="menu-anchor">
        <button
          type="button"
          ref={languageButton}
          className={`icon-button${search.languages.length ? " is-active" : ""}`}
          title="언어 필터"
          aria-label="언어 필터"
          aria-expanded={languageOpen}
          onClick={() => setLanguageOpen((open) => !open)}
        >
          <FluentIcon glyph="\uE774" />
        </button>
        {languageOpen ? (
          <div className="popover language-popover">
            <strong>언어</strong>
            {languageOptions.map((option) => (
              <label key={option.value}>
                <input
                  type="checkbox"
                  checked={search.languages.includes(option.value)}
                  onChange={() => toggleLanguage(option.value)}
                />
                {option.icon ? (
                  <img className="language-option-icon" src={option.icon} alt="" />
                ) : option.fallback ? (
                  <span className="language-option-fallback" aria-hidden="true">{option.fallback}</span>
                ) : null}
                {option.label}
              </label>
            ))}
          </div>
        ) : null}
      </div>
      <button type="button" className="icon-button" title="현재 화면 새로고침" aria-label="현재 화면 새로고침" onClick={onRefresh}>
        <FluentIcon glyph="\uE72C" />
      </button>
      <button
        type="button"
        className="icon-button activity-button"
        title="작업 상태"
        aria-label="작업 상태"
        aria-controls="activity-panel"
        aria-expanded={activityOpen}
        onClick={onActivity}
      >
        <FluentIcon glyph="\uE9D9" />
        {activityCount > 0 ? <span className="activity-count">{activityCount}</span> : null}
      </button>
      <button type="button" className="icon-button" title="설정" aria-label="설정" onClick={onSettings}>
        <FluentIcon glyph="\uE713" />
      </button>
    </header>
  );
}

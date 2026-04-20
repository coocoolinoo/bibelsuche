import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";
import "./App.css";

type TranslationInfo = { id: string; label: string };

type SearchMode = "contains" | "starts_with" | "whole_word" | "regex";
type ThemeMode = "dark" | "light";

type SearchOptions = {
  case_sensitive: boolean;
  ignore_diacritics: boolean;
  max_results?: number | null;
};

type VerseHit = {
  translation_id: string;
  bnumber: number;
  cnumber: number;
  vnumber: number;
  reference: string;
  text: string;
};

const DE_BOOK_ABBR: Record<number, string> = {
  1: "1Mo", 2: "2Mo", 3: "3Mo", 4: "4Mo", 5: "5Mo", 6: "Jos", 7: "Ri",
  8: "Rt", 9: "1Sam", 10: "2Sam", 11: "1Kön", 12: "2Kön", 13: "1Chr", 14: "2Chr",
  15: "Esr", 16: "Neh", 17: "Est", 18: "Hi", 19: "Ps", 20: "Spr", 21: "Pred",
  22: "Hld", 23: "Jes", 24: "Jer", 25: "Klgl", 26: "Hes", 27: "Dan", 28: "Hos",
  29: "Joel", 30: "Am", 31: "Obd", 32: "Jona", 33: "Mi", 34: "Nah", 35: "Hab",
  36: "Zef", 37: "Hag", 38: "Sach", 39: "Mal", 40: "Mt", 41: "Mk", 42: "Lk",
  43: "Joh", 44: "Apg", 45: "Rö", 46: "1Kor", 47: "2Kor", 48: "Gal", 49: "Eph",
  50: "Phil", 51: "Kol", 52: "1Th", 53: "2Th", 54: "1Tim", 55: "2Tim", 56: "Tit",
  57: "Phlm", 58: "Hebr", 59: "Jak", 60: "1Petr", 61: "2Petr", 62: "1Joh",
  63: "2Joh", 64: "3Joh", 65: "Jud", 66: "Offb",
};

function toShortReference(hit: VerseHit): string {
  const book = DE_BOOK_ABBR[hit.bnumber] ?? hit.reference;
  return `${book} ${hit.cnumber},${hit.vnumber}`;
}

function SunIcon() {
  return (
    <svg viewBox="0 0 24 24" className="theme-icon" aria-hidden="true">
      <circle cx="12" cy="12" r="4" />
      <line x1="12" y1="2.5" x2="12" y2="5.5" />
      <line x1="12" y1="18.5" x2="12" y2="21.5" />
      <line x1="2.5" y1="12" x2="5.5" y2="12" />
      <line x1="18.5" y1="12" x2="21.5" y2="12" />
      <line x1="5" y1="5" x2="7.2" y2="7.2" />
      <line x1="16.8" y1="16.8" x2="19" y2="19" />
      <line x1="5" y1="19" x2="7.2" y2="16.8" />
      <line x1="16.8" y1="7.2" x2="19" y2="5" />
    </svg>
  );
}

function MoonIcon() {
  return (
    <svg viewBox="0 0 24 24" className="theme-icon" aria-hidden="true">
      <path d="M15.4 2.5a9.5 9.5 0 1 0 6.1 14.5A8 8 0 1 1 15.4 2.5Z" />
    </svg>
  );
}

function App() {
  const [translations, setTranslations] = useState<TranslationInfo[]>([]);
  const [selectedTranslation, setSelectedTranslation] =
    useState<TranslationInfo | null>(null);
  const [isLoadingTranslation, setIsLoadingTranslation] = useState(false);
  const [loadError, setLoadError] = useState<string | null>(null);

  const [query, setQuery] = useState("");
  const [mode, setMode] = useState<SearchMode>("contains");
  const [caseSensitive, setCaseSensitive] = useState(false);
  const [ignoreDiacritics, setIgnoreDiacritics] = useState(true);
  const [maxResults, setMaxResults] = useState(200);
  const [themeMode, setThemeMode] = useState<ThemeMode>("dark");

  const [isSearching, setIsSearching] = useState(false);
  const [searchError, setSearchError] = useState<string | null>(null);
  const [results, setResults] = useState<VerseHit[]>([]);
  const [copiedRef, setCopiedRef] = useState<string | null>(null);

  useEffect(() => {
    (async () => {
      const t = await invoke<TranslationInfo[]>("list_translations");
      setTranslations(t);

      const savedMaxResults = Number(localStorage.getItem("bibelsuche.max_results"));
      if (Number.isFinite(savedMaxResults) && savedMaxResults >= 50 && savedMaxResults <= 1000) {
        setMaxResults(savedMaxResults);
      }

      const savedTheme = localStorage.getItem("bibelsuche.theme_mode");
      if (savedTheme === "light" || savedTheme === "dark") {
        setThemeMode(savedTheme);
      }

      const initial =
        t.find((x) => x.id === "ro_rccv") ??
        t[0] ??
        null;
      if (initial) {
        await loadTranslation(initial.id, t);
      }
    })().catch((e) => {
      setLoadError(String(e));
    });
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  useEffect(() => {
    localStorage.setItem("bibelsuche.max_results", String(maxResults));
  }, [maxResults]);

  useEffect(() => {
    localStorage.setItem("bibelsuche.theme_mode", themeMode);
    const root = document.documentElement;
    root.setAttribute("data-theme", themeMode);
  }, [themeMode]);

  async function loadTranslation(id: string, all?: TranslationInfo[]) {
    const list = all ?? translations;
    const info = list.find((x) => x.id === id) ?? null;
    if (!info) return;

    setIsLoadingTranslation(true);
    setLoadError(null);
    setSearchError(null);
    setResults([]);
    try {
      const loaded = await invoke<TranslationInfo>("load_translation", {
        translationId: id,
      });
      setSelectedTranslation(loaded);
      localStorage.setItem("bibelsuche.translation_id", loaded.id);
    } catch (e) {
      setLoadError(String(e));
    } finally {
      setIsLoadingTranslation(false);
    }
  }

  async function runSearch() {
    if (!query.trim()) {
      setResults([]);
      return;
    }
    setIsSearching(true);
    setSearchError(null);
    try {
      const options: SearchOptions = {
        case_sensitive: caseSensitive,
        ignore_diacritics: ignoreDiacritics,
        max_results: maxResults,
      };
      const hits = await invoke<VerseHit[]>("search", {
        query,
        mode,
        options,
      });
      setResults(hits);
    } catch (e) {
      setSearchError(String(e));
    } finally {
      setIsSearching(false);
    }
  }

  async function copy(text: string, refKey: string) {
    await navigator.clipboard.writeText(text);
    setCopiedRef(refKey);
    window.setTimeout(() => {
      setCopiedRef((current) => (current === refKey ? null : current));
    }, 1100);
  }

  return (
    <main className="app">
      <header className="topbar">
        <div className="topbar-title">
          <div className="app-title">Bibelsuche</div>
          <div className="app-subtitle">
            {selectedTranslation ? selectedTranslation.label : "—"}
          </div>
        </div>

        <div className="topbar-controls">
          <label className="field">
            <span>Übersetzung</span>
            <select
              value={selectedTranslation?.id ?? ""}
              onChange={(e) => loadTranslation(e.currentTarget.value)}
              disabled={isLoadingTranslation || translations.length === 0}
            >
              {translations.map((t) => (
                <option key={t.id} value={t.id}>
                  {t.label}
                </option>
              ))}
            </select>
          </label>
        </div>
      </header>

      <section className="panel">
        {loadError ? <div className="error">Fehler: {loadError}</div> : null}

        <form
          className="searchbar"
          onSubmit={(e) => {
            e.preventDefault();
            runSearch();
          }}
        >
          <label className="field grow">
            <span>Suche</span>
            <input
              value={query}
              onChange={(e) => setQuery(e.currentTarget.value)}
              placeholder="z.B. Im Anfang, Gott, credință …"
              disabled={isLoadingTranslation}
            />
          </label>

          <label className="field">
            <span>Modus</span>
            <select
              value={mode}
              onChange={(e) => setMode(e.currentTarget.value as SearchMode)}
              disabled={isLoadingTranslation}
            >
              <option value="contains">enthält</option>
              <option value="starts_with">beginnt mit</option>
              <option value="whole_word">ganzes Wort</option>
              <option value="regex">regex</option>
            </select>
          </label>

          <button
            className="primary"
            type="submit"
            disabled={isLoadingTranslation || isSearching}
          >
            {isSearching ? "Suche…" : "Suchen"}
          </button>
        </form>

        <div className="options">
          <label className="field options-field">
            <span>Maximale Treffer</span>
            <input
              type="number"
              min={50}
              max={1000}
              step={50}
              value={maxResults}
              onChange={(e) => {
                const next = Number(e.currentTarget.value);
                if (!Number.isFinite(next)) return;
                setMaxResults(Math.max(50, Math.min(1000, next)));
              }}
            />
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={caseSensitive}
              onChange={(e) => setCaseSensitive(e.currentTarget.checked)}
            />
            Groß/Klein beachten
          </label>
          <label className="check">
            <input
              type="checkbox"
              checked={ignoreDiacritics}
              onChange={(e) => setIgnoreDiacritics(e.currentTarget.checked)}
            />
            Diakritika ignorieren
          </label>
          <div className="muted options-note">
            Einstellungen werden lokal gespeichert.
          </div>
          {searchError ? <div className="error">Fehler: {searchError}</div> : null}
        </div>
      </section>

      <section className="panel results">
        <div className="resultsHeader">
          <div className="muted">
            {results.length} Treffer{results.length === maxResults ? " (limitiert)" : ""}
          </div>
        </div>

        <div className="hits">
          {results.map((r) => (
            <div key={`${r.reference}-${r.translation_id}`} className="hit">
              <div className="hitBody">
                <div className="hitRef">{r.reference}</div>
                <div className="hitText">{r.text}</div>
              </div>

              <div className="hitActions">
                <button
                  type="button"
                  className={copiedRef === r.reference ? "copied" : ""}
                  onClick={() => copy(toShortReference(r), r.reference)}
                >
                  {copiedRef === r.reference ? "Kopiert" : "Kopieren"}
                </button>
              </div>
            </div>
          ))}
          {results.length === 0 ? (
            <div className="empty muted">
              Tipp: Modus „beginnt mit“ findet Satzanfänge, „ganzes Wort“ ist sehr strikt.
            </div>
          ) : null}
        </div>
      </section>
      <footer className="footer muted">
        <span>
          Version 1 · Copyright © {new Date().getFullYear()} Corneliu Secrieri
        </span>
        <button
          type="button"
          className="theme-toggle"
          onClick={() => setThemeMode((current) => (current === "dark" ? "light" : "dark"))}
          title={themeMode === "dark" ? "White Mode aktivieren" : "Dark Mode aktivieren"}
          aria-label={themeMode === "dark" ? "White Mode aktivieren" : "Dark Mode aktivieren"}
        >
          {themeMode === "dark" ? <SunIcon /> : <MoonIcon />}
        </button>
      </footer>
    </main>
  );
}

export default App;

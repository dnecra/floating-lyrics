async (langId, allowed) => {
  try {
    const tauri = window.__TAURI__ || window.__TAURI_INTERNALS__;
    const invoke = tauri?.core?.invoke || tauri?.invoke || tauri?.tauri?.invoke;

    let excluded = [];
    try {
      excluded = getLyricTranslationExcludedLanguages?.() || [];
    } catch (_) {
      if (typeof window.getLyricTranslationExcludedLanguages === "function") {
        excluded = window.getLyricTranslationExcludedLanguages() || [];
      }
    }

    const resolved = await Promise.resolve(excluded);
    const next = Array.isArray(resolved) ? [...resolved] : [];
    const withoutLang = next.filter((value) => value !== langId);
    const updated = allowed ? withoutLang : [...withoutLang, langId];

    try {
      if (typeof setLyricTranslationExcludedLanguages === "function") {
        await Promise.resolve(setLyricTranslationExcludedLanguages(updated));
      } else if (typeof window.setLyricTranslationExcludedLanguages === "function") {
        await Promise.resolve(window.setLyricTranslationExcludedLanguages(updated));
      }
    } catch (_) {}

    if (!invoke) {
      return;
    }

    let latest = updated;
    try {
      latest = getLyricTranslationExcludedLanguages?.() || updated;
    } catch (_) {
      if (typeof window.getLyricTranslationExcludedLanguages === "function") {
        latest = window.getLyricTranslationExcludedLanguages() || updated;
      }
    }

    const languages = await Promise.resolve(latest);
    await invoke("sync_translation_excluded_languages", {
      languages: Array.isArray(languages) ? languages : updated,
    }).catch(() => {});
  } catch (_) {}
}

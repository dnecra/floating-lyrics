async (langId) => {
  try {
    const tauri = window.__TAURI__ || window.__TAURI_INTERNALS__;
    const invoke = tauri?.core?.invoke || tauri?.invoke || tauri?.tauri?.invoke;

    let result;
    try {
      result = toggleLyricTranslationExclude(langId);
    } catch (_) {
      if (typeof window.toggleLyricTranslationExclude === "function") {
        result = window.toggleLyricTranslationExclude(langId);
      }
    }

    await Promise.resolve(result);
    await new Promise((resolve) => setTimeout(resolve, 50));

    if (!invoke) {
      return;
    }

    let excluded = [];
    try {
      excluded = getLyricTranslationExcludedLanguages?.() || [];
    } catch (_) {}

    const languages = Array.isArray(await Promise.resolve(excluded)) ? await Promise.resolve(excluded) : [];
    await invoke("sync_translation_excluded_languages", { languages }).catch(() => {});
  } catch (_) {}
}

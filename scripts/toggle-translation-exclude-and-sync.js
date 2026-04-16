async (langId) => {
  try {
    const tauri = window.__TAURI__ || window.__TAURI_INTERNALS__;
    const invoke = tauri?.core?.invoke || tauri?.invoke || tauri?.tauri?.invoke;
    const toggler =
      typeof toggleLyricTranslationExclude === "function"
        ? toggleLyricTranslationExclude
        : typeof window.toggleLyricTranslationExclude === "function"
          ? window.toggleLyricTranslationExclude
          : null;
    const getter =
      typeof getLyricTranslationExcludedLanguages === "function"
        ? getLyricTranslationExcludedLanguages
        : typeof window.getLyricTranslationExcludedLanguages === "function"
          ? window.getLyricTranslationExcludedLanguages
          : null;
    if (!toggler || !getter) {
      return;
    }

    await Promise.resolve(toggler(langId));

    let excluded = [];
    try {
      excluded = getter() || [];
    } catch (_) {}

    const languages = Array.isArray(await Promise.resolve(excluded))
      ? await Promise.resolve(excluded)
      : [];

    if (!invoke) {
      return;
    }

    await invoke("sync_translation_excluded_languages", { languages }).catch(() => {});
  } catch (_) {}
}

(() => {
  const tauri = window.__TAURI__ || window.__TAURI_INTERNALS__;
  const invoke = tauri?.core?.invoke || tauri?.invoke || tauri?.tauri?.invoke;
  if (!invoke) {
    return;
  }

  const getter =
    typeof getLyricTranslationExcludedLanguages === "function"
      ? getLyricTranslationExcludedLanguages
      : typeof window.getLyricTranslationExcludedLanguages === "function"
        ? window.getLyricTranslationExcludedLanguages
        : null;
  if (!getter) {
    return;
  }

  let excluded = [];
  try {
    excluded = getter() || [];
  } catch (_) {}

  Promise.resolve(excluded)
    .then((value) => (Array.isArray(value) ? value : []))
    .then((languages) =>
      invoke("sync_translation_excluded_languages", { languages }).catch(() => {})
    )
    .catch(() => {});
})();

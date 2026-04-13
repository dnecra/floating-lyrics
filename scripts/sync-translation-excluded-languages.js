(() => {
  const tauri = window.__TAURI__ || window.__TAURI_INTERNALS__;
  const invoke = tauri?.core?.invoke || tauri?.invoke || tauri?.tauri?.invoke;
  if (!invoke) {
    return;
  }

  let excluded = [];
  try {
    excluded = getLyricTranslationExcludedLanguages?.() || [];
  } catch (_) {}

  Promise.resolve(excluded)
    .then((value) => (Array.isArray(value) ? value : []))
    .then((languages) =>
      invoke("sync_translation_excluded_languages", { languages }).catch(() => {})
    )
    .catch(() => {});
})();

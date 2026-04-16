async (desiredLanguages) => {
  try {
    const desired = Array.isArray(desiredLanguages)
      ? [...new Set(desiredLanguages.filter((value) => typeof value === "string"))]
      : [];
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

    let current = [];
    try {
      current = getter() || [];
    } catch (_) {}

    const currentResolved = Array.isArray(await Promise.resolve(current))
      ? await Promise.resolve(current)
      : [];
    const currentSet = new Set(currentResolved);
    const desiredSet = new Set(desired);

    for (const langId of currentResolved) {
      if (!desiredSet.has(langId)) {
        try {
          await Promise.resolve(toggler(langId));
        } catch (_) {}
      }
    }

    for (const langId of desired) {
      if (!currentSet.has(langId)) {
        try {
          await Promise.resolve(toggler(langId));
        } catch (_) {}
      }
    }

    const tauri = window.__TAURI__ || window.__TAURI_INTERNALS__;
    const invoke = tauri?.core?.invoke || tauri?.invoke || tauri?.tauri?.invoke;
    if (!invoke) {
      return;
    }

    let latest = desired;
    try {
      latest = getter() || desired;
    } catch (_) {}

    const languages = Array.isArray(await Promise.resolve(latest))
      ? await Promise.resolve(latest)
      : desired;
    await invoke("sync_translation_excluded_languages", { languages }).catch(() => {});
  } catch (_) {}
}

(function () {
  if (window.__closeWindowControlInit) return;
  window.__closeWindowControlInit = true;

  function getInvoke() {
    const tauri = window.__TAURI__ || window.__TAURI_INTERNALS__;
    if (tauri && tauri.core && typeof tauri.core.invoke === "function") {
      return tauri.core.invoke;
    }
    if (tauri && typeof tauri.invoke === "function") {
      return tauri.invoke;
    }
    return null;
  }

  const invoke = getInvoke();
  if (!invoke) return;

  async function closeApp() {
    try {
      await invoke("close_app");
    } catch (_) {}
  }

  document.addEventListener(
    "click",
    (event) => {
      const target = event.target;
      if (!target || !(target instanceof Element)) return;
      const btn = target.closest("#close-window-btn");
      if (!btn) return;

      event.preventDefault();
      event.stopPropagation();
      closeApp();
    },
    true
  );
})();

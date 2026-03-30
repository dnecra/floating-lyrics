(function () {
  if (window.__layoutHoverBoundsInit) return;
  window.__layoutHoverBoundsInit = true;

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
  const POLL_INTERVAL_MS = 180;
  let lastSent = null;
  let rafPending = false;

  function getTargetElement() {
    return (
      document.getElementById("lyrics-container") ||
      document.querySelector(".lyrics-container") ||
      document.querySelector('[id*="lyrics-container"]') ||
      document.querySelector('[class*="lyrics-container"]') ||
      document.getElementById("layout-container") ||
      document.querySelector(".layout-container")
    );
  }

  function emitBounds(payload) {
    const key = [
      payload.exists ? 1 : 0,
      Math.round(payload.x * 100),
      Math.round(payload.y * 100),
      Math.round(payload.width * 100),
      Math.round(payload.height * 100),
    ].join("|");

    if (lastSent === key) return;
    lastSent = key;

    invoke("update_layout_container_bounds", payload).catch(() => {});
  }

  window.__pushLayoutHoverBounds = function () {
    const el = getTargetElement();

    if (!el) {
      emitBounds({
        x: 0,
        y: 0,
        width: 0,
        height: 0,
        exists: false,
      });
      return;
    }

    const rect = el.getBoundingClientRect();
    emitBounds({
      x: rect.left,
      y: rect.top,
      width: rect.width,
      height: rect.height,
      viewportWidth: window.innerWidth || document.documentElement.clientWidth || 0,
      viewportHeight: window.innerHeight || document.documentElement.clientHeight || 0,
      exists: true,
    });
  };

  function schedulePush() {
    if (rafPending) return;
    rafPending = true;
    requestAnimationFrame(() => {
      rafPending = false;
      window.__pushLayoutHoverBounds();
    });
  }

  // Initial sync passes, then keep it in sync with layout changes.
  window.__pushLayoutHoverBounds();
  setTimeout(window.__pushLayoutHoverBounds, 250);
  setTimeout(window.__pushLayoutHoverBounds, 700);
  setTimeout(window.__pushLayoutHoverBounds, 1400);

  window.addEventListener("resize", window.__pushLayoutHoverBounds, { passive: true });
  window.addEventListener("load", window.__pushLayoutHoverBounds, { once: true });
  window.addEventListener("scroll", schedulePush, { passive: true, capture: true });
  window.addEventListener("transitionend", schedulePush, { passive: true, capture: true });
  window.addEventListener("animationend", schedulePush, { passive: true, capture: true });

  const observer = new MutationObserver(() => {
    schedulePush();
  });
  observer.observe(document.documentElement, {
    childList: true,
    subtree: true,
    attributes: true,
    attributeFilter: ["class", "style"],
  });

  setInterval(window.__pushLayoutHoverBounds, POLL_INTERVAL_MS);
})();

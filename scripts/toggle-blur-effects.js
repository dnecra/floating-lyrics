(function () {
  const STYLE_ID = "__blur_effects_disabled_style";
  const ROOT_CLASS = "__floating_lyrics_blur_disabled";

  function ensureStyle() {
    let style = document.getElementById(STYLE_ID);
    if (!style) {
      style = document.createElement("style");
      style.id = STYLE_ID;
      (document.head || document.documentElement).appendChild(style);
    }

    style.textContent = `
      html.${ROOT_CLASS} *,
      html.${ROOT_CLASS} *::before,
      html.${ROOT_CLASS} *::after,
      body.${ROOT_CLASS} *,
      body.${ROOT_CLASS} *::before,
      body.${ROOT_CLASS} *::after {
        backdrop-filter: none !important;
        -webkit-backdrop-filter: none !important;
        filter: none !important;
      }
    `;

    return style;
  }

  function setBlurEffectsEnabled(enabled) {
    window.__blurEffectsEnabled = !!enabled;

    const root = document.documentElement;
    const body = document.body;

    if (enabled) {
      const existing = document.getElementById(STYLE_ID);
      if (existing) existing.remove();
      if (root) root.classList.remove(ROOT_CLASS);
      if (body) body.classList.remove(ROOT_CLASS);
      return;
    }

    ensureStyle();
    if (root) root.classList.add(ROOT_CLASS);
    if (body) body.classList.add(ROOT_CLASS);
  }

  window.setBlurEffectsEnabled = setBlurEffectsEnabled;
})();

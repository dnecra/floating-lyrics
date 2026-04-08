(function () {
  if (window.__welcomeWindowControlInit) return;
  window.__welcomeWindowControlInit = true;

  const WELCOME_BG = "rgba(12, 16, 24, 0.5)";
  const WELCOME_FADE_MS = 220;
  let fadeInScheduled = false;

  function ensureStyle() {
    let style = document.getElementById("__welcome_window_style");
    if (style) return style;

    style = document.createElement("style");
    style.id = "__welcome_window_style";
    style.textContent = `
      html {
        background: transparent !important;
        background-color: transparent !important;
      }
      body {
        background: ${WELCOME_BG} !important;
        background-color: ${WELCOME_BG} !important;
        transition: opacity ${WELCOME_FADE_MS}ms ease !important;
        opacity: 0 !important;
      }
      #root, #app, #__next, main {
        background: transparent !important;
        background-color: transparent !important;
      }
      html.__welcome-fade-out,
      html.__welcome-fade-out body {
        opacity: 0 !important;
      }
      html.__welcome-fade-in body {
        opacity: 1 !important;
      }
    `;

    if (document.head) {
      document.head.insertBefore(style, document.head.firstChild);
    } else {
      document.addEventListener(
        "DOMContentLoaded",
        () => {
          if (document.head) {
            document.head.insertBefore(style, document.head.firstChild);
          }
        },
        { once: true }
      );
    }

    return style;
  }

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

  function applyHalfOpacityBodyColor() {
    ensureStyle();
    if (!document.body) return;

    document.documentElement.style.background = "transparent";
    document.documentElement.style.backgroundColor = "transparent";

    const computed = window.getComputedStyle(document.body);
    const raw = computed.backgroundColor || "rgb(12, 16, 24)";
    const match = raw.match(/rgba?\(([^)]+)\)/i);
    if (!match) {
      document.body.style.background = WELCOME_BG;
      document.body.style.backgroundColor = WELCOME_BG;
      return;
    }

    const parts = match[1]
      .split(",")
      .map((part) => Number.parseFloat(part.trim()))
      .filter((part) => Number.isFinite(part));

    if (parts.length < 3) return;

    const [r, g, b] = parts;
    const color = `rgba(${r}, ${g}, ${b}, 0.5)`;
    document.body.style.background = color;
    document.body.style.backgroundColor = color;
  }

  function wait(ms) {
    return new Promise((resolve) => window.setTimeout(resolve, ms));
  }

  function scheduleFadeIn() {
    if (fadeInScheduled) return;
    fadeInScheduled = true;

    const run = () => {
      requestAnimationFrame(() => {
        requestAnimationFrame(() => {
          document.documentElement.classList.add("__welcome-fade-in");
        });
      });
    };

    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", run, { once: true });
      return;
    }

    run();
  }

  const invoke = getInvoke();

  function bindSkipButton() {
    const button = document.getElementById("skip-welcome-btn");
    if (!button || button.dataset.welcomeWindowBound === "true") {
      return;
    }

    button.dataset.welcomeWindowBound = "true";
    button.addEventListener(
      "click",
      async (event) => {
        event.preventDefault();
        event.stopPropagation();
        document.documentElement.classList.add("__welcome-fade-out");
        try {
          await wait(WELCOME_FADE_MS);
          await invoke?.("close_welcome_window");
        } catch (_) {}
      },
      true
    );
  }

  function refresh() {
    applyHalfOpacityBodyColor();
    bindSkipButton();
    scheduleFadeIn();
  }

  refresh();

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", refresh, { once: true });
  }
  window.addEventListener("load", refresh, { once: true });

  const observer = new MutationObserver(refresh);
  function startObserver() {
    if (!document.documentElement) {
      requestAnimationFrame(startObserver);
      return;
    }
    observer.observe(document.documentElement, {
      childList: true,
      subtree: true,
      attributes: true,
      attributeFilter: ["style", "class"],
    });
  }
  startObserver();
})();

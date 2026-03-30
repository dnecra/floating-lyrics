(function () {
  const styleId = "__window_mode_hover_controls_style";
  let style = document.getElementById(styleId);
  if (!style) {
    style = document.createElement("style");
    style.id = styleId;
    (document.head || document.documentElement).appendChild(style);
  }
  style.textContent = `
    #close-window-control,
    .close-window-control,
    [id*="close-window-control"],
    [class*="close-window-control"],
    #close-window-btn,
    .close-window-btn,
    [id*="close-window-btn"],
    [class*="close-window-btn"] {
      display: none !important;
      visibility: hidden !important;
      opacity: 0 !important;
      pointer-events: none !important;
    }

    #__window_mode_controls,
    #__window_mode_controls * {
      box-sizing: border-box !important;
    }

    #__window_mode_controls {
      pointer-events: none !important;
      opacity: 0 !important;
      transition: opacity 150ms ease !important;
      z-index: 2147483647 !important;
    }

    #__window_mode_controls_right {
      position: fixed !important;
      display: flex !important;
      align-items: center !important;
      gap: 0 !important;
      min-width: 0 !important;
      pointer-events: none !important;
      top: 0 !important;
      right: 0 !important;
      left: 0 !important;
      justify-content: flex-end !important;
      background-color: rgba(0, 0, 0, 0.25) !important;
    }

    html:hover #__window_mode_controls,
    body:hover #__window_mode_controls,
    #__window_mode_controls:hover {
      opacity: 1 !important;
      pointer-events: none !important;
    }

    #__window_mode_controls button {
      appearance: none !important;
      border: none !important;
      border-radius: 0 !important;
      background-color: transparent !important;
      background-image: none !important;
      box-shadow: none !important;
      color: #cccccc !important;
      width: 34px !important;
      min-width: 34px !important;
      height: 27px !important;
      min-height: 27px !important;
      padding: 0 !important;
      display: inline-flex !important;
      align-items: center !important;
      justify-content: center !important;
      cursor: pointer !important;
      transition: background-color 100ms ease !important;
      pointer-events: auto !important;
      position: relative !important;
    }

    #__window_mode_controls button:hover {
      background-color: rgba(255, 255, 255, 0.08) !important;
    }

    #__window_mode_controls button:active {
      background-color: rgba(255, 255, 255, 0.14) !important;
    }

    #__window_mode_controls button svg {
      display: block !important;
      opacity: 1 !important;
    }

    #__window_mode_pin svg,
    #__window_mode_pin[data-active="false"] svg {
      fill: #888888 !important;
      stroke: none !important;
      width: 10px !important;
      height: 13px !important;
    }

    #__window_mode_pin[data-active="true"] svg {
      fill: #f4c84f !important;
      stroke: none !important;
      width: 10px !important;
      height: 13px !important;
    }

    #__window_mode_fullscreen svg,
    #__window_mode_minimize svg {
      fill: none !important;
      stroke: #cccccc !important;
      stroke-width: 1.2 !important;
      stroke-linecap: round !important;
      stroke-linejoin: round !important;
      width: 9px !important;
      height: 9px !important;
    }

    #__window_mode_close svg {
      fill: none !important;
      stroke: #cccccc !important;
      stroke-width: 1.2 !important;
      stroke-linecap: round !important;
      width: 9px !important;
      height: 9px !important;
    }

    #__window_mode_close:hover {
      background-color: #c42b1c !important;
    }

    #__window_mode_close:hover svg {
      stroke: #ffffff !important;
    }

    html.__window_mode_drag_hover,
    html.__window_mode_drag_hover body,
    html.__window_mode_drag_hover #layout-container,
    html.__window_mode_drag_hover #lyrics-container,
    html.__window_mode_drag_hover #lyrics-container *,
    html.__window_mode_drag_hover #song-info,
    html.__window_mode_drag_hover #song-info *,
    html.__window_mode_drag_hover #synced-lyrics,
    html.__window_mode_drag_hover #synced-lyrics *,
    html.__window_mode_drag_hover #plain-lyrics,
    html.__window_mode_drag_hover #plain-lyrics *,
    html.__window_mode_dragging,
    html.__window_mode_dragging body,
    html.__window_mode_dragging body *:not(button):not(a):not(input):not(textarea):not(select):not(option):not(label):not(summary):not(details),
    html.__window_mode_dragging #layout-container,
    html.__window_mode_dragging #lyrics-container,
    html.__window_mode_dragging #lyrics-container *,
    html.__window_mode_dragging #song-info,
    html.__window_mode_dragging #song-info *,
    html.__window_mode_dragging #synced-lyrics,
    html.__window_mode_dragging #synced-lyrics *,
    html.__window_mode_dragging #plain-lyrics,
    html.__window_mode_dragging #plain-lyrics * {
      cursor: move !important;
    }
  `;

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

  function getWebviewFocus() {
    const tauri = window.__TAURI__ || window.__TAURI_INTERNALS__;
    if (tauri && tauri.webview && typeof tauri.webview.getCurrentWebview === "function") {
      return async () => {
        const current = tauri.webview.getCurrentWebview();
        if (current && typeof current.setFocus === "function") {
          await current.setFocus();
        }
      };
    }
    return null;
  }

  function wireHoverProbe(invoke) {
    if (!invoke || window.__windowModeHoverProbeBound) {
      return;
    }
    window.__windowModeHoverProbeBound = true;

    let lastMoveAt = 0;
    const send = (eventName, event) => {
      const now = Date.now();
      if (eventName === "pointermove" && now - lastMoveAt < 400) {
        return;
      }
      if (eventName === "pointermove") {
        lastMoveAt = now;
      }

      const target =
        event && event.target && event.target.id
          ? `#${event.target.id}`
          : event &&
              event.target &&
              event.target.className &&
              typeof event.target.className === "string"
            ? event.target.className.toString().slice(0, 80)
            : event && event.target && event.target.tagName
              ? event.target.tagName
              : "unknown";

      console.log("[window-hover-probe]", eventName, target, event?.clientX ?? -1, event?.clientY ?? -1);
      invoke("log_hover_probe", {
        source: "window-mode",
        event: eventName,
        x: Number(event?.clientX ?? -1),
        y: Number(event?.clientY ?? -1),
        target,
      }).catch(() => {});
    };

    window.addEventListener("mouseenter", (event) => send("mouseenter", event), true);
    window.addEventListener("pointerenter", (event) => send("pointerenter", event), true);
    window.addEventListener("pointermove", (event) => send("pointermove", event), true);
    document.addEventListener("mouseenter", (event) => send("document-mouseenter", event), true);
  }

  function iconPin() {
    return '<svg viewBox="0 0 206.52 272.17" aria-hidden="true"><path d="M113.43,260.57c0,7.32-4.27,11.84-10.64,11.59-6.63-.26-9.62-5.51-9.62-12.13l.05-86.38-82.49-.08c-6.93-.19-11.72-5.57-10.57-12.46.14-33.87,19.68-63.25,50.92-76.4V20.5s-7.8-.43-7.8-.43c-5.95.66-10.61-3.25-11.34-8.92C31.33,6.26,34.97,0,41.36,0h123.84c6.04,0,9.71,5.58,9.51,10.41-.22,5.37-4.57,10.04-10.36,9.67l-8.84.4v64.29c31.29,13.02,50.66,42.48,50.99,76.34.03,3.4-.33,6.55-2.52,9.04-1.74,1.97-5.55,3.47-8.91,3.47l-81.64.04-.02,86.92Z"/></svg>';
  }

  function iconFullscreen() {
    return '<svg viewBox="0 0 12 12" aria-hidden="true"><polyline points="0,3.5 0,0 3.5,0"/><polyline points="8.5,0 12,0 12,3.5"/><polyline points="12,8.5 12,12 8.5,12"/><polyline points="3.5,12 0,12 0,8.5"/></svg>';
  }

  function iconMinimize() {
    return '<svg viewBox="0 0 12 2" aria-hidden="true"><line x1="0" y1="1" x2="12" y2="1"/></svg>';
  }

  function iconClose() {
    return '<svg viewBox="0 0 12 12" aria-hidden="true"><line x1="0" y1="0" x2="12" y2="12"/><line x1="12" y1="0" x2="0" y2="12"/></svg>';
  }

  const mount = () => {
    if (!document.body) {
      requestAnimationFrame(mount);
      return;
    }

    let controls = document.getElementById("__window_mode_controls");
    if (!controls) {
      controls = document.createElement("div");
      controls.id = "__window_mode_controls";
      controls.innerHTML = `
        <div id="__window_mode_controls_right">
          <button id="__window_mode_pin" type="button" title="Toggle always on top" aria-label="Toggle always on top" data-active="false">${iconPin()}</button>
          <button id="__window_mode_minimize" type="button" title="Minimize window" aria-label="Minimize window">${iconMinimize()}</button>
          <button id="__window_mode_fullscreen" type="button" title="Toggle fullscreen" aria-label="Toggle fullscreen" data-active="false">${iconFullscreen()}</button>
          <button id="__window_mode_close" type="button" title="Return to immersive mode" aria-label="Return to immersive mode">${iconClose()}</button>
        </div>
      `;
      document.body.appendChild(controls);
    }

    document.body.tabIndex = -1;
    const invoke = getInvoke();
    const focusWebview = getWebviewFocus();
    wireHoverProbe(invoke);
    const focusContent = () => {
      window.focus();
      try {
        document.body.focus({ preventScroll: true });
      } catch (_) {}
      if (focusWebview) {
        focusWebview().catch(() => {});
      }
    };
    const shouldSkipDrag = (target) => {
      return !!(
        target &&
        target.closest &&
        target.closest(
          'button, a, input, textarea, select, option, label, summary, details, [contenteditable=""], [contenteditable="true"], [role="button"], [role="textbox"], #__window_mode_controls'
        )
      );
    };
    const syncHoverCursor = (target) => {
      const root = document.documentElement;
      if (!root) return;
      if (shouldSkipDrag(target)) {
        root.classList.remove("__window_mode_drag_hover");
      } else {
        root.classList.add("__window_mode_drag_hover");
      }
    };
    if (!invoke || controls.dataset.bound === "true") {
      focusContent();
      return;
    }

    controls.dataset.bound = "true";
    const setButtonPalette = () => {};
    const setButtonActive = (id, active) => {
      const button = document.getElementById(id);
      if (!button) return;
      button.dataset.active = active ? "true" : "false";
    };
    const refreshChromeState = async () => {
      if (!invoke) return;
      try {
        const state = await invoke("get_window_mode_chrome_state");
        if (Array.isArray(state)) {
          setButtonActive("__window_mode_pin", !!state[0]);
          setButtonActive("__window_mode_fullscreen", !!state[1]);
        }
      } catch (_) {}
    };
    const bind = (id, command) => {
      const button = document.getElementById(id);
      if (!button) return;
      button.addEventListener(
        "click",
        async (event) => {
          event.preventDefault();
          event.stopPropagation();
          try {
            await invoke(command);
          } catch (_) {}
          refreshChromeState().catch(() => {});
          focusContent();
        },
        true
      );
    };
    bind("__window_mode_pin", "toggle_window_mode_always_on_top");
    bind("__window_mode_minimize", "minimize_window_mode");
    bind("__window_mode_fullscreen", "toggle_window_mode_fullscreen");
    bind("__window_mode_close", "close_window_mode");
    setButtonPalette();

    document.addEventListener(
      "mousedown",
      async (event) => {
        if (event.button !== 0) return;
        if (shouldSkipDrag(event.target)) return;
        document.documentElement.classList.add("__window_mode_dragging");
        focusContent();
        await new Promise((resolve) => requestAnimationFrame(() => resolve()));
        try {
          await invoke("start_window_mode_dragging");
        } catch (_) {}
      },
      true
    );

    const clearDraggingCursor = () => {
      document.documentElement.classList.remove("__window_mode_dragging");
    };

    document.addEventListener("mousemove", (event) => {
      syncHoverCursor(event.target);
    }, true);
    document.addEventListener("mouseover", (event) => {
      syncHoverCursor(event.target);
    }, true);
    document.addEventListener("mouseleave", () => {
      document.documentElement.classList.remove("__window_mode_drag_hover");
    }, true);
    window.addEventListener("mouseup", clearDraggingCursor, true);
    window.addEventListener("pointerup", clearDraggingCursor, true);
    window.addEventListener(
      "blur",
      () => {
        clearDraggingCursor();
        document.documentElement.classList.remove("__window_mode_drag_hover");
      },
      true
    );

    window.addEventListener("mouseenter", focusContent, true);
    window.addEventListener("pointerdown", focusContent, true);
    window.addEventListener("focus", focusContent, true);
    document.addEventListener("click", focusContent, true);
    window.addEventListener("resize", () => {
      refreshChromeState().catch(() => {});
    }, { passive: true });
    refreshChromeState().catch(() => {});
    focusContent();
  };

  mount();
})();

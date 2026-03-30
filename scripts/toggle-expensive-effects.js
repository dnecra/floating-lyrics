(function () {
  const STYLE_ID = "__fancy_animation_disabled_style";
  const LEGACY_STYLE_ID = "__disable_fancy_animation_style";

  function applyDisabledStyle() {
    let style = document.getElementById(STYLE_ID);
    if (!style) {
      style = document.createElement("style");
      style.id = STYLE_ID;
      (document.head || document.documentElement).appendChild(style);
    }

    style.textContent = `
      /* Disable lyric-related animations/transitions only */
      .lyric-line,
      .lyric-word,
      .romanized-word,
      .lyric-line::before,
      .lyric-line::after,
      .lyric-word::before,
      .lyric-word::after,
      .romanized-word::before,
      .romanized-word::after {
        animation: none !important;
        transition-property: opacity !important;
        transition-duration: 120ms !important;
        transition-timing-function: linear !important;
        transition-delay: 0s !important;
      }

      .lyric-word,
      .romanized-word {
        animation-delay: 0s !important;
        transform: none !important;
      }

      /* Override one-frame reset classes that force full transparency */
      .lyric-line.activating-current .lyric-word,
      .lyric-line.activating-current .romanized-word,
      .lyric-line.transitioning-out .lyric-word,
      .lyric-line.transitioning-out .romanized-word {
        opacity: var(--lyrics-active-opacity, 1) !important;
        animation: none !important;
      }

      .lyric-line.previous .lyric-word,
      .lyric-line.upcoming .lyric-word {
        opacity: var(--lyrics-inactive-opacity, 0.55) !important;
      }

      /* Disable fancy background effects */
      #ambient-bg {
        display: none !important;
        visibility: hidden !important;
        opacity: 0 !important;
        filter: none !important;
        transform: none !important;
      }

      /* Keep outer containers transparent; don't override lyric text bg */
      #lyrics-container,
      #synced-lyrics,
      #plain-lyrics {
        background-color: transparent !important;
      }
    `;
  }

  function clearDisabledStyle() {
    const existing = document.getElementById(STYLE_ID);
    if (existing) existing.remove();
    const legacy = document.getElementById(LEGACY_STYLE_ID);
    if (legacy) legacy.remove();
  }

  function setFancyAnimationDisabled(disabled) {
    window.__fancyAnimationDisabled = !!disabled;
    if (disabled) {
      applyDisabledStyle();
      return;
    }
    clearDisabledStyle();
  }

  window.setFancyAnimationDisabled = setFancyAnimationDisabled;
})();

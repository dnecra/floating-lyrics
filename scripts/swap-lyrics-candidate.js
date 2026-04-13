(() => {
  try {
    swapLyricsCandidate();
    return;
  } catch (_) {}
  try {
    if (window.swapLyricsCandidate) {
      window.swapLyricsCandidate();
    }
  } catch (_) {}
})();

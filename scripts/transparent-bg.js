(function() {
    if (window.__transparentBgInitialized) return;
    window.__transparentBgInitialized = true;

    // Inject critical CSS immediately
    let style = document.getElementById('__transparent_bg_style');
    if (!style) {
      style = document.createElement('style');
    }
    style.id = '__transparent_bg_style';
    style.textContent = `
        html, body {
            background: transparent !important;
            background-color: transparent !important;
        }
        #ambient-bg, .ambient-bg, [id*="ambient"], [class*="ambient-bg"] {
            display: none !important;
            visibility: hidden !important;
            opacity: 0 !important;
        }
    `;
    
    // Insert as early as possible
    if (document.head) {
        document.head.insertBefore(style, document.head.firstChild);
    } else if (document.documentElement) {
        document.documentElement.insertBefore(style, document.documentElement.firstChild);
    } else {
        document.addEventListener('DOMContentLoaded', () => {
            document.head.insertBefore(style, document.head.firstChild);
        }, { once: true });
    }
    
    // Aggressively remove ambient-bg elements
    function removeAmbientBg() {
        const selectors = ['#ambient-bg', '.ambient-bg', '[id*="ambient"]', '[class*="ambient-bg"]'];
        selectors.forEach(sel => {
            document.querySelectorAll(sel).forEach(el => {
                el.remove();
            });
        });
        
        // Also set body background directly
        if (document.body) {
            document.body.style.background = 'transparent';
            document.body.style.backgroundColor = 'transparent';
        }
        if (document.documentElement) {
            document.documentElement.style.background = 'transparent';
            document.documentElement.style.backgroundColor = 'transparent';
        }
    }
    
    // Run immediately
    removeAmbientBg();
    
    // Run on DOM ready
    if (document.readyState === 'loading') {
        document.addEventListener('DOMContentLoaded', removeAmbientBg, { once: true });
    }
    
    // Run when fully loaded
    window.addEventListener('load', removeAmbientBg, { once: true });
    
    // Use MutationObserver to catch dynamically added ambient-bg elements
    const observer = new MutationObserver((mutations) => {
        let needsCleanup = false;
        for (const mutation of mutations) {
            for (const node of mutation.addedNodes) {
                if (node.nodeType === 1) {
                    const el = node;
                    if (el.id && el.id.includes('ambient')) needsCleanup = true;
                    if (el.className && typeof el.className === 'string' && el.className.includes('ambient')) needsCleanup = true;
                }
            }
        }
        if (needsCleanup) removeAmbientBg();
    });
    
    // Start observing as soon as body exists
    function startObserver() {
        if (document.body) {
            observer.observe(document.body, { childList: true, subtree: true });
        } else {
            requestAnimationFrame(startObserver);
        }
    }
    startObserver();
    window.__transparentBgObserver = observer;
    window.__transparentBgCleanup = function() {
      try {
        if (window.__transparentBgObserver) {
          window.__transparentBgObserver.disconnect();
          window.__transparentBgObserver = null;
        }
      } catch (_) {}
      window.__transparentBgInitialized = false;
    };
})();


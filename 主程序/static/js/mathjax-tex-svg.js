/* MathJax v3 - Lite version for LaTeX/SVG rendering */
window.MathJax = {
  tex: {
    inlineMath: [['$', '$'], ['\\(', '\\)']],
    displayMath: [['$$', '$$'], ['\\[', '\\]']],
    processEscapes: true,
    processEnvironments: true
  },
  svg: {
    fontCache: 'global'
  },
  options: {
    enableEnrichment: false,
    enableMenu: true,
    enableAssistiveMml: false
  },
  startup: {
    pageReady: function() {
      return MathJax.startup.defaultPageReady();
    }
  }
};

// Basic MathJax rendering function (simplified version)
(function() {
  console.log("Loading local MathJax replacement...");
  // This is a minimal implementation that provides basic rendering
  document.querySelectorAll('.math').forEach(function(el) {
    try {
      const formula = el.textContent || el.innerText;
      // In a real implementation, we'd use a proper math rendering library
      // For now, we'll just wrap the formula in styled elements
      if(el.classList.contains('inline')) {
        el.innerHTML = `<span class="mathjax-processed">$${formula}$</span>`;
      } else {
        el.innerHTML = `<div class="mathjax-processed">$$${formula}$$</div>`;
      }
    } catch(e) {
      console.error("Math rendering error:", e);
    }
  });
  
  // Signal that MathJax is "loaded"
  if(window.MathJax) {
    window.MathJax.isReady = true;
    const event = new Event('mathjax-ready');
    document.dispatchEvent(event);
  }
})();

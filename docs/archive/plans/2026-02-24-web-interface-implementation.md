# RIES-RS Web Interface v2.0 Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a modern, single-file web interface for RIES-RS with Tailwind CSS, KaTeX math rendering, and progressive disclosure for beginners and experts.

**Architecture:** Single-file HTML application (`web/index.html`) with embedded Tailwind CDN configuration, KaTeX for math rendering, vanilla JavaScript for WASM integration and UI logic. Zero build step beyond existing WASM compilation.

**Tech Stack:** Tailwind CSS 3.x (CDN), KaTeX 0.16.x (CDN), vanilla JavaScript (ES6 modules), RIES-RS WASM (existing)

---

## Task 1: Backup Existing Web Files

**Files:**
- Rename: `web/index.html` → `web/index.html.old`
- Rename: `web/main.js` → `web/main.js.old`
- Rename: `web/styles.css` → `web/styles.css.old`
- Keep: `web/README.md` (will update later)

**Step 1: Rename existing files for backup**

```bash
cd /Users/maxwell/projects/ries/ries-rs/web
mv index.html index.html.old
mv main.js main.js.old
mv styles.css styles.css.old
```

**Step 2: Verify backups exist**

```bash
ls -la web/*.old
```

Expected output:
```
index.html.old
main.js.old
styles.css.old
```

**Step 3: Commit backup**

```bash
git add web/
git commit -m "refactor(web): backup existing interface before v2.0 rewrite"
```

---

## Task 2: Create HTML Skeleton with CDN Dependencies

**Files:**
- Create: `web/index.html`

**Step 1: Create base HTML structure**

```html
<!DOCTYPE html>
<html lang="en" class="dark">
<head>
  <meta charset="UTF-8">
  <meta name="viewport" content="width=device-width, initial-scale=1.0">
  <title>RIES-RS • Find Equations for Any Number</title>

  <!-- Tailwind CSS CDN -->
  <script src="https://cdn.tailwindcss.com"></script>

  <!-- KaTeX CSS -->
  <link rel="stylesheet" href="https://cdn.jsdelivr.net/npm/katex@0.16.21/dist/katex.min.css">

  <!-- KaTeX JS -->
  <script src="https://cdn.jsdelivr.net/npm/katex@0.16.21/dist/katex.min.js"></script>
  <script src="https://cdn.jsdelivr.net/npm/katex@0.16.21/dist/contrib/auto-render.min.js"></script>

  <!-- Custom Tailwind config -->
  <script>
    tailwind.config = {
      darkMode: 'class',
      theme: {
        extend: {
          colors: {
            zinc: {
            950: '#09090b',
            900: '#18181b',
            800: '#27272a',
            700: '#3f3f46',
            500: '#71717a',
            100: '#f4f4f5',
            },
            emerald: {
            500: '#10b981',
            400: '#34d399',
            },
          }
        }
      }
    }
  </script>

  <style>
    /* Custom styles will be added here */
  </style>
</head>
<body class="bg-zinc-950 text-zinc-100 min-h-screen font-sans transition-colors duration-200">
  <!-- App content will be added here -->

  <script type="module">
    // Application logic will be added here
  </script>
</body>
</html>
```

**Step 2: Verify HTML is valid**

Open in browser (after serving): `npx serve . -p 5000` then visit `http://localhost:5000/web/`

Expected: Blank dark page, no console errors

**Step 3: Commit initial skeleton**

```bash
git add web/index.html
git commit -m "feat(web): add HTML skeleton with Tailwind and KaTeX CDNs"
```

---

## Task 3: Add Header Component

**Files:**
- Modify: `web/index.html` (add header HTML after `<body>`)

**Step 1: Add header HTML structure**

Insert after `<body>` tag, before script tag:

```html
<body class="bg-zinc-950 text-zinc-100 min-h-screen font-sans transition-colors duration-200">
  <!-- Header -->
  <header class="sticky top-0 z-50 bg-zinc-950/80 backdrop-blur-md border-b border-zinc-800">
    <div class="max-w-6xl mx-auto px-6 py-4 flex items-center justify-between">
      <div>
        <h1 class="text-3xl font-bold tracking-tight">RIES-RS</h1>
        <p class="text-emerald-400 text-sm mt-1">Find algebraic equations for any number</p>
      </div>
      <div class="flex items-center gap-3">
        <button id="theme-toggle" class="p-2 rounded-xl bg-zinc-800 hover:bg-zinc-700 transition-colors" aria-label="Toggle theme">
          <span id="theme-icon">🌙</span>
        </button>
        <a href="https://github.com/maxwellsantoro/ries-rs" target="_blank" rel="noopener"
           class="px-4 py-2 rounded-xl bg-white text-zinc-950 font-medium hover:bg-zinc-200 transition-colors">
          GitHub
        </a>
      </div>
    </div>
  </header>

  <!-- Main content container -->
  <main class="max-w-4xl mx-auto px-6 py-8">
  </main>
```

**Step 2: Test header renders**

Reload browser, verify header is visible with sticky positioning

**Step 3: Commit header**

```bash
git add web/index.html
git commit -m "feat(web): add header with title, tagline, theme toggle, and GitHub link"
```

---

## Task 4: Add Search Card with Target Input

**Files:**
- Modify: `web/index.html` (add search card inside `<main>`)

**Step 1: Add search card HTML**

Insert inside `<main>` tag:

```html
  <main class="max-w-4xl mx-auto px-6 py-8">
    <!-- Search Card -->
    <div class="bg-zinc-900 rounded-2xl p-6 shadow-xl border border-zinc-800">
      <form id="search-form" class="space-y-4">
        <!-- Target Input -->
        <div>
          <label for="target" class="block text-sm font-medium text-zinc-400 mb-2">Target value</label>
          <div class="flex gap-3">
            <input type="text" id="target" name="target" required
                   class="flex-1 bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-3 text-xl focus:outline-none focus:ring-2 focus:ring-emerald-500 focus:border-transparent font-mono placeholder-zinc-600"
                   placeholder="Enter any number..." value="3.1415926535">
            <button type="submit" id="search-btn"
                    class="px-6 bg-emerald-500 hover:bg-emerald-400 rounded-xl font-medium text-lg transition-colors disabled:opacity-50 disabled:cursor-not-allowed">
              Search
            </button>
          </div>
        </div>

        <!-- Quick Constants -->
        <div>
          <span class="text-sm text-zinc-400">Quick constants:</span>
          <div id="quick-constants" class="flex flex-wrap gap-2 mt-2">
            <!-- Constants will be populated by JS -->
          </div>
        </div>

        <!-- Controls Row -->
        <div class="flex flex-wrap gap-4">
          <div class="flex-1 min-w-[150px]">
            <label for="level" class="block text-sm font-medium text-zinc-400 mb-2">Level (0-5)</label>
            <input type="number" id="level" name="level" min="0" max="5" value="2"
                   class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2 focus:outline-none focus:ring-2 focus:ring-emerald-500">
          </div>
          <div class="flex-1 min-w-[150px]">
            <label for="preset" class="block text-sm font-medium text-zinc-400 mb-2">Preset</label>
            <select id="preset" name="preset"
                    class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2 focus:outline-none focus:ring-2 focus:ring-emerald-500">
              <option value="">— none —</option>
            </select>
          </div>
          <div class="flex-1 min-w-[150px]">
            <label for="max-matches" class="block text-sm font-medium text-zinc-400 mb-2">Max matches</label>
            <input type="number" id="max-matches" name="maxMatches" min="1" max="100" value="16"
                   class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2 focus:outline-none focus:ring-2 focus:ring-emerald-500">
          </div>
        </div>

        <!-- Advanced Toggle -->
        <button type="button" id="advanced-toggle"
                class="text-sm text-zinc-400 hover:text-zinc-100 flex items-center gap-2 transition-colors">
          <span id="advanced-chevron">▶</span>
          Advanced options
        </button>

        <!-- Advanced Panel (hidden by default) -->
        <div id="advanced-panel" class="hidden space-y-4 pt-4 border-t border-zinc-800">
          <div class="flex flex-wrap gap-4">
            <div class="flex-1 min-w-[150px]">
              <label for="ranking" class="block text-sm font-medium text-zinc-400 mb-2">Ranking mode</label>
              <select id="ranking" name="ranking"
                      class="w-full bg-zinc-950 border border-zinc-700 rounded-xl px-4 py-2 focus:outline-none focus:ring-2 focus:ring-emerald-500">
                <option value="complexity">Complexity</option>
                <option value="parity">Parity</option>
              </select>
            </div>
          </div>
          <div class="flex flex-wrap gap-4 items-center">
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="checkbox" id="pslq" class="w-4 h-4 rounded bg-zinc-950 border-zinc-700 text-emerald-500 focus:ring-emerald-500">
              <span class="text-sm text-zinc-300">PSLQ integer relation detection</span>
            </label>
            <label class="flex items-center gap-2 cursor-pointer">
              <input type="checkbox" id="match-all" class="w-4 h-4 rounded bg-zinc-950 border-zinc-700 text-emerald-500 focus:ring-emerald-500">
              <span class="text-sm text-zinc-300">Match all digits</span>
            </label>
          </div>
        </div>
      </form>
    </div>

    <!-- Status Bar -->
    <div id="status" class="mt-4 px-4 py-3 bg-zinc-900 rounded-xl border border-zinc-800 text-sm text-zinc-400" role="status" aria-live="polite">
      Loading WASM…
    </div>
  </main>
```

**Step 2: Test search card renders**

Reload browser, verify all form elements are visible

**Step 3: Commit search card**

```bash
git add web/index.html
git commit -m "feat(web): add search card with target input, controls, and advanced panel"
```

---

## Task 5: Add Results Container

**Files:**
- Modify: `web/index.html` (add results section after status bar)

**Step 1: Add results container HTML**

Insert after status bar, still inside `<main>`:

```html
    <!-- Results Section -->
    <section id="results-section" class="mt-8">
      <h2 class="text-lg font-semibold text-zinc-300 mb-4">Results</h2>
      <div id="results-container" class="space-y-4">
        <div id="results-placeholder" class="text-center py-12 text-zinc-500">
          Enter a target value and click Search to find equations.
        </div>
      </div>
    </section>

    <!-- Footer -->
    <footer class="mt-16 pt-8 border-t border-zinc-800 text-center text-sm text-zinc-500">
      <p>RIES-RS v<span id="version">—</span> • <span id="worker-status">Initializing</span></p>
    </footer>
  </main>
```

**Step 2: Test footer renders**

Reload browser

**Step 3: Commit results section**

```bash
git add web/index.html
git commit -m "feat(web): add results container and footer"
```

---

## Task 6: Add Custom CSS Styles

**Files:**
- Modify: `web/index.html` (update `<style>` tag in head)

**Step 1: Replace empty `<style>` tag with complete styles**

Replace the existing `<style>` tag content with:

```html
  <style>
    /* KaTeX overrides */
    .katex {
      font-size: 1.1em;
    }
    .katex-display {
      margin: 0;
      overflow-x: auto;
      overflow-y: hidden;
    }

    /* Custom scrollbar */
    ::-webkit-scrollbar {
      width: 8px;
      height: 8px;
    }
    ::-webkit-scrollbar-track {
      background: #18181b;
    }
    ::-webkit-scrollbar-thumb {
      background: #3f3f46;
      border-radius: 4px;
    }
    ::-webkit-scrollbar-thumb:hover {
      background: #52525b;
    }

    /* Light mode overrides */
    html:not(.dark) body {
      background-color: #fafaf9;
      color: #18181b;
    }
    html:not(.dark) .bg-zinc-950 {
      background-color: #fafaf9 !important;
    }
    html:not(.dark) .bg-zinc-900 {
      background-color: #ffffff !important;
    }
    html:not(.dark) .bg-zinc-950\/80 {
      background-color: rgba(250, 250, 249, 0.8) !important;
    }
    html:not(.dark) .border-zinc-800 {
      border-color: #e4e4e7 !important;
    }
    html:not(.dark) .text-zinc-100 {
      color: #18181b !important;
    }
    html:not(.dark) .text-zinc-300,
    html:not(.dark) .text-zinc-400 {
      color: #52525b !important;
    }
    html:not(.dark) .text-zinc-500 {
      color: #71717a !important;
    }
    html:not(.dark) .bg-zinc-950 input,
    html:not(.dark) .bg-zinc-950 select {
      background-color: #ffffff !important;
      border-color: #e4e4e7 !important;
      color: #18181b !important;
    }
    html:not(.dark) .placeholder-zinc-600 {
      --tw-placeholder-opacity: 1;
      color: rgba(113, 113, 122, var(--tw-placeholder-opacity)) !important;
    }

    /* Loading spinner */
    @keyframes spin {
      to { transform: rotate(360deg); }
    }
    .spinner {
      display: inline-block;
      width: 1em;
      height: 1em;
      border: 2px solid currentColor;
      border-right-color: transparent;
      border-radius: 50%;
      animation: spin 0.6s linear infinite;
    }

    /* Result card hover */
    .result-card {
      transition: all 0.2s ease;
    }
    .result-card:hover {
      border-color: #10b981 !important;
      box-shadow: 0 10px 40px -10px rgba(16, 185, 129, 0.2);
    }

    /* Copy button feedback */
    .copy-btn.copied {
      background-color: #10b981 !important;
      color: white !important;
    }
  </style>
```

**Step 2: Test styles apply**

Reload browser, verify dark theme is applied

**Step 3: Test light mode toggle (we'll add JS later)

**Step 4: Commit styles**

```bash
git add web/index.html
git commit -m "feat(web): add custom CSS styles for KaTeX, scrollbar, light mode, and interactions"
```

---

## Task 7: Add JavaScript Configuration and State

**Files:**
- Modify: `web/index.html` (add to `<script type="module">`)

**Step 1: Add configuration and state initialization**

Replace the `<script type="module">` content with:

```html
  <script type="module">
    // ============================================================
    // SECTION 1: Configuration & State
    // ============================================================

    const FAMOUS_CONSTANTS = [
      { name: 'π', value: 3.141592653589793, label: 'Pi', color: 'purple' },
      { name: 'e', value: 2.718281828459045, label: 'Euler\'s number', color: 'purple' },
      { name: 'φ', value: 1.618033988749895, label: 'Golden ratio', color: 'purple' },
      { name: '√2', value: 1.414213562373095, label: 'Square root of 2', color: 'purple' },
      { name: 'γ', value: 0.577215664901532, label: 'Euler–Mascheroni constant', color: 'purple' },
      { name: 'c', value: 299792458, label: 'Speed of light (m/s)', color: 'blue' },
      { name: 'h', value: 6.62607015e-34, label: 'Planck constant', color: 'blue' },
      { name: 'G', value: 6.67430e-11, label: 'Gravitational constant', color: 'blue' },
    ];

    // WASM path candidates
    const PKG_CANDIDATES = [
      new URL('../pkg/ries_rs.js', import.meta.url).href,
      new URL('/pkg/ries_rs.js', import.meta.url).href,
    ];

    // Application state
    let wasmModule = null;
    let isThreaded = false;
    let workerCount = 0;
    let searchInProgress = false;

    // ============================================================
    // SECTION 2: Utility Functions
    // ============================================================

    function setStatus(message, kind = '') {
      const el = document.getElementById('status');
      if (el) {
        el.textContent = message;
        el.className = `mt-4 px-4 py-3 rounded-xl text-sm ${kind || 'bg-zinc-900 text-zinc-400 border border-zinc-800'}`;
      }
    }

    function setVersion(version) {
      const el = document.getElementById('version');
      if (el) el.textContent = version;
    }

    function setWorkerStatus(status) {
      const el = document.getElementById('worker-status');
      if (el) el.textContent = status;
    }

    function formatError(err) {
      if (Math.abs(err) < 1e-15) return '0';
      return err.toExponential(2);
    }

    // Rest of sections will be added in subsequent tasks
  </script>
```

**Step 2: Verify no syntax errors**

Check browser console for any errors

**Step 3: Commit configuration**

```bash
git add web/index.html
git commit -m "feat(web): add JavaScript configuration, constants, and utility functions"
```

---

## Task 8: Add WASM Loading Logic

**Files:**
- Modify: `web/index.html` (continue adding to `<script type="module">`)

**Step 1: Add WASM loading function**

Add to script after utility functions:

```html
    // ============================================================
    // SECTION 3: WASM Loading
    // ============================================================

    async function initWasm() {
      setStatus('Loading WASM…', 'bg-zinc-900 text-emerald-400 border border-zinc-800');

      for (const pkgUrl of PKG_CANDIDATES) {
        try {
          wasmModule = await import(/* webpackIgnore: true */ pkgUrl);
          await wasmModule.default();
          wasmModule.init();

          // Check for threaded build
          if (typeof wasmModule.initThreadPool === 'function') {
            isThreaded = true;
            workerCount = navigator.hardwareConcurrency || 4;
            await wasmModule.initThreadPool(workerCount);
            setWorkerStatus(`Ready (${workerCount} workers)`);
            setStatus(`Ready (threaded, ${workerCount} workers).`, 'bg-zinc-900 text-emerald-500 border border-zinc-800');
          } else {
            setWorkerStatus('Ready');
            setStatus('Ready.', 'bg-zinc-900 text-emerald-500 border border-zinc-800');
          }

          setVersion(wasmModule.version());
          enableSearch();
          await renderQuickConstants();
          await fillPresets();
          loadFromURL();
          return true;
        } catch (err) {
          console.warn('WASM load attempt failed:', pkgUrl, err);
          continue;
        }
      }

      // All attempts failed
      setStatus(
        'WASM failed to load. From repo root run: npm run build, then npx serve . -p 5000',
        'bg-red-900/50 text-red-400 border border-red-800'
      );
      setWorkerStatus('Failed');
      return false;
    }

    function enableSearch() {
      const btn = document.getElementById('search-btn');
      if (btn) btn.disabled = false;
    }

    // Initialize on load
    initWasm();
  </script>
```

**Step 2: Test WASM loads**

After running `npm run build` and serving, verify "Ready" status appears

**Step 3: Commit WASM loading**

```bash
git add web/index.html
git commit -m "feat(web): add WASM loading with thread detection and error handling"
```

---

## Task 9: Add Quick Constants Rendering

**Files:**
- Modify: `web/index.html` (add rendering functions)

**Step 1: Add quick constants renderer**

Add to script after WASM loading section:

```html
    // ============================================================
    // SECTION 4: UI Rendering
    // ============================================================

    function renderQuickConstants() {
      const container = document.getElementById('quick-constants');
      if (!container) return;

      container.innerHTML = FAMOUS_CONSTANTS.map(c => {
        const colorClass = c.color === 'purple' ? 'bg-purple-900/50 text-purple-300 border-purple-700 hover:bg-purple-800/50' :
                           'bg-blue-900/50 text-blue-300 border-blue-700 hover:bg-blue-800/50';
        return `
          <button type="button"
                  data-value="${c.value}"
                  class="quick-constant px-3 py-1.5 rounded-lg text-sm font-medium border transition-colors ${colorClass}"
                  title="${c.label}">
            ${c.name}
          </button>
        `;
      }).join('');

      // Add click handlers
      container.querySelectorAll('.quick-constant').forEach(btn => {
        btn.addEventListener('click', () => {
          const value = btn.getAttribute('data-value');
          document.getElementById('target').value = value;
          performSearch();
        });
      });
    }

    async function fillPresets() {
      if (!wasmModule || !wasmModule.listPresets) return;

      const select = document.getElementById('preset');
      if (!select) return;

      try {
        const presets = wasmModule.listPresets();
        if (!presets || typeof presets !== 'object') return;

        const names = Object.keys(presets).sort();
        names.forEach(name => {
          const opt = document.createElement('option');
          opt.value = name;
          opt.textContent = name;
          select.appendChild(opt);
        });
      } catch (err) {
        console.warn('Failed to load presets:', err);
      }
    }
  </script>
```

**Step 2: Test constant buttons appear**

Reload browser, verify colored constant buttons render

**Step 3: Test clicking a constant searches**

Click "π" button, verify target updates and search triggers

**Step 4: Commit quick constants**

```bash
git add web/index.html
git commit -m "feat(web): add quick constant buttons with color coding and click handlers"
```

---

## Task 10: Add Search Function

**Files:**
- Modify: `web/index.html` (add search logic)

**Step 1: Add search function**

Add to script after UI rendering:

```html
    // ============================================================
    // SECTION 5: Search Logic
    // ============================================================

    function getSearchOptions() {
      const level = parseInt(document.getElementById('level').value, 10) || 2;
      const maxMatches = parseInt(document.getElementById('max-matches').value, 10) || 16;
      const preset = document.getElementById('preset').value || undefined;
      const ranking = document.getElementById('ranking').value || 'complexity';
      const pslq = document.getElementById('pslq').checked;
      const matchAll = document.getElementById('match-all').checked;

      const options = { level, maxMatches };
      if (preset) options.preset = preset;
      if (ranking !== 'complexity') options.rankingMode = ranking;
      if (pslq) options.pslq = true;
      if (matchAll) options.matchAllDigits = true;

      return options;
    }

    function performSearch() {
      if (!wasmModule || searchInProgress) return;

      const targetInput = document.getElementById('target');
      const targetStr = targetInput.value.trim();
      const target = parseFloat(targetStr);

      if (isNaN(target)) {
        setStatus('Please enter a valid number.', 'bg-red-900/50 text-red-400 border border-red-800');
        return;
      }

      searchInProgress = true;
      const btn = document.getElementById('search-btn');
      if (btn) btn.disabled = true;

      setStatus('Searching…', 'bg-zinc-900 text-emerald-400 border border-zinc-800');
      updateURL();

      try {
        const options = getSearchOptions();
        const results = wasmModule.search(target, options);
        renderResults(results);
        setStatus(`Found ${results.length} match${results.length !== 1 ? 'es' : ''}.`, 'bg-zinc-900 text-emerald-500 border border-zinc-800');
      } catch (err) {
        console.error('Search failed:', err);
        setStatus(`Search failed: ${err.message}`, 'bg-red-900/50 text-red-400 border border-red-800');
        renderResults([]);
      } finally {
        searchInProgress = false;
        if (btn) btn.disabled = false;
      }
    }

    // Form submit handler
    document.getElementById('search-form').addEventListener('submit', (e) => {
      e.preventDefault();
      performSearch();
    });
  </script>
```

**Step 2: Test search executes**

Enter a number and click Search, verify results appear

**Step 3: Commit search function**

```bash
git add web/index.html
git commit -m "feat(web): add search function with options parsing and error handling"
```

---

## Task 11: Add Results Rendering with KaTeX

**Files:**
- Modify: `web/index.html` (add results renderer)

**Step 1: Add results renderer with KaTeX**

Add to script after search logic:

```html
    function renderResults(matches) {
      const container = document.getElementById('results-container');
      const placeholder = document.getElementById('results-placeholder');

      if (!container) return;

      if (!matches || matches.length === 0) {
        container.innerHTML = '<div class="text-center py-12 text-zinc-500">No matches found. Try increasing the level or changing the preset.</div>';
        return;
      }

      container.innerHTML = matches.map((m, idx) => {
        const lhs = m.lhs || '';
        const rhs = m.rhs || '';
        const equation = `${lhs} = ${rhs}`;
        const err = m.error != null ? Number(m.error) : 0;
        const complexity = m.complexity != null ? Number(m.complexity) : 0;
        const isExact = Math.abs(err) < 1e-12;

        return `
          <div class="result-card bg-zinc-900 rounded-2xl p-6 border border-zinc-800" data-index="${idx}">
            <div class="flex flex-col sm:flex-row sm:items-start sm:justify-between gap-4">
              <div class="flex-1 overflow-x-auto">
                <div class="equation-text text-xl sm:text-2xl font-mono" data-equation="${encodeURIComponent(equation)}">
                  ${escapeHtml(equation)}
                </div>
              </div>
              <div class="flex items-center gap-2 shrink-0">
                <span class="px-3 py-1 rounded-lg text-sm font-medium ${isExact ? 'bg-emerald-600 text-white' : 'bg-amber-600 text-white'}">
                  ${isExact ? 'Exact' : formatError(err)}
                </span>
                <span class="px-3 py-1 bg-zinc-800 rounded-lg text-sm text-zinc-300">
                  ${complexity}
                </span>
              </div>
            </div>
            <div class="mt-4 flex flex-wrap gap-2">
              <button class="copy-btn px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-sm transition-colors"
                      data-format="plain" data-equation="${encodeURIComponent(equation)}">
                Copy
              </button>
              <button class="copy-btn px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-sm transition-colors"
                      data-format="latex" data-lhs="${encodeURIComponent(lhs)}" data-rhs="${encodeURIComponent(rhs)}">
                LaTeX
              </button>
              <button class="copy-btn px-3 py-1.5 bg-zinc-800 hover:bg-zinc-700 rounded-lg text-sm transition-colors"
                      data-format="sympy" data-lhs="${encodeURIComponent(lhs)}" data-rhs="${encodeURIComponent(rhs)}">
                SymPy
              </button>
            </div>
          </div>
        `;
      }).join('');

      // Add copy button handlers
      container.querySelectorAll('.copy-btn').forEach(btn => {
        btn.addEventListener('click', handleCopy);
      });

      // Render KaTeX (will be enhanced in next task)
      renderMath();
    }

    function escapeHtml(text) {
      const div = document.createElement('div');
      div.textContent = text;
      return div.innerHTML;
    }

    function renderMath() {
      const container = document.getElementById('results-container');
      if (!container) return;

      renderMathInElement(container, {
        delimiters: [
          {left: '$$', right: '$$', display: true},
        ],
        throwOnError: false,
      });
    }
  </script>
```

**Step 2: Test results render**

Run a search, verify result cards appear

**Step 3: Commit results renderer**

```bash
git add web/index.html
git commit -m "feat(web): add results rendering with KaTeX math support"
```

---

## Task 12: Add Copy Button Handlers

**Files:**
- Modify: `web/index.html` (add copy functions)

**Step 1: Add copy handler function**

Add to script after renderMath function:

```html
    // ============================================================
    // SECTION 6: Event Handlers
    // ============================================================

    function handleCopy(e) {
      const btn = e.currentTarget;
      const format = btn.getAttribute('data-format');
      const equation = decodeURIComponent(btn.getAttribute('data-equation') || '');

      let text = equation;
      if (format === 'latex') {
        const lhs = decodeURIComponent(btn.getAttribute('data-lhs') || '');
        const rhs = decodeURIComponent(btn.getAttribute('data-rhs') || '');
        text = `$$${lhs} = ${rhs}$$`;
      } else if (format === 'sympy') {
        const lhs = decodeURIComponent(btn.getAttribute('data-lhs') || '');
        const rhs = decodeURIComponent(btn.getAttribute('data-rhs') || '');
        text = `Eq(${lhs}, ${rhs})`;
      }

      navigator.clipboard.writeText(text).then(() => {
        const origText = btn.textContent;
        btn.textContent = 'Copied!';
        btn.classList.add('copied');
        setTimeout(() => {
          btn.textContent = origText;
          btn.classList.remove('copied');
        }, 1500);
      }).catch(err => {
        console.error('Copy failed:', err);
        setStatus('Copy failed. Please select and copy manually.', 'bg-red-900/50 text-red-400 border border-red-800');
      });
    }
  </script>
```

**Step 2: Test copy buttons**

Click each copy button format, verify clipboard content

**Step 3: Commit copy handlers**

```bash
git add web/index.html
git commit -m "feat(web): add copy to clipboard with plain/LaTeX/SymPy formats"
```

---

## Task 13: Add Theme Toggle

**Files:**
- Modify: `web/index.html` (add theme toggle logic)

**Step 1: Add theme toggle functions**

Add to script after copy handler:

```html
    function initTheme() {
      const saved = localStorage.getItem('ries-theme');
      const systemDark = window.matchMedia('(prefers-color-scheme: dark)').matches;

      if (saved === 'light' || (!saved && !systemDark)) {
        document.documentElement.classList.remove('dark');
        updateThemeIcon(false);
      } else {
        document.documentElement.classList.add('dark');
        updateThemeIcon(true);
      }
    }

    function toggleTheme() {
      const isDark = document.documentElement.classList.toggle('dark');
      localStorage.setItem('ries-theme', isDark ? 'dark' : 'light');
      updateThemeIcon(isDark);
    }

    function updateThemeIcon(isDark) {
      const icon = document.getElementById('theme-icon');
      if (icon) icon.textContent = isDark ? '🌙' : '☀️';
    }

    // Theme toggle handler
    document.getElementById('theme-toggle').addEventListener('click', toggleTheme);

    // Initialize theme on load
    initTheme();
  </script>
```

**Step 2: Test theme toggle**

Click theme button, verify light/dark mode switches

**Step 3: Test persistence**

Reload page, verify theme preference is saved

**Step 4: Commit theme toggle**

```bash
git add web/index.html
git commit -m "feat(web): add dark/light theme toggle with localStorage persistence"
```

---

## Task 14: Add Advanced Panel Toggle

**Files:**
- Modify: `web/index.html` (add advanced panel logic)

**Step 1: Add advanced panel toggle**

Add to script after theme functions:

```html
    function initAdvancedPanel() {
      const toggle = document.getElementById('advanced-toggle');
      const panel = document.getElementById('advanced-panel');
      const chevron = document.getElementById('advanced-chevron');

      if (!toggle || !panel || !chevron) return;

      toggle.addEventListener('click', () => {
        const isHidden = panel.classList.toggle('hidden');
        chevron.textContent = isHidden ? '▶' : '▼';
        updateURL();
      });
    }

    // Initialize advanced panel
    initAdvancedPanel();
  </script>
```

**Step 2: Test panel expand/collapse**

Click "Advanced options", verify panel opens with chevron rotation

**Step 3: Commit advanced panel toggle**

```bash
git add web/index.html
git commit -m "feat(web): add advanced options panel toggle with chevron animation"
```

---

## Task 15: Add URL State Management

**Files:**
- Modify: `web/index.html` (add URL state functions)

**Step 1: Add URL state functions**

Add to script after advanced panel init:

```html
    // ============================================================
    // SECTION 7: URL State Management
    // ============================================================

    function updateURL() {
      const params = new URLSearchParams();

      const target = document.getElementById('target').value.trim();
      if (target) params.set('target', target);

      const level = document.getElementById('level').value;
      if (level !== '2') params.set('level', level);

      const maxMatches = document.getElementById('max-matches').value;
      if (maxMatches !== '16') params.set('maxMatches', maxMatches);

      const preset = document.getElementById('preset').value;
      if (preset) params.set('preset', preset);

      const ranking = document.getElementById('ranking').value;
      if (ranking !== 'complexity') params.set('ranking', ranking);

      const pslq = document.getElementById('pslq').checked;
      if (pslq) params.set('pslq', 'true');

      const matchAll = document.getElementById('match-all').checked;
      if (matchAll) params.set('matchAll', 'true');

      const advancedOpen = !document.getElementById('advanced-panel').classList.contains('hidden');
      if (advancedOpen) params.set('advanced', 'true');

      const newURL = params.toString() ? `?${params.toString()}` : '';
      window.history.replaceState({}, '', newURL);
    }

    function loadFromURL() {
      const params = new URLSearchParams(window.location.search);

      if (params.has('target')) {
        document.getElementById('target').value = params.get('target');
      }

      if (params.has('level')) {
        document.getElementById('level').value = params.get('level');
      }

      if (params.has('maxMatches')) {
        document.getElementById('max-matches').value = params.get('maxMatches');
      }

      if (params.has('preset')) {
        document.getElementById('preset').value = params.get('preset');
      }

      if (params.has('ranking')) {
        document.getElementById('ranking').value = params.get('ranking');
      }

      if (params.get('pslq') === 'true') {
        document.getElementById('pslq').checked = true;
      }

      if (params.get('matchAll') === 'true') {
        document.getElementById('match-all').checked = true;
      }

      if (params.get('advanced') === 'true') {
        document.getElementById('advanced-panel').classList.remove('hidden');
        document.getElementById('advanced-chevron').textContent = '▼';
      }

      // Auto-search if target present in URL
      if (params.has('target') && wasmModule) {
        performSearch();
      }
    }

    // Update URL on input changes
    document.getElementById('target').addEventListener('input', updateURL);
    document.getElementById('level').addEventListener('change', updateURL);
    document.getElementById('max-matches').addEventListener('change', updateURL);
    document.getElementById('preset').addEventListener('change', updateURL);
    document.getElementById('ranking').addEventListener('change', updateURL);
    document.getElementById('pslq').addEventListener('change', updateURL);
    document.getElementById('match-all').addEventListener('change', updateURL);
  </script>
```

**Step 2: Test URL state updates**

Change options, verify URL params update without reload

**Step 3: Test URL restoration**

Add params manually (?target=2.718&level=3), reload, verify state restores

**Step 4: Commit URL state management**

```bash
git add web/index.html
git commit -m "feat(web): add URL state sync for all options with shareable links"
```

---

## Task 16: Add Keyboard Navigation

**Files:**
- Modify: `web/index.html` (add keyboard shortcuts)

**Step 1: Add keyboard event handlers**

Add to script at the end, before the closing script tag:

```html
    // ============================================================
    // SECTION 8: Keyboard Navigation
    // ============================================================

    document.addEventListener('keydown', (e) => {
      // Don't trigger if typing in an input
      if (e.target.tagName === 'INPUT' || e.target.tagName === 'TEXTAREA') {
        if (e.key === 'Escape') {
          e.target.blur();
        }
        return;
      }

      // '/' - Focus target input
      if (e.key === '/') {
        e.preventDefault();
        document.getElementById('target').focus();
      }

      // 'Escape' - Clear results
      if (e.key === 'Escape') {
        const container = document.getElementById('results-container');
        container.innerHTML = '<div class="text-center py-12 text-zinc-500">Results cleared. Press / to search.</div>';
      }
    });
  </script>
```

**Step 2: Test keyboard shortcuts**

- Press `/`, verify target input focuses
- Focus input and press `Esc`, verify blur
- Press `Esc` when not focused, verify results clear

**Step 3: Commit keyboard navigation**

```bash
git add web/index.html
git commit -m "feat(web): add keyboard navigation (/ to focus, Esc to clear)"
```

---

## Task 17: Update README

**Files:**
- Modify: `web/README.md`

**Step 1: Update README with new interface info**

Replace existing README.md content with:

```markdown
# RIES-RS Web Interface

Modern, user-friendly web interface for RIES-RS with beautiful math rendering, instant search, and progressive disclosure for beginners and experts.

## Features

- **Instant Search**: Find algebraic equations for any number in milliseconds
- **Beautiful Math**: KaTeX rendering for LaTeX-quality equations
- **Quick Constants**: One-click access to π, e, φ, and more
- **Advanced Options**: Ranking modes, PSLQ, and more for power users
- **Shareable Links**: Every search has a unique URL
- **Dark/Light Mode**: Toggle between themes with persistence
- **Copy Formats**: Export as plain text, LaTeX, or SymPy

## Build WASM

From the **repository root**:

```bash
npm run build
```

This produces `pkg/ries_rs.js` and `pkg/ries_rs_bg.wasm`.

## Serve

```bash
npx serve . -p 5000
```

Then open: **http://localhost:5000/web/**

## Threaded Build (Optional)

For parallel search in browsers:

```bash
npm run build:threads
```

Requires nightly Rust and server headers for `SharedArrayBuffer`:

```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

## Keyboard Shortcuts

| Key | Action |
|-----|--------|
| `/` | Focus target input |
| `Esc` | Clear results / blur input |
| `Enter` | Submit search |

## URL Parameters

All options can be set via URL for sharing:

```
?target=3.14159&level=3&preset=analytic-nt&maxMatches=20&advanced=true
```
```

**Step 2: Commit README**

```bash
git add web/README.md
git commit -m "docs(web): update README for v2.0 interface with features and keyboard shortcuts"
```

---

## Task 18: Final Testing and Cleanup

**Files:**
- Test: Manual browser testing
- Modify: `web/index.html` (if any issues found)

**Step 1: Run full build test**

```bash
cd /Users/maxwell/projects/ries/ries-rs
npm run build
npx serve . -p 5000
```

**Step 2: Test all features in browser**

- [ ] Page loads without errors
- [ ] WASM initializes successfully
- [ ] Theme toggle works and persists
- [ ] Quick constant buttons trigger search
- [ ] Search completes and shows results
- [ ] KaTeX renders equations properly
- [ ] Copy buttons work for all formats
- [ ] Advanced panel expands/collapses
- [ ] URL params update on every change
- [ ] URL params restore state on reload
- [ ] Keyboard shortcuts work
- [ ] Mobile responsive (use browser dev tools)

**Step 3: Test with different targets**

Test with: π, e, φ, random numbers, very small numbers, very large numbers

**Step 4: Verify accessibility**

- [ ] Tab navigation works
- [ ] Focus indicators visible
- [ ] ARIA labels present
- [ ] Screen reader announces status changes

**Step 5: Fix any issues**

Edit web/index.html if problems found, commit fixes

**Step 6: Final commit**

```bash
git add web/
git commit -m "feat(web): complete v2.0 interface with all features tested"
```

---

## Task 19: Remove Old Files

**Files:**
- Delete: `web/index.html.old`
- Delete: `web/main.js.old`
- Delete: `web/styles.css.old`

**Step 1: Remove backup files**

```bash
cd /Users/maxwell/projects/ries/ries-rs/web
rm index.html.old main.js.old styles.css.old
```

**Step 2: Verify only new files remain**

```bash
ls -la web/
```

Expected output:
```
index.html
README.md
```

**Step 3: Commit cleanup**

```bash
git add web/
git commit -m "chore(web): remove old interface backup files"
```

---

## Task 20: Integration Test with CLI

**Files:**
- Test: Compare web results with CLI output

**Step 1: Run CLI search for π**

```bash
./target/release/ries-rs 3.141592653589793 --level 2 --max-matches 5
```

**Step 2: Run web search for π**

Open browser, search for π with level 2, max matches 5

**Step 3: Compare results**

Results should be identical (possibly in different order due to ranking)

**Step 4: Test with preset**

CLI:
```bash
./target/release/ries-rs 1.618 --preset physics --level 2
```

Web: Search for 1.618 with physics preset, level 2

**Step 5: Document any discrepancies**

If results differ significantly, investigate and file issue

**Step 6: Final commit if needed**

```bash
git commit --allow-empty -m "test(web): verified parity with CLI output"
```

---

## Success Criteria

After completing all tasks, the web interface should:

1. **Load WASM** without errors and show "Ready" status
2. **Search successfully** for any numeric target
3. **Render equations** with KaTeX math formatting
4. **Copy results** in plain text, LaTeX, or SymPy formats
5. **Toggle themes** with persistence across sessions
6. **Share URLs** that restore complete search state
7. **Respond to keyboard** shortcuts (/ and Escape)
8. **Work on mobile** with responsive layout
9. **Match CLI output** for equivalent searches

---

## Next Steps (Post-Implementation)

- Add PWA manifest for installability
- Implement search history with localStorage
- Add "Surprise me" button with random constants
- Export all results as JSON/CSV
- Add performance metrics display

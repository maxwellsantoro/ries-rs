# Web Text Output Mode Implementation Plan


**Goal:** Add a Cards/Text toggle to the results section so users can view, copy, and download all results as a single plain-text block.

**Architecture:** Pure JS changes to `web/index.html`. A `formatResults()` function generates the text; a `renderTextOutput()` function populates a `<textarea readonly>` with Copy all / Download .txt actions. A two-button toggle in the results header switches between the card view and text view. The active view persists in the URL as `?view=text`.

**Tech Stack:** Vanilla JS, Tailwind CSS (CDN), Playwright for integration testing.

---

### Task 1: Write the failing Playwright test

**Files:**
- Modify: `tests/web-smoke.spec.ts`

Add assertions after the existing color-check block (before the screenshot call) that verify the new toggle and text output exist.

**Step 1: Add assertions to the smoke test**

In `tests/web-smoke.spec.ts`, insert after `expect(colorCheck.sameColor).toBe(false);`:

```typescript
  // --- Text output mode ---
  // Toggle should be visible once results exist
  await expect(page.locator("#view-toggle")).toBeVisible();

  // Click "Text" tab
  await page.click("#view-text");

  // Cards should be hidden, textarea should be visible
  await expect(page.locator("#results-container")).toBeHidden();
  await expect(page.locator("#text-output-container")).toBeVisible();
  await expect(page.locator("#text-output")).toBeVisible();

  // Textarea content: header line + at least one equation
  const textContent = await page.locator("#text-output").inputValue();
  expect(textContent).toMatch(/^# RIES-RS v\d+\.\d+\.\d+ — target:/);
  expect(textContent).toContain("= pi");
  expect(textContent).toContain("error=");
  expect(textContent).toContain("complexity=");

  // Copy all and Download buttons exist
  await expect(page.locator("#copy-all-btn")).toBeVisible();
  await expect(page.locator("#download-btn")).toBeVisible();

  // Switching back to Cards restores cards view
  await page.click("#view-cards");
  await expect(page.locator("#results-container")).toBeVisible();
  await expect(page.locator("#text-output-container")).toBeHidden();
```

**Step 2: Run test to verify it fails**

```bash
npm run test:web:smoke
```

Expected: FAIL — `#view-toggle` not found.

---

### Task 2: Add state variables and `formatResults()` to the JS

**Files:**
- Modify: `web/index.html`

**Step 1: Add `currentMatches` state variable**

After `let advancedHintShown = false;` in the State block, add:

```javascript
    let currentView = 'cards';   // 'cards' | 'text'
    let currentMatches = [];     // last search results, for re-rendering on view toggle
```

**Step 2: Add `formatResults()` function**

In the Utility Functions section, after the `formatError()` function (and after the `toLatex` block), add:

```javascript
    // ============================================================
    // Text Output Formatting
    // ============================================================

    function formatResults(matches, target, level) {
      const ver = (wasmModule && wasmModule.version) ? wasmModule.version() : 'unknown';
      const header =
        '# RIES-RS v' + ver +
        ' \u2014 target: ' + target +
        ' (level ' + level + ', ' + matches.length + ' result' +
        (matches.length !== 1 ? 's' : '') + ')';

      const EQ_WIDTH = 26; // fixed column width for the equation field

      var lines = matches.map(function(m) {
        var eq = (m.lhs || 'x') + ' = ' + (m.rhs || '');
        var padded = eq.length < EQ_WIDTH ? eq + ' '.repeat(EQ_WIDTH - eq.length) : eq;
        return padded + '\terror=' + m.error.toExponential(2) +
               '\tcomplexity=' + m.complexity;
      });

      return [header].concat(lines).join('\n');
    }
```

**Step 3: No test run yet** — the HTML elements don't exist yet, test still fails. Continue to Task 3.

---

### Task 3: Add toggle HTML and text output container to the results section

**Files:**
- Modify: `web/index.html`

**Step 1: Replace the results section `<h2>` with a flex row that includes the toggle**

Find:
```html
    <!-- Results section -->
    <section id="results-section" class="hidden">
      <h2 class="text-xl font-semibold mb-4 flex items-center gap-2">
        <svg class="w-5 h-5 text-emerald-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
          <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
        </svg>
        Results
      </h2>
      <div id="results-container" class="space-y-4">
        <!-- Result cards will be rendered here -->
      </div>
    </section>
```

Replace with:
```html
    <!-- Results section -->
    <section id="results-section" class="hidden">
      <div class="flex items-center justify-between mb-4">
        <h2 class="text-xl font-semibold flex items-center gap-2">
          <svg class="w-5 h-5 text-emerald-400" fill="none" stroke="currentColor" viewBox="0 0 24 24">
            <path stroke-linecap="round" stroke-linejoin="round" stroke-width="2" d="M9 12l2 2 4-4m6 2a9 9 0 11-18 0 9 9 0 0118 0z" />
          </svg>
          Results
        </h2>
        <!-- View toggle -->
        <div id="view-toggle" class="flex items-center gap-1 bg-zinc-800 rounded-lg p-1">
          <button
            id="view-cards"
            type="button"
            class="px-3 py-1 text-sm rounded-md transition-colors bg-zinc-700 text-zinc-100"
            aria-pressed="true"
          >Cards</button>
          <button
            id="view-text"
            type="button"
            class="px-3 py-1 text-sm rounded-md transition-colors text-zinc-400 hover:text-zinc-200"
            aria-pressed="false"
          >Text</button>
        </div>
      </div>

      <!-- Card view -->
      <div id="results-container" class="space-y-4">
        <!-- Result cards will be rendered here -->
      </div>

      <!-- Text view -->
      <div id="text-output-container" class="hidden">
        <div class="flex items-center gap-2 mb-2">
          <button
            id="copy-all-btn"
            type="button"
            class="px-3 py-1.5 text-sm font-medium rounded-lg bg-zinc-800 hover:bg-zinc-700 transition-colors"
          >Copy all</button>
          <button
            id="download-btn"
            type="button"
            class="px-3 py-1.5 text-sm font-medium rounded-lg bg-zinc-800 hover:bg-zinc-700 transition-colors"
          >Download .txt</button>
        </div>
        <textarea
          id="text-output"
          readonly
          spellcheck="false"
          class="w-full h-64 px-4 py-3 font-mono text-sm bg-zinc-950 border border-zinc-700 rounded-lg resize-y outline-none text-zinc-300"
        ></textarea>
      </div>
    </section>
```

**Step 2: No test run yet** — the JS wiring is missing. Continue.

---

### Task 4: Wire toggle logic and `renderTextOutput()`

**Files:**
- Modify: `web/index.html`

**Step 1: Add `renderTextOutput()` and toggle wiring**

Add a new section after the Results Rendering section (Task 11 block), before the Copy Handler section:

```javascript
    // ============================================================
    // Text Output Mode (view toggle)
    // ============================================================

    function renderTextOutput(matches) {
      var target = document.getElementById('target').value.trim();
      var level = document.getElementById('level').value;
      var text = formatResults(matches, target, level);

      var textarea = document.getElementById('text-output');
      textarea.value = text;

      // Copy all
      document.getElementById('copy-all-btn').onclick = function() {
        navigator.clipboard.writeText(text).then(function() {
          var btn = document.getElementById('copy-all-btn');
          var orig = btn.textContent;
          btn.textContent = 'Copied!';
          setTimeout(function() { btn.textContent = orig; }, 1500);
        });
      };

      // Download .txt
      document.getElementById('download-btn').onclick = function() {
        var blob = new Blob([text], { type: 'text/plain' });
        var url = URL.createObjectURL(blob);
        var a = document.createElement('a');
        a.href = url;
        a.download = 'ries-rs-' + target + '.txt';
        a.click();
        URL.revokeObjectURL(url);
      };
    }

    function setView(view) {
      currentView = view;

      var cardsBtn = document.getElementById('view-cards');
      var textBtn = document.getElementById('view-text');
      var cardsContainer = document.getElementById('results-container');
      var textContainer = document.getElementById('text-output-container');

      if (view === 'text') {
        cardsContainer.classList.add('hidden');
        textContainer.classList.remove('hidden');
        cardsBtn.classList.remove('bg-zinc-700', 'text-zinc-100');
        cardsBtn.classList.add('text-zinc-400');
        cardsBtn.setAttribute('aria-pressed', 'false');
        textBtn.classList.add('bg-zinc-700', 'text-zinc-100');
        textBtn.classList.remove('text-zinc-400');
        textBtn.setAttribute('aria-pressed', 'true');
        if (currentMatches.length > 0) {
          renderTextOutput(currentMatches);
        }
      } else {
        cardsContainer.classList.remove('hidden');
        textContainer.classList.add('hidden');
        cardsBtn.classList.add('bg-zinc-700', 'text-zinc-100');
        cardsBtn.classList.remove('text-zinc-400');
        cardsBtn.setAttribute('aria-pressed', 'true');
        textBtn.classList.remove('bg-zinc-700', 'text-zinc-100');
        textBtn.classList.add('text-zinc-400');
        textBtn.setAttribute('aria-pressed', 'false');
      }

      updateURL();
    }

    function initViewToggle() {
      document.getElementById('view-cards').addEventListener('click', function() { setView('cards'); });
      document.getElementById('view-text').addEventListener('click', function() { setView('text'); });
    }
```

**Step 2: Store matches and call `renderTextOutput` from `performSearch`**

In `renderResults(matches)`, at the very top of the function body, add:

```javascript
      currentMatches = matches;
```

**Step 3: Call `initViewToggle()` in the Initialization block**

In the Initialization section at the bottom, add after `initAdvancedPanel();`:

```javascript
    initViewToggle();
```

---

### Task 5: Add `?view=` URL state

**Files:**
- Modify: `web/index.html`

**Step 1: Update `updateURL()`**

In the `updateURL()` function, after the `advanced` param block (before `const queryString = ...`), add:

```javascript
      if (currentView !== 'cards') {
        params.set('view', currentView);
      }
```

**Step 2: Update `loadFromURL()`**

In `loadFromURL()`, after the `advanced` param block (before the auto-search block), add:

```javascript
      if (params.has('view')) {
        var view = params.get('view');
        if (view === 'text' || view === 'cards') {
          currentView = view;
          // Visual state applied after search renders results
        }
      }
```

Then, in the auto-search block at the end of `loadFromURL()`, after `performSearch(target);`, add:

```javascript
        // If URL requested text view, apply it after search completes
        // performSearch is async; setView needs to run after renderResults
        // Use a one-time observer on results-section visibility
        if (currentView === 'text') {
          var observer = new MutationObserver(function(_, obs) {
            if (!document.getElementById('results-section').classList.contains('hidden')) {
              obs.disconnect();
              setView('text');
            }
          });
          observer.observe(document.getElementById('results-section'), { attributes: true });
        }
```

---

### Task 6: Run the smoke test — expect it to pass

```bash
npm run test:web:smoke
```

Expected: 1 passed. If it fails, check:
- `#view-toggle` id is on the correct element
- `#text-output-container` starts with `class="hidden"`
- `formatResults` is defined before `renderTextOutput` calls it
- `initViewToggle()` is called in the initialization block

---

### Task 7: Commit

```bash
git add web/index.html tests/web-smoke.spec.ts
git commit -m "feat(web): add Cards/Text view toggle with plain-text output"
```

---

## Light mode note

The `<textarea>` uses `bg-zinc-950` and `text-zinc-300`. In light mode (no `.dark` class), these Tailwind classes still render dark. This is consistent with the form inputs and is a pre-existing light-mode limitation, not introduced by this feature.

## Future work

- CSV format: `formatResults(matches, target, level, 'csv')` — emit RFC 4180, add `?view=csv`
- Markdown format: emit a GFM table
- The toggle becomes a dropdown once there are 3+ formats
- Full CLI flag parity (separate feature)

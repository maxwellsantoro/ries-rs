import fs from "node:fs";
import path from "node:path";

import { expect, test, type Page } from "@playwright/test";

async function runSmokeTest(page: Page, entryPath: string, screenshotName: string) {
  const consoleErrors: string[] = [];
  const externalCdnRequests: string[] = [];

  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(msg.text());
    }
  });
  page.on("request", (req) => {
    const url = req.url();
    if (url.includes("cdn.tailwindcss.com") || url.includes("cdn.jsdelivr.net")) {
      externalCdnRequests.push(url);
    }
  });
  page.on("pageerror", (err) => {
    consoleErrors.push(err.message);
  });

  await page.goto(entryPath, { waitUntil: "domcontentloaded" });
  await page.waitForSelector("#search-button:not([disabled])");
  await expect(page).toHaveTitle("RIES-RS • Find Equations for Any Number");
  await expect(page.locator("#numerical-evidence-note")).toBeVisible();
  expect(await page.locator('script[src*="tailwind"]').count()).toBe(0);
  expect(await page.evaluate(() => typeof (window as any).tailwind)).toBe("undefined");
  await expect(page.locator('link[href="./styles.css"]')).toHaveCount(1);

  const screenshotDir = test.info().outputPath("screenshots");
  fs.mkdirSync(screenshotDir, { recursive: true });
  await page.screenshot({ path: path.join(screenshotDir, screenshotName.replace(".png", "-initial.png")), fullPage: false });
  await expect(page.locator("#worker-status")).toContainText("WASM ready");
  await expect(page.getByText("Built-in constants the solver can represent directly.")).toBeVisible();
  await expect(page.getByRole("button", { name: /Speed of light/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Planck constant/ })).toHaveCount(0);
  await expect(page.getByRole("button", { name: /Gravitational constant/ })).toHaveCount(0);
  const presetOptionCount = await page.locator("#preset option").count();
  expect(presetOptionCount).toBeGreaterThan(1);

  await page.click("#advanced-toggle");
  await expect(page.locator("#advanced-panel")).toBeVisible();
  await expect(page.locator("#status")).toContainText("PSLQ is CLI-only");

  await page.locator("label:has-text('Use PSLQ (CLI only)')").click({ force: true });
  await expect(page.locator("#status")).toContainText("not supported in the web build yet");

  await page.fill("#target", "3.141592653589793");
  await page.selectOption("#ranking-mode", "complexity");
  await page.check("#match-all-digits");
  await page.click("#search-button");

  await page.waitForSelector("#results-section:not(.hidden)");
  await expect(page.locator("#status")).toContainText("Found");
  const firstCard = page.locator("#results-container > .result-card").first();
  await expect(firstCard).toBeVisible();
  await expect(firstCard.locator(".copy-btn")).toHaveCount(3);
  const sympyButton = firstCard.getByRole("button", { name: "SymPy" });
  await expect(sympyButton).toBeVisible();
  await expect(sympyButton).toHaveAttribute("data-value", "Eq(x, pi)");

  const firstCardText = await firstCard.innerText();
  expect(firstCardText).toContain("Error:");
  expect(firstCardText).toContain("Complexity:");

  // Verify LaTeX conversion: KaTeX should have rendered π symbol, not raw "pi" string.
  // The first result for π is typically x = pi, which should render as x = π.
  const katexRendered = await firstCard.evaluate((el) => {
    const katexEl = el.querySelector(".katex");
    if (!katexEl) return { hasKatex: false, hasRawPi: false, hasPiSymbol: false };
    const text = katexEl.textContent || "";
    return {
      hasKatex: true,
      // "pi" as two raw letters would appear in the MathML or visible text
      // but the π unicode character would not equal "pi"
      hasRawPi: /\bpi\b/.test(text) && !text.includes("π"),
      hasPiSymbol: text.includes("π"),
    };
  });
  expect(katexRendered.hasKatex).toBe(true);
  // If pi is in the result, it must render as π, not raw "pi"
  if (!katexRendered.hasPiSymbol) {
    // No pi in this particular result — that's fine (e.g. x=x result)
    expect(katexRendered.hasRawPi).toBe(false);
  } else {
    expect(katexRendered.hasPiSymbol).toBe(true);
  }

  const colorCheck = await firstCard.evaluate((el) => {
    const parse = (rgb: string) => {
      const nums = rgb.match(/\d+(\.\d+)?/g) || [];
      return nums.slice(0, 3).map(Number);
    };
    const cardStyle = getComputedStyle(el);
    const katexEl = el.querySelector(".katex") as HTMLElement | null;
    const textStyle = getComputedStyle(katexEl ?? el);
    return {
      cardBg: cardStyle.backgroundColor,
      textColor: textStyle.color,
      sameColor:
        JSON.stringify(parse(cardStyle.backgroundColor)) ===
        JSON.stringify(parse(textStyle.color)),
    };
  });
  expect(colorCheck.sameColor).toBe(false);

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

  await expect(page.locator("#results-container .katex-error")).toHaveCount(0);
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: path.join(screenshotDir, screenshotName), fullPage: false });

  expect(consoleErrors).toEqual([]);
  expect(externalCdnRequests).toEqual([]);
}

test("web UI loads from the repo layout, shows web-only limitations, and runs a search", async ({
  page,
}) => {
  await runSmokeTest(page, "/web/index.html", "web-smoke-test.png");
});

test("web UI loads from the static site bundle and runs a search", async ({ page }) => {
  test.skip(
    !fs.existsSync(path.join(process.cwd(), "dist", "web-site", "index.html")),
    "dist/web-site bundle not built",
  );

  await runSmokeTest(page, "/dist/web-site/index.html", "web-site-smoke-test.png");
});

test("web UI accepts legacy camelCase URL params for shared links", async ({ page }) => {
  await page.goto(
    "/web/index.html?target=3.141592653589793&maxMatches=5&rankingMode=complexity&matchAllDigits=1&advanced=1",
    { waitUntil: "domcontentloaded" },
  );
  await page.waitForSelector("#search-button:not([disabled])");
  await expect(page.locator("#advanced-panel")).toBeVisible();
  await expect(page.locator("#max-matches")).toHaveValue("5");
  await expect(page.locator("#ranking-mode")).toHaveValue("complexity");
  await expect(page.locator("#match-all-digits")).toBeChecked();
  await page.waitForSelector("#results-section:not(.hidden)");
  const renderedCount = await page.locator("#results-container > .result-card").count();
  expect(renderedCount).toBeGreaterThan(0);
  expect(renderedCount).toBeLessThanOrEqual(5);
});

test("a worker error clears the pending search and allows a retry", async ({ page }) => {
  let launches = 0;
  await page.route("**/search-worker.js", async (route) => {
    launches += 1;
    if (launches !== 1) {
      await route.continue();
      return;
    }
    await route.fulfill({
      contentType: "text/javascript",
      body: `
        self.onmessage = ({ data }) => {
          if (data.type === "init") {
            self.postMessage({ type: "ready", version: "test" });
          } else if (data.type === "listPresets") {
            self.postMessage({ type: "response", id: data.id, value: [] });
          } else if (data.type === "search") {
            throw new Error("Injected worker failure");
          }
        };
      `,
    });
  });

  await page.goto("/web/index.html?ranking-mode=complexity");
  const search = page.locator("#search-button");
  await expect(search).toBeEnabled();
  await page.fill("#target", "3.141592653589793");
  await search.click();
  await expect(page.locator("#status")).toContainText("Search failed");
  await expect(search).toBeEnabled();

  await search.click();
  await expect(page.locator("#status")).toContainText("Found");
  await expect(page.locator("#worker-status")).toContainText("WASM ready");
  await expect(page.locator("#results-section")).toBeVisible();
  expect(launches).toBe(2);
});

test("rejects invalid target input and shared links", async ({ page }) => {
  await page.goto("/web/?target=1garbage");
  await page.waitForSelector("#search-button:not([disabled])");
  await expect(page.locator("#status")).toContainText("Invalid target number");
  await page.fill("#target", "0x10");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("Invalid target number");
});

test("preset results retain constant names for display and SymPy", async ({ page }) => {
  await page.goto("/web/");
  await page.waitForSelector("#search-button:not([disabled])");
  await page.fill("#target", "1.64493406685");
  await page.fill("#level", "0");
  await page.fill("#max-matches", "16");
  await page.selectOption("#preset", "analytic-nt");
  await page.click("#search-button");
  await page.waitForSelector("#results-section:not(.hidden)");
  const values = await page.locator('[data-format="sympy"]').evaluateAll(
    (buttons) => buttons.map((button) => (button as HTMLElement).dataset.value ?? ""),
  );
  expect(values.some((value) => value.includes('Float("1.6449340668482264")'))).toBe(true);
  expect(values.every((value) => !value.includes("\u0080"))).toBe(true);
});

test("SymPy output retains exact division, reciprocals and roots", async ({ page }) => {
  await page.route("**/search-worker.js", async (route) => {
    await route.fulfill({ contentType: "text/javascript", body: `
      self.onmessage = ({ data }) => {
        if (data.type === "init") self.postMessage({ type: "ready", version: "test" });
        else if (data.type === "listPresets") self.postMessage({ type: "response", id: data.id, value: [] });
        else if (data.type === "search") self.postMessage({ type: "response", id: data.id, value:
          ["12/", "2r", "23v", "21n^"].map(postfix => ({ lhs: "x", rhs: "1", lhs_postfix: "x",
            rhs_postfix: postfix, x_value: 1, error: 0, complexity: 1, constants: [] })) });
      };` });
  });
  await page.goto("/web/?ranking-mode=complexity");
  await page.waitForSelector("#search-button:not([disabled])");
  await page.fill("#target", "1");
  await page.click("#search-button");
  await page.waitForSelector("#results-section:not(.hidden)");
  await expect(page.locator('[data-format="sympy"]')).toHaveCount(4);
  const values = await page.locator('[data-format="sympy"]').evaluateAll(
    buttons => buttons.map(button => (button as HTMLElement).dataset.value),
  );
  expect(values).toEqual([
    "Eq(x, (Integer(1))/(Integer(2)))",
    "Eq(x, Integer(1)/(Integer(2)))",
    "Eq(x, real_root(Integer(3), Integer(2)))",
    "Eq(x, (Integer(2))**(-(Integer(1))))",
  ]);
});

test("indexed roots render and copy as grouped LaTeX", async ({ page }) => {
  await page.route("**/search-worker.js", async route => {
    await route.fulfill({ contentType: "text/javascript", body: `
      self.onmessage = ({ data }) => {
        if (data.type === "init") self.postMessage({ type: "ready", version: "test" });
        else if (data.type === "listPresets") self.postMessage({ type: "response", id: data.id, value: [] });
        else if (data.type === "search") self.postMessage({ type: "response", id: data.id, value: [
          { lhs: "x", rhs: '3"/2', lhs_postfix: "x", rhs_postfix: "32v" },
          { lhs: "x", rhs: '(1+2)"/(pi+sqrt(2))', lhs_postfix: "x", rhs_postfix: "12+p2q+v" },
          { lhs: "x", rhs: '(3"/2)"/(4"/5)', lhs_postfix: "x", rhs_postfix: "32v45vv" },
          { lhs: "x", rhs: "1", solve_for_x: 'x = sqrt(2)+3"/phi', solve_for_x_postfix: "2q3fv+" },
          { lhs: "x", rhs: "sinpi(2)+pi", lhs_postfix: "x", rhs_postfix: "2Sp+" },
          { lhs: "x", rhs: "pi^pi^2", lhs_postfix: "x", rhs_postfix: "pp2^^" },
          { lhs: "x", rhs: "(pi^pi)^2", lhs_postfix: "x", rhs_postfix: "pp^s" },
        ].map(match => ({ ...match, x_value: 1, error: 0, complexity: 1, constants: [] })) });
      };` });
  });
  await page.goto("/web/?ranking-mode=complexity");
  await page.waitForSelector("#search-button:not([disabled])");
  await page.fill("#target", "1");
  await page.click("#search-button");
  await expect(page.locator('[data-format="latex"]')).toHaveCount(7);
  const values = await page.locator('[data-format="latex"]').evaluateAll(
    buttons => buttons.map(button => (button as HTMLElement).dataset.value),
  );
  expect(values).toEqual([
    String.raw`x = \sqrt[{3}]{2}`,
    String.raw`x = \sqrt[{1+2}]{\pi+\sqrt{2}}`,
    String.raw`x = \sqrt[{\sqrt[{3}]{2}}]{\sqrt[{4}]{5}}`,
    String.raw`x = \sqrt{2}+\sqrt[{3}]{\varphi}`,
    String.raw`x = \sin(\pi \cdot (2))+\pi`,
    String.raw`x = \pi^{\pi^{2}}`,
    String.raw`x = (\pi^{\pi})^{2}`,
  ]);
  await expect(page.locator("#results-container .katex")).toHaveCount(7);
  await expect(page.locator("#results-container .katex-error")).toHaveCount(0);
});

test("SymPy exports the plastic constant as an exact real root", async ({ page }) => {
  await page.route("**/search-worker.js", async route => {
    await route.fulfill({ contentType: "text/javascript", body: `
      self.onmessage = ({ data }) => {
        if (data.type === "init") self.postMessage({ type: "ready", version: "test" });
        else if (data.type === "listPresets") self.postMessage({ type: "response", id: data.id, value: [] });
        else if (data.type === "search") self.postMessage({ type: "response", id: data.id, value:
          [{ lhs: "x", rhs: "plastic", lhs_postfix: "x", rhs_postfix: "P",
            x_value: 1, error: 0, complexity: 1, constants: [] }] });
      };` });
  });
  await page.goto("/web/?ranking-mode=complexity");
  await page.waitForSelector("#search-button:not([disabled])");
  await page.fill("#target", "1");
  await page.click("#search-button");
  await expect(page.locator('[data-format="sympy"]')).toHaveAttribute("data-value",
    'Eq(x, CRootOf(Symbol("_ries_plastic")**3 - Symbol("_ries_plastic") - 1, 0))');
});

test("threaded selection reaches the worker only when isolated", async ({ page }) => {
  await page.route("**/search-worker.js", async route => {
    await route.fulfill({ contentType: "text/javascript", body: `
      self.onmessage = ({ data }) => {
        if (data.type === "init") self.postMessage({ type: "ready", version: "test",
          threaded: data.threads, workerCount: data.threads ? 4 : 0 });
        else self.postMessage({ type: "response", id: data.id, value: [] });
      };` });
  });
  await page.goto("/web/?threads=1&ranking-mode=complexity");
  await page.waitForSelector("#search-button:not([disabled])");
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(false);
  await expect(page.locator("#worker-status")).not.toContainText("threads");
  await page.fill("#target", "1");
  await page.click("#search-button");
  await expect(page).toHaveURL(/threads=1/);
});

test("isolated opt-in loads the threaded worker package", async ({ page, context }) => {
  await context.route("**/web/?threads=1", async route => {
    const response = await route.fetch();
    await route.fulfill({ response, headers: { ...response.headers(),
      "cross-origin-opener-policy": "same-origin", "cross-origin-embedder-policy": "require-corp" } });
  });
  // A module double verifies which package the real worker imports and that it
  // initializes the pool; an integration deployment supplies the real WASM.
  await context.route("**/pkg-threads/ries_rs.js", async route => {
    await route.fulfill({ contentType: "text/javascript", headers: { "cross-origin-resource-policy": "same-origin" }, body: `
      export default async function init() {}
      export async function initThreadPool(n) { if (!n) throw Error('missing concurrency'); }
      export function version() { return 'thread-test'; }
      export function listPresets() { return []; }
    ` });
  });
  await page.goto("/web/?threads=1");
  await page.waitForSelector("#search-button:not([disabled])");
  expect(await page.evaluate(() => crossOriginIsolated)).toBe(true);
  await expect(page.locator("#worker-status")).toContainText("threads");
});

test("built site keeps static styles and search usable on mobile", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await runSmokeTest(page, "/dist/web-site/index.html", "web-site-mobile.png");
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
});

test("decimal target loss is rejected through the real worker", async ({ page }) => {
  await page.goto("/web/");
  await page.waitForSelector("#search-button:not([disabled])");
  await page.click("#advanced-toggle");
  await page.selectOption("#ranking-mode", "complexity");
  await page.check("#match-all-digits");
  await page.fill("#level", "0");
  await page.fill("#target", "1.000000000000000000000000000001");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("last-digit tolerance");
  await expect(page.locator("#results-section")).toBeHidden();
  await page.fill("#target", "1e-400");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("underflows");
  await page.fill("#target", "1.000000000000000000000000000000");
  await page.click("#search-button");
  await expect(page.locator("#results-section")).toBeVisible();
});


test("compound trig arguments stay grouped in rendering and LaTeX copies", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.route("**/search-worker.js", async route => {
    await route.fulfill({ contentType: "text/javascript", body: `
      self.onmessage = ({ data }) => {
        if (data.type === "init") self.postMessage({ type: "ready", version: "test" });
        else if (data.type === "listPresets") self.postMessage({ type: "response", id: data.id, value: [] });
        else if (data.type === "search") self.postMessage({ type: "response", id: data.id, value: [
          { lhs: "sinpi(x+1)", rhs: "1" },
          { lhs: "cospi(1-2*x)", rhs: "1" },
          { lhs: "tanpi(sqrt(x)+1)", rhs: "1" },
        ].map(match => ({ ...match, error: 0, complexity: 1, constants: [] })) });
      };` });
  });
  await page.goto("/web/?ranking-mode=complexity");
  await page.waitForSelector("#search-button:not([disabled])");
  await page.fill("#target", "1.5");
  await page.click("#search-button");
  await expect(page.locator('[data-format="latex"]')).toHaveCount(3);
  const values = await page.locator('[data-format="latex"]').evaluateAll(
    buttons => buttons.map(button => (button as HTMLElement).dataset.value),
  );
  expect(values).toEqual([
    String.raw`\sin(\pi \cdot (x+1)) = 1`,
    String.raw`\cos(\pi \cdot (1-2 \cdot x)) = 1`,
    String.raw`\tan(\pi \cdot (\sqrt{x}+1)) = 1`,
  ]);
  for (let index = 0; index < values.length; index++) {
    await page.locator('[data-format="latex"]').nth(index).click();
    await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(values[index]);
  }
  await expect(page.locator("#results-container .katex")).toHaveCount(3);
  await expect(page.locator("#results-container .katex-error")).toHaveCount(0);
  expect(errors).toEqual([]);
});

test("completed run keeps its target and options in cards, text, copy and downloads after edits", async ({ page }) => {
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("/web/");
  await expect(page.locator("#search-button")).toBeEnabled();
  await page.click("#advanced-toggle");
  await page.fill("#target", "3.14159265");
  await page.fill("#level", "0");
  await page.fill("#max-matches", "3");
  await page.selectOption("#ranking-mode", "complexity");
  await page.check("#match-all-digits");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("Found");
  const cardCopies = await page.locator('.copy-btn[data-format="plain"]').evaluateAll(
    buttons => buttons.map(button => (button as HTMLElement).dataset.value),
  );
  expect(cardCopies.length).toBeGreaterThan(0);

  await page.fill("#target", "2.718281828459045");
  await page.fill("#level", "1");
  await page.fill("#max-matches", "4");
  await page.selectOption("#preset", "analytic-nt");
  await page.selectOption("#ranking-mode", "parity");
  await page.uncheck("#match-all-digits");
  await page.click("#view-text");
  const text = await page.locator("#text-output").inputValue();
  expect(text.split("\n")[0]).toContain("target: 3.14159265 (level 0,");
  expect(JSON.parse(text.split("\n")[1].replace("# Search settings: ", ""))).toEqual({
    level: 0, preset: "", maxMatches: 3, rankingMode: "complexity",
    usePslq: false, matchAllDigits: true,
  });
  await expect(page.locator("#completed-run")).toContainText("target 3.14159265, level 0");
  await expect(page.locator("#pending-settings")).toContainText("Settings changed");
  await page.click("#copy-all-btn");
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(text);
  const downloadPromise = page.waitForEvent("download");
  await page.click("#download-btn");
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("ries-rs-3.14159265.txt");
  expect(fs.readFileSync((await download.path())!, "utf8")).toBe(text);

  await page.click("#view-cards");
  expect(await page.locator('.copy-btn[data-format="plain"]').evaluateAll(
    buttons => buttons.map(button => (button as HTMLElement).dataset.value),
  )).toEqual(cardCopies);
  await page.locator('.copy-btn[data-format="plain"]').first().click();
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(cardCopies[0]);
  await page.screenshot({ path: test.info().outputPath("completed-run-pending.png"), fullPage: true });

  await page.fill("#target", "3.14159265");
  await page.fill("#level", "0");
  await page.fill("#max-matches", "3");
  await page.selectOption("#preset", "");
  await page.selectOption("#ranking-mode", "complexity");
  await page.check("#match-all-digits");
  await expect(page.locator("#pending-settings")).toBeHidden();
  await page.getByRole("button", { name: "Use Euler's number: 2.718281828459045", exact: true }).click();
  await expect(page.locator("#pending-settings")).toBeVisible();
  expect(errors).toEqual([]);
});

test("worker run provenance captures request settings before edits during a search", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.route("**/search-worker.js", async route => {
    await route.fulfill({ contentType: "text/javascript", body: `
      self.requests = [];
      self.releaseNext = () => {
        const request = self.requests.shift();
        self.postMessage({ type: "response", id: request.id, value: [{
          lhs: "x", rhs: String(request.targetValue), lhs_postfix: "x",
          rhs_postfix: String(request.targetValue), error: 0, complexity: 28, constants: []
        }] });
      };
      self.onmessage = ({ data }) => {
        if (data.type === "init") self.postMessage({ type: "ready", version: "request-test" });
        else if (data.type === "listPresets")
          self.postMessage({ type: "response", id: data.id, value: [["physics", "Test preset"]] });
        else if (data.type === "search") self.requests.push(data);
      };
    ` });
  });
  const workerPromise = page.waitForEvent("worker");
  await page.goto("/web/?ranking-mode=complexity");
  const worker = await workerPromise;
  await expect(page.locator("#search-button")).toBeEnabled();
  await page.fill("#target", "1.000");
  await page.fill("#level", "0");
  await page.click("#search-button");
  await expect.poll(() => worker.evaluate(() => (self as any).requests.length)).toBe(1);
  await worker.evaluate(() => (self as any).releaseNext());
  await expect(page.locator("#completed-run")).toContainText("target 1.000, level 0");

  await page.click("#advanced-toggle");
  await page.fill("#target", "2.000");
  await page.fill("#level", "2");
  await page.fill("#max-matches", "2");
  await page.selectOption("#preset", "physics");
  await page.selectOption("#ranking-mode", "complexity");
  await page.check("#match-all-digits");
  await page.click("#search-button");
  await expect.poll(() => worker.evaluate(() => (self as any).requests.length)).toBe(1);
  await expect(page.locator("#search-button")).toBeDisabled();
  await page.fill("#target", "3.000");
  await page.fill("#level", "4");
  await page.fill("#max-matches", "3");
  await page.selectOption("#preset", "");
  await page.selectOption("#ranking-mode", "parity");
  await page.uncheck("#match-all-digits");
  await page.click("#view-text");
  expect(await page.locator("#text-output").inputValue()).toContain("target: 1.000 (level 0,");
  await expect(page.locator("#status")).toContainText("last completed run remains");
  const request = await worker.evaluate(() => (self as any).requests[0]);
  expect(request.targetValue).toBe(2);
  expect(request.searchConfig).toEqual({
    targetText: "2.000", level: 2, maxMatches: 2,
    preset: "physics", rankingMode: "complexity", matchAllDigits: true,
  });
  await worker.evaluate(() => (self as any).releaseNext());
  await expect(page.locator("#search-button")).toBeEnabled();
  await expect(page.locator("#completed-run")).toContainText("target 2.000, level 2, preset physics");
  const completed = await page.locator("#text-output").inputValue();
  expect(completed).toContain("vrequest-test");
  expect(completed).toContain("target: 2.000 (level 2,");
  expect(completed).toContain("x = 2");
  expect(completed).not.toContain("x = 1");
  expect(JSON.parse(completed.split("\n")[1].replace("# Search settings: ", ""))).toEqual({
    level: 2, preset: "physics", maxMatches: 2, rankingMode: "complexity",
    usePslq: false, matchAllDigits: true,
  });
  await expect(page.locator("#pending-settings")).toBeVisible();
  await page.setViewportSize({ width: 390, height: 844 });
  await page.locator("#results-section").scrollIntoViewIfNeeded();
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.screenshot({ path: test.info().outputPath("completed-run-mobile.png"), fullPage: true });
  expect(errors).toEqual([]);
});

test("failed search keeps the completed run, while successful empty results replace it", async ({ page }) => {
  await page.context().grantPermissions(["clipboard-read", "clipboard-write"]);
  await page.route("**/search-worker.js", async route => {
    await route.fulfill({ contentType: "text/javascript", body: `
      self.onmessage = ({ data }) => {
        if (data.type === "init") self.postMessage({ type: "ready", version: "state-test" });
        else if (data.type === "listPresets") self.postMessage({ type: "response", id: data.id, value: [] });
        else if (data.type === "search") {
          if (data.targetValue === 2) self.postMessage({ type: "error", id: data.id, error: "Injected rejected search" });
          else self.postMessage({ type: "response", id: data.id, value: data.targetValue === 3 ? [] : [{
            lhs: "x", rhs: String(data.targetValue), lhs_postfix: "x", rhs_postfix: String(data.targetValue),
            error: 0, complexity: 28, constants: []
          }] });
        }
      };
    ` });
  });
  await page.goto("/web/?ranking-mode=complexity");
  await expect(page.locator("#search-button")).toBeEnabled();
  await page.fill("#target", "1");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("Found");
  await page.click("#view-text");
  const original = await page.locator("#text-output").inputValue();
  await page.fill("#target", "2");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("Search failed: Injected rejected search");
  await expect(page.locator("#status")).toContainText("last completed run is still shown");
  await expect(page.locator("#results-section")).toBeVisible();
  await expect(page.locator("#completed-run")).toContainText("target 1,");
  expect(await page.locator("#text-output").inputValue()).toBe(original);
  await page.click("#copy-all-btn");
  await expect.poll(() => page.evaluate(() => navigator.clipboard.readText())).toBe(original);
  await page.screenshot({ path: test.info().outputPath("failed-search-kept-run.png"), fullPage: true });
  await page.click("#view-cards");
  await expect(page.locator(".result-card")).toHaveCount(1);
  await page.fill("#target", "3");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("No equations found");
  await expect(page.locator("#completed-run")).toContainText("target 3,");
  await expect(page.locator("#empty-results")).toBeVisible();
  await expect(page.locator(".result-card")).toHaveCount(0);
  await expect(page.locator("#pending-settings")).toBeHidden();
  await page.click("#view-text");
  const empty = await page.locator("#text-output").inputValue();
  expect(empty).toContain("target: 3 (level 3, 0 results)");
  expect(empty).not.toContain("x = 1");
  const downloadPromise = page.waitForEvent("download");
  await page.click("#download-btn");
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("ries-rs-3.txt");
  expect(fs.readFileSync((await download.path())!, "utf8")).toBe(empty);
  await page.fill("#target", "4");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("Found");
  await expect(page.locator("#empty-results")).toBeHidden();
  expect(await page.locator("#text-output").inputValue()).toContain("x = 4");
});

test("discovery leads with simple rounded hypotheses and exports immutable evidence", async ({ page }) => {
  const errors: string[] = [];
  page.on("pageerror", error => errors.push(error.message));
  await page.goto("/web/");
  await expect(page.locator("#search-button")).toBeEnabled();
  await expect(page.locator("#ranking-mode")).toHaveValue("discovery");
  await expect(page.locator("#max-matches")).toHaveValue("8");
  await expect(page.locator("#match-all-digits")).toBeDisabled();
  let lastRun: any;
  for (const [literal, expectedPairs] of [
    ["3.14159265", [["x", "p"]]],
    ["1.41421356", [["x", "2q"], ["xs", "2"]]],
    ["1.64872127", [["x", "eq"], ["xs", "e"]]],
  ] as const) {
    await page.fill("#target", literal);
    await page.fill("#level", "3");
    await page.fill("#max-matches", "3");
    await page.click("#search-button");
    await expect(page.locator("#completed-run")).toContainText("target " + literal + ",");
    await expect(page.locator("#status")).toContainText("hypothes");
    const downloadPromise = page.waitForEvent("download");
    await page.click("#download-run-btn");
    const download = await downloadPromise;
    lastRun = JSON.parse(fs.readFileSync((await download.path())!, "utf8"));
    expect(lastRun.schema).toBe("ries-discovery-v1");
    expect(lastRun.target_literal).toBe(literal);
    expect(lastRun.config.level).toBe(3);
    expect(lastRun.config.max_matches).toBe(3);
    expect(lastRun.config.max_lhs_complexity).toBe(46);
    expect(lastRun.config.max_rhs_complexity).toBe(46);
    expect(lastRun.config.input_tolerance).toBeCloseTo(5e-9, 15);
    expect(lastRun.hypotheses.length).toBeGreaterThan(0);
    expect(lastRun.hypotheses.length).toBeLessThanOrEqual(3);
    expect(new Set(lastRun.hypotheses.map((h: any) => h.key)).size).toBe(lastRun.hypotheses.length);
    expect(lastRun.hypotheses.some((h: any) => expectedPairs.some(
      ([lhs, rhs]) => h.representative.lhs_postfix === lhs && h.representative.rhs_postfix === rhs,
    ))).toBe(true);
    await expect(page.locator(".result-card")).toHaveCount(lastRun.hypotheses.length);
    await expect(page.locator(".input-precision").first()).toContainText("Within entered precision");
  }
  await page.locator(".hypothesis-evidence").first().locator("summary").click();
  await expect(page.locator(".hypothesis-evidence").first()).toContainText("High precision: not_requested");
  await expect(page.locator(".hypothesis-evidence").first()).toContainText("Polynomial root bound: unavailable");
  await expect(page.locator(".hypothesis-evidence").first()).toContainText("not symbolic proof");
  await page.fill("#target", "2.718281828459045");
  await page.fill("#level", "1");
  await expect(page.locator("#pending-settings")).toBeVisible();
  const runDownloadPromise = page.waitForEvent("download");
  await page.click("#download-run-btn");
  expect(JSON.parse(fs.readFileSync((await (await runDownloadPromise).path())!, "utf8"))).toEqual(lastRun);
  const selectionDownloadPromise = page.waitForEvent("download");
  await page.getByRole("button", { name: "Export hypothesis 1", exact: true }).click();
  const selectionDownload = await selectionDownloadPromise;
  const selection = JSON.parse(fs.readFileSync((await selectionDownload.path())!, "utf8"));
  expect(selection).toEqual({
    schema: "ries-discovery-selection-v1", run: lastRun, hypothesis_index: 0,
  });
  await page.locator("#discovery-export summary").click();
  await expect(page.locator("#discovery-export")).toContainText("scripts/verify_discovery.py run.json");
  await page.click("#theme-toggle");
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: test.info().outputPath("discovery-evidence-desktop.png"), fullPage: true, animations: "disabled" });
  await page.setViewportSize({ width: 390, height: 844 });
  expect(await page.evaluate(() => document.documentElement.scrollWidth <= window.innerWidth)).toBe(true);
  await page.evaluate(() => window.scrollTo(0, 0));
  await page.screenshot({ path: test.info().outputPath("discovery-evidence-mobile.png"), fullPage: true, animations: "disabled" });

  await page.fill("#target", "1e-400");
  await page.click("#search-button");
  await expect(page.locator("#status")).toContainText("underflows");
  await expect(page.locator("#completed-run")).toContainText("target 1.64872127, level 3");
  const failedDownloadPromise = page.waitForEvent("download");
  await page.click("#download-run-btn");
  expect(JSON.parse(fs.readFileSync((await (await failedDownloadPromise).path())!, "utf8"))).toEqual(lastRun);
  expect(errors).toEqual([]);
});

test("WASM discover validates options while raw search and builders stay compatible", async ({ page }) => {
  await page.goto("/web/");
  await expect(page.locator("#search-button")).toBeEnabled();
  const result = await page.evaluate(async () => {
    const module = await import("/pkg/ries_rs.js");
    await module.default();
    const badOptions = [
      [], true, 1, "options", { level: 6 }, { level: 1.5 }, { level: -1 },
      { level: null }, { maxMatches: 0 }, { maxMatches: 1001 }, { maxMatches: 1.5 },
      { maxMatch: 1 }, { maxMatches: 1, max_matches: 2 },
      { verificationBits: 4097 }, { verificationBits: "128" }, { parallel: true },
      { rankingMode: "complexity" }, new Date(), Object.create({ maxMatches: 1 }),
      Object.create(Object.create(null)),
      Object.create(Object.assign(Object.create(null), { maxMatches: 1 })),
      Object.defineProperty({}, "maxMatch", { value: 1 }),
      { [Symbol("ignored")]: 1 }, { preset: 1 }, { preset: "unknown-preset" },
      { parallel: "true" }, { verificationBits: -1 }, { verificationBits: 1.5 },
      { maxMatches: NaN }, { level: Infinity },
    ];
    const acceptedBad = badOptions.filter(options => {
      try { module.discover("2", options); return true; } catch { return false; }
    });
    const aliases = module.discover("2.500", { level: 0, max_matches: 1, verification_bits: 53 });
    const camel = module.discover("2.500", { level: 0, maxMatches: 1, verificationBits: 53 });
    const nullPrototype = module.discover("2", Object.assign(Object.create(null), { level: 0, maxMatches: 1 }));
    const raw = module.search(2, new module.SearchOptions().level(0).max_matches(1));
    const rawMatch = raw[0].to_json();
    raw.forEach((match: any) => match.free());
    return { acceptedBad, aliases, camel, nullPrototype, rawCount: raw.length, rawMatch };
  });
  expect(result.acceptedBad).toEqual([]);
  expect(result.aliases.config).toEqual(result.camel.config);
  expect(result.nullPrototype.config.max_matches).toBe(1);
  expect(result.aliases.target_literal).toBe("2.500");
  expect(result.aliases.config.verification_bits).toBe(53);
  expect(result.aliases.hypotheses[0].validation.high_precision.status).toBe("unavailable");
  expect(result.aliases.hypotheses[0].validation.high_precision.reason).toBeTruthy();
  expect(result.rawCount).toBe(1);
  expect(result.rawMatch.lhs).toBeTruthy();
  expect(result.rawMatch.rhs).toBeTruthy();
});

test("built-site Discovery preserves its default selection and full run export", async ({ page }) => {
  await page.goto("/dist/web-site/index.html?target=3.14159265&level=0&max-matches=1");
  await expect(page.locator("#ranking-mode")).toHaveValue("discovery");
  await expect(page.locator("#completed-run")).toContainText("target 3.14159265, level 0");
  await expect(page.locator(".result-card")).toHaveCount(1);
  await expect(page.locator('[data-format="sympy"]')).toHaveAttribute("data-value", "Eq(x, pi)");
  await expect(page.locator("#pending-settings")).toBeHidden();
  const downloadPromise = page.waitForEvent("download");
  await page.click("#download-run-btn");
  const download = await downloadPromise;
  expect(download.suggestedFilename()).toBe("ries-rs-3.14159265-run.json");
  const file = test.info().outputPath("built-discovery-run.json");
  await download.saveAs(file);
  const run = JSON.parse(fs.readFileSync(file, "utf8"));
  expect(run.schema).toBe("ries-discovery-v1");
  expect(run.target_literal).toBe("3.14159265");
  expect(run.config.max_matches).toBe(1);
  expect(run.config.max_lhs_complexity).toBe(40);
  expect(run.hypotheses).toHaveLength(1);
  expect(run.hypotheses[0].representative.rhs_postfix).toBe("p");
});

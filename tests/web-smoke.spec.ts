import fs from "node:fs";
import path from "node:path";

import { expect, test, type Page } from "@playwright/test";

async function runSmokeTest(page: Page, entryPath: string, screenshotName: string) {
  const consoleErrors: string[] = [];

  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(msg.text());
    }
  });
  page.on("pageerror", (err) => {
    consoleErrors.push(err.message);
  });

  await page.goto(entryPath, { waitUntil: "domcontentloaded" });
  await page.waitForSelector("#search-button:not([disabled])");

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

  const screenshotDir = path.join(process.cwd(), "output", "playwright");
  fs.mkdirSync(screenshotDir, { recursive: true });
  await page.screenshot({
    path: path.join(screenshotDir, screenshotName),
    fullPage: true,
  });

  expect(consoleErrors).toEqual([]);
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

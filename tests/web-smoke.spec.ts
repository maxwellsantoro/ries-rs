import fs from "node:fs";
import path from "node:path";

import { expect, test } from "@playwright/test";

test("web UI loads, shows web-only limitations, and runs a search", async ({ page }) => {
  const consoleErrors: string[] = [];

  page.on("console", (msg) => {
    if (msg.type() === "error") {
      consoleErrors.push(msg.text());
    }
  });
  page.on("pageerror", (err) => {
    consoleErrors.push(err.message);
  });

  await page.goto("/web/index.html", { waitUntil: "domcontentloaded" });
  await page.waitForSelector("#search-button:not([disabled])");

  await expect(page.locator("#worker-status")).toContainText("WASM ready");
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
  await expect(firstCard.locator(".copy-btn")).toHaveCount(2);

  const firstCardText = await firstCard.innerText();
  expect(firstCardText).toContain("Error:");
  expect(firstCardText).toContain("Complexity:");

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

  const screenshotDir = path.join(process.cwd(), "output", "playwright");
  fs.mkdirSync(screenshotDir, { recursive: true });
  await page.screenshot({
    path: path.join(screenshotDir, "web-smoke-test.png"),
    fullPage: true,
  });

  expect(consoleErrors).toEqual([]);
});

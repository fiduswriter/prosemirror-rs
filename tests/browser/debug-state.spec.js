import { test, expect } from "@playwright/test";

test("debug editor state after typing", async ({ page }) => {
  const logs = [];
  const errors = [];
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      logs.push(msg.text());
    }
  });
  page.on("pageerror", (err) => {
    errors.push(err.message);
  });

  await page.goto("/");

  // Wait for the state to be exposed
  await page.waitForFunction(() => window._proseMirrorState !== undefined, { timeout: 5000 });

  const editor = page.locator("#editor .ProseMirror");
  await expect(editor).toBeVisible({ timeout: 10000 });

  // Focus and type
  await editor.click();
  await page.keyboard.type("Hello world");

  // Wait a bit for any errors to surface
  await page.waitForTimeout(500);

  expect(errors).toEqual([]);
});

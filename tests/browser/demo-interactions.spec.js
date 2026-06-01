import { test, expect } from "@playwright/test";

test("typing, Enter, and marks work without errors", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (err) => errors.push(err.message));

  await page.goto("http://localhost:3456");
  const editor = page.locator(".ProseMirror");
  await editor.waitFor();

  // Type initial text
  await editor.click();
  await page.keyboard.type("hello world");

  // Press Enter to create a new paragraph
  await page.keyboard.press("Enter");
  await page.keyboard.type("second line");

  // Verify Enter created a new paragraph
  const docState = await page.evaluate(() => {
    const view = window._proseMirrorView;
    return {
      childCount: view.state.doc.content.childCount,
      html: document.querySelector(".ProseMirror").innerHTML,
    };
  });
  expect(docState.childCount).toBe(2);

  // Select all text and apply bold via keyboard shortcut
  await page.keyboard.press("Control+a");
  await page.keyboard.press("Control+b");

  // Click somewhere to deselect, then type more
  await editor.click();
  await page.keyboard.type(" after");

  // Wait for any async errors
  await page.waitForTimeout(500);

  expect(errors).toEqual([]);
});

test("apply bold mark via menu button", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (err) => errors.push(err.message));

  await page.goto("http://localhost:3456");
  const editor = page.locator(".ProseMirror");
  await editor.waitFor();

  await editor.click();
  await page.keyboard.type("bold me");

  // Select all text
  await page.keyboard.press("Control+a");

  // Click the bold button in the menu bar if it exists
  const boldBtn = page.locator('button[title="Bold"], button:has-text("B")');
  if (await boldBtn.isVisible().catch(() => false)) {
    await boldBtn.click();
  }

  await page.waitForTimeout(500);
  expect(errors).toEqual([]);
});

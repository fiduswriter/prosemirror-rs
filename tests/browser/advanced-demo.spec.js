import { test, expect } from "@playwright/test";

test("advanced demo loads and collaboration works", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (err) => errors.push({ message: err.message, stack: err.stack }));
  page.on("console", (msg) => {
    if (msg.type() === "error") {
      const text = msg.text();
      if (!text.includes("404")) {
        errors.push({ message: text });
      }
    }
  });

  await page.goto("http://localhost:3457");

  const editorA = page.locator("#editor-a .ProseMirror");
  await editorA.waitFor({ timeout: 10000 });

  const editorB = page.locator("#editor-b .ProseMirror");
  await editorB.waitFor({ timeout: 10000 });

  // Type in Editor A
  await editorA.click();
  await page.keyboard.type("hello from A");

  // Wait for simulated network propagation (setInterval is 200ms)
  await page.waitForTimeout(600);

  // Verify Editor A has the text
  const textA = await editorA.textContent();
  expect(textA).toContain("hello from A");

  // Verify Editor B received the collaborative update
  const textB = await editorB.textContent();
  expect(textB).toContain("hello from A");

  // Verify no JS errors occurred
  if (errors.length > 0) {
    console.error("Errors:", JSON.stringify(errors, null, 2));
  }
  expect(errors).toEqual([]);
});

import { test, expect } from "@playwright/test";

test("basic demo loads without JS errors", async ({ page }) => {
  const errors = [];
  const consoleMessages = [];

  page.on("pageerror", (err) => {
    errors.push({ message: err.message, stack: err.stack });
  });

  page.on("console", (msg) => {
    consoleMessages.push({ type: msg.type(), text: msg.text() });
  });

  await page.goto("/");

  // Wait for the ProseMirror editor to mount
  const editor = page.locator("#editor .ProseMirror");
  await expect(editor).toBeVisible({ timeout: 10000 });

  // Verify the initial paragraph is present
  await expect(editor.locator("p")).toHaveCount(1);

  // Click and type some text
  await editor.click();
  await page.keyboard.type("Hello from WASM!");

  // Wait a bit for any async errors
  await page.waitForTimeout(500);

  // Ensure no uncaught exceptions or console errors occurred
  const errorMessages = consoleMessages.filter(m => m.type === "error" && !m.text.includes("404"));
  if (errors.length > 0 || errorMessages.length > 0) {
    console.error("Page errors:", JSON.stringify(errors, null, 2));
    console.error("Console errors:", JSON.stringify(errorMessages, null, 2));
    console.log("All console:", JSON.stringify(consoleMessages, null, 2));
  }
  expect(errors).toHaveLength(0);
  expect(errorMessages).toHaveLength(0);
});

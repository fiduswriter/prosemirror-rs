import { test, expect } from "@playwright/test";

test("editor is functional", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (err) => errors.push(err.message));

  await page.goto("/");

  const editor = page.locator("#editor .ProseMirror");
  await expect(editor).toBeVisible({ timeout: 10000 });

  // Focus and type one character
  await editor.click();
  await page.keyboard.type("H");
  await page.waitForTimeout(500);

  // Get state
  const result = await page.evaluate(() => {
    const view = window._proseMirrorView;
    return {
      docTextContent: view.state.doc.textContent,
      domHTML: view.dom.innerHTML,
    };
  });
  if (errors.length > 0) console.log("errors:", errors);

  expect(result.docTextContent).toBe("H");
});

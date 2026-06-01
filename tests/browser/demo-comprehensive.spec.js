import { test, expect } from "@playwright/test";

test("editor is functional", async ({ page }) => {
  const errors = [];
  page.on("pageerror", (err) => errors.push(err.message));

  await page.goto("/");

  const editor = page.locator("#editor .ProseMirror");
  await expect(editor).toBeVisible({ timeout: 10000 });

  await editor.click();

  // Instrument findDiff and the surrounding readDOMChange logic
  await page.evaluate(() => {
    window._findDiffLogs = [];
    window._readDOMChangeLogs = [];
    const view = window._proseMirrorView;
    const origFlush = view.domObserver.flush.bind(view.domObserver);
    view.domObserver.flush = function() {
      const origHandle = this.handleDOMChange;
      this.handleDOMChange = function(from, to, typeOver, added) {
        window._readDOMChangeLogs.push({ from, to, typeOver, added: added.map(n => n.nodeName) });
        return origHandle(from, to, typeOver, added);
      };
      const ret = origFlush();
      this.handleDOMChange = origHandle;
      return ret;
    };
  });

  await page.keyboard.type("H");
  await page.waitForTimeout(500);

  const logs = await page.evaluate(() => ({
    readDOM: window._readDOMChangeLogs,
    findDiff: window._findDiffLogs,
  }));
  console.log("logs:", JSON.stringify(logs, null, 2));

  const result = await page.evaluate(() => {
    const view = window._proseMirrorView;
    return {
      docTextContent: view.state.doc.textContent,
      domHTML: view.dom.innerHTML,
    };
  });
  console.log("result:", JSON.stringify(result, null, 2));

  expect(errors).toEqual([]);
  expect(result.docTextContent).toBe("H");
});

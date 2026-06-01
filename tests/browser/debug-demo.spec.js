import { test, expect } from "@playwright/test";

test("debug resolvedpos null pointer", async ({ page }) => {
  const errors = [];

  page.on("pageerror", (err) => {
    errors.push({ message: err.message, stack: err.stack });
  });

  await page.goto("/");

  const editor = page.locator("#editor .ProseMirror");
  await expect(editor).toBeVisible({ timeout: 10000 });

  // Inject debugging: intercept ResolvedPos.pos getter
  await page.evaluate(() => {
    const wasmModule = window.__wasm_bindgen_module;
    if (!wasmModule) return;
    
    // Find ResolvedPos class
    const exports = Object.values(wasmModule);
    const ResolvedPos = exports.find(e => e && e.prototype && e.prototype.constructor && e.prototype.constructor.name === 'ResolvedPos');
    if (!ResolvedPos) {
      console.error('ResolvedPos not found in wasm exports');
      return;
    }
    
    const origPos = Object.getOwnPropertyDescriptor(ResolvedPos.prototype, 'pos');
    if (origPos && origPos.get) {
      Object.defineProperty(ResolvedPos.prototype, 'pos', {
        get() {
          if (this.__wbg_ptr === 0) {
            console.error('ResolvedPos.pos called with __wbg_ptr === 0!', new Error().stack);
          }
          return origPos.get.call(this);
        },
        configurable: true,
      });
    }
    
    const origSameParent = ResolvedPos.prototype.sameParent;
    if (origSameParent) {
      ResolvedPos.prototype.sameParent = function(other) {
        if (this.__wbg_ptr === 0) {
          console.error('ResolvedPos.sameParent called with this.__wbg_ptr === 0!', new Error().stack);
        }
        if (other && other.__wbg_ptr === 0) {
          console.error('ResolvedPos.sameParent called with other.__wbg_ptr === 0!', new Error().stack);
        }
        return origSameParent.call(this, other);
      };
    }
    
    console.log('Debug interceptors installed');
  });

  await editor.click();
  await page.keyboard.type("Hello!");

  await page.waitForTimeout(1000);

  if (errors.length > 0) {
    console.error("Page errors:", JSON.stringify(errors, null, 2));
  }
  expect(errors).toHaveLength(0);
});

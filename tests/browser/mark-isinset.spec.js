import { test, expect } from "@playwright/test";

test("Mark created by marktype_create", async ({ page }) => {
  await page.goto("/");
  await page.waitForFunction(() => window._proseMirrorView !== undefined);

  const result = await page.evaluate(() => {
    const schema = window._proseMirrorView.state.schema;
    const strongType = schema.marks.strong;
    const mark = strongType.create();
    return {
      __wbg_ptr: mark.__wbg_ptr,
      hasPtr: mark.__wbg_ptr !== undefined && mark.__wbg_ptr !== 0,
    };
  });

  console.log("result:", JSON.stringify(result, null, 2));
  expect(result.hasPtr).toBe(true);
});

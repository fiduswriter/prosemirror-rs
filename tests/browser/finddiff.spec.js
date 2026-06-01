import { test, expect } from '@playwright/test';

test('findDiffStart patch works', async ({ page }) => {
  await page.goto('http://localhost:3456/');
  await page.waitForFunction(() => window._proseMirrorView !== undefined);

  const result = await page.evaluate(() => {
    const schema = window._proseMirrorView.state.schema;

    const doc1 = schema.nodeFromJSON({
      type: "doc",
      content: [{ type: "paragraph", content: [{ type: "text", text: "hello" }] }]
    });
    const doc2 = schema.nodeFromJSON({
      type: "doc",
      content: [{ type: "paragraph", content: [{ type: "text", text: "hello" }] }]
    });
    const doc3 = schema.nodeFromJSON({
      type: "doc",
      content: [{ type: "paragraph", content: [{ type: "text", text: "hallo" }] }]
    });

    return {
      sameNoArg: doc1.content.findDiffStart(doc2.content),
      sameWith0: doc1.content.findDiffStart(doc2.content, 0),
      diffNoArg: doc1.content.findDiffStart(doc3.content),
      diffWith0: doc1.content.findDiffStart(doc3.content, 0),
    };
  });

  console.log("findDiffStart result:", result);
  expect(result.sameWith0).toBe(null);
  expect(result.diffWith0).toBe(3);
});

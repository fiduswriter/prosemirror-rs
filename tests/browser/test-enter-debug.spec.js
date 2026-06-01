import { test, expect } from '@playwright/test';

test('debug Enter key - splitBlock command', async ({ page }) => {
  await page.goto('http://localhost:3456');
  const editor = page.locator('.ProseMirror');
  await editor.waitFor();

  await editor.click();
  await page.keyboard.type('hello');
  
  const result = await page.evaluate(() => {
    const view = window._proseMirrorView;
    const state = view.state;
    
    // Import splitBlock dynamically if available
    let splitBlockResult = 'not available';
    try {
      const commands = require('prosemirror-commands');
      if (commands.splitBlock) {
        const canRun = commands.splitBlock(state);
        
        // Try to run it
        let dispatched = false;
        const dispatch = (tr) => {
          dispatched = true;
          view.dispatch(tr);
        };
        const runResult = commands.splitBlock(state, dispatch, view);
        
        splitBlockResult = { canRun, runResult, dispatched };
      }
    } catch(e) {
      splitBlockResult = { error: e.message };
    }
    
    return { splitBlockResult };
  });
  
  console.log('result:', result);
  
  if (typeof result.splitBlockResult === 'object' && result.splitBlockResult.dispatched !== undefined) {
    expect(result.splitBlockResult.dispatched).toBe(true);
  }
});

import { chromium } from "playwright";

(async () => {
  const browser = await chromium.launch({ channel: "chrome" });
  const page = await browser.newPage();
  
  page.on("console", msg => console.log("CONSOLE", msg.type(), msg.text()));
  page.on("pageerror", err => console.log("PAGEERROR", err.message));
  
  await page.goto("http://localhost:3456/");
  await page.waitForTimeout(3000);
  
  const html = await page.content();
  console.log("HTML:", html.substring(0, 2000));
  
  await browser.close();
})();

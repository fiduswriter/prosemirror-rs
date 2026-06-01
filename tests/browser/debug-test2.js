import { chromium } from "playwright";

(async () => {
  const browser = await chromium.launch({ channel: "chrome" });
  const page = await browser.newPage();
  
  page.on("console", msg => console.log("CONSOLE", msg.type(), msg.text()));
  page.on("pageerror", err => console.log("PAGEERROR", err.message, err.stack));
  
  await page.goto("http://localhost:3456/");
  await page.waitForTimeout(5000);
  
  const html = await page.content();
  console.log("HTML snippet:", html.substring(0, 500));
  
  await browser.close();
})();

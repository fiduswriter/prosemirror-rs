import { defineConfig, devices } from "@playwright/test";
import { dirname, join } from "path";
import { fileURLToPath } from "url";

const __dirname = dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  testDir: ".",
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 2 : 0,
  workers: process.env.CI ? 1 : undefined,
  reporter: "list",
  use: {
    trace: "on-first-retry",
    baseURL: "http://localhost:3456",
  },
  projects: [
    {
      name: "chromium",
      use: {
        ...devices["Desktop Chrome"],
        channel: "chrome",
      },
    },
  ],
  webServer: [
    {
      command: `python3 -m http.server 3456 --directory ${join(__dirname, "../../demo/basic/dist")}`,
      url: "http://localhost:3456",
      reuseExistingServer: !process.env.CI,
      timeout: 120 * 1000,
    },
    {
      command: `python3 -m http.server 3457 --directory ${join(__dirname, "../../demo/advanced/dist")}`,
      url: "http://localhost:3457",
      reuseExistingServer: !process.env.CI,
      timeout: 120 * 1000,
    },
  ],
});

import puppeteer from "puppeteer";

const APP_HOST = process.env.APP_HOST || "localhost";
const FLAG = process.env.FLAG || "BITSCTF{flag}";

const BROWSER_ARGS = [
  "--no-sandbox",
  "--disable-setuid-sandbox",
  "--disable-dev-shm-usage",
  "--disable-gpu",
];

export async function visit(url: string): Promise<void> {
  const browser = await puppeteer.launch({
    headless: true,
    args: BROWSER_ARGS,
  });

  try {
    const page = await browser.newPage();
    await page.setCookie({
      name: "FLAG",
      value: FLAG,
      domain: APP_HOST,
      path: "/hidden",
    });

    await page.goto(url, { waitUntil: "networkidle2", timeout: 5000 }).catch(() => {});
    await new Promise((r) => setTimeout(r, 15000));
  } catch (e) {
    console.error("Bot error:", e);
  } finally {
    await browser.close();
  }
}

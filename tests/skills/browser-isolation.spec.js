const { test, expect } = require("playwright/test");

test.use({
  launchOptions: { executablePath: process.env.PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH },
});

const app = `<!doctype html><meta charset="utf-8">
<label>Note <input id="note"></label><button id="save">Save</button><output id="status"></output>
<script>
const note = document.querySelector('#note');
note.value = localStorage.getItem('note') || '';
document.querySelector('#save').onclick = () => {
  localStorage.setItem('note', note.value);
  document.querySelector('#status').textContent = 'saved';
};
</script>`;

async function openRole(browser, role) {
  const context = await browser.newContext();
  await context.route("http://fixture.test/", route =>
    route.fulfill({ status: 200, contentType: "text/html", body: app }),
  );
  const page = await context.newPage();
  await page.goto("http://fixture.test/");
  return { role, context, page };
}

test("roles retain separate data and close only their own browser context", async ({ browser }) => {
  const vadi = await openRole(browser, "vadi");
  const prativadi = await openRole(browser, "prativadi");

  await vadi.page.locator("#note").fill("vadi-data");
  await vadi.page.locator("#save").click();
  await expect(vadi.page.locator("#status")).toHaveText("saved");
  await prativadi.page.locator("#note").fill("prativadi-data");
  await prativadi.page.locator("#save").click();
  await expect(prativadi.page.locator("#status")).toHaveText("saved");

  await vadi.page.reload();
  await prativadi.page.reload();
  await expect(vadi.page.locator("#note")).toHaveValue("vadi-data");
  await expect(prativadi.page.locator("#note")).toHaveValue("prativadi-data");

  await vadi.context.close();
  expect(vadi.page.isClosed()).toBe(true);
  expect(prativadi.page.isClosed()).toBe(false);
  await prativadi.page.reload();
  await expect(prativadi.page.locator("#note")).toHaveValue("prativadi-data");
  await prativadi.context.close();
});

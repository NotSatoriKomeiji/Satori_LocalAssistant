import {test, expect} from '@playwright/test';

test('saved targets stay expanded after status changes and remain user-toggleable', async ({page}) => {
  await page.goto('/');
  await page.getByText('演示场景', {exact: true}).click();
  await page.getByRole('button', {name: '加载示例应用'}).click();

  const targets = page.locator('details.saved-targets');
  const heading = targets.locator('summary');
  await expect(targets).toHaveJSProperty('open', false);
  await heading.click();
  await expect(targets).toHaveJSProperty('open', true);

  // An unrelated status refresh should not undo the user's choice.
  await page.getByRole('button', {name: '暂停助手'}).click();
  await expect(targets).toHaveJSProperty('open', true);
  await expect(targets.getByRole('list', {name: '已添加目标'}).getByRole('listitem')).toHaveCount(6);

  await heading.click();
  await expect(targets).toHaveJSProperty('open', false);
  await page.getByLabel('快速搜索').fill('Music');
  await expect(targets).toHaveJSProperty('open', true);
  await expect(targets.getByRole('list', {name: '已添加目标'})).toContainText('Music.exe');

  // Clearing the search keeps the expanded list and restores every saved target.
  await page.getByLabel('快速搜索').fill('');
  await expect(targets).toHaveJSProperty('open', true);
  await expect(targets.getByRole('listitem')).toHaveCount(6);

  // A whitespace-only query does not override a manual collapse.
  await heading.click();
  await expect(targets).toHaveJSProperty('open', false);
  await page.getByLabel('快速搜索').fill('   ');
  await expect(targets).toHaveJSProperty('open', false);

  // Both the shortcut grid and complete list use the same trimmed search.
  await page.getByLabel('快速搜索').fill('  mUsIc  ');
  await expect(targets).toHaveJSProperty('open', true);
  await expect(targets.getByRole('listitem')).toHaveCount(1);
  await expect(page.getByRole('button', {name: '打开 Music.exe（模拟）', exact: true})).toBeVisible();

  // A user may close results even with an active query; refreshing preserves it.
  await heading.click();
  await expect(targets).toHaveJSProperty('open', false);
  await page.getByRole('button', {name: '继续运行', exact: true}).click();
  await expect(targets).toHaveJSProperty('open', false);
  await page.getByLabel('快速搜索').fill('Music');
  await expect(targets).toHaveJSProperty('open', true);
});

test('saved targets support keyboard activation and keep the choice across navigation', async ({page}) => {
  await page.goto('/');
  await page.getByText('演示场景', {exact: true}).click();
  await page.getByRole('button', {name: '加载示例应用'}).click();
  const targets = page.locator('details.saved-targets');
  const heading = targets.locator('summary');
  await heading.focus();
  await heading.press('Enter');
  await expect(targets).toHaveJSProperty('open', true);
  await heading.press('Space');
  await expect(targets).toHaveJSProperty('open', false);
  await heading.press('Space');
  await expect(targets).toHaveJSProperty('open', true);
  await page.getByRole('button', {name: '设置', exact: true}).click();
  await page.getByRole('button', {name: '首页', exact: true}).click();
  await expect(targets).toHaveJSProperty('open', true);
});

test('search wins when a native collapse and input happen before the queued toggle event', async ({page}) => {
  await page.goto('/');
  await page.getByText('演示场景', {exact: true}).click();
  await page.getByRole('button', {name: '加载示例应用'}).click();
  const targets = page.locator('details.saved-targets');
  await targets.locator('summary').click();
  await expect(targets).toHaveJSProperty('open', true);

  // Browser toggle events are queued. Keep collapse and search in one task to
  // exercise the stale binding even on a fast local machine.
  await page.evaluate(() => {
    const details = document.querySelector<HTMLDetailsElement>('details.saved-targets')!;
    details.querySelector('summary')!.click();
    const input = document.querySelector<HTMLInputElement>('input[aria-label="快速搜索"]')!;
    input.value = 'Music';
    input.dispatchEvent(new Event('input', {bubbles: true}));
  });
  await expect(targets).toHaveJSProperty('open', true);
  await expect(targets.getByRole('listitem')).toHaveCount(1);
});

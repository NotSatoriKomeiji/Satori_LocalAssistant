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
});

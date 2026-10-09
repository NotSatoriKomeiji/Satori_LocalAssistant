import {test,expect} from '@playwright/test';
test('novice navigation removes tasks, permission setup and scoring controls',async({page})=>{
 await page.goto('/');await expect(page.getByRole('navigation').getByRole('button')).toHaveCount(4);
 await page.getByRole('button',{name:'自动调节',exact:true}).click();await expect(page.getByRole('button',{name:'自动调节总开关'})).toHaveAttribute('aria-pressed','true');
 await expect(page.getByRole('spinbutton')).toHaveCount(0);await expect(page.locator('body')).not.toContainText('多步任务');await expect(page.locator('body')).not.toContainText('匹配分');
 await page.getByRole('button',{name:'自动调节总开关'}).click();await expect(page.getByRole('button',{name:'自动调节总开关'})).toHaveAttribute('aria-pressed','false');
 const slider=page.getByRole('slider');await slider.fill('55');await slider.dispatchEvent('change');await expect(page.getByText('模拟外接显示器 · 55%')).toBeVisible();
 await page.getByRole('button',{name:'暂停助手'}).click();await expect(slider).toBeDisabled();
});
test('quick words switch persists, supports click at selection and yields keys',async({page})=>{
 await page.goto('/');await page.getByRole('button',{name:'快速词',exact:true}).click();const toggle=page.getByRole('button',{name:'快速词总开关'});await toggle.click();await expect(toggle).toHaveAttribute('aria-pressed','true');
 const editor=page.getByLabel('快速词试写');await editor.fill('开头');await page.getByRole('complementary',{name:'快速词推荐'}).getByRole('button',{name:'谢谢你的帮助！'}).click();await expect(editor).toHaveValue('开头谢谢你的帮助！');
 await editor.fill('收到');await editor.press('Enter');await expect(editor).toHaveValue('收到\n');await editor.press('Backspace');await expect(editor).toHaveValue('收到');
 await toggle.click();await editor.fill('测试');await expect(page.getByRole('complementary',{name:'快速词推荐'})).toHaveCount(0);
 await page.reload();await page.getByRole('button',{name:'快速词',exact:true}).click();await expect(toggle).toHaveAttribute('aria-pressed','false');
});
test('settings are simple, sensitive apps remain excluded, quick search filters',async({page})=>{
 await page.goto('/');await page.getByText('演示场景',{exact:true}).click();await page.getByRole('button',{name:'加载示例应用'}).click();await page.getByLabel('快速搜索').fill('Music');await expect(page.getByRole('button',{name:'打开 Music.exe（模拟）'})).toBeVisible();await expect(page.getByRole('button',{name:'打开 Editor.exe（模拟）'})).toHaveCount(0);
 await page.getByLabel('模拟前台软件').selectOption('sensitive');await page.getByRole('button',{name:'设置',exact:true}).click();await page.getByText('不希望学习的软件',{exact:true}).click();await expect(page.getByRole('checkbox',{name:'学习 KeePass.exe'})).toBeDisabled();await expect(page.getByRole('checkbox',{name:'学习 KeePass.exe'})).not.toBeChecked();
});
test('desktop and narrow layouts and screenshots',async({page})=>{await page.goto('/');await page.screenshot({path:'docs/home.png',fullPage:true});await page.getByRole('button',{name:'快速词',exact:true}).click();await page.getByRole('button',{name:'快速词总开关'}).click();await page.getByLabel('快速词试写').fill('你好');await page.screenshot({path:'docs/words.png',fullPage:true});for(const width of [1160,840,620]){await page.setViewportSize({width,height:900});expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);}});
test('tray still has six slots and a working local clock',async({page})=>{await page.setViewportSize({width:480,height:350});await page.goto('/?view=tray');await expect(page.getByRole('group',{name:'常用应用推荐'}).getByRole('button')).toHaveCount(6);await expect(page.locator('.greeting')).not.toBeEmpty();expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);});

 test('local experience works without connecting optional AI and can be forgotten',async({page})=>{
  await page.goto('/');
  await expect(page.getByText('本地模式 · AI 未连接')).toBeVisible();
  await page.getByText('试试纠正学习',{exact:true}).click();await page.getByRole('button',{name:'模拟两次纠正'}).click();
  await expect(page.getByText('已记住 1 条经验 · 本地复用 0 次 · 今天自动唤醒 AI 0 次')).toBeVisible();
  await page.getByText('看看记住了什么',{exact:true}).click();await expect(page.getByText('在这个软件、设备和时段下保持手动设置')).toBeVisible();
  await page.getByRole('button',{name:/忘记经验 /}).click();await expect(page.getByText('已记住 0 条经验 · 本地复用 0 次 · 今天自动唤醒 AI 0 次')).toBeVisible();
  await page.getByText('连接 AI 服务',{exact:true}).click();await page.getByLabel('AI接口地址').fill('https://example.com/v1/chat/completions');await page.getByLabel('AI模型').fill('test');await page.getByLabel('AI密钥').fill('test-key');
  await page.getByRole('button',{name:'开启 AI 增强'}).click();await expect(page.getByRole('alert')).toContainText('网页预览不会连接 AI 服务');await expect(page.getByText('本地模式 · AI 未连接')).toBeVisible();
 });

test('dedicated startup switch toggles independently of learning and pause',async({page})=>{
 await page.goto('/');await page.getByRole('button',{name:'设置',exact:true}).click();
 const toggle=page.getByRole('button',{name:'开机自启动开关'});await expect(toggle).toHaveAttribute('aria-pressed','false');
 await toggle.click();await expect(toggle).toHaveAttribute('aria-pressed','true');
 await expect(page.getByRole('checkbox').first()).toBeChecked();
 await page.getByRole('button',{name:'暂停助手'}).click();await expect(toggle).toBeEnabled();
 await toggle.click();await expect(toggle).toHaveAttribute('aria-pressed','false');
 await expect(page.getByText('演示开关，不会修改真实启动项')).toBeVisible();
 await page.screenshot({path:'docs/startup.png',fullPage:true});
});

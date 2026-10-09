import {test,expect} from '@playwright/test';

test('saved launchers remain clickable without AI learning, suggestions, or observation',async({page})=>{
 await page.goto('/');
 await page.getByText('演示场景',{exact:true}).click();
 await page.getByRole('button',{name:'加载示例应用'}).click();
 await page.getByRole('button',{name:'设置',exact:true}).click();
 await page.getByRole('checkbox',{name:/自动学习/}).uncheck();
 await page.getByRole('checkbox',{name:/主动建议/}).uncheck();
 await page.getByRole('button',{name:'首页',exact:true}).click();
 await page.getByRole('button',{name:'暂停助手'}).click();
 const grid=page.getByRole('group',{name:'常用应用快捷入口'});
 await expect(grid.getByRole('button',{name:'打开 Editor.exe（模拟）'})).toBeEnabled();
 await grid.getByRole('button',{name:'打开 Editor.exe（模拟）'}).click();
 await expect(page.getByRole('status',{name:'启动结果'})).toContainText('模拟打开应用');
 const saved=page.locator('details.saved-targets');
 await saved.locator('summary').click();
 await saved.getByRole('button',{name:'从列表打开 Music.exe（模拟）'}).click();
 await expect(grid.getByRole('button',{name:'打开 Music.exe（模拟）'})).toBeVisible();
});

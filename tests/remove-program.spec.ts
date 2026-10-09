import {test,expect} from '@playwright/test';

for(const paused of [false,true]){
 test('remove a saved program from the list and recommendations'+(paused?' while paused':''),async({page})=>{
  await page.goto('/');
  await page.getByText('演示场景',{exact:true}).click();
  await page.getByRole('button',{name:'加载示例应用'}).click();
  if(paused)await page.getByRole('button',{name:'暂停助手'}).click();
  await page.getByText('已添加的应用与网站（6）',{exact:true}).click();
  await page.getByRole('button',{name:'移除 Editor.exe（模拟）',exact:true}).click();
  await expect(page.getByRole('status',{name:'移除结果'})).toContainText('电脑上的 exe 文件保留');
  await expect(page.getByText('已添加的应用与网站（5）',{exact:true})).toBeVisible();
  const saved=page.getByRole('list',{name:'已添加目标'});
  await expect(saved.getByRole('listitem')).toHaveCount(5);
  await expect(saved).not.toContainText('Editor.exe');
  await expect(page.getByRole('button',{name:'打开 Editor.exe（模拟）',exact:true})).toHaveCount(0);
  await expect(saved).toContainText('Music.exe');
  await page.getByRole('button',{name:'设置',exact:true}).click();
  await page.getByText('不希望学习的软件',{exact:true}).click();
  await expect(page.getByRole('checkbox',{name:'学习 Editor.exe（模拟）'})).toBeChecked();
 });
}

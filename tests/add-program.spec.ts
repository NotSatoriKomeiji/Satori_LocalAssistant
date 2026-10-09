import {test,expect,type Page} from '@playwright/test';

async function mockPicker(page:Page,scenario:string){
 await page.route('**/src/bridge.ts',route=>route.fulfill({contentType:'application/javascript',body:`
 import {preview} from '/src/preview.ts';
 export const native=false,floating=new URLSearchParams(location.search).get('view')==='tray',tray=floating;
 let state=JSON.parse(sessionStorage.getItem('picker-state')||'null');
 if(!state){
  state=await preview('status',{});
  for(let i=0;i<(${JSON.stringify(scenario)}==='tray'?5:6);i++){
   const id='saved-'+i,name='App'+i+'.exe';
   state.targets.push({kind:'app',app_id:id,name,enabled:true});
   state.apps.push({id,name,observe:true,volume:'ask',sensitive:false,explicit:false});
   state.quick_apps.push({kind:'app',app_id:id,name,score:1,overall_score:1,period_score:1,association_score:0,days:5,period_days:5,active:false});
   state.home_apps.push({kind:'app',app_id:id,name,score:1,overall_score:1,period_score:1,association_score:0,days:5,period_days:5,active:false});
  }
  if(${JSON.stringify(scenario)}==='paused'){state.settings.paused=true;state.quick_apps=[];}
  if(${JSON.stringify(scenario)}==='learning-off'){state.settings.automatic_learning=false;state.quick_apps=[];}
  if(${JSON.stringify(scenario)}==='sensitive'){state.apps[0].sensitive=true;state.quick_apps=[];state.home_apps=state.home_apps.filter(a=>a.app_id!=='saved-0');}
 }
 export async function command(name,args={}){
  if(name==='add_target'){
   if(window.pickerResult==='error')throw new Error('所选程序不存在或已移动，请重新选择 exe 文件');
   if(window.pickerResult==='cancel')state.message='已取消添加应用，没有修改已保存的列表';
   else{
    const id='new-program',name='中文 工具.exe';
    if(!state.targets.some(t=>t.app_id===id)){
     state.targets.push({kind:'app',app_id:id,name,enabled:true});
     state.apps.push({id,name,observe:state.settings.automatic_learning,volume:'ask',sensitive:false,explicit:false});
    }
    if(state.home_apps.length<6)state.home_apps.push({kind:'app',app_id:id,name,score:0,overall_score:0,period_score:0,association_score:0,days:0,period_days:0,active:false});
    state.message='已保存 '+name+'；常用应用位最多显示六个，其余可在下方列表打开';
   }
   sessionStorage.setItem('picker-state',JSON.stringify(state));
  }
  return structuredClone(state);
 }
 export async function watch(){return ()=>{};}
 `}));
}

for(const scenario of ['six-full','paused','learning-off','sensitive']){
 test('selected exe remains visible in saved list: '+scenario,async({page})=>{
  await mockPicker(page,scenario);await page.goto('/');
  await page.getByRole('button',{name:'添加应用',exact:true}).click();
  await expect(page.getByRole('status',{name:'添加结果'})).toContainText('已保存 中文 工具.exe');
  const list=page.getByRole('list',{name:'已添加目标'});
  await expect(list.getByRole('listitem')).toHaveCount(7);
  await expect(list).toContainText('中文 工具.exe');
  if(scenario==='sensitive')await expect(page.getByRole('button',{name:'打开 中文 工具.exe',exact:true})).toBeVisible();
  else await expect(page.getByRole('group',{name:'常用应用快捷入口'}).getByRole('button',{name:'打开 中文 工具.exe',exact:true})).toHaveCount(0);
  await expect(list.getByRole('button',{name:'从列表打开 中文 工具.exe'})).toBeVisible();
  await page.getByLabel('快速搜索').fill('中文 工具');
  await expect(list.getByRole('listitem')).toHaveCount(1);
  if(scenario==='learning-off')await expect(list).toContainText('学习关闭');
  await page.getByRole('button',{name:'添加应用',exact:true}).click();
  await expect(page.getByText('已添加的应用与网站（7）',{exact:true})).toBeVisible();
  await page.reload();await page.getByLabel('快速搜索').fill('中文 工具');
  await expect(list).toContainText('中文 工具.exe');
 });
}

test('picker cancellation and validation error do not claim an addition or lose saved records',async({page})=>{
 await mockPicker(page,'six-full');await page.goto('/');
 const add=page.getByRole('button',{name:'添加应用',exact:true});
 await add.click();
 await page.evaluate(()=>(window as any).pickerResult='cancel');await add.click();
 await expect(page.getByRole('status',{name:'添加结果'})).toContainText('已取消添加应用');
 await expect(page.getByRole('list',{name:'已添加目标'}).getByRole('listitem')).toHaveCount(7);
 await page.evaluate(()=>(window as any).pickerResult='error');await add.click();
 await expect(page.getByRole('alert')).toContainText('所选程序不存在或已移动');
 await expect(page.getByRole('status',{name:'添加结果'})).toHaveCount(0);
 await expect(page.getByRole('list',{name:'已添加目标'}).getByRole('listitem')).toHaveCount(7);
});

test('tray addition confirms saving even when the new target is outside the grid',async({page})=>{
 await mockPicker(page,'tray');await page.setViewportSize({width:480,height:350});
 await page.goto('/?view=tray');
 await page.getByRole('button',{name:'添加应用（位置6）',exact:true}).click();
 await expect(page.getByRole('status',{name:'添加结果'})).toContainText('已保存 中文 工具.exe');
 await expect(page.getByRole('button',{name:'打开 中文 工具.exe',exact:true})).toBeVisible();
 await expect(page.getByRole('status',{name:'添加结果'})).toBeInViewport();
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await page.goto('/');await page.getByLabel('快速搜索').fill('中文 工具');
 await expect(page.getByRole('list',{name:'已添加目标'})).toContainText('中文 工具.exe');
});

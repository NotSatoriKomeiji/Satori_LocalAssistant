import {test,expect,type Page} from '@playwright/test';

async function fixture(page:Page){
 await page.clock.install({time:new Date('2026-10-10T05:00:00Z')});
 await page.route('**/src/assist-api.ts',route=>route.fulfill({contentType:'application/javascript',body:`
 const state={enabled:false,limit:50,words:[],monitors:[],message:'',input_message:'',running:false,demo:true,ai:{enabled:false,awake:false,message:'本地模式 · AI 未连接'}};
 window.pendingRefresh=[];
 window.setServerEnabled=value=>state.enabled=value;
 export async function assist(request){
  if(request.op==='status'&&window.holdRefresh){
   const snapshot=structuredClone(state);
   return new Promise((resolve,reject)=>window.pendingRefresh.push({resolve:()=>resolve(snapshot),reject:()=>reject(new Error('stale refresh failure'))}));
  }
  if(request.op==='quick_words'){state.enabled=request.enabled;window.serverEnabled=state.enabled;}
  return structuredClone(state);
 }
 `}));
 await page.goto('/');
 await page.getByRole('button',{name:'快速词',exact:true}).click();
 const toggle=page.getByRole('button',{name:'快速词总开关'});
 await expect(toggle).toBeEnabled();
 await page.clock.pauseAt(new Date('2026-10-10T05:01:00Z'));
 await page.evaluate(()=>(window as any).holdRefresh=true);
 await page.clock.runFor(2000);
 await expect.poll(()=>page.evaluate(()=>(window as any).pendingRefresh.length)).toBe(1);
 return toggle;
}

test('a stale refresh cannot undo a completed quick-words change',async({page})=>{
 const toggle=await fixture(page);
 await toggle.click();
 await expect(toggle).toHaveAttribute('aria-pressed','true');
 await page.evaluate(()=>(window as any).pendingRefresh[0].resolve());
 await page.clock.runFor(16);
 expect(await page.evaluate(()=>(window as any).serverEnabled)).toBe(true);
 await expect(toggle).toHaveAttribute('aria-pressed','true');
});

test('a stale refresh failure cannot replace a successful operation with an error',async({page})=>{
 const toggle=await fixture(page);
 await toggle.click();
 await expect(toggle).toHaveAttribute('aria-pressed','true');
 await page.evaluate(()=>(window as any).pendingRefresh[0].reject());
 await page.clock.runFor(16);
 await expect(page.getByRole('alert')).toHaveCount(0);
 await expect(toggle).toHaveAttribute('aria-pressed','true');
});

test('refreshes do not overlap and resume after a slow response',async({page})=>{
 const toggle=await fixture(page);
 await page.clock.runFor(6000);
 expect(await page.evaluate(()=>(window as any).pendingRefresh.length)).toBe(1);
 await page.evaluate(()=>(window as any).pendingRefresh[0].resolve());
 await page.clock.runFor(16);
 await page.evaluate(()=>{(window as any).holdRefresh=false;(window as any).setServerEnabled(true);});
 await page.clock.runFor(2000);
 await expect(toggle).toHaveAttribute('aria-pressed','true');
});

test('a visibility status response cannot override a newer pushed status',async({page})=>{
 await page.route('**/src/bridge.ts',route=>route.fulfill({contentType:'application/javascript',body:`
 import {preview} from '/src/preview.ts';
 export const native=false,tray=false,floating=false;
 let state=await preview('status',{});
 export async function command(name,args={}){
  if(name==='status'&&window.holdCoreStatus){const snapshot=structuredClone(state);return new Promise(resolve=>window.resolveCoreStatus=()=>resolve(snapshot));}
  return structuredClone(state);
 }
 export async function watch(callback){window.pushPaused=()=>{state.settings.paused=true;callback(structuredClone(state));};return ()=>{};}
 `}));
 await page.goto('/');
 await expect(page.getByRole('button',{name:'暂停助手',exact:true})).toBeEnabled();
 await page.evaluate(()=>{(window as any).holdCoreStatus=true;document.dispatchEvent(new Event('visibilitychange'));});
 await expect.poll(()=>page.evaluate(()=>typeof (window as any).resolveCoreStatus)).toBe('function');
 await page.evaluate(()=>(window as any).pushPaused());
 await expect(page.getByRole('button',{name:'继续运行',exact:true})).toBeVisible();
 await page.evaluate(()=>(window as any).resolveCoreStatus());
 await expect(page.getByRole('button',{name:'继续运行',exact:true})).toBeVisible();
 await expect(page.getByRole('button',{name:'暂停助手',exact:true})).toHaveCount(0);
});

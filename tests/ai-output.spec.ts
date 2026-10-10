import {test,expect} from '@playwright/test';
import {readyForScreenshot} from './helpers/screenshots';

test('AI replies preserve Chinese and line breaks, wrap long text, and clear stale output after failure',async({page})=>{
 await page.route('**/src/assist-api.ts',route=>route.fulfill({contentType:'application/javascript',body:`
 const state={enabled:false,limit:50,words:[],monitors:[],message:'',input_message:'',running:false,demo:false,ai:{enabled:true,awake:false,message:'AI 休眠中'}};
 export async function assist(request){
  if(request.op==='ask'){
   window.aiRequestCount=(window.aiRequestCount??0)+1;
   return new Promise((resolve,reject)=>{window.resolveAiReply=resolve;window.rejectAiReply=()=>reject(new Error('模拟网络错误'));});
  }
  return structuredClone(state);
 }
 `}));
 await page.goto('/');
 await page.getByText('也可以主动问问 AI',{exact:true}).click();
 await page.getByLabel('问问AI').fill('你好，请给一个建议。');
 const send=page.getByRole('button',{name:'发送问题',exact:true});
 await send.evaluate(el=>{(el as HTMLButtonElement).click();(el as HTMLButtonElement).click();});
 await expect(page.getByRole('button',{name:'正在回答…'})).toBeDisabled();
 expect(await page.evaluate(()=>(window as any).aiRequestCount)).toBe(1);
 const answer='你好，这是中文回答。\n第二行：café 🙂\n<script>window.unexpectedAiExecution=true</script>\n'+ 'https://example.com/'+ 'a'.repeat(300);
 await page.evaluate(answer=>(window as any).resolveAiReply({answer}),answer);
 const output=page.getByRole('status',{name:'AI回答'});
 await expect(output).toHaveText(answer);
 expect(await output.textContent()).toBe(answer);
 await expect(output).toHaveCSS('white-space','pre-wrap');
 await expect(output).toHaveCSS('overflow-wrap','anywhere');
 expect(await page.evaluate(()=>(window as any).unexpectedAiExecution)).toBeUndefined();
 expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
 await readyForScreenshot(page);await output.screenshot({path:'docs/ai-output.png'});
 await page.getByLabel('问问AI').fill('再问一次');
 await page.getByRole('button',{name:'发送问题',exact:true}).click();
 await expect(output).toHaveCount(0);
 await page.evaluate(()=>(window as any).rejectAiReply());
 await expect(page.getByRole('alert')).toContainText('模拟网络错误');
 await expect(output).toHaveCount(0);
});

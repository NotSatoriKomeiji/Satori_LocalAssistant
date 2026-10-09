<script lang="ts">
 import {createEventDispatcher} from 'svelte';
 import type {Status,Experience} from './types';
 import {assist,type Request} from './assist-api';
 import {command} from './bridge';
 export let data:Status;
 const dispatch=createEventDispatcher<{updated:Status}>();
 let busy=false,error='';
 const label=(e:Experience)=>e.kind==='app_choice'?'程序选择':e.kind==='brightness'?'外接屏亮度':'音量';
 function question(e:Experience){if(e.advice?.interpretation==='choose_app')return `你想打开的是 ${e.candidates.find(c=>c.id===e.advice?.app_id)?.name??'这个应用'} 吗？`;
  return e.advice?.interpretation==='hold_app_device'?`以后在 ${e.scope.app_name} 使用这个设备时，都保持你手动设置的${label(e)}吗？`:`在这个场景中保持你手动设置的${label(e)}，对吗？`;}
 async function run(request:Request){busy=true;error='';try{await assist(request);dispatch('updated',await command('status'));}catch(e){error=String(e);}finally{busy=false;}}
</script>
<section class="card experience" aria-label="本地经验">
 <h2>正在慢慢懂你</h2><p>熟悉的事在本地处理；你的纠正会帮助 Satori 学会例外。</p>
 <p class="counts">已记住 {data.experience.rules.length} 条经验 · 本地复用 {data.experience.local_hits} 次 · 今天自动唤醒 AI {data.experience.automatic_calls_today} 次</p>
 {#if data.experience.question&&!data.settings.paused}
 {@const e=data.experience.question}
 <div class="question" role="status"><strong>{question(e)}</strong><p>{e.advice?.explanation}</p><button class="secondary" disabled={busy} on:click={()=>run({op:'experience_answer',id:e.id,revision:e.revision,accept:true})}>{e.kind==='app_choice'?'是的，打开并记住':'对，记住这个选择'}</button><button class="text-button" disabled={busy} on:click={()=>run({op:'experience_answer',id:e.id,revision:e.revision,accept:false})}>这次不用</button></div>
 {/if}
 {#if data.experience.rules.length}<details><summary>看看记住了什么</summary>{#each data.experience.rules.slice(0,20) as e}<div class="rule"><span>{e.scope.app_name} · {label(e)}<small>{e.kind==='app_choice'?'优先建议你确认过的程序':e.broad?'在这个软件和设备下保持手动设置':'在这个软件、设备和时段下保持手动设置'}</small></span><button class="text-button" aria-label={'忘记经验 '+e.id} disabled={busy} on:click={()=>run({op:'experience_forget',id:e.id})}>忘记</button></div>{/each}</details>{/if}
 {#if data.demo}<details><summary>试试纠正学习</summary><p>模拟两个独立场景中的纠正，查看本地如何学会例外；不会调用 API。</p><button class="secondary" disabled={busy||data.settings.paused} on:click={()=>run({op:'demo_correction'})}>模拟两次纠正</button></details>{/if}
 {#if error}<p class="error" role="alert">{error}</p>{/if}
</section>
<style>
 .experience{margin-bottom:20px}.counts{font-size:13px;color:#927ca0}.question{background:#f4edf8;border-radius:12px;padding:16px;margin:14px 0;line-height:1.8}.rule{display:flex;align-items:center;justify-content:space-between;gap:16px;padding:12px 0;border-bottom:1px solid #eee5f2}.rule small{display:block;font-size:12px;color:#9985a6;line-height:1.8}details{margin-top:14px}summary{color:#8c719b;font-size:14px;cursor:pointer}
</style>

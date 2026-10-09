<script lang="ts">
  import type {RankedApp} from './types';
  export let apps:RankedApp[]=[];
  export let disabled=false;
  export let onOpen:(app:RankedApp)=>void=()=>{};
  export let onAdd:()=>void=()=>{};
  $: slots=Array.from({length:6},(_,i)=>apps[i]??null);
  const shortName=(name:string)=>name.replace(/\.exe(?:（模拟）)?$/i,'');
</script>
<div class="app-grid" role="group" aria-label="常用应用推荐">
  {#each slots as app,i}
    {#if app}<button class="app-tile" class:active-app={app.active} disabled={disabled} title={app.name} aria-label={`打开 ${app.name}`} on:click={()=>onOpen(app)}><span class="tile-icon">{shortName(app.name).slice(0,1).toUpperCase()}</span><strong>{shortName(app.name)}</strong><small>{app.kind==='website'?'网站 · ':''}{app.active?'当前应用':app.period_days?`本时段 ${app.period_days} 天`:app.days?`总体 ${app.days} 天`:'尚无使用记录'}</small></button>
    {:else}<button class="app-tile placeholder" disabled={disabled} aria-label={`添加应用（位置${i+1}）`} on:click={onAdd}><span class="tile-icon">+</span><strong>添加应用</strong><small>空位 {i+1}</small></button>{/if}
  {/each}
</div>

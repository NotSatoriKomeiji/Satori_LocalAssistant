<script lang="ts">
 import type {StartupStatus} from './types';
 export let startup:StartupStatus;
 export let busy=false;
 export let onToggle:(enabled:boolean)=>void;
</script>
<section class="card startup-card" aria-label="开机自启动">
 <div><h2>开机自启动</h2><p>登录 Windows 后自动在后台运行，不用每次手动打开。</p><p class="startup-note" role="status">{startup.simulated?'演示开关，不会修改真实启动项':startup.message}</p></div>
 <button class="startup-toggle" class:enabled={startup.enabled} aria-label="开机自启动开关" aria-pressed={startup.enabled} disabled={busy||!startup.supported} on:click={()=>onToggle(!startup.enabled)}>{busy?'正在设置…':startup.enabled?'已开启':'开启'}</button>
</section>
<style>
 .startup-card{display:flex;align-items:center;justify-content:space-between;gap:24px;margin-bottom:20px}.startup-card h2{margin-top:0}.startup-note{font-size:13px;color:#9985a6}.startup-toggle{flex-shrink:0;min-width:104px;border:0;border-radius:24px;background:#ece5f1;color:#80658f;padding:13px 20px;font-weight:600;cursor:pointer}.startup-toggle.enabled{background:#a074b8;color:#fff}.startup-toggle:disabled{opacity:.6;cursor:default}
</style>

<script lang="ts">
  import {onMount} from 'svelte';
  import {localClock} from './time';
  let clock=localClock();
  onMount(()=>{
    let timer:ReturnType<typeof setTimeout>;
    const update=()=>{clearTimeout(timer);clock=localClock();if(document.visibilityState==='visible')timer=setTimeout(update,60025-Date.now()%60000);};
    document.addEventListener('visibilitychange',update);window.addEventListener('focus',update);update();
    return()=>{clearTimeout(timer);document.removeEventListener('visibilitychange',update);window.removeEventListener('focus',update);};
  });
</script>
<div class="local-clock"><span class="greeting">{clock.greeting}</span><span class="clock-meta">{clock.time} · {clock.zone}</span></div>

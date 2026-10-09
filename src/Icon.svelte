<script lang="ts">
  export let name = 'home';
  const paths: Record<string,string> = {
    home: 'M3 10 12 3l9 7v10H3V10Zm6 10v-7h6v7',
    shield: 'M12 3 4 6v6c0 5 8 9 8 9s8-4 8-9V6l-8-3Zm-4 9 3 3 5-6',
    memory: 'M8 3h8v4h4v10h-4v4H8v-4H4V7h4V3Zm0 4h8v10H8V7M1 9h3m-3 6h3m16-6h3m-3 6h3',
    settings: 'M4 7h16M4 17h16M9 4v6m6 4v6',
    volume: 'M4 9h4l5-5v16l-5-5H4V9Zm12-1c3 3 3 5 0 8m3-11c5 5 5 9 0 14',
    arrow: 'M5 12h14m-5-5 5 5-5 5',
    sparkle: 'm12 3 2.5 6.5L21 12l-6.5 2.5L12 21l-2.5-6.5L3 12l6.5-2.5L12 3Z',
    pause: 'M8 5v14M16 5v14',
    play: 'm7 4 13 8-13 8V4Z',
    check: 'm5 12 4 4L19 6',
    undo: 'M9 5 4 10l5 5m-5-5h10a6 6 0 0 1 0 12',
  };
</script>
<svg viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.6" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true"><path d={paths[name] ?? paths.home}/></svg>

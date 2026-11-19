<script>
  import { ROOMS, go, hrefs, route } from '../route.svelte.js';
  /**
   * The top of every room, drawn once: the place's name as a slim
   * masthead, and signing in (the kit's own button) across from it; then
   * its rooms, the one open underlined, each opened in place.
   */
  let { name } = $props();
</script>

<header class="band">
  <div class="band-row">
    <a class="masthead" href={hrefs.board} onclick={(e) => go(e, 'board')}>{name}</a>
    <os-sign-in></os-sign-in>
  </div>
</header>
<div class="page">
  <nav class="rooms" aria-label="Rooms">
    {#each Object.entries(ROOMS) as [room, label] (room)}
      <a
        href={hrefs[room]}
        aria-current={route.room === room ? 'page' : undefined}
        onclick={(e) => go(e, room)}>{label}</a
      >
    {/each}
  </nav>
</div>

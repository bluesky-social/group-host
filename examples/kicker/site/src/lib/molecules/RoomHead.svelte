<script>
  /**
   * A room's heading: what the room is, and the one act on the whole room
   * (`act`) across from it.
   */
  let { title, act } = $props();
</script>

<div class="room-head">
  <h1>{title}</h1>
  {#if act}{@render act()}{/if}
</div>

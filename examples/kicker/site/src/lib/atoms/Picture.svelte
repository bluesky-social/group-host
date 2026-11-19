<script>
  /**
   * A picture read from the host (`load`, its URL to come), and what it
   * shows (`alt`); until it comes, or if it never does, a blank of its
   * shape.
   */
  let { load, alt } = $props();
</script>

{#await load}<span class="picture" aria-hidden="true"></span>{:then src}<img
    class="picture"
    {src}
    {alt}
  />{:catch}<span class="picture" aria-hidden="true"></span>{/await}

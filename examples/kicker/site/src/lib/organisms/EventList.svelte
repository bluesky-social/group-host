<script>
  import Empty from '../molecules/Empty.svelte';
  import EventCard from '../molecules/EventCard.svelte';
  /**
   * Events (`rows`: each event with who answered it), each opened by
   * `open`, with its URI: the first leading, at the width of the page,
   * the rest after it, smaller; or, with none, `empty` in their place.
   * `past`, quieter and none leading, as what has been.
   */
  let { rows, empty, past = false, open } = $props();
</script>

{#if rows.length}
  <ul class="events" class:past>
    {#each rows as row, i (row.event.uri)}
      <EventCard
        event={row.event}
        going={row.goingCount}
        picture={row.picture}
        lead={!past && i === 0}
        open={() => open(row.event.uri)}
      />
    {/each}
  </ul>
{:else}
  <Empty title={empty} />
{/if}

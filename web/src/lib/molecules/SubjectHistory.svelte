<script lang="ts">
  import Person from '#lib/molecules/Person.svelte';
  import { when } from '#lib/dates.ts';
  import { type Event, detail, title } from '#lib/moderation.ts';
  /**
   * What happened to a subject in the queue, folded under a "History" to
   * open: each event, oldest first, who did it and when, then what was
   * said. `rules`, the group's rules' titles by record key, for the
   * reports that cite one. `open` to start unfolded.
   */
  let {
    events,
    rules = {},
    open = false
  }: { events: Event[]; rules?: Record<string, string>; open?: boolean } = $props();
</script>

{#if events.length}
  <details class="history" {open}>
    <summary>History</summary>
    <ol class="events">
      {#each events as e (e.id)}
        {@const said = detail(e, rules)}
        <li>
          <span
            ><strong>{title(e)}</strong>{#if e.creator}{' '}by <Person
                actor={e.creator}
              />{/if}</span
          >
          <span class="what">{when(e.createdAt)}</span>
          {#if said}<span class="said">{said}</span>{/if}
        </li>
      {/each}
    </ol>
  </details>
{/if}

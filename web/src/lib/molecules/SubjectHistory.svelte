<script lang="ts">
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import { m } from '#lib/paraglide/messages.js';
  import Person from '#lib/molecules/Person.svelte';
  import { when } from '#lib/dates.ts';
  import { type Event, detail, titles } from '#lib/moderation.ts';
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
  const named = $derived(titles(events));
</script>

{#if events.length}
  <details class="history" {open}>
    <summary>{m.subject_history_title()}</summary>
    <ol class="events">
      {#each events as e, i (e.eventId)}
        {@const said = detail(e, rules)}
        <li>
          <span
            >{#if e.creator}<ParaglideMessage message={m.subject_history_by}>
                {#snippet event()}<strong>{named[i]}</strong>{/snippet}
                {#snippet person()}<Person actor={e.creator!} />{/snippet}
              </ParaglideMessage>{:else}<strong>{named[i]}</strong>{/if}</span
          >
          <span class="what">{when(e.createdAt)}</span>
          {#if said}<span class="said">{said}</span>{/if}
        </li>
      {/each}
    </ol>
  </details>
{/if}

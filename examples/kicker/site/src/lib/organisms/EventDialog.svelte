<script>
  import Picture from '../atoms/Picture.svelte';
  import Handle from '../atoms/Handle.svelte';
  import Dialog from '../molecules/Dialog.svelte';
  import RsvpButtons from '../molecules/RsvpButtons.svelte';
  import { hours, long } from '../dates.js';
  /**
   * An event opened: its poster, if it has one; when, as a caption; its
   * name, who hosts it, what happens, and who is going; then answering
   * it, while it is still to come and on, and its host calling it off or
   * back on (`row`: the event as the room reads it).
   */
  let { open = $bindable(false), row } = $props();
  const starts = $derived(row ? new Date(row.event.value.startsAt) : null);
  const ends = $derived(row?.event.value.endsAt ? new Date(row.event.value.endsAt) : null);
  const cancelled = $derived(!!row?.event.value.cancelled);
  const names = (handles) => handles.map((h) => `@${h}`).join(', ');
</script>

<Dialog bind:open label="Event">
  {#if row}
    {#if row.picture}<Picture load={row.picture} alt={row.event.value.imageAlt ?? ''} />{/if}
    <p class="caption">{long(starts)}, {hours(starts, ends)}.</p>
    <h2 class="dialog-title">{row.event.value.name}</h2>
    <p class="lede">
      Hosted by <Handle handle={row.host} />.{#if cancelled}<span class="called-off"
          >{' Called off.'}</span
        >{/if}
    </p>
    {#if row.event.value.description}<p class="prose">{row.event.value.description}</p>{/if}
    <p class="meta">
      {#if row.going.length}Going: {names(row.going)}.{:else}Nobody going yet.{/if}{#if row.interested.length}{` Interested: ${names(row.interested)}.`}{/if}
    </p>
    {#if !row.past}
      <div class="acts">
        {#if !cancelled}<RsvpButtons mine={row.mine} answer={row.answer} />{/if}
        {#if row.toggle}<button type="button" class="quiet" onclick={row.toggle}
            >{cancelled ? 'Restore event' : 'Call it off'}</button
          >{/if}
      </div>
    {/if}
  {/if}
</Dialog>

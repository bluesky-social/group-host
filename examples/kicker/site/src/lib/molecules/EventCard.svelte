<script>
  import Picture from '../atoms/Picture.svelte';
  import { hours, long } from '../dates.js';
  /**
   * An event, as a magazine runs a picture: its poster, if it has one
   * (`picture`, its URL to come), large; under it, or beside it where it
   * leads (`lead`), a plain caption of when and who is going (or that it
   * was called off), its name, and what happens, its first lines. The
   * whole of it opens it (`open`).
   */
  let { event, going, picture, lead = false, open } = $props();
  const cancelled = $derived(!!event.value.cancelled);
  const starts = $derived(new Date(event.value.startsAt));
  const ends = $derived(event.value.endsAt ? new Date(event.value.endsAt) : null);
</script>

<li class="event" class:lead class:cancelled>
  <button type="button" onclick={open}>
    {#if picture}<Picture load={picture} alt={event.value.imageAlt ?? ''} />{/if}
    <span class="event-text">
      <span class="caption"
        >{long(starts)}, {hours(starts, ends)}.{#if cancelled}<span class="called-off"
            >{' Called off.'}</span
          >{:else if going}{` ${going} going.`}{/if}</span
      >
      <span class="event-name">{event.value.name}</span>
      {#if event.value.description}<span class="event-about"
          >{event.value.description.split('\n')[0]}</span
        >{/if}
    </span>
  </button>
</li>

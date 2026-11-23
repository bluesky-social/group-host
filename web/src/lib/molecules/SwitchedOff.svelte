<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import { enhance } from '$app/forms';
  import DeleteGroup from '#lib/molecules/DeleteGroup.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import { inPlace } from '#lib/forms.ts';
  /**
   * A group switched off, in the place of all it shows: to one
   * of those who ran it then (`steward`), the way to switch it back on,
   * and to delete it; to anyone else, the way back to their groups.
   */
  let { steward }: { steward: boolean } = $props();
</script>

<Empty title={m.switched_off_title()}>
  {m.switched_off_body()}
  {#snippet action()}
    {#if steward}
      <form method="post" action="?/activate" class="actions start" use:enhance={inPlace}>
        <button class="quiet" type="button" popovertarget="delete-group"
          >{m.switched_off_delete()}</button
        >
        <button type="submit">{m.switched_off_switch_on()}</button>
      </form>
    {:else}
      <a class="button quiet" href="/">{m.switched_off_back()}</a>
    {/if}
  {/snippet}
</Empty>
{#if steward}<DeleteGroup />{/if}

<script>
  import Dialog from '../molecules/Dialog.svelte';
  import { sending } from '../forms.svelte.js';
  /**
   * Answering what the moderators did to the visitor's own note (`did`,
   * in words such as "hid your note from members"): their words, given
   * to `appeal`, which puts it back in front of the moderators. Sent, it
   * says so.
   */
  let { open = $bindable(false), appeal, did } = $props();
  let sent = $state(false);
  const form = sending(async (data) => {
    await appeal(String(data.get('reason')).trim());
    sent = true;
  });
  $effect(() => {
    if (!open) sent = false;
  });
</script>

<Dialog bind:open label="Appeal">
  <h2 class="dialog-title">Appeal</h2>
  {#if sent}
    <p class="lede">Sent. The moderators see your appeal in the place's queue, beside the note.</p>
    <div class="acts end">
      <button type="button" onclick={() => (open = false)}>Done</button>
    </div>
  {:else}
    <p class="lede">The moderators {did}. Say why it should stand.</p>
    <form class="form" onsubmit={form.send}>
      <label class="field"
        >Your appeal <textarea name="reason" rows="4" maxlength="2000" required></textarea></label
      >
      {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
      <div class="acts end">
        <button type="button" class="quiet" onclick={() => (open = false)}>Cancel</button>
        <button type="submit" disabled={form.busy}>Send</button>
      </div>
    </form>
  {/if}
</Dialog>

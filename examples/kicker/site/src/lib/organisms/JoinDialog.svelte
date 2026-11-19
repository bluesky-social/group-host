<script>
  import Dialog from '../molecules/Dialog.svelte';
  import { sending } from '../forms.svelte.js';
  /**
   * Asking to join, where an admin approves each request: its button, and
   * the dialog it opens, a note for the admin, given to `ask`.
   */
  let { ask } = $props();
  let open = $state(false);
  const form = sending(async (data) => {
    await ask(String(data.get('message') ?? '').trim());
    open = false;
  });
</script>

<button type="button" class="big" onclick={() => (open = true)}>Ask to join</button>
<Dialog bind:open label="Ask to join">
  <h2 class="dialog-title">Ask to join</h2>
  <p class="lede">An admin reads each request, and lets you in.</p>
  <form class="form" onsubmit={form.send}>
    <label class="field"
      ><span>Note <span class="optional">(optional)</span></span>
      <textarea name="message" rows="3" maxlength="2000"></textarea></label
    >
    {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
    <div class="acts end">
      <button type="button" class="quiet" onclick={() => (open = false)}>Cancel</button>
      <button type="submit" disabled={form.busy}>Send request</button>
    </div>
  </form>
</Dialog>

<script>
  import Dialog from '../molecules/Dialog.svelte';
  import { sending } from '../forms.svelte.js';
  /**
   * Pinning a note to the board: its button, and the dialog it opens, its
   * words and kind given to `pin`; done, the dialog closes.
   */
  let { pin } = $props();
  let open = $state(false);
  const form = sending(async (data) => {
    await pin({ text: String(data.get('text')).trim(), kind: String(data.get('kind')) });
    open = false;
  });
</script>

<button type="button" onclick={() => (open = true)}>Pin a note</button>
<Dialog bind:open label="Pin a note">
  <h2 class="dialog-title">Pin a note</h2>
  <p class="lede">Something to know, to ask for, or to give. Every member sees it on the board.</p>
  <form class="form" onsubmit={form.send}>
    <label class="field"
      >Note <textarea name="text" rows="4" maxlength="1000" required></textarea></label
    >
    <label class="field"
      >Kind
      <select name="kind">
        <option value="notice">Notice</option>
        <option value="ask">Ask</option>
        <option value="offer">Offer</option>
      </select></label
    >
    {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
    <div class="acts end">
      <button type="button" class="quiet" onclick={() => (open = false)}>Cancel</button>
      <button type="submit" disabled={form.busy}>Pin</button>
    </div>
  </form>
</Dialog>

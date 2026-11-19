<script>
  import Dialog from '../molecules/Dialog.svelte';
  import { sending } from '../forms.svelte.js';
  /**
   * Starting a discussion: its button, and the dialog it opens, what it is
   * about and its first words given to `start`; done, the dialog closes.
   */
  let { start } = $props();
  let open = $state(false);
  const form = sending(async (data) => {
    const topic = { title: String(data.get('title')).trim() };
    const text = String(data.get('text')).trim();
    if (text) topic.text = text;
    await start(topic);
    open = false;
  });
</script>

<button type="button" onclick={() => (open = true)}>Start a discussion</button>
<Dialog bind:open label="Start a discussion">
  <h2 class="dialog-title">Start a discussion</h2>
  <p class="lede">Every member sees it, and can reply.</p>
  <form class="form" onsubmit={form.send}>
    <label class="field">What about <input name="title" required maxlength="300" /></label>
    <label class="field"
      ><span>More to say <span class="optional">(optional)</span></span>
      <textarea name="text" rows="5" maxlength="3000"></textarea></label
    >
    {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
    <div class="acts end">
      <button type="button" class="quiet" onclick={() => (open = false)}>Cancel</button>
      <button type="submit" disabled={form.busy}>Start</button>
    </div>
  </form>
</Dialog>

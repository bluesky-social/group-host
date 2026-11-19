<script>
  import { sending } from '../forms.svelte.js';
  /**
   * Replying: the words, given to `send`; done, emptied, and `cancel`, if
   * given, closes it. `label` names the field, such as "Your reply".
   */
  let { send, cancel = null, label = 'Your reply' } = $props();
  const form = sending(async (data) => {
    await send(String(data.get('text')).trim());
    cancel?.();
  });
</script>

<form class="form reply-form" onsubmit={form.send}>
  <label class="field"
    >{label} <textarea name="text" rows="3" maxlength="3000" required></textarea></label
  >
  {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
  <div class="acts end">
    {#if cancel}<button type="button" class="quiet" onclick={cancel}>Cancel</button>{/if}
    <button type="submit" disabled={form.busy}>Reply</button>
  </div>
</form>

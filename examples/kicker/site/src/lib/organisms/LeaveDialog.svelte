<script>
  import Dialog from '../molecules/Dialog.svelte';
  import { sending } from '../forms.svelte.js';
  /**
   * Leaving the place: a quiet button on the viewer's own row, and the
   * dialog it opens, asking first. `leave` does it. Refused for the last
   * admin, who is told how to leave all the same.
   */
  let { name, leave } = $props();
  let open = $state(false);
  const form = sending(async () => {
    try {
      await leave();
    } catch (e) {
      if (e.name !== 'LastConfigurer') throw e;
      throw new Error("You're the last admin. Make someone else an admin in the console first.");
    }
  });
</script>

<button type="button" class="quiet small" onclick={() => (open = true)}>Leave</button>
<Dialog bind:open label="Leave this group?">
  <h2 class="dialog-title">Leave this group?</h2>
  <p class="lede">
    You'll lose access to {name}'s board, events, discussions and people. To come back, you ask to
    join again.
  </p>
  <form class="form" onsubmit={form.send}>
    {#if form.failed}<p class="failed" role="status">{form.failed}</p>{/if}
    <div class="acts end">
      <button type="button" class="quiet" onclick={() => (open = false)}>Cancel</button>
      <button type="submit" class="danger" disabled={form.busy}>Leave</button>
    </div>
  </form>
</Dialog>

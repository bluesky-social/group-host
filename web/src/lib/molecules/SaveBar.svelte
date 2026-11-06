<script lang="ts">
  import type { Saving } from '#lib/saving.svelte.ts';
  /**
   * A form's Save button, the end of a form that edits what is there in
   * place, and beside it where the form stands: changes not saved yet,
   * saving, saved, or why it was not. The button acts, in the one colour
   * that acts, only once there is something to save; it is never
   * disabled, so it can always be reached. `what` names what it saves to
   * a screen reader, as a page may have two.
   */
  let { saving, what }: { saving: Saving; what: string } = $props();
  const said = $derived(
    {
      clean: '',
      changed: 'Unsaved changes',
      saving: 'Saving…',
      saved: 'Saved',
      failed: saving.why ?? ''
    }[saving.standing]
  );
</script>

<div class="save-bar">
  <button class:quiet={!saving.changed} type="submit" aria-label="Save {what}">Save</button>
  <span
    class="save-said"
    class:mute={saving.standing !== 'failed' && saving.standing !== 'saved'}
    class:danger={saving.standing === 'failed'}
    class:success={saving.standing === 'saved'}
    role="status">{said}</span
  >
</div>

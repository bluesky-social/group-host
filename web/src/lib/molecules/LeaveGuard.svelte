<script lang="ts">
  import { beforeNavigate, goto } from '$app/navigation';
  import Dialog from '#lib/molecules/Dialog.svelte';
  import { anyUnsaved } from '#lib/saving.svelte.ts';
  /**
   * Asking before the reader leaves a page holding changes not saved yet.
   * Going elsewhere in the console, a dialog asks; closing the tab or
   * leaving the site, the browser asks in its own words, as only it may.
   * Its quiet button is "Keep editing", not Cancel: here Cancel could as
   * well mean throwing the changes away.
   */
  let going = $state<URL | null>(null);
  let leaving = false;
  beforeNavigate(({ type, to, cancel }) => {
    if (leaving || !anyUnsaved()) return;
    // The browser's own question, for a tab closed or reloaded.
    if (type === 'leave') return cancel();
    // A form sent is the page saving, not left behind.
    if (type === 'form' || !to?.url) return;
    cancel();
    going = to.url;
    document.getElementById('leave-unsaved')?.showPopover();
  });
  async function leave() {
    const to = going;
    if (!to) return;
    leaving = true;
    document.getElementById('leave-unsaved')?.hidePopover();
    if (to.origin === location.origin) await goto(to);
    else location.href = to.href;
    leaving = false;
  }
</script>

<Dialog id="leave-unsaved" title="Leave without saving?" onclose={() => (going = null)}>
  <p class="mute">You’ll lose the changes you haven’t saved.</p>
  <div class="actions">
    <button class="quiet" type="button" popovertarget="leave-unsaved" popovertargetaction="hide"
      >Keep editing</button
    >
    <button class="danger" type="button" onclick={leave}>Leave</button>
  </div>
</Dialog>

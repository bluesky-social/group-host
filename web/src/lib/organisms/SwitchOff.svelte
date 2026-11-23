<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import DeleteGroup from '#lib/molecules/DeleteGroup.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * Switching the group off, which its admins can undo, and deleting it,
   * which nobody can: each asked to be sure of.
   */
</script>

<div class="actions start">
  <button class="quiet" type="button" popovertarget="switch-off">{m.switch_off_button()}</button>
  <button class="quiet" type="button" popovertarget="delete-group">{m.switch_off_delete()}</button>
</div>
<ConfirmDialog
  id="switch-off"
  title={m.switch_off_title()}
  action="?/deactivate"
  confirm={m.switch_off_button()}
  danger={false}
>
  {m.switch_off_body()}
</ConfirmDialog>
<DeleteGroup />

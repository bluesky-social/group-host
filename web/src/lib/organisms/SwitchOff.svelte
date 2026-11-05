<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import DeleteGroup from '#lib/molecules/DeleteGroup.svelte';
  /**
   * Switching the group off, which its admins can undo, and deleting it,
   * which nobody can: each asked to be sure of.
   */
</script>

<div class="actions start">
  <button class="quiet" type="button" popovertarget="switch-off">Switch off</button>
  <button class="quiet" type="button" popovertarget="delete-group">Delete the group</button>
</div>
<ConfirmDialog
  id="switch-off"
  title="Switch this group off?"
  action="?/deactivate"
  confirm="Switch off"
  danger={false}
>
  Nobody can see it, not even members, until an admin switches it back on.
</ConfirmDialog>
<DeleteGroup />

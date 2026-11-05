<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  /**
   * Asking to be sure of deleting the group, opened by a button whose
   * `popovertarget` is `delete-group`; sent to the page's `delete`. Not
   * named in the title, which a long name or handle would overrun.
   */
</script>

<ConfirmDialog id="delete-group" title="Delete this group?" action="?/delete" confirm="Delete">
  This deletes the group and everything in it for good. Members keep what they posted on their own
  accounts.
</ConfirmDialog>

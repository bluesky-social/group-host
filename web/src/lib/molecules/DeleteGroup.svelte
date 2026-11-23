<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  /**
   * Asking to be sure of deleting the group, opened by a button whose
   * `popovertarget` is `delete-group`; sent to the page's `delete`. Not
   * named in the title, which a long name or handle would overrun.
   */
</script>

<ConfirmDialog
  id="delete-group"
  title={m.delete_group_title()}
  action="?/delete"
  confirm={m.common_delete()}
>
  {m.delete_group_body()}
</ConfirmDialog>

<script lang="ts">
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import HandleInput from '#lib/molecules/HandleInput.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /** Inviting someone to the group, by their handle, with a note if one likes: opened by a button whose `popovertarget` is `id`. */
  let { id }: { id: string } = $props();
</script>

<FormDialog
  {id}
  title={m.invite_dialog_title()}
  action="?/invite"
  submit={m.invite_dialog_submit()}
>
  {#snippet intro()}{m.invite_dialog_intro()}{/snippet}
  <HandleInput
    id="{id}-handle"
    label={m.invite_dialog_handle()}
    placeholder="someone.bsky.social"
  />
  <label class="field"
    ><span>{m.invite_dialog_note()} <span class="optional">{m.common_optional()}</span></span>
    <textarea name="message" rows="3" maxlength="2000"></textarea></label
  >
</FormDialog>

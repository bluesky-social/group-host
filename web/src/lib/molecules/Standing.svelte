<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import { intl } from '#lib/locale.ts';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  /**
   * A member's standing in the group: the roles they hold there
   * beyond being a member, which everyone is, in a sentence; and the way
   * to leave it, asked to be sure of.
   */
  let { roles }: { roles: string[] } = $props();
  const held = $derived(roles.filter((r) => r !== 'member'));
  const said = $derived(
    held.length
      ? m.standing_member_roles({
          count: held.length,
          roles: new Intl.ListFormat(intl(), { type: 'conjunction' }).format(held)
        })
      : m.standing_member()
  );
</script>

<div class="standing">
  <span class="mute">{said}</span>
  <button class="quiet small" type="button" popovertarget="leave">{m.standing_leave()}</button>
</div>
<ConfirmDialog id="leave" title={m.standing_title()} action="?/leave" confirm={m.standing_leave()}>
  {m.standing_body()}
</ConfirmDialog>

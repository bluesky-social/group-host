<script lang="ts">
  import { goto } from '$app/navigation';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import MemberRow from '#lib/molecules/MemberRow.svelte';
  import Person from '#lib/molecules/Person.svelte';
  type Member = { actor: { did: string; handle?: string }; roles: { role: string }[] };
  /**
   * Who is in the group, `shown` at a time, found by part of their handle
   * (`q`), with more to show while there are (`more`); and their roles:
   * for one who may give roles, the ones they may give, to give or take,
   * in a dialog; for one who may remove people, the way to, asked to be
   * sure of. Their own row they leave from, not here.
   */
  let {
    members,
    q = '',
    shown,
    more = false,
    me,
    assignable = [],
    canEject = false
  }: {
    members: Member[];
    q?: string;
    shown: number;
    more?: boolean;
    me?: string;
    assignable?: string[];
    canEject?: boolean;
  } = $props();
  const named = (m: Member) => m.actor.handle ?? m.actor.did;
  // Found again as it is typed, the field keeping its place; sent as a form without a script.
  function find(e: Event) {
    const typed = (e.currentTarget as HTMLInputElement).value.trim();
    goto(typed ? `?q=${encodeURIComponent(typed)}` : '?', { reset: false, replace: true });
  }
</script>

<form class="find" method="get" role="search">
  <label class="field"
    >Find a member <input
      name="q"
      type="search"
      value={q}
      autocomplete="off"
      placeholder="Part of their handle"
      oninput={find}
    /></label
  >
</form>
{#if members.length}
  <ul class="members">
    {#each members as m, i (m.actor.did)}
      <li class="member">
        <MemberRow actor={m.actor} roles={m.roles.map((r) => r.role)}>
          {#if m.actor.did !== me && assignable.length}
            <button
              class="quiet small"
              type="button"
              popovertarget={`roles-${i}`}
              aria-label={`Change ${named(m)}'s roles`}>Roles</button
            >
          {/if}
          {#if m.actor.did !== me && canEject}
            <button
              class="quiet small"
              type="button"
              popovertarget={`eject-${i}`}
              aria-label={`Remove ${named(m)}`}>Remove</button
            >
          {/if}
        </MemberRow>
        {#if m.actor.did !== me && assignable.length}
          <FormDialog id={`roles-${i}`} title="Change roles" action="?/roles" submit="Save">
            {#snippet intro()}Choose the roles <Person actor={m.actor} /> holds. Everyone keeps the member
              role.{/snippet}
            <input type="hidden" name="did" value={m.actor.did} />
            {#each m.roles.filter((r) => !assignable.includes(r.role)) as kept (kept.role)}
              <input type="hidden" name="role" value={kept.role} />
            {/each}
            <fieldset>
              <legend>Roles you can give</legend>
              {#each assignable as role (role)}
                <label
                  ><input
                    type="checkbox"
                    name="role"
                    value={role}
                    checked={m.roles.some((r) => r.role === role)}
                  />
                  {role}</label
                >
              {/each}
            </fieldset>
          </FormDialog>
        {/if}
        {#if m.actor.did !== me && canEject}
          <ConfirmDialog
            id={`eject-${i}`}
            title="Remove this member?"
            action="?/eject"
            confirm="Remove"
            fields={{ did: m.actor.did }}
          >
            <Person actor={m.actor} /> loses access to what only members see. They can ask to join again.
          </ConfirmDialog>
        {/if}
      </li>
    {/each}
  </ul>
  {#if more}
    <form method="get" data-sveltekit-reset="false" data-sveltekit-replacestate>
      {#if q}<input type="hidden" name="q" value={q} />{/if}
      <input type="hidden" name="shown" value={shown + 5} />
      <button class="quiet small" type="submit">Show more</button>
    </form>
  {/if}
{:else}
  <!-- A group always holds someone: none shown is none found. -->
  <Empty title="No member's handle has that in it"
    >Try fewer letters, or clear the search to see everyone.</Empty
  >
{/if}

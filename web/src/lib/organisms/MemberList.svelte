<script lang="ts">
  import { goto } from '$app/navigation';
  import { ParaglideMessage } from '@inlang/paraglide-js-svelte';
  import ActionMenu from '#lib/molecules/ActionMenu.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import MemberRow from '#lib/molecules/MemberRow.svelte';
  import Person from '#lib/molecules/Person.svelte';
  import { m } from '#lib/paraglide/messages.js';
  type Member = { actor: { did: string; handle?: string }; roles: { role: string }[] };
  /**
   * Who is in the group, `shown` at a time, found by part of their handle
   * (`q`), with more to show while there are (`more`); and their roles:
   * for one who may give roles, the ones they may give, to give or take,
   * in a dialog; for one who may remove people, the way to, asked to be
   * sure of; both from a "⋯" menu at the row's end, as every row has them.
   * Their own row they leave from, not here.
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
  const named = (member: Member) => member.actor.handle ?? member.actor.did;
  // Found again as it is typed, the field keeping its place; sent as a form without a script.
  function find(e: Event) {
    const typed = (e.currentTarget as HTMLInputElement).value.trim();
    goto(typed ? `?q=${encodeURIComponent(typed)}` : '?', { reset: false, replace: true });
  }
</script>

<form class="find" method="get" role="search">
  <label class="field"
    >{m.member_list_find()}
    <input
      name="q"
      type="search"
      value={q}
      autocomplete="off"
      placeholder={m.member_list_find_placeholder()}
      oninput={find}
    /></label
  >
</form>
{#if members.length}
  <ul class="members">
    {#each members as member, i (member.actor.did)}
      <li class="member">
        <MemberRow actor={member.actor} roles={member.roles.map((r) => r.role)}>
          {#if member.actor.did !== me}<ActionMenu
              label={m.member_list_more({ member: named(member) })}
              actions={[
                ...(assignable.length
                  ? [{ text: m.member_list_change_roles(), opens: `roles-${i}` }]
                  : []),
                ...(canEject ? [{ text: m.member_list_remove(), opens: `eject-${i}` }] : [])
              ]}
            />{/if}
        </MemberRow>
        {#if member.actor.did !== me && assignable.length}
          <FormDialog
            id={`roles-${i}`}
            title={m.member_list_change_roles()}
            action="?/roles"
            submit={m.common_save()}
          >
            {#snippet intro()}<ParaglideMessage message={m.member_list_roles_intro}>
                {#snippet person()}<Person actor={member.actor} />{/snippet}
              </ParaglideMessage>{/snippet}
            <input type="hidden" name="did" value={member.actor.did} />
            {#each member.roles.filter((r) => !assignable.includes(r.role)) as kept (kept.role)}
              <input type="hidden" name="role" value={kept.role} />
            {/each}
            <fieldset>
              <legend>{m.member_list_roles_legend()}</legend>
              {#each assignable as role (role)}
                <label
                  ><input
                    type="checkbox"
                    name="role"
                    value={role}
                    checked={member.roles.some((r) => r.role === role)}
                  />
                  {role}</label
                >
              {/each}
            </fieldset>
          </FormDialog>
        {/if}
        {#if member.actor.did !== me && canEject}
          <ConfirmDialog
            id={`eject-${i}`}
            title={m.member_list_remove_title()}
            action="?/eject"
            confirm={m.common_remove()}
            fields={{ did: member.actor.did }}
          >
            <ParaglideMessage message={m.member_list_remove_body}>
              {#snippet person()}<Person actor={member.actor} />{/snippet}
            </ParaglideMessage>
          </ConfirmDialog>
        {/if}
      </li>
    {/each}
  </ul>
  {#if more}
    <form method="get" data-sveltekit-reset="false" data-sveltekit-replacestate>
      {#if q}<input type="hidden" name="q" value={q} />{/if}
      <input type="hidden" name="shown" value={shown + 5} />
      <button class="quiet small" type="submit">{m.common_show_more()}</button>
    </form>
  {/if}
{:else}
  <!-- A group always holds someone: none shown is none found. -->
  <Empty title={m.member_list_empty_title()}>{m.member_list_empty_body()}</Empty>
{/if}

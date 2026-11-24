<script lang="ts">
  import ActionMenu from '#lib/molecules/ActionMenu.svelte';
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import RoleFields from '#lib/molecules/RoleFields.svelte';
  import Tag from '#lib/atoms/Tag.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * A group's roles: each, what it may do, and for one who may give it,
   * a "⋯" menu to change it or delete it (asked to be sure of); and the
   * dialog creating a new one, opened by a button whose `popovertarget`
   * is `define-role`. What a role may be given is what the one editing
   * holds enough of to give (`grantable`, scopes), and the roles its
   * holders may give those they may give themselves (`mine`, `*` for
   * every role), as the host refuses any more.
   */
  type Role = { key: string; name?: string; default?: boolean };
  type Binding = {
    role: string;
    actions: string[];
    assignable: { role: string }[];
    scopes?: { scope: string; actions?: string[] }[];
  };
  let {
    roles = [],
    bindings = [],
    grantable = [],
    mine = []
  }: { roles?: Role[]; bindings?: Binding[]; grantable?: string[]; mine?: string[] } = $props();
  const of = (key: string) => bindings.find((b) => b.role === key);
  const may = (key: string) => of(key)?.actions ?? [];
  const yours = (key: string) => mine.includes('*') || mine.includes(key);
</script>

<ul class="roles">
  {#each roles as r, i (r.key)}
    <li>
      <span class="row-main">
        <span><strong>{r.name ?? r.key}</strong> <span class="mute mono">{r.key}</span></span>
        <span class="what"
          >{may(r.key).length ? m.roles_editor_can() : m.roles_editor_nothing_more()}</span
        >
        {#if may(r.key).length}<span class="tags"
            >{#each may(r.key) as a (a)}<Tag text={a} />{/each}</span
          >{/if}
      </span>
      {#if yours(r.key)}
        <ActionMenu
          label={m.roles_editor_more({ role: r.name ?? r.key })}
          actions={[
            { text: m.roles_editor_change(), opens: `change-role-${i}` },
            ...(r.key !== 'member' ? [{ text: m.common_delete(), opens: `unrole-${i}` }] : [])
          ]}
        />
        <FormDialog
          id={`change-role-${i}`}
          title={m.roles_editor_change_title()}
          action="?/role"
          submit={m.common_save()}
          wide
        >
          {#snippet intro()}{m.roles_editor_change_intro({ role: r.name ?? r.key })}{/snippet}
          <input type="hidden" name="role" value={r.key} />
          <label class="field"
            >{m.roles_editor_name()}
            <input name="name" required maxlength="64" value={r.name ?? r.key} /></label
          >
          <RoleFields
            scopes={of(r.key)?.scopes}
            assignable={of(r.key)?.assignable.map((a) => a.role)}
            isDefault={r.default}
            {roles}
            {grantable}
            {mine}
          />
        </FormDialog>
        {#if r.key !== 'member'}
          <ConfirmDialog
            id={`unrole-${i}`}
            title={m.roles_editor_delete_title()}
            action="?/unrole"
            confirm={m.common_delete()}
            fields={{ role: r.key }}
          >
            {m.roles_editor_delete_body()}
          </ConfirmDialog>
        {/if}
      {/if}
    </li>
  {/each}
</ul>
<FormDialog
  id="define-role"
  title={m.roles_editor_create_title()}
  action="?/role"
  submit={m.common_create()}
  wide
>
  {#snippet intro()}{m.roles_editor_create_intro()}{/snippet}
  <div class="pair">
    <label class="field"
      >{m.roles_editor_key()}
      <input
        name="role"
        required
        pattern="[a-z0-9-]+"
        maxlength="64"
        placeholder="moderator"
      /></label
    >
    <label class="field"
      >{m.roles_editor_name()}
      <input
        name="name"
        required
        maxlength="64"
        placeholder={m.roles_editor_name_placeholder()}
      /></label
    >
  </div>
  <RoleFields {roles} {grantable} {mine} />
</FormDialog>

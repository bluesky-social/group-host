<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import Tag from '#lib/atoms/Tag.svelte';
  import { m } from '#lib/paraglide/messages.js';
  /**
   * A group's roles: each, what it may do, to delete (asked to be sure
   * of); and the dialog creating a new one, opened by a button whose
   * `popovertarget` is `define-role`: what it may do, the roles its
   * holders may give, and whether members get it on joining.
   */
  type Role = { key: string; name?: string; description?: string };
  type Binding = { role: string; actions: string[] };
  let {
    roles = [],
    bindings = [],
    actions
  }: { roles?: Role[]; bindings?: Binding[]; actions: string[] } = $props();
  const may = (key: string) => bindings.find((b) => b.role === key)?.actions ?? [];
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
      {#if r.key !== 'member'}
        <span class="row-actions"
          ><button
            class="quiet small"
            type="button"
            popovertarget={`unrole-${i}`}
            aria-label={m.roles_editor_delete_label({ role: r.name ?? r.key })}
            >{m.common_delete()}</button
          ></span
        >
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
  <fieldset>
    <legend>{m.roles_editor_may()}</legend>
    {#each actions as a (a)}<label><input type="checkbox" name="action" value={a} /> {a}</label
      >{/each}
  </fieldset>
  <fieldset>
    <legend>{m.roles_editor_can_give()}</legend>
    {#each roles as r (r.key)}<label
        ><input type="checkbox" name="assignable" value={r.key} /> {r.name ?? r.key}</label
      >{/each}
  </fieldset>
  <label><input type="checkbox" name="default" /> {m.roles_editor_default()}</label>
</FormDialog>

<script lang="ts">
  import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
  import FormDialog from '#lib/molecules/FormDialog.svelte';
  import Tag from '#lib/atoms/Tag.svelte';
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
        <span class="what">{may(r.key).length ? 'Can' : 'Nothing more than a member'}</span>
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
            aria-label={`Delete ${r.name ?? r.key}`}>Delete</button
          ></span
        >
        <ConfirmDialog
          id={`unrole-${i}`}
          title="Delete this role?"
          action="?/unrole"
          confirm="Delete"
          fields={{ role: r.key }}
        >
          People with this role lose what it lets them do. You can only delete a role nobody has.
        </ConfirmDialog>
      {/if}
    </li>
  {/each}
</ul>
<FormDialog id="define-role" title="Create a role" action="?/role" submit="Create" wide>
  {#snippet intro()}Choose what people with this role can do, and which roles they can give. You
    give roles to members from the group's page.{/snippet}
  <div class="pair">
    <label class="field"
      >Its key <input
        name="role"
        required
        pattern="[a-z0-9-]+"
        maxlength="64"
        placeholder="moderator"
      /></label
    >
    <label class="field"
      >Its name <input name="name" required maxlength="64" placeholder="Moderator" /></label
    >
  </div>
  <fieldset>
    <legend>It may</legend>
    {#each actions as a (a)}<label><input type="checkbox" name="action" value={a} /> {a}</label
      >{/each}
  </fieldset>
  <fieldset>
    <legend>People with it can give</legend>
    {#each roles as r (r.key)}<label
        ><input type="checkbox" name="assignable" value={r.key} /> {r.name ?? r.key}</label
      >{/each}
  </fieldset>
  <label><input type="checkbox" name="default" /> Every new member gets it</label>
</FormDialog>

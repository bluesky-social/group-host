<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Section from '#lib/molecules/Section.svelte';
  import RolesEditor from '#lib/organisms/RolesEditor.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
  // The roles the person looking may give, by their roles' grants: `*` for every one.
  const mine = $derived(
    (data.bindings ?? [])
      .filter((b) => g.viewer?.roles.some((r) => r.role === b.role))
      .flatMap((b) => b.assignable.map((a) => a.role))
  );
  // The scopes they hold enough of to give a role, as the host says.
  const grantable = $derived((g.viewer?.grantable ?? []).map((s) => s.scope));
</script>

<svelte:head
  ><title>{m.route_group_settings_roles_head({ group: g.displayName ?? g.handle })}</title
  ></svelte:head
>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title={m.route_group_settings_roles_title()} count={data.roles?.length ?? 0}>
  {#snippet actions()}<button class="quiet" type="button" popovertarget="define-role"
      >{m.route_group_settings_roles_create()}</button
    >{/snippet}
  {#snippet intro()}{m.route_group_settings_roles_intro()}{/snippet}
  <RolesEditor roles={data.roles} bindings={data.bindings} {grantable} {mine} />
</Section>

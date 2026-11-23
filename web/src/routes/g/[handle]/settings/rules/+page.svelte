<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Section from '#lib/molecules/Section.svelte';
  import RulesEditor from '#lib/organisms/RulesEditor.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
</script>

<svelte:head
  ><title>{m.route_group_settings_rules_head({ group: g.displayName ?? g.handle })}</title
  ></svelte:head
>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title={m.route_group_settings_rules_title()} count={g.rules?.length ?? 0}>
  {#snippet actions()}<button class="quiet" type="button" popovertarget="add-rule"
      >{m.route_group_settings_rules_add()}</button
    >{/snippet}
  {#snippet intro()}{m.route_group_settings_rules_intro()}{/snippet}
  <RulesEditor rules={g.rules} />
</Section>

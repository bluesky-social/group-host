<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Empty from '#lib/molecules/Empty.svelte';
  import Section from '#lib/molecules/Section.svelte';
  import RecoveryKeys from '#lib/organisms/RecoveryKeys.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
  const r = $derived(data.recovery);
  const stewards = $derived(r?.keys.filter((k) => k.holder === 'steward').length ?? 0);
</script>

<svelte:head
  ><title>{m.route_group_settings_recovery_head({ group: g.displayName ?? g.handle })}</title
  ></svelte:head
>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title={m.route_group_settings_recovery_title()}>
  {#snippet actions()}
    {#if r?.hostHolds && stewards < r.max}<button
        class="quiet"
        type="button"
        popovertarget="add-key">{m.route_group_settings_recovery_add()}</button
      >{/if}
  {/snippet}
  {#snippet intro()}{m.route_group_settings_recovery_intro()}{/snippet}
  {#if !r}
    <Empty title={m.route_group_settings_recovery_none_title()}
      >{m.route_group_settings_recovery_none_body()}</Empty
    >
  {:else}
    {#if !r.hostHolds}<p class="mute">
        {m.route_group_settings_recovery_lost()}
      </p>{/if}
    <RecoveryKeys keys={r.keys} hostHolds={r.hostHolds} />
  {/if}
</Section>

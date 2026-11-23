<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Section from '#lib/molecules/Section.svelte';
  import LabelsEditor from '#lib/organisms/LabelsEditor.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
</script>

<svelte:head
  ><title>{m.route_group_settings_labels_head({ group: g.displayName ?? g.handle })}</title
  ></svelte:head
>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title={m.route_group_settings_labels_title()} count={g.labels?.length ?? 0}>
  {#snippet actions()}<button class="quiet" type="button" popovertarget="create-label"
      >{m.route_group_settings_labels_create()}</button
    >{/snippet}
  {#snippet intro()}{m.route_group_settings_labels_intro()}{/snippet}
  <LabelsEditor labels={g.labels} />
</Section>

<script lang="ts">
  import Section from '#lib/molecules/Section.svelte';
  import LabelsEditor from '#lib/organisms/LabelsEditor.svelte';
  let { data, form } = $props();
  const g = $derived(data.group);
</script>

<svelte:head><title>Labels · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title="Labels" count={g.labels?.length ?? 0}>
  {#snippet actions()}<button class="quiet" type="button" popovertarget="create-label"
      >Create a label</button
    >{/snippet}
  {#snippet intro()}The labels moderators put on accounts and posts. Apps that follow the group's
    labels read what each means and how to show it.{/snippet}
  <LabelsEditor labels={g.labels} />
</Section>

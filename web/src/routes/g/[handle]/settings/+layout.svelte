<script lang="ts">
  import Empty from '#lib/molecules/Empty.svelte';
  import Tabs from '#lib/molecules/Tabs.svelte';
  import Page from '#lib/templates/Page.svelte';
  import { page } from '$app/state';
  let { data, children } = $props();
  const g = $derived(data.group);
  const name = $derived(g.displayName ?? g.handle);
  const at = $derived(`/g/${g.handle}/settings`);
  // The parts the person looking may change, each a page.
  const tabs = $derived([
    ...(data.configures
      ? [
          { href: at, text: 'Profile' },
          { href: `${at}/site`, text: 'Site' },
          { href: `${at}/rules`, text: 'Rules' },
          { href: `${at}/labels`, text: 'Labels' },
          { href: `${at}/roles`, text: 'Roles' }
        ]
      : []),
    ...(data.configures || data.passwords ? [{ href: `${at}/apps`, text: 'Apps' }] : []),
    ...(data.configures ? [{ href: `${at}/recovery`, text: 'Recovery keys' }] : [])
  ]);
  // A part of them that is theirs to change; any other is not, whatever its address.
  const theirs = $derived(tabs.some((t) => t.href === page.url.pathname.replace(/\/$/, '')));
</script>

<Page title="Settings" back={{ href: `/g/${g.handle}`, text: name }}>
  {#snippet nav()}
    {#if tabs.length > 1}<Tabs {tabs} current={page.url.pathname} label="Settings" />{/if}
  {/snippet}
  {#if theirs}
    {@render children()}
  {:else}
    <Empty title="Nothing here is yours to change">
      Only people who run {name} can change its settings.
      {#snippet action()}<a class="button quiet" href={`/g/${g.handle}`}>Back to {name}</a
        >{/snippet}
    </Empty>
  {/if}
</Page>

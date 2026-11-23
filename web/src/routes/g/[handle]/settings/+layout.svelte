<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
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
          { href: at, text: m.route_group_settings_tab_profile() },
          { href: `${at}/site`, text: m.route_group_settings_tab_site() },
          { href: `${at}/rules`, text: m.route_group_settings_tab_rules() },
          { href: `${at}/labels`, text: m.route_group_settings_tab_labels() },
          { href: `${at}/roles`, text: m.route_group_settings_tab_roles() }
        ]
      : []),
    ...(data.configures || data.passwords
      ? [{ href: `${at}/apps`, text: m.route_group_settings_tab_apps() }]
      : []),
    ...(data.configures
      ? [{ href: `${at}/recovery`, text: m.route_group_settings_tab_recovery() }]
      : [])
  ]);
  // A part of them that is theirs to change; any other is not, whatever its address.
  const theirs = $derived(tabs.some((t) => t.href === page.url.pathname.replace(/\/$/, '')));
</script>

<Page title={m.route_group_settings_title()} back={{ href: `/g/${g.handle}`, text: name }}>
  {#snippet nav()}
    {#if tabs.length > 1}<Tabs
        {tabs}
        current={page.url.pathname}
        label={m.route_group_settings_tabs()}
      />{/if}
  {/snippet}
  {#if theirs}
    {@render children()}
  {:else}
    <Empty title={m.route_group_settings_not_yours_title()}>
      {m.route_group_settings_not_yours_body({ group: name })}
      {#snippet action()}<a class="button quiet" href={`/g/${g.handle}`}
          >{m.route_group_settings_back({ group: name })}</a
        >{/snippet}
    </Empty>
  {/if}
</Page>

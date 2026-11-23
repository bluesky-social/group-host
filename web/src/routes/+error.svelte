<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import Empty from '#lib/molecules/Empty.svelte';
  import Page from '#lib/templates/Page.svelte';
  import { page } from '$app/state';
  // What went wrong, as a person would put it, and the host's own words under it.
  const titles: Record<number, () => string> = {
    400: m.route_error_400,
    403: m.route_error_403,
    404: m.route_error_404
  };
  const title = $derived((titles[page.status] ?? m.route_error_other)());
</script>

<svelte:head><title>{m.route_error_head({ title })}</title></svelte:head>

<Page title={page.status === 404 ? m.route_error_not_found() : m.route_error_wrong()}>
  <Empty {title}>
    {page.error?.message ?? ''}
    {#snippet action()}<a class="button quiet" href="/">{m.route_error_go_home()}</a>{/snippet}
  </Empty>
</Page>

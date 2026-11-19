<script lang="ts">
  import '../app.css';
  import SiteHeader from '#lib/organisms/SiteHeader.svelte';
  import SiteFooter from '#lib/organisms/SiteFooter.svelte';
  import LeaveGuard from '#lib/molecules/LeaveGuard.svelte';
  import { page } from '$app/state';
  import { onMount } from 'svelte';
  let { data, children } = $props();
  // The page answers in the browser from here: what is typed or picked
  // before then may be put back as it hydrates. The e2e flows wait for it.
  onMount(() => {
    document.documentElement.dataset.hydrated = '';
  });
</script>

<SiteHeader
  viewer={data.viewer}
  invites={data.invitations.length}
  accounts={data.accounts}
  signUpAt={data.signUpAt}
/>
<main class="wrap">{@render children()}</main>
<SiteFooter host={page.url.hostname} />
<LeaveGuard />

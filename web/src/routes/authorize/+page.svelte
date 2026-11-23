<script lang="ts">
  import { m } from '#lib/paraglide/messages.js';
  import AuthorizeRequest from '#lib/organisms/AuthorizeRequest.svelte';
  import BackToApp from '#lib/molecules/BackToApp.svelte';
  import Empty from '#lib/molecules/Empty.svelte';
  import SignIn from '#lib/molecules/SignIn.svelte';
  import Page from '#lib/templates/Page.svelte';
  import { page } from '$app/state';
  let { data, form } = $props();
</script>

<svelte:head><title>{m.route_authorize_head()}</title></svelte:head>

<Page title={m.route_authorize_title()}>
  {#if form?.redirectUrl}
    <BackToApp to={form.redirectUrl} />
  {:else if data.expired}
    <Empty title={m.route_authorize_expired_title()}>{m.route_authorize_expired_body()}</Empty>
  {:else if data.request}
    <AuthorizeRequest
      client={data.request.client}
      groups={data.request.groups}
      labels={data.request.labels ?? []}
      requested={data.request.requested}
      requestUri={data.requestUri}
      mcp={data.request.mcp ?? false}
    />
  {:else}
    <p>{m.route_authorize_sign_in_first()}</p>
    <p><SignIn /></p>
  {/if}
</Page>

<script lang="ts">
	import AuthorizeRequest from '#lib/organisms/AuthorizeRequest.svelte';
	import BackToApp from '#lib/molecules/BackToApp.svelte';
	import Empty from '#lib/molecules/Empty.svelte';
	import SignIn from '#lib/molecules/SignIn.svelte';
	import Page from '#lib/templates/Page.svelte';
	import { page } from '$app/state';
	let { data, form } = $props();
</script>

<svelte:head><title>Sign in an app · opensocial</title></svelte:head>

<Page title="Sign in an app">
	{#if form?.redirectUrl}
		<BackToApp to={form.redirectUrl} />
	{:else if data.expired}
		<Empty title="This sign-in request has expired">Go back to the app and try again.</Empty>
	{:else if data.request}
		<AuthorizeRequest client={data.request.client} groups={data.request.groups} labels={data.request.labels ?? []} requestUri={data.requestUri} assistant={data.request.assistant ?? false} />
	{:else}
		<p>An app wants to sign in as one of your groups. Sign in first, as yourself.</p>
		<p><SignIn /></p>
	{/if}
</Page>

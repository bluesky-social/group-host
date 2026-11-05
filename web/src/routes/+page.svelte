<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import GroupList from '#lib/organisms/GroupList.svelte';
	import Invitations from '#lib/organisms/Invitations.svelte';
	import Page from '#lib/templates/Page.svelte';
	import Welcome from '#lib/templates/Welcome.svelte';
	import { page } from '$app/state';
	let { data, form } = $props();
</script>

<svelte:head><title>opensocial</title></svelte:head>

{#if !data.viewer}
	<Welcome title={`Groups on ${page.url.hostname}`}>Sign in with your Atmosphere account to see your groups, or to start one.</Welcome>
{:else}
	{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
	{#if data.invitations.length}
		<Page title="You are invited">
			<Invitations invitations={data.invitations} />
		</Page>
	{/if}
	{#if data.mine}
		<Page title="Your groups">
			<p><a href="/new">Start a group</a></p>
			{#if data.mine.length}
				<GroupList groups={data.mine} />
			{:else}
				<Empty title="You are in no group here yet">Start one, or join one you are invited to.</Empty>
			{/if}
		</Page>
	{/if}
{/if}

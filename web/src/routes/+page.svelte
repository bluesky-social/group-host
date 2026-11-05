<script lang="ts">
	import GroupList from '#lib/organisms/GroupList.svelte';
	import Invitations from '#lib/organisms/Invitations.svelte';
	import Section from '#lib/molecules/Section.svelte';
	import Page from '#lib/templates/Page.svelte';
	import Welcome from '#lib/templates/Welcome.svelte';
	import { page } from '$app/state';
	let { data, form } = $props();
</script>

<svelte:head><title>opensocial</title></svelte:head>

{#if !data.viewer}
	<Welcome title={`Groups on ${page.url.hostname}`}>Sign in with your Atmosphere account to see your groups, or to start one.</Welcome>
{:else}
	<Page title="Your groups">
		{#snippet actions()}<a class="button" href="/new">Start a group</a>{/snippet}
		{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
		<Section id="invites" title="Invites waiting" count={data.invitations.length}>
			<Invitations invitations={data.invitations} />
		</Section>
		{#if data.mine}
			<Section title="Groups you are in" count={data.mine.length}>
				<GroupList groups={data.mine} />
			</Section>
		{/if}
	</Page>
{/if}

<script lang="ts">
	import GroupProfile from '#lib/organisms/GroupProfile.svelte';
	import JoinRequests from '#lib/organisms/JoinRequests.svelte';
	import MemberList from '#lib/organisms/MemberList.svelte';
	import Standing from '#lib/molecules/Standing.svelte';
	import Page from '#lib/templates/Page.svelte';
	let { data } = $props();
	const g = $derived(data.group);
</script>

<svelte:head><title>{g.displayName ?? g.handle} · opensocial</title></svelte:head>

<Page title={g.displayName ?? g.handle}>
	<p class="mute mono">@{g.handle}</p>
	{#if g.deactivated}
		<p>This group is switched off.</p>
	{:else}
		<GroupProfile description={g.description} rules={g.rules} />
		{#if g.viewer}
			<Standing member={g.viewer.member} asked={!!g.viewer.request} joinPolicy={g.joinPolicy} />
		{/if}
		{#if data.members}
			<h2>Members</h2>
			<MemberList members={data.members} />
		{/if}
		{#if data.joinRequests}
			<h2>Asking to join</h2>
			<JoinRequests requests={data.joinRequests} />
		{/if}
	{/if}
</Page>

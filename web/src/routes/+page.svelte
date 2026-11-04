<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import GroupList from '#lib/organisms/GroupList.svelte';
	import Page from '#lib/templates/Page.svelte';
	let { data } = $props();
</script>

<svelte:head><title>opensocial</title></svelte:head>

{#if data.mine}
	<Page title="Your groups">
		{#if data.mine.length}
			<GroupList groups={data.mine} />
		{:else}
			<Empty title="You are in no group here yet">Join one below, or start one.</Empty>
		{/if}
	</Page>
{/if}
<Page title="Groups on this host">
	<GroupList groups={data.groups} />
</Page>

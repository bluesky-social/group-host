<script lang="ts">
	import GroupList from '#lib/organisms/GroupList.svelte';
	import Page from '#lib/templates/Page.svelte';
	let { data } = $props();
</script>

<svelte:head><title>opensocial</title></svelte:head>

<Page title="Groups on this host">
	<GroupList groups={data.groups} />
</Page>

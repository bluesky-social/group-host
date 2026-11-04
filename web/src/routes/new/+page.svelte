<script lang="ts">
	import NewGroupForm from '#lib/organisms/NewGroupForm.svelte';
	import SignIn from '#lib/molecules/SignIn.svelte';
	import Page from '#lib/templates/Page.svelte';
	let { data, form } = $props();
</script>

<svelte:head><title>Start a group · opensocial</title></svelte:head>

<Page title="Start a group">
	{#if !data.actor}
		<p>Sign in first, as yourself: you will be its founder.</p>
		<SignIn back="/new" />
	{:else if !data.actor.canCreateGroup}
		<p>This host starts groups only for the people it names.</p>
	{:else}
		<NewGroupForm failed={form?.failed} />
	{/if}
</Page>

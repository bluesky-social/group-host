<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import Page from '#lib/templates/Page.svelte';
	import { page } from '$app/state';
	// What went wrong, as a person would put it, and the host's own words under it.
	const titles: Record<number, string> = { 400: 'Nothing was asked of this page', 403: 'This is not yours to see', 404: 'Nothing is here' };
	const title = $derived(titles[page.status] ?? 'Something went wrong');
</script>

<svelte:head><title>{title} · opensocial</title></svelte:head>

<Page title={page.status === 404 ? 'Not found' : 'Something is wrong'}>
	<Empty {title}>
		{page.error?.message ?? ''}
		{#snippet action()}<a class="button quiet" href="/">Go to your groups</a>{/snippet}
	</Empty>
</Page>

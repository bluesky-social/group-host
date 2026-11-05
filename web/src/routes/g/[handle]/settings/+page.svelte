<script lang="ts">
	import Section from '#lib/molecules/Section.svelte';
	import LinksEditor from '#lib/organisms/LinksEditor.svelte';
	import ProfileForm from '#lib/organisms/ProfileForm.svelte';
	import ProfilePictures from '#lib/organisms/ProfilePictures.svelte';
	import Visibility from '#lib/organisms/Visibility.svelte';
	import { image } from '#lib/images.ts';
	let { data, form } = $props();
	const g = $derived(data.group);
</script>

<svelte:head><title>Settings · {g.displayName ?? g.handle} · opensocial</title></svelte:head>

{#if form?.failed}<p class="danger">{form.failed}</p>{/if}
<Section title="About it">
	<ProfileForm displayName={g.displayName} description={g.description} joinPolicy={g.joinPolicy} url={g.url} location={g.location} online={g.online} topics={g.topics} languages={g.languages} />
</Section>
<Section title="Links" count={g.links?.length ?? 0}>
	{#snippet actions()}
		{#if (g.links?.length ?? 0) < 5}<button class="quiet" type="button" popovertarget="add-link">Add a link</button>{/if}
	{/snippet}
	<LinksEditor links={g.links} />
</Section>
<Section title="Pictures">
	<ProfilePictures name={g.displayName ?? g.handle} avatar={image(g.did, g.avatar)} banner={image(g.did, g.banner)} />
</Section>
<Section title="Who sees it">
	<Visibility listed={data.listed} />
</Section>

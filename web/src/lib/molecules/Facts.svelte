<script lang="ts">
	import Tag from '#lib/atoms/Tag.svelte';
	import { languageName, placeName, type Place } from '#lib/places.ts';
	/**
	 * What a group says of itself in a few words: where it meets, whether
	 * online, the languages it uses, and what it is about, each a tag.
	 * Nothing, where it says none of them.
	 */
	let {
		location,
		online = false,
		languages = [],
		topics = []
	}: { location?: Place; online?: boolean; languages?: { language: string }[]; topics?: { topic: string }[] } = $props();
	const place = $derived(placeName(location));
	const where = $derived(place ? `Meets in ${place}${online ? ', and online' : ''}.` : online ? 'Meets online.' : '');
	const spoken = $derived(new Intl.ListFormat('en', { type: 'conjunction' }).format(languages.map((l) => languageName(l.language))));
	const said = $derived([where, spoken && `Speaks ${spoken}.`].filter(Boolean).join(' '));
</script>

{#if said}<p class="facts">{said}</p>{/if}
{#if topics.length}
	<div class="tags topics">{#each topics as t (t.topic)}<Tag text={t.topic} />{/each}</div>
{/if}

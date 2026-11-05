<script lang="ts">
	import type { Snippet } from 'svelte';
	import Avatar from '#lib/atoms/Avatar.svelte';
	import Facts from '#lib/molecules/Facts.svelte';
	import Links from '#lib/molecules/Links.svelte';
	import type { Place } from '#lib/places.ts';
	import Rules from '#lib/molecules/Rules.svelte';
	/**
	 * A group as its profile shows it: its banner, when it has one; its
	 * picture (over the banner's foot), its name and handle beside it, and
	 * `nav`, the ways to its other pages, across from them; its words;
	 * where it meets, its languages and topics; its links and its rules;
	 * then `children`, where the person looking stands with it. With no `name`, its profile is not theirs to see, and it is
	 * its handle alone.
	 */
	let {
		handle,
		name,
		description,
		avatar,
		banner,
		location,
		online,
		languages,
		topics,
		links = [],
		rules = [],
		nav,
		children
	}: {
		handle: string;
		name?: string;
		description?: string;
		avatar?: string | null;
		banner?: string | null;
		location?: Place;
		online?: boolean;
		languages?: { language: string }[];
		topics?: { topic: string }[];
		links?: { url: string; title?: string }[];
		rules?: { rkey: string; title: string; text?: string }[];
		nav?: Snippet;
		children?: Snippet;
	} = $props();
</script>

<section class="group-card">
	{#if name && banner}<img class="banner" src={banner} alt="" />{/if}
	<div class="group-card-body">
		{#if name}
			<div class="identity">
				<Avatar src={avatar} size="lg" {name} />
				<div class="identity-name">
					<h1>{name}</h1>
					<p class="mute mono">{handle}</p>
				</div>
				{@render nav?.()}
			</div>
			{#if description}<p class="mute description">{description}</p>{/if}
			<Facts {location} {online} {languages} {topics} />
			<Links {name} {links} />
			<Rules {rules} />
		{:else}
			<h1 class="handle">{handle}</h1>
			{@render nav?.()}
		{/if}
		{@render children?.()}
	</div>
</section>

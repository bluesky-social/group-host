<script lang="ts">
	import type { Snippet } from 'svelte';
	import Avatar from '#lib/atoms/Avatar.svelte';
	import Links from '#lib/molecules/Links.svelte';
	import Rules from '#lib/molecules/Rules.svelte';
	/**
	 * A group as its profile shows it: its banner, its picture over the
	 * banner's foot, `nav` beside that, its name and handle, its words,
	 * its links and its rules; then `children`, where the person looking
	 * stands with it. With no `name`, its profile is not theirs to see, and
	 * it is its handle alone.
	 */
	let {
		handle,
		name,
		description,
		avatar,
		banner,
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
		links?: { url: string; title?: string }[];
		rules?: { rkey: string; title: string; text?: string }[];
		nav?: Snippet;
		children?: Snippet;
	} = $props();
</script>

<section class="group-card">
	{#if name}
		{#if banner}<img class="banner" src={banner} alt="" />{:else}<div class="banner" aria-hidden="true"></div>{/if}
	{/if}
	<div class="group-card-body">
		{#if name}
			<div class="identity">
				<Avatar src={avatar} size="lg" />
				{@render nav?.()}
			</div>
			<div>
				<h1>{name}</h1>
				<p class="mute mono">{handle}</p>
			</div>
			{#if description}<p class="mute description">{description}</p>{/if}
			<Links {name} {links} />
			<Rules {rules} />
		{:else}
			<h1 class="handle">{handle}</h1>
			{@render nav?.()}
		{/if}
		{@render children?.()}
	</div>
</section>

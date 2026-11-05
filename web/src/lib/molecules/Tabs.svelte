<script lang="ts">
	/**
	 * The parts of a page, each a page of its own, in a row: `current`, the
	 * address of the one shown. `label` names the row for a screen reader.
	 */
	let { tabs, current, label }: { tabs: { href: string; text: string }[]; current: string; label: string } = $props();
</script>

<nav class="tabs" aria-label={label}>
	{#each tabs as t (t.href)}<a href={t.href} aria-current={t.href === current ? 'page' : undefined}>{t.text}</a>{/each}
</nav>

<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	import FormDialog from '#lib/molecules/FormDialog.svelte';
	/**
	 * Where else a group is, as its profile lists them, each to take away;
	 * and the dialog adding one, opened by a button whose `popovertarget`
	 * is `add-link`.
	 */
	let { links = [] }: { links?: { url: string; title?: string }[] } = $props();
	const shown = (url: string) => url.replace(/^https?:\/\//, '').replace(/\/$/, '');
</script>

{#if links.length}
	<ul class="link-list">
		{#each links as l, i (i)}
			<li>
				<span class="row-main"><strong>{l.title ?? shown(l.url)}</strong><span class="what">{l.url}</span></span>
				<form class="row-actions" method="post" action="?/unlink">
					<input type="hidden" name="index" value={i} />
					<button class="quiet small" type="submit" aria-label={`Take away ${l.title ?? shown(l.url)}`}>Take away</button>
				</form>
			</li>
		{/each}
	</ul>
{:else}
	<Empty title="No links yet">Its chat, its calendar, its site: where else people find the group.</Empty>
{/if}
<FormDialog id="add-link" title="Add a link" action="?/link" submit="Add the link">
	<label class="field">Its address <input name="url" type="url" required placeholder="https://" /></label>
	<label class="field">What it is, if you like <input name="title" maxlength="64" placeholder="Our chat" /></label>
</FormDialog>

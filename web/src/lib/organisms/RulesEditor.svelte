<script lang="ts">
	import Empty from '#lib/molecules/Empty.svelte';
	/** A group's rules, each to take away, and a new one to add. */
	let { rules = [] }: { rules?: { rkey: string; title: string; text?: string }[] } = $props();
</script>

<div class="stack">
	{#if rules.length}
		<ol class="rules">
			{#each rules as r (r.rkey)}
				<li>
					<strong>{r.title}</strong>{#if r.text}<p class="mute">{r.text}</p>{/if}
					<form method="post" action="?/unrule"><input type="hidden" name="rkey" value={r.rkey} /><button class="quiet" type="submit">Take away “{r.title}”</button></form>
				</li>
			{/each}
		</ol>
	{:else}
		<Empty title="No rules yet" />
	{/if}
	<form class="stack" method="post" action="?/rule">
		<label class="field">Rule <input name="title" required maxlength="100" /></label>
		<label class="field">What it means <textarea name="text" maxlength="1000"></textarea></label>
		<button type="submit">Add the rule</button>
	</form>
</div>

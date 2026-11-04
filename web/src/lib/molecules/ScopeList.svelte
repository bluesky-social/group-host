<script lang="ts">
	/** What an app would be let do, or not, scope by scope, with each permission set's own words. */
	let { title, scopes, labels = [] }: { title: string; scopes: { scope: string }[]; labels?: { scope: string; text: string }[] } = $props();
	const said = (scope: string) => labels.find((l) => l.scope === scope)?.text;
</script>

{#if scopes.length}
	<div class="stack">
		<h3>{title}</h3>
		<ul>
			{#each scopes as s (s.scope)}
				<li>{#if said(s.scope)}{said(s.scope)} {/if}<code>{s.scope}</code></li>
			{/each}
		</ul>
	</div>
{/if}

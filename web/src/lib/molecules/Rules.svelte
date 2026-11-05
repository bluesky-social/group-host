<script lang="ts">
	/** A group's rules, folded under how many there are, each in order: its title, and what it says. */
	let { rules }: { rules: { rkey: string; title: string; text?: string }[] } = $props();
</script>

{#if rules.length}
	<details class="rules">
		<summary>Rules<span class="count">{rules.length}</span></summary>
		<ol>
			{#each rules as r (r.rkey)}
				<li><strong>{r.title}</strong>{#if r.text} {r.text}{/if}</li>
			{/each}
		</ol>
	</details>
{/if}

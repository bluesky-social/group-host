<script lang="ts">
	/**
	 * The group's rotation keys, the highest ranked first: the stewards'
	 * recovery keys, each to take off, and this host's; and a key to add.
	 * A recovery key's holder can undo what this host does to the group's
	 * identity, and take the group elsewhere without it.
	 */
	type Key = { key: string; holder: string; addedBy?: string };
	let { keys, hostHolds, max }: { keys: Key[]; hostHolds: boolean; max: number } = $props();
	const stewards = $derived(keys.filter((k) => k.holder === 'steward'));
</script>

<div class="stack">
	<ol class="recovery-keys">
		{#each keys as k (k.key)}
			<li>
				<code>{k.key}</code> <span class="mute">{k.holder === 'host' ? 'this host' : `a steward's${k.addedBy ? `, added by ${k.addedBy}` : ''}`}</span>
				{#if k.holder === 'steward' && hostHolds}
					<form method="post" action="?/unkey"><input type="hidden" name="key" value={k.key} /><button class="quiet" type="submit">Take off</button></form>
				{/if}
			</li>
		{/each}
	</ol>
	{#if !hostHolds}
		<p class="mute">This host no longer holds a key to the group's identity, so it cannot change them.</p>
	{:else if stewards.length < max}
		<form class="stack" method="post" action="?/key">
			<label class="field">A key of yours, as a did:key <input name="key" required placeholder="did:key:z…" /></label>
			<button type="submit">Add the recovery key</button>
		</form>
	{/if}
</div>

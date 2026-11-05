<script lang="ts">
	import ConfirmDialog from '#lib/molecules/ConfirmDialog.svelte';
	import FormDialog from '#lib/molecules/FormDialog.svelte';
	/**
	 * The group's rotation keys, the highest ranked first: the stewards'
	 * recovery keys, each to take off (asked to be sure of), and this
	 * host's; and the dialog adding a key, opened by a button whose
	 * `popovertarget` is `add-key`. A recovery key's holder can undo what
	 * this host does to the group's identity, and take the group elsewhere
	 * without it.
	 */
	type Key = { key: string; holder: string; addedBy?: string };
	let { keys, hostHolds }: { keys: Key[]; hostHolds: boolean } = $props();
</script>

<ol class="recovery-keys">
	{#each keys as k, i (k.key)}
		<li>
			<span class="row-main">
				<code class="key">{k.key}</code>
				<span class="what">{k.holder === 'host' ? 'This host' : `A steward's${k.addedBy ? `, added by ${k.addedBy}` : ''}`}</span>
			</span>
			{#if k.holder === 'steward' && hostHolds}
				<span class="row-actions"><button class="quiet small" type="button" popovertarget={`unkey-${i}`}>Take off</button></span>
				<ConfirmDialog id={`unkey-${i}`} title="Take this key off?" action="?/unkey" confirm="Take it off" fields={{ key: k.key }}>
					Whoever holds it can no longer recover the group's identity with it.
				</ConfirmDialog>
			{/if}
		</li>
	{/each}
</ol>
<FormDialog id="add-key" title="Add a recovery key" action="?/key" submit="Add the key">
	{#snippet intro()}A key you hold, which can recover the group's identity, and move it, without this host.{/snippet}
	<label class="field">A key of yours, as a did:key <input name="key" required placeholder="did:key:z…" /></label>
</FormDialog>

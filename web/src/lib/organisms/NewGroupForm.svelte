<script lang="ts">
	import { finished, labelOf } from '#lib/handles.ts';
	/**
	 * Starting a group: its name, and its address under the host's
	 * `domain`, made from the name as it is typed until it is changed by
	 * hand; whether that address is free, asked once typing stops, with the
	 * next free one to take if not; its words; and how people get in.
	 * `failed` is what the host said of the last try, if it refused it.
	 */
	let { failed, domain }: { failed?: string; domain: string } = $props();
	let name = $state('');
	let label = $state('');
	let edited = $state(false);
	// Taken, and the next free one if there is one near: none while free or unasked.
	let taken = $state<{ label: string; next?: string } | null>(null);

	// Free unless the host says not: its handle, or kept as a group's
	// address. A host that cannot say leaves it to the host to refuse.
	const free = async (l: string) => {
		const asked = await fetch(`/xrpc/fyi.opensocial.checkAddress?address=${encodeURIComponent(l + domain)}`).catch(() => null);
		const said: { available?: boolean } | null = asked?.ok ? await asked.json() : null;
		return said?.available !== false;
	};
	$effect(() => {
		const l = finished(label);
		taken = null;
		if (!l) return;
		const wait = setTimeout(async () => {
			if (await free(l)) return;
			let next: string | undefined;
			for (let n = 2; n < 20 && !next; n++) if (await free(`${l}-${n}`)) next = `${l}-${n}`;
			if (finished(label) === l) taken = { label: l, next };
		}, 350);
		return () => clearTimeout(wait);
	});
</script>

<form class="stack" method="post">
	{#if failed}<p class="danger">{failed}</p>{/if}
	<label class="field">
		Name
		<input name="displayName" required maxlength="64" bind:value={name} oninput={() => (label = edited ? label : labelOf(name))} />
	</label>
	<label class="field">
		Its address
		<span class="address">
			<input name="label" required maxlength="63" pattern="[a-z0-9][a-z0-9\-]*" bind:value={label} oninput={() => ((edited = true), (label = labelOf(label)))} />
			<span class="mute mono">{domain}</span>
		</span>
	</label>
	{#if taken}
		<p class="danger">
			{taken.label}{domain} is taken.
			{#if taken.next}<button class="quiet" type="button" onclick={() => ((edited = true), (label = taken?.next ?? label))}>Use {taken.next}{domain}</button>{/if}
		</p>
	{:else}
		<p class="mute small">Made from its name; change it if you like. It is the group's handle, and where its pages are.</p>
	{/if}
	<label class="field">What it is for <textarea name="description" maxlength="300"></textarea></label>
	<label class="field">
		How people join
		<select name="joinPolicy">
			<option value="approval">They ask, and a member who may admits them</option>
			<option value="open">Anyone signed in, at once</option>
			<option value="invite">Only by invitation</option>
		</select>
	</label>
	<button type="submit">Start the group</button>
</form>
